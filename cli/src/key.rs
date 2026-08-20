use std::{
    fmt::Write as _,
    fs,
    io::{ErrorKind, Write as _},
    path::Path,
};

use anyhow::{Context, Result};
use iroh::SecretKey;

pub const SECRET_KEY_PATH: &str = "secret.key";

pub fn load_or_create_secret_key(path: &Path) -> Result<SecretKey> {
    match fs::read_to_string(path) {
        Ok(contents) => contents
            .trim()
            .parse()
            .with_context(|| format!("failed to parse the secret key in {}", path.display())),
        Err(err) if err.kind() == ErrorKind::NotFound => {
            let key = SecretKey::generate();
            write_secret_key(path, &key)
                .with_context(|| format!("failed to write a secret key to {}", path.display()))?;
            println!("generated a new secret key at {}", path.display());
            Ok(key)
        }
        Err(err) => {
            Err(err).with_context(|| format!("failed to read the secret key in {}", path.display()))
        }
    }
}

fn write_secret_key(path: &Path, key: &SecretKey) -> Result<()> {
    if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
        fs::create_dir_all(parent)?;
    }

    let mut encoded = String::with_capacity(65);
    for byte in key.to_bytes() {
        write!(&mut encoded, "{byte:02x}")?;
    }
    encoded.push('\n');

    let mut options = fs::OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt as _;
        // Set the mode as the file is created; writing first and tightening
        // afterwards would leave the key world-readable in between.
        options.mode(0o600);
    }
    options.open(path)?.write_all(encoded.as_bytes())?;
    Ok(())
}

#[cfg(test)]
#[path = "tests/key.rs"]
mod tests;
