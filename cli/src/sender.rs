use std::{env, path::Path};

use anyhow::{Context, Result, anyhow, bail};
use iroh::{
    Endpoint, EndpointAddr,
    endpoint::{Connection, ConnectionError, presets},
};
use iroh_blobs::{
    BlobFormat,
    api::{
        TempTag,
        blobs::{AddPathOptions, ImportMode},
    },
    format::collection::Collection,
};
use iroh_ping::Ping;
use iroh_tickets::endpoint::EndpointTicket;

use crate::{
    config::Config,
    paths::absolute,
    push::{self, Announce, CLOSE_FAILED, CLOSE_OK, CLOSE_REJECTED},
    store::{TempStore, send_store_path},
    tree::{self, Walk},
    upload,
};

/// The environment variable the sender takes the receiver's ticket from.
pub const TICKET_ENV: &str = "IROH_TICKET";

/// Reads the receiver's ticket from [`TICKET_ENV`].
pub fn ticket_from_env() -> Result<EndpointTicket> {
    let raw = env::var(TICKET_ENV)
        .map_err(|_| anyhow!("{TICKET_ENV} must be set to the ticket printed by the receiver"))?;
    parse_ticket(&raw)
}

/// A ticket is usually pasted or piped in, so surrounding whitespace and the
/// trailing newline of the receiver's output are not a malformed ticket.
fn parse_ticket(raw: &str) -> Result<EndpointTicket> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(anyhow!("{TICKET_ENV} is empty"));
    }
    trimmed
        .parse()
        .map_err(|e| anyhow!("failed to parse the ticket in {TICKET_ENV}: {}", e))
}

pub async fn run_sender(config: &Config, ticket: EndpointTicket, path: &Path) -> Result<()> {
    // Everything that can fail locally fails before anything goes on the wire.
    let walk = tree::walk(path)?;
    if walk.entries.is_empty() {
        bail!("nothing to send: {} contains no files", path.display());
    }

    let temp = TempStore::create(send_store_path(Path::new("."))).await?;
    // The tags keep the imported blobs alive until serving is done.
    let (announce, _tags) = import(&temp, &walk).await?;

    let endpoint = Endpoint::builder(presets::N0)
        .relay_mode(config.relay_mode.clone())
        .bind()
        .await?;

    let result = serve(&endpoint, ticket.endpoint_addr().clone(), &temp, &announce).await;

    endpoint.close().await;
    temp.shutdown().await?;
    result
}

/// Imports every file and builds the collection the receiver will fetch.
async fn import(temp: &TempStore, walk: &Walk) -> Result<(Announce, Vec<TempTag>)> {
    let mut tags = Vec::with_capacity(walk.entries.len() + 1);
    let mut links = Vec::with_capacity(walk.entries.len());

    for entry in &walk.entries {
        let tag = temp
            .store()
            .add_path_with_opts(AddPathOptions {
                // iroh-blobs refuses a relative import path.
                path: absolute(&entry.path)?,
                format: BlobFormat::Raw,
                // Reference the file where it is rather than copying every byte
                // into the temp store first.
                mode: ImportMode::TryReference,
            })
            .temp_tag()
            .await
            .with_context(|| format!("failed to read {}", entry.path.display()))?;
        links.push((entry.name.clone(), tag.hash()));
        tags.push(tag);
    }

    let collection: Collection = links.into_iter().collect();
    let root = collection
        .store(temp.store())
        .await
        .map_err(|err| anyhow!("failed to build the collection: {err}"))?;

    let announce = Announce {
        hash: root.hash(),
        format: BlobFormat::HashSeq,
        total_size: walk.total_size(),
        file_count: walk.entries.len() as u32,
        is_dir: walk.is_dir,
        name: walk.root.clone(),
    };
    tags.push(root);
    Ok((announce, tags))
}

/// Pings, announces, then serves on the connection it dialed.
async fn serve(
    endpoint: &Endpoint,
    addr: EndpointAddr,
    temp: &TempStore,
    announce: &Announce,
) -> Result<()> {
    // A cheap diagnostic on a self-hosted relay, and running it first means a
    // broken path shows up as a ping failure rather than a stalled transfer.
    let rtt = Ping::new().ping(endpoint, addr.clone()).await?;
    println!("ping took: {rtt:?} to complete");

    let conn = endpoint.connect(addr, push::ALPN).await?;
    let mut uni = conn.open_uni().await?;
    uni.write_all(&announce.encode()?).await?;
    uni.finish()?;

    println!(
        "sending {} file(s), {} bytes, as {:?}",
        announce.file_count, announce.total_size, announce.name
    );

    // iroh-blobs is a pull protocol, so the sender is the provider even though it
    // is the side that dialed. `tests/feasibility.rs` pins that down.
    upload::serve_blobs(temp.store(), &conn, announce).await?;

    report(&conn, announce).await
}

/// The receiver's close code is the only thing that tells a refusal apart from a
/// network failure.
async fn report(conn: &Connection, announce: &Announce) -> Result<()> {
    let ConnectionError::ApplicationClosed(closed) = conn.closed().await else {
        bail!("the connection dropped before the receiver reported an outcome");
    };
    let reason = String::from_utf8_lossy(&closed.reason);

    match u32::try_from(u64::from(closed.error_code)) {
        Ok(CLOSE_OK) => {
            println!("the receiver stored {:?}", announce.name);
            Ok(())
        }
        Ok(CLOSE_REJECTED) => bail!("the receiver rejected the push ({reason})"),
        Ok(CLOSE_FAILED) => bail!("the receiver failed part way through ({reason})"),
        Ok(other) => bail!("the receiver closed with an unknown code {other} ({reason})"),
        Err(_) => bail!("the receiver closed with an out-of-range code ({reason})"),
    }
}

#[cfg(test)]
#[path = "tests/sender.rs"]
mod tests;
