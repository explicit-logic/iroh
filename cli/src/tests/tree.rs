use std::fs;

use super::*;
use crate::test_support::TempDir;

fn names(walk: &Walk) -> Vec<&str> {
    walk.entries.iter().map(|e| e.name.as_str()).collect()
}

#[test]
fn walks_a_single_file() {
    let dir = TempDir::new("walk-file");
    let path = dir.path().join("notes.txt");
    fs::write(&path, "hello").unwrap();

    let walk = walk(&path).unwrap();

    assert_eq!(walk.root, "notes.txt");
    assert!(!walk.is_dir);
    assert_eq!(names(&walk), ["notes.txt"]);
    assert_eq!(walk.entries[0].path, path);
    assert_eq!(walk.total_size(), 5);
}

/// The collection the receiver rebuilds is ordered, so the same tree has to
/// produce the same order every time.
#[test]
fn walks_nested_directories_in_sorted_order() {
    let dir = TempDir::new("walk-nested");
    let root = dir.path().join("photos");
    fs::create_dir_all(root.join("b").join("deep")).unwrap();
    fs::create_dir_all(root.join("a")).unwrap();
    fs::write(root.join("z.txt"), "z").unwrap();
    fs::write(root.join("a").join("one.txt"), "11").unwrap();
    fs::write(root.join("b").join("deep").join("two.txt"), "222").unwrap();

    let walk = walk(&root).unwrap();

    assert_eq!(walk.root, "photos");
    assert!(walk.is_dir);
    assert_eq!(names(&walk), ["a/one.txt", "b/deep/two.txt", "z.txt"]);
    assert_eq!(walk.total_size(), 6);
}

/// Following a link inside the folder would let it pull in a file from anywhere
/// on the sender's disk, which is not what "send this folder" means.
#[cfg(unix)]
#[test]
fn skips_symlinks() {
    let dir = TempDir::new("walk-symlink");
    let root = dir.path().join("photos");
    fs::create_dir(&root).unwrap();
    fs::write(root.join("real.txt"), "real").unwrap();
    fs::write(dir.path().join("outside.txt"), "outside").unwrap();
    std::os::unix::fs::symlink(dir.path().join("outside.txt"), root.join("link.txt")).unwrap();

    let walk = walk(&root).unwrap();

    assert_eq!(names(&walk), ["real.txt"]);
}

/// The walk reports emptiness rather than erroring on it, so the sender can own
/// the wording of the error and the receiver never hears about it.
#[test]
fn yields_no_entries_for_an_empty_directory() {
    let dir = TempDir::new("walk-empty");
    let root = dir.path().join("empty");
    fs::create_dir(&root).unwrap();

    let walk = walk(&root).unwrap();

    assert!(walk.entries.is_empty());
    assert!(walk.is_dir);
    assert_eq!(walk.root, "empty");
}

#[test]
fn errors_on_a_missing_path() {
    let dir = TempDir::new("walk-missing");

    assert!(walk(&dir.path().join("nope")).is_err());
}
