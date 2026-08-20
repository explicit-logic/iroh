//! Throwaway probe for the one unverified assumption in the design: that
//! `iroh-blobs` will serve on a connection the provider *dialed*, and fetch on a
//! connection the getter *accepted*.

use std::{
    net::{Ipv4Addr, SocketAddr},
    path::PathBuf,
};

use anyhow::Result;
use iroh::{
    Endpoint, EndpointAddr, RelayMode,
    endpoint::{Connection, presets},
    protocol::{AcceptError, ProtocolHandler, Router},
};
use iroh_blobs::{BlobsProtocol, Hash, store::fs::FsStore};
use tokio::sync::mpsc;

const PROBE_ALPN: &[u8] = b"iroh-app/push/0";

#[derive(Debug, Clone)]
struct PullHandler {
    dir: PathBuf,
    tx: mpsc::Sender<Vec<u8>>,
}

impl ProtocolHandler for PullHandler {
    async fn accept(&self, conn: Connection) -> Result<(), AcceptError> {
        let mut uni = conn.accept_uni().await?;
        let raw = uni.read_to_end(32).await.map_err(AcceptError::from_err)?;
        let hash = Hash::from_bytes(<[u8; 32]>::try_from(raw.as_slice()).unwrap());

        let store = FsStore::load(&self.dir).await.unwrap();
        store.remote().fetch(conn.clone(), hash).await.unwrap();
        let bytes = store.get_bytes(hash).await.unwrap();
        store.shutdown().await.unwrap();

        self.tx.send(bytes.to_vec()).await.unwrap();
        conn.close(0u32.into(), b"done");
        Ok(())
    }
}

fn local_addr(endpoint: &Endpoint) -> EndpointAddr {
    let port = endpoint.bound_sockets()[0].port();
    EndpointAddr::new(endpoint.id()).with_ip_addr(SocketAddr::from((Ipv4Addr::LOCALHOST, port)))
}

#[tokio::test]
async fn provider_can_serve_on_a_dialed_connection() -> Result<()> {
    let dir = std::env::temp_dir().join(format!("iroh-probe-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir)?;

    let payload = b"hello from the dialing side".to_vec();

    // Receiver: accepts, then pulls.
    let (tx, mut rx) = mpsc::channel(1);
    let recv_ep = Endpoint::builder(presets::N0)
        .relay_mode(RelayMode::Disabled)
        .bind()
        .await?;
    let recv_addr = local_addr(&recv_ep);
    let router = Router::builder(recv_ep)
        .accept(
            PROBE_ALPN,
            PullHandler {
                dir: dir.join("recv"),
                tx,
            },
        )
        .spawn();

    // Sender: dials, then serves.
    let send_ep = Endpoint::builder(presets::N0)
        .relay_mode(RelayMode::Disabled)
        .bind()
        .await?;
    let store = FsStore::load(dir.join("send")).await?;
    let tag = store.add_slice(&payload).temp_tag().await?;

    let conn = send_ep.connect(recv_addr, PROBE_ALPN).await?;
    let mut uni = conn.open_uni().await?;
    uni.write_all(tag.hash().as_bytes()).await?;
    uni.finish()?;

    let blobs = BlobsProtocol::new(&store, None);
    blobs.accept(conn.clone()).await?;

    let got = rx.recv().await.expect("handler produced no bytes");
    assert_eq!(got, payload);

    store.shutdown().await?;
    router.shutdown().await?;
    send_ep.close().await;
    let _ = std::fs::remove_dir_all(&dir);
    Ok(())
}
