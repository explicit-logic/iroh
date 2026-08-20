use std::{
    fmt, fs,
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

use anyhow::{Context, Result, anyhow};
use iroh::endpoint::Connection;
use iroh_blobs::{
    BlobFormat, Hash, HashAndFormat,
    api::{
        Store,
        blobs::{ExportMode, ExportOptions},
        remote::GetProgressItem,
    },
    format::collection::Collection,
};
use n0_future::StreamExt;

use crate::{
    paths::{absolute, safe_component, safe_relative_path, unique_destination},
    progress::{PROGRESS_INTERVAL, progress},
    push::{Announce, CLOSE_FAILED, CLOSE_REJECTED},
    store::{TempStore, recv_store_path},
};

pub const DOWNLOAD_DIR: &str = "downloads";

#[derive(Debug)]
pub enum PushFailure {
    /// Refused before anything was fetched or written.
    Rejected(anyhow::Error),
    /// Accepted, then broken part way through.
    Failed(anyhow::Error),
}

impl PushFailure {
    pub fn close_code(&self) -> u32 {
        match self {
            Self::Rejected(_) => CLOSE_REJECTED,
            Self::Failed(_) => CLOSE_FAILED,
        }
    }

    /// The close reason the sender prints. Kept short: QUIC carries it in the
    /// CONNECTION_CLOSE frame.
    pub fn reason(&self) -> &'static [u8] {
        match self {
            Self::Rejected(_) => b"rejected",
            Self::Failed(_) => b"failed",
        }
    }
}

impl fmt::Display for PushFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Rejected(err) => write!(f, "rejected: {err:#}"),
            Self::Failed(err) => write!(f, "failed: {err:#}"),
        }
    }
}

/// What landed on disk.
#[derive(Debug)]
pub struct Received {
    pub destination: PathBuf,
    pub file_count: usize,
    pub total_bytes: u64,
    pub elapsed: Duration,
}

pub async fn receive(
    conn: &Connection,
    announce: &Announce,
    download_dir: &Path,
    temp_root: &Path,
) -> Result<Received, PushFailure> {
    let started = Instant::now();

    // Refusals first — none of this touches the disk or the network.
    if announce.format != BlobFormat::HashSeq {
        return Err(PushFailure::Rejected(anyhow!(
            "a push must be a collection, but the announce says {:?}",
            announce.format
        )));
    }
    safe_component(&announce.name)
        .with_context(|| format!("the announced root name {:?} is unsafe", announce.name))
        .map_err(PushFailure::Rejected)?;

    let temp = TempStore::create(recv_store_path(temp_root, &announce.hash))
        .await
        .map_err(PushFailure::Failed)?;

    fetch(conn, temp.store(), announce)
        .await
        .map_err(PushFailure::Failed)?;

    let collection = Collection::load(announce.hash, temp.store())
        .await
        .map_err(|err| PushFailure::Failed(anyhow!("failed to read the collection: {err}")))?;

    // Every name is checked before any of them is exported, so a hostile name at
    // position 9 cannot leave 8 files behind.
    let mut targets = Vec::with_capacity(collection.len());
    for (name, hash) in collection.iter() {
        let relative = safe_relative_path(name).map_err(PushFailure::Rejected)?;
        targets.push((relative, *hash));
    }
    if !announce.is_dir && targets.len() != 1 {
        return Err(PushFailure::Rejected(anyhow!(
            "a single-file push must carry exactly one entry, but the collection has {}",
            targets.len()
        )));
    }

    let (destination, total_bytes) = export_all(temp.store(), announce, &targets, download_dir)
        .await
        .map_err(PushFailure::Failed)?;
    let elapsed = started.elapsed();

    temp.shutdown().await.map_err(PushFailure::Failed)?;

    Ok(Received {
        destination,
        file_count: targets.len(),
        total_bytes,
        elapsed,
    })
}

async fn export_all(
    store: &Store,
    announce: &Announce,
    targets: &[(PathBuf, Hash)],
    download_dir: &Path,
) -> Result<(PathBuf, u64)> {
    fs::create_dir_all(download_dir)
        .with_context(|| format!("failed to create {}", download_dir.display()))?;

    let destination = unique_destination(download_dir, &announce.name, announce.is_dir);
    let mut total_bytes = 0;

    if announce.is_dir {
        fs::create_dir_all(&destination)
            .with_context(|| format!("failed to create {}", destination.display()))?;
        for (relative, hash) in targets {
            total_bytes += export(store, *hash, &destination.join(relative)).await?;
        }
    } else {
        // The name comes from the announce rather than from the entry, so a lone
        // `notes.txt` lands as `downloads/notes.txt` and not in a directory of
        // its own.
        let (_, hash) = &targets[0];
        total_bytes += export(store, *hash, &destination).await?;
    }

    Ok((destination, total_bytes))
}

async fn export(store: &Store, hash: Hash, target: &Path) -> Result<u64> {
    // iroh-blobs refuses a relative target, and creates the parent itself.
    let target = absolute(target)?;
    store
        .export_with_opts(ExportOptions {
            hash,
            mode: ExportMode::Copy,
            target: target.clone(),
        })
        .finish()
        .await
        .with_context(|| format!("failed to write {}", target.display()))
}

async fn fetch(conn: &Connection, store: &Store, announce: &Announce) -> Result<()> {
    let content = HashAndFormat::hash_seq(announce.hash);
    let mut stream = store.remote().fetch(conn.clone(), content).stream();
    let mut last_printed = Instant::now();

    while let Some(item) = stream.next().await {
        match item {
            GetProgressItem::Progress(done) => {
                if last_printed.elapsed() >= PROGRESS_INTERVAL {
                    println!(
                        "receiving {}: {}",
                        announce.name,
                        progress(done, announce.total_size)
                    );
                    last_printed = Instant::now();
                }
            }
            GetProgressItem::Done(_stats) => {}
            GetProgressItem::Error(err) => return Err(anyhow!("the transfer failed: {err}")),
        }
    }
    Ok(())
}

#[cfg(test)]
#[path = "tests/download.rs"]
mod tests;
