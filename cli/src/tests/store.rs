use super::*;
use crate::test_support::TempDir;

#[tokio::test]
async fn creates_the_directory_and_removes_it_on_drop() {
    let dir = TempDir::new("temp-store-drop");
    let path = send_store_path(dir.path());

    {
        let store = TempStore::create(path.clone()).await.unwrap();
        assert!(store.path().is_dir());
        // Enough of a smoke test to prove the store really opened.
        let _tag = store.store().add_slice(b"hello").temp_tag().await.unwrap();
    }

    assert!(!path.exists(), "the guard left {} behind", path.display());
}

/// The error and panic paths cannot await, so `Drop` has to be the thing that
/// removes the directory; `shutdown` is the orderly path on top of it.
#[tokio::test]
async fn shutdown_also_removes_the_directory() {
    let dir = TempDir::new("temp-store-shutdown");
    let path = send_store_path(dir.path());

    let store = TempStore::create(path.clone()).await.unwrap();
    store.shutdown().await.unwrap();

    assert!(!path.exists());
}

/// Two senders in the same working directory must not open the same store.
#[test]
fn send_paths_are_unique() {
    let dir = TempDir::new("temp-store-unique");

    assert_ne!(send_store_path(dir.path()), send_store_path(dir.path()));
}

#[test]
fn recv_paths_are_keyed_by_hash() {
    let dir = TempDir::new("temp-store-keyed");
    let hash = Hash::from_bytes([3; 32]);

    assert_eq!(
        recv_store_path(dir.path(), &hash),
        recv_store_path(dir.path(), &hash)
    );
    assert_ne!(
        recv_store_path(dir.path(), &hash),
        recv_store_path(dir.path(), &Hash::from_bytes([4; 32]))
    );
}
