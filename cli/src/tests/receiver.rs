use std::fs;

use iroh::{RelayMode, endpoint::ConnectionError};
use iroh_blobs::{BlobFormat, BlobsProtocol, format::collection::Collection};
use iroh_tickets::endpoint::EndpointTicket;

use super::*;
use crate::{
    push::CLOSE_REJECTED,
    store::{TempStore, send_store_path},
    test_support::{TempDir, local_addr, test_endpoint},
};

/// Tests that pin a port each use their own: `cargo test` runs them on parallel
/// threads, and two endpoints cannot hold the same UDP port at once.
fn config(bind_port: u16) -> Config {
    Config {
        relay_mode: RelayMode::Disabled,
        bind_port,
    }
}

/// Dropping the router leaves the endpoint open — iroh logs that as an
/// ungraceful abort — and its spawned task keeps a clone of the endpoint, which
/// holds the pinned port so the next start cannot reclaim it.
#[tokio::test]
async fn releases_the_port_when_it_stops() {
    let dir = TempDir::new("shutdown");
    let key_path = dir.path().join("secret.key");
    let config = config(41903);
    let endpoint = build_endpoint(&config, &key_path).await.unwrap();

    serve_until(
        endpoint,
        dir.path().join("downloads"),
        std::future::ready(Ok(())),
    )
    .await
    .unwrap();

    // Rebinding the same port only succeeds if the endpoint was really closed.
    let again = build_endpoint(&config, &key_path).await.unwrap();
    again.close().await;
}

/// The endpoint id is what a ticket carries, so a receiver that regenerates its
/// key every start hands out a ticket that is useless after a restart.
#[tokio::test]
async fn keeps_the_endpoint_id_across_restarts() {
    let dir = TempDir::new("endpoint-id");
    let key_path = dir.path().join("secret.key");
    // An ephemeral port: this test is about the key alone, and the two endpoints
    // below are alive at the same time.
    let config = config(0);

    let first = build_endpoint(&config, &key_path).await.unwrap();
    let second = build_endpoint(&config, &key_path).await.unwrap();

    assert_eq!(first.id(), second.id());
    first.close().await;
    second.close().await;
}

/// A ticket also carries the direct addresses, so an ephemeral port would change
/// the printed ticket on every start even with a stable id.
#[tokio::test]
async fn prints_an_identical_ticket_across_restarts() {
    let dir = TempDir::new("identical-ticket");
    let key_path = dir.path().join("secret.key");
    let config = config(41902);

    let first = build_endpoint(&config, &key_path).await.unwrap();
    let first_ticket = EndpointTicket::new(first.addr()).to_string();
    first.close().await;
    // The UDP socket is released only once every clone of the endpoint is
    // dropped, so the restart below cannot bind the port until this one is gone.
    drop(first);

    let second = build_endpoint(&config, &key_path).await.unwrap();
    let second_ticket = EndpointTicket::new(second.addr()).to_string();
    second.close().await;

    assert_eq!(first_ticket, second_ticket);
}

/// A malformed announce must cost the sender its own push and nothing more: a
/// single bad push stopping the receiver is the failure this design most needs
/// to rule out.
#[tokio::test]
async fn survives_a_malformed_announce_and_serves_the_next_push() {
    let dir = TempDir::new("receiver-malformed");
    let download_dir = dir.path().join("downloads");
    let temp_root = dir.path().join("temp");
    fs::create_dir_all(&temp_root).unwrap();

    let recv_ep = test_endpoint().await;
    let recv_addr = local_addr(&recv_ep);
    let router = Router::builder(recv_ep)
        .accept(
            push::ALPN,
            PushHandler::new(download_dir.clone(), temp_root),
        )
        .spawn();

    // A push whose announce is nonsense.
    {
        let send_ep = test_endpoint().await;
        let conn = send_ep
            .connect(recv_addr.clone(), push::ALPN)
            .await
            .unwrap();
        let mut uni = conn.open_uni().await.unwrap();
        uni.write_all(b"not an announce").await.unwrap();
        uni.finish().unwrap();

        let ConnectionError::ApplicationClosed(closed) = conn.closed().await else {
            panic!("expected the receiver to close with a code");
        };
        assert_eq!(u64::from(closed.error_code), u64::from(CLOSE_REJECTED));
        send_ep.close().await;
    }
    assert!(
        !download_dir.exists(),
        "a rejected push must not create the download directory"
    );

    // A good push immediately afterwards.
    {
        let send_ep = test_endpoint().await;
        let serving = TempStore::create(send_store_path(dir.path()))
            .await
            .unwrap();
        let tag = serving
            .store()
            .add_bytes(b"hello".to_vec())
            .temp_tag()
            .await
            .unwrap();
        let collection: Collection = vec![("notes.txt".to_string(), tag.hash())]
            .into_iter()
            .collect();
        let root = collection.store(serving.store()).await.unwrap();

        let announce = Announce {
            hash: root.hash(),
            format: BlobFormat::HashSeq,
            total_size: 5,
            file_count: 1,
            is_dir: false,
            name: "notes.txt".to_string(),
        };

        let conn = send_ep.connect(recv_addr, push::ALPN).await.unwrap();
        let mut uni = conn.open_uni().await.unwrap();
        uni.write_all(&announce.encode().unwrap()).await.unwrap();
        uni.finish().unwrap();

        BlobsProtocol::new(serving.store(), None)
            .accept(conn.clone())
            .await
            .unwrap();

        let ConnectionError::ApplicationClosed(closed) = conn.closed().await else {
            panic!("expected the receiver to close with a code");
        };
        assert_eq!(u64::from(closed.error_code), u64::from(CLOSE_OK));

        send_ep.close().await;
        serving.shutdown().await.unwrap();
    }

    assert_eq!(
        fs::read_to_string(download_dir.join("notes.txt")).unwrap(),
        "hello"
    );
    router.shutdown().await.unwrap();
}
