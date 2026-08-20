use std::{
    fs,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, bail};

/// One file to send.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    /// The name the receiver exports under. For a directory push this is the
    /// path relative to that directory, joined with `/` on every platform,
    /// because it travels over the wire.
    pub name: String,
    pub path: PathBuf,
    pub size: u64,
}

/// A path resolved into the files it holds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Walk {
    /// The pushed file's or directory's own name.
    pub root: String,
    pub is_dir: bool,
    /// Sorted by name, so the same tree always yields the same collection.
    pub entries: Vec<Entry>,
}

impl Walk {
    pub fn total_size(&self) -> u64 {
        self.entries.iter().map(|entry| entry.size).sum()
    }
}

pub fn walk(path: &Path) -> Result<Walk> {
    let metadata = fs::metadata(path).with_context(|| format!("cannot read {}", path.display()))?;
    let root = root_name(path)?;

    if metadata.is_file() {
        return Ok(Walk {
            entries: vec![Entry {
                name: root.clone(),
                path: path.to_path_buf(),
                size: metadata.len(),
            }],
            root,
            is_dir: false,
        });
    }
    if !metadata.is_dir() {
        bail!("{} is neither a file nor a directory", path.display());
    }

    let mut entries = Vec::new();
    collect(path, "", &mut entries)?;
    entries.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(Walk {
        root,
        is_dir: true,
        entries,
    })
}

fn collect(dir: &Path, prefix: &str, out: &mut Vec<Entry>) -> Result<()> {
    let listing = fs::read_dir(dir).with_context(|| format!("cannot read {}", dir.display()))?;
    for entry in listing {
        let entry = entry.with_context(|| format!("cannot read an entry of {}", dir.display()))?;
        let file_type = entry
            .file_type()
            .with_context(|| format!("cannot stat {}", entry.path().display()))?;

        // Following one would let a link inside the folder pull in a file from
        // anywhere on the sender's disk.
        if file_type.is_symlink() {
            continue;
        }

        let Some(name) = entry.file_name().to_str().map(str::to_owned) else {
            bail!(
                "{} has a name that is not valid UTF-8",
                entry.path().display()
            );
        };
        let name = match prefix.is_empty() {
            true => name,
            false => format!("{prefix}/{name}"),
        };

        if file_type.is_dir() {
            collect(&entry.path(), &name, out)?;
        } else if file_type.is_file() {
            let size = entry
                .metadata()
                .with_context(|| format!("cannot stat {}", entry.path().display()))?
                .len();
            out.push(Entry {
                name,
                path: entry.path(),
                size,
            });
        }
    }
    Ok(())
}

/// `.`, `..` and a bare `/` have no name of their own, so they fall back to
/// whatever they resolve to on disk.
fn root_name(path: &Path) -> Result<String> {
    let resolved;
    let name = match path.file_name() {
        Some(name) => name,
        None => {
            resolved = path
                .canonicalize()
                .with_context(|| format!("cannot resolve {}", path.display()))?;
            resolved
                .file_name()
                .with_context(|| format!("{} has no name to send it under", path.display()))?
        }
    };
    name.to_str()
        .map(str::to_owned)
        .with_context(|| format!("{} has a name that is not valid UTF-8", path.display()))
}

#[cfg(test)]
#[path = "tests/tree.rs"]
mod tests;
