use std::{
    future::Future,
    io,
    net::{Ipv4Addr, Ipv6Addr},
    path::{Path, PathBuf},
};

use anyhow::Result;
use iroh::{
    Endpoint,
    endpoint::{BindOpts, Connection, presets},
    protocol::{AcceptError, ProtocolHandler, Router},
};
use iroh_ping::Ping;
use iroh_tickets::endpoint::EndpointTicket;

use crate::{
    config::Config,
    download::{self, DOWNLOAD_DIR, PushFailure},
    key::{SECRET_KEY_PATH, load_or_create_secret_key},
    progress::elapsed,
    push::{self, Announce, CLOSE_OK, MAX_ANNOUNCE_LEN},
};

pub async fn run_receiver(config: &Config) -> Result<()> {
    let endpoint = build_endpoint(config, Path::new(SECRET_KEY_PATH)).await?;
    endpoint.online().await;
    println!("{}", EndpointTicket::new(endpoint.addr()));

    serve_until(
        endpoint,
        PathBuf::from(DOWNLOAD_DIR),
        tokio::signal::ctrl_c(),
    )
    .await
}

async fn build_endpoint(config: &Config, secret_key_path: &Path) -> Result<Endpoint> {
    let secret_key = load_or_create_secret_key(secret_key_path)?;
    let endpoint = Endpoint::builder(presets::N0)
        .secret_key(secret_key)
        .relay_mode(config.relay_mode.clone())
        .clear_ip_transports()
        .bind_addr_with_opts(
            (Ipv4Addr::UNSPECIFIED, config.bind_port),
            BindOpts::default(),
        )?
        .bind_addr_with_opts(
            (Ipv6Addr::UNSPECIFIED, config.bind_port),
            BindOpts::default().set_is_required(false),
        )?
        .bind()
        .await?;
    Ok(endpoint)
}

async fn serve_until(
    endpoint: Endpoint,
    download_dir: PathBuf,
    shutdown: impl Future<Output = io::Result<()>>,
) -> Result<()> {
    let router = Router::builder(endpoint)
        .accept(iroh_ping::ALPN, Ping::new())
        // The temp store sits in the working directory rather than under
        // `download_dir`, which must stay untouched until a transfer succeeds.
        .accept(
            push::ALPN,
            PushHandler::new(download_dir, PathBuf::from(".")),
        )
        .spawn();

    shutdown.await?;
    router.shutdown().await?;
    Ok(())
}

#[derive(Debug, Clone)]
pub struct PushHandler {
    download_dir: PathBuf,
    temp_root: PathBuf,
}

impl PushHandler {
    pub fn new(download_dir: PathBuf, temp_root: PathBuf) -> Self {
        Self {
            download_dir,
            temp_root,
        }
    }

    async fn read_announce(conn: &Connection) -> Result<Announce> {
        let mut uni = conn.accept_uni().await?;
        // Capped, so a peer cannot make the receiver buffer without bound.
        let raw = uni.read_to_end(MAX_ANNOUNCE_LEN).await?;
        Announce::decode(&raw)
    }
}

impl ProtocolHandler for PushHandler {
    async fn accept(&self, conn: Connection) -> Result<(), AcceptError> {
        let peer = conn.remote_id();

        let outcome = match Self::read_announce(&conn).await {
            Ok(announce) => {
                println!(
                    "accepting {} file(s), {} bytes, as {:?} from {peer}",
                    announce.file_count, announce.total_size, announce.name
                );
                download::receive(&conn, &announce, &self.download_dir, &self.temp_root).await
            }
            Err(err) => Err(PushFailure::Rejected(err)),
        };

        match outcome {
            Ok(received) => {
                conn.close(CLOSE_OK.into(), b"ok");
                println!(
                    "received {} file(s), {} bytes, in {}, into {}",
                    received.file_count,
                    received.total_bytes,
                    elapsed(received.elapsed),
                    received.destination.display()
                );
            }
            Err(failure) => {
                conn.close(failure.close_code().into(), failure.reason());
                eprintln!("push from {peer} {failure}");
            }
        }

        Ok(())
    }
}

#[cfg(test)]
#[path = "tests/receiver.rs"]
mod tests;
