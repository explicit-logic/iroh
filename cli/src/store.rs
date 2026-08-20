use std::{
    fs,
    path::{Path, PathBuf},
    process,
    time::{SystemTime, UNIX_EPOCH},
};

use anyhow::{Context, Result};
use iroh_blobs::{Hash, api::Store, store::fs::FsStore};

/// A blob store that exists for exactly one transfer.
///
/// Both sides build one: the sender to hash and serve from, the receiver to land
/// verified bytes in before exporting them. Neither should outlive the transfer,
/// so the directory goes away on drop and the error and panic paths clean up too.
#[derive(Debug)]
pub struct TempStore {
    /// `None` only after [`TempStore::shutdown`], which consumes `self`.
    store: Option<FsStore>,
    path: PathBuf,
}

impl TempStore {
    pub async fn create(path: PathBuf) -> Result<Self> {
        fs::create_dir_all(&path)
            .with_context(|| format!("failed to create a temp store at {}", path.display()))?;
        let store = FsStore::load(&path)
            .await
            .with_context(|| format!("failed to open a temp store at {}", path.display()))?;
        Ok(Self {
            store: Some(store),
            path,
        })
    }

    pub fn store(&self) -> &Store {
        self.store
            .as_ref()
            .expect("the store is taken only by shutdown, which consumes self")
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Flushes and closes the store, then lets `Drop` remove it.
    ///
    /// `Drop` cannot await, so this is the only path that stops the store's own
    /// task in an orderly way.
    pub async fn shutdown(mut self) -> Result<()> {
        if let Some(store) = self.store.take() {
            store.shutdown().await.with_context(|| {
                format!("failed to close the temp store at {}", self.path.display())
            })?;
        }
        Ok(())
    }
}

impl Drop for TempStore {
    fn drop(&mut self) {
        // Nothing useful to do with a failure here: the process is either on its
        // way out or already reporting the error that got us here.
        let _ = fs::remove_dir_all(&self.path);
    }
}

/// A temp store path for one send.
///
/// The pid and the clock together keep two senders started in the same directory
/// at the same moment apart.
pub fn send_store_path(parent: &Path) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|since| since.as_nanos())
        .unwrap_or_default();
    parent.join(format!(".iroh-send-{}-{nanos}", process::id()))
}

/// A temp store path for one receive, keyed by the hash being fetched.
pub fn recv_store_path(parent: &Path, hash: &Hash) -> PathBuf {
    parent.join(format!(".iroh-recv-{}", hash.to_hex()))
}

#[cfg(test)]
#[path = "tests/store.rs"]
mod tests;
