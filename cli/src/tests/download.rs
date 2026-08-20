use std::fs;

use iroh::protocol::{AcceptError, ProtocolHandler, Router};
use iroh_blobs::BlobsProtocol;

use super::*;
use crate::{
    push,
    store::send_store_path,
    test_support::{TempDir, local_addr, test_endpoint},
};

/// Builds a collection out of raw `(name, contents)` pairs and returns the store
/// holding it.
///
/// This stands in for the sender so a test can put a name on the wire that the
/// real sender would never produce.
async fn collection_store(
    dir: &TempDir,
    entries: Vec<(String, Vec<u8>)>,
) -> (TempStore, Hash, u64) {
    let store = TempStore::create(send_store_path(dir.path()))
        .await
        .unwrap();
    let mut links = Vec::new();
    let mut total = 0;
    for (name, contents) in entries {
        total += contents.len() as u64;
        let tag = store.store().add_bytes(contents).temp_tag().await.unwrap();
        links.push((name, tag.hash()));
    }
    let collection: Collection = links.into_iter().collect();
    let root = collection.store(store.store()).await.unwrap();
    let hash = root.hash();
    (store, hash, total)
}

/// A hostile name has to be caught before anything is written, so this drives
/// `receive` directly with a collection an honest sender would never build.
#[tokio::test]
async fn refuses_a_collection_entry_that_escapes_the_download_dir() {
    let dir = TempDir::new("download-traversal");
    let download_dir = dir.path().join("downloads");
    let temp_root = dir.path().join("temp");
    fs::create_dir_all(&temp_root).unwrap();

    let (serving, hash, total) = collection_store(
        &dir,
        vec![
            ("safe.txt".to_string(), b"safe".to_vec()),
            ("../escape.txt".to_string(), b"escaped".to_vec()),
        ],
    )
    .await;

    let announce = Announce {
        hash,
        format: BlobFormat::HashSeq,
        total_size: total,
        file_count: 2,
        is_dir: true,
        name: "tree".to_string(),
    };

    let provider_ep = test_endpoint().await;
    let provider_addr = local_addr(&provider_ep);
    let getter_ep = test_endpoint().await;
    let router = Router::builder(provider_ep)
        .accept(push::ALPN, BlobsProtocol::new(serving.store(), None))
        .spawn();
    let conn = getter_ep.connect(provider_addr, push::ALPN).await.unwrap();

    let outcome = receive(&conn, &announce, &download_dir, &temp_root).await;

    let Err(PushFailure::Rejected(err)) = outcome else {
        panic!("expected a rejection, got {outcome:?}");
    };
    assert!(
        format!("{err:#}").contains("escape"),
        "the offending name should be reported: {err:#}"
    );
    assert!(
        !dir.path().join("escape.txt").exists(),
        "a file escaped the download directory"
    );
    assert!(
        !download_dir.join("tree").exists(),
        "nothing should be exported when a name is refused"
    );

    conn.close(0u32.into(), b"done");
    // The router owns a clone of the store and shuts it down, so `serving` is
    // only dropped here — shutting it down again would fail.
    router.shutdown().await.unwrap();
    getter_ep.close().await;
    drop(serving);
}

/// A raw blob is refused outright: there is one fetch path and it expects a
/// collection.
#[tokio::test]
async fn refuses_an_announce_that_is_not_a_collection() {
    let dir = TempDir::new("download-raw");
    let download_dir = dir.path().join("downloads");
    let temp_root = dir.path().join("temp");
    fs::create_dir_all(&temp_root).unwrap();

    let announce = Announce {
        hash: Hash::from_bytes([1; 32]),
        format: BlobFormat::Raw,
        total_size: 1,
        file_count: 1,
        is_dir: false,
        name: "notes.txt".to_string(),
    };

    let idle_ep = test_endpoint().await;
    let idle_addr = local_addr(&idle_ep);
    let getter_ep = test_endpoint().await;
    let router = Router::builder(idle_ep).accept(push::ALPN, Idle).spawn();
    let conn = getter_ep.connect(idle_addr, push::ALPN).await.unwrap();

    let outcome = receive(&conn, &announce, &download_dir, &temp_root).await;

    assert!(matches!(outcome, Err(PushFailure::Rejected(_))));
    // The refusal happens before a temp store is opened, so nothing is on disk.
    assert!(!download_dir.exists());

    conn.close(0u32.into(), b"done");
    router.shutdown().await.unwrap();
    getter_ep.close().await;
}

/// Holds the connection open and does nothing, so the dial above has something
/// to reach.
#[derive(Debug, Clone)]
struct Idle;

impl ProtocolHandler for Idle {
    async fn accept(&self, conn: Connection) -> Result<(), AcceptError> {
        conn.closed().await;
        Ok(())
    }
}
