use std::fs;

use super::*;
use crate::test_support::TempDir;

#[test]
fn creates_the_key_file_when_it_is_missing() {
    let dir = TempDir::new("creates");
    let path = dir.path().join("secret.key");

    load_or_create_secret_key(&path).unwrap();

    assert!(path.exists());
}

#[test]
fn reuses_the_key_from_an_existing_file() {
    let dir = TempDir::new("reuses");
    let path = dir.path().join("secret.key");

    let first = load_or_create_secret_key(&path).unwrap();
    let second = load_or_create_secret_key(&path).unwrap();

    assert_eq!(first.public(), second.public());
}

#[test]
fn creates_missing_parent_directories() {
    let dir = TempDir::new("parents");
    let path = dir.path().join("keys").join("receiver.key");

    load_or_create_secret_key(&path).unwrap();

    assert!(path.exists());
}

#[test]
fn tolerates_surrounding_whitespace_in_the_key_file() {
    let dir = TempDir::new("whitespace");
    let path = dir.path().join("secret.key");
    let key = load_or_create_secret_key(&path).unwrap();
    let stored = fs::read_to_string(&path).unwrap();
    fs::write(&path, format!("\n  {}  \n", stored.trim())).unwrap();

    let reloaded = load_or_create_secret_key(&path).unwrap();

    assert_eq!(key.public(), reloaded.public());
}

#[test]
fn rejects_a_corrupt_key_file() {
    let dir = TempDir::new("corrupt");
    let path = dir.path().join("secret.key");
    fs::write(&path, "not a key").unwrap();

    assert!(load_or_create_secret_key(&path).is_err());
}

#[cfg(unix)]
#[test]
fn writes_a_key_file_only_its_owner_can_read() {
    use std::os::unix::fs::PermissionsExt;

    let dir = TempDir::new("perms");
    let path = dir.path().join("secret.key");

    load_or_create_secret_key(&path).unwrap();

    let mode = fs::metadata(&path).unwrap().permissions().mode();
    assert_eq!(mode & 0o777, 0o600);
}
