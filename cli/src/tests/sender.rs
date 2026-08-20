use std::{fs, path::PathBuf};

use iroh::{EndpointAddr, RelayMode, SecretKey, protocol::Router};

use super::*;
use crate::{
    receiver::PushHandler,
    test_support::{TempDir, local_addr, test_endpoint},
};

fn ticket() -> EndpointTicket {
    EndpointTicket::new(EndpointAddr::new(SecretKey::generate().public()))
}

#[test]
fn parses_a_ticket() {
    let expected = ticket();

    let parsed = parse_ticket(&expected.to_string()).unwrap();

    assert_eq!(parsed.endpoint_addr(), expected.endpoint_addr());
}

/// `IROH_TICKET="$(app receiver)"` and a copy-paste both carry whitespace the
/// ticket encoding itself never contains.
#[test]
fn ignores_surrounding_whitespace() {
    let expected = ticket();

    let parsed = parse_ticket(&format!("  {expected}\n")).unwrap();

    assert_eq!(parsed.endpoint_addr(), expected.endpoint_addr());
}

/// An exported-but-unset variable reaches us as an empty string rather than as a
/// missing one, and "failed to parse" would be a confusing way to report it.
#[test]
fn rejects_an_empty_ticket() {
    let err = parse_ticket("   \n").unwrap_err().to_string();

    assert!(err.contains("is empty"), "unexpected error: {err}");
}

#[test]
fn rejects_a_malformed_ticket() {
    let err = parse_ticket("not-a-ticket").unwrap_err().to_string();

    assert!(err.contains("failed to parse"), "unexpected error: {err}");
}

/// Runs a real receiver in-process, pushes `path` to it, and returns the download
/// directory.
async fn push_to_receiver(dir: &TempDir, label: &str, path: &Path) -> PathBuf {
    let download_dir = dir.path().join(format!("downloads-{label}"));
    let temp_root = dir.path().join(format!("temp-{label}"));
    fs::create_dir_all(&temp_root).unwrap();

    let recv_ep = test_endpoint().await;
    let ticket = EndpointTicket::new(local_addr(&recv_ep));
    let router = Router::builder(recv_ep)
        // `run_sender` pings before it transfers, so the test receiver has to
        // answer both ALPNs exactly as `serve_until` does.
        .accept(iroh_ping::ALPN, Ping::new())
        .accept(
            push::ALPN,
            PushHandler::new(download_dir.clone(), temp_root),
        )
        .spawn();

    run_sender(&test_config(), ticket, path).await.unwrap();

    router.shutdown().await.unwrap();
    download_dir
}

fn test_config() -> Config {
    Config {
        relay_mode: RelayMode::Disabled,
        bind_port: 0,
    }
}

/// A lone file lands as `downloads/notes.txt`, not `downloads/notes.txt/notes.txt`.
#[tokio::test]
async fn sends_a_single_file() {
    let dir = TempDir::new("send-file");
    let source = dir.path().join("notes.txt");
    fs::write(&source, "the quick brown fox").unwrap();

    let downloads = push_to_receiver(&dir, "file", &source).await;

    assert_eq!(
        fs::read_to_string(downloads.join("notes.txt")).unwrap(),
        "the quick brown fox"
    );
    assert!(!downloads.join("notes.txt").is_dir());
}

#[tokio::test]
async fn sends_a_folder_with_its_tree_intact() {
    let dir = TempDir::new("send-folder");
    let source = dir.path().join("photos");
    fs::create_dir_all(source.join("holiday").join("2026")).unwrap();
    fs::write(source.join("cover.txt"), "cover").unwrap();
    fs::write(source.join("holiday").join("beach.txt"), "beach").unwrap();
    fs::write(
        source.join("holiday").join("2026").join("nye.txt"),
        "fireworks",
    )
    .unwrap();

    let downloads = push_to_receiver(&dir, "folder", &source).await;

    let root = downloads.join("photos");
    assert_eq!(fs::read_to_string(root.join("cover.txt")).unwrap(), "cover");
    assert_eq!(
        fs::read_to_string(root.join("holiday").join("beach.txt")).unwrap(),
        "beach"
    );
    assert_eq!(
        fs::read_to_string(root.join("holiday").join("2026").join("nye.txt")).unwrap(),
        "fireworks"
    );
}

/// The second push must neither merge into the first nor overwrite it.
#[tokio::test]
async fn a_second_push_of_the_same_name_lands_beside_the_first() {
    let dir = TempDir::new("send-twice");
    let download_dir = dir.path().join("downloads");
    let temp_root = dir.path().join("temp");
    fs::create_dir_all(&temp_root).unwrap();

    let file = dir.path().join("notes.txt");
    fs::write(&file, "first").unwrap();
    let folder = dir.path().join("photos");
    fs::create_dir(&folder).unwrap();
    fs::write(folder.join("a.txt"), "a").unwrap();

    let recv_ep = test_endpoint().await;
    let ticket = EndpointTicket::new(local_addr(&recv_ep));
    let router = Router::builder(recv_ep)
        // `run_sender` pings before it transfers, so the test receiver has to
        // answer both ALPNs exactly as `serve_until` does.
        .accept(iroh_ping::ALPN, Ping::new())
        .accept(
            push::ALPN,
            PushHandler::new(download_dir.clone(), temp_root),
        )
        .spawn();
    let config = test_config();

    for _ in 0..2 {
        run_sender(&config, ticket.clone(), &file).await.unwrap();
        run_sender(&config, ticket.clone(), &folder).await.unwrap();
    }

    router.shutdown().await.unwrap();

    assert_eq!(
        fs::read_to_string(download_dir.join("notes.txt")).unwrap(),
        "first"
    );
    assert_eq!(
        fs::read_to_string(download_dir.join("notes-1.txt")).unwrap(),
        "first"
    );
    assert!(download_dir.join("photos").join("a.txt").is_file());
    assert!(download_dir.join("photos-1").join("a.txt").is_file());
}

/// Nothing goes on the wire for an empty folder, so the sender has to say so.
#[tokio::test]
async fn refuses_to_send_an_empty_folder() {
    let dir = TempDir::new("send-empty");
    let source = dir.path().join("empty");
    fs::create_dir(&source).unwrap();

    let err = run_sender(&test_config(), ticket(), &source)
        .await
        .unwrap_err()
        .to_string();

    assert!(err.contains("nothing to send"), "unexpected error: {err}");
}
