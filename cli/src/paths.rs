use std::path::{Component, Path, PathBuf};

use anyhow::{Context, Result, bail, ensure};

pub fn safe_component(component: &str) -> Result<()> {
    ensure!(!component.is_empty(), "a path component is empty");
    if let "." | ".." = component {
        bail!("path component {component:?} would escape the download directory");
    }
    if component.contains('\0') {
        bail!("path component {component:?} contains a NUL byte");
    }

    if let Some(found) = component.chars().find(|c| matches!(c, '/' | '\\' | ':')) {
        bail!("path component {component:?} contains a reserved character {found:?}");
    }

    let mut components = Path::new(component).components();
    let Some(Component::Normal(only)) = components.next() else {
        bail!("path component {component:?} is not an ordinary name");
    };
    ensure!(
        components.next().is_none() && only.to_str() == Some(component),
        "path component {component:?} is not an ordinary name"
    );
    Ok(())
}

pub fn safe_relative_path(name: &str) -> Result<PathBuf> {
    ensure!(!name.is_empty(), "an entry name is empty");
    let mut path = PathBuf::new();
    for component in name.split('/') {
        safe_component(component).with_context(|| format!("entry name {name:?} is unsafe"))?;
        path.push(component);
    }
    Ok(path)
}

pub fn unique_destination(parent: &Path, name: &str, is_dir: bool) -> PathBuf {
    let first = parent.join(name);
    if !first.exists() {
        return first;
    }
    // `notes.txt-1` would stop looking like a text file, so a file keeps its
    // extension. A directory has none to preserve.
    let (stem, extension) = match is_dir {
        true => (name, None),
        false => {
            let path = Path::new(name);
            match path.file_stem().and_then(|stem| stem.to_str()) {
                Some(stem) => (stem, path.extension().and_then(|ext| ext.to_str())),
                None => (name, None),
            }
        }
    };
    (1u32..)
        .map(|n| match extension {
            Some(extension) => parent.join(format!("{stem}-{n}.{extension}")),
            None => parent.join(format!("{stem}-{n}")),
        })
        .find(|candidate| !candidate.exists())
        .expect("an unused suffix exists below u32::MAX")
}

pub fn absolute(path: &Path) -> Result<PathBuf> {
    std::path::absolute(path)
        .with_context(|| format!("failed to resolve {} to an absolute path", path.display()))
}

#[cfg(test)]
#[path = "tests/paths.rs"]
mod tests;
