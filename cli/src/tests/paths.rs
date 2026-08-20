use std::fs;

use super::*;
use crate::test_support::TempDir;

#[test]
fn accepts_an_ordinary_name() {
    assert!(safe_component("notes.txt").is_ok());
    assert!(safe_component("a name with spaces").is_ok());
    assert!(safe_component(".bashrc").is_ok());
}

#[test]
fn accepts_a_nested_relative_path() {
    assert_eq!(
        safe_relative_path("a/b/c.txt").unwrap(),
        Path::new("a").join("b").join("c.txt")
    );
}

/// The sender picks every name the receiver writes, so each of these is a real
/// attempt someone could make with a hand-rolled sender.
#[test]
fn rejects_traversal_and_absolute_names() {
    for name in [
        "..",
        ".",
        "",
        "/etc/passwd",
        "a/../../b",
        "../escape",
        "a//b",
    ] {
        assert!(
            safe_relative_path(name).is_err(),
            "{name:?} should have been rejected"
        );
    }
}

/// A backslash is a separator on Windows and a colon introduces a drive letter or
/// an NTFS alternate data stream, so neither may reach a join.
#[test]
fn rejects_windows_separators_and_drive_letters() {
    for name in ["a\\b", "..\\escape", "C:", "C:\\Windows", "file.txt:stream"] {
        assert!(
            safe_relative_path(name).is_err(),
            "{name:?} should have been rejected"
        );
    }
}

#[test]
fn rejects_a_nul_byte() {
    assert!(safe_relative_path("a\0b").is_err());
}

/// A root name is one component, so a nested one must not slip through the check
/// the announce uses.
#[test]
fn rejects_a_nested_root_name() {
    assert!(safe_component("a/b").is_err());
}

#[test]
fn uniquifies_a_directory_with_a_plain_suffix() {
    let dir = TempDir::new("uniquify-dir");

    assert_eq!(
        unique_destination(dir.path(), "photos", true),
        dir.path().join("photos")
    );
    fs::create_dir(dir.path().join("photos")).unwrap();
    assert_eq!(
        unique_destination(dir.path(), "photos", true),
        dir.path().join("photos-1")
    );
    fs::create_dir(dir.path().join("photos-1")).unwrap();
    assert_eq!(
        unique_destination(dir.path(), "photos", true),
        dir.path().join("photos-2")
    );
}

/// `notes.txt-1` would stop being a text file, so the suffix goes before the
/// extension.
#[test]
fn uniquifies_a_file_before_its_extension() {
    let dir = TempDir::new("uniquify-file");
    fs::write(dir.path().join("notes.txt"), "first").unwrap();

    assert_eq!(
        unique_destination(dir.path(), "notes.txt", false),
        dir.path().join("notes-1.txt")
    );
}

#[test]
fn uniquifies_a_file_with_no_extension() {
    let dir = TempDir::new("uniquify-bare");
    fs::write(dir.path().join("README"), "first").unwrap();

    assert_eq!(
        unique_destination(dir.path(), "README", false),
        dir.path().join("README-1")
    );
}

/// iroh-blobs refuses a relative import path or export target, and an export
/// target does not exist yet, so this cannot be `canonicalize`.
#[test]
fn makes_a_relative_path_absolute_without_touching_the_disk() {
    let made = absolute(Path::new("downloads/does-not-exist.txt")).unwrap();

    assert!(made.is_absolute());
    assert!(made.ends_with("downloads/does-not-exist.txt"));
}
