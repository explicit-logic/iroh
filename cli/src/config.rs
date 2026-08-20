use std::{collections::BTreeSet, fs, path::Path};

use anyhow::{Context, Result, bail};
use iroh::{RelayConfig, RelayMap, RelayMode};
use serde::Deserialize;

const CONFIG_PATH: &str = "config.toml";

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawConfig {
    bind_port: u16,

    relay: Vec<RelayConfig>,
}

#[derive(Debug)]
pub struct Config {
    /// Which relay servers the endpoint should use.
    pub relay_mode: RelayMode,
    /// The UDP port the receiver binds, or 0 for an ephemeral one.
    pub bind_port: u16,
}

impl Config {
    /// Reads `config.toml` from the working directory.
    pub fn load() -> Result<Self> {
        Self::from_path(Path::new(CONFIG_PATH))
    }

    /// Reads the config from `path`.
    pub fn from_path(path: &Path) -> Result<Self> {
        let contents = fs::read_to_string(path)
            .with_context(|| format!("failed to read config file {}", path.display()))?;
        Self::parse(&contents)
            .with_context(|| format!("failed to parse config file {}", path.display()))
    }

    /// Parses the config from the contents of a config file.
    pub fn parse(contents: &str) -> Result<Self> {
        if contents.trim().is_empty() {
            bail!("config is empty");
        }
        let raw: RawConfig = toml::from_str(contents)?;
        if raw.relay.is_empty() {
            bail!("config must define at least one [[relay]] entry");
        }
        for relay in &raw.relay {
            // An empty token is never valid on the relay side either, and would
            // otherwise fail much later as an opaque connection refusal.
            if relay.auth_token.as_deref() == Some("") {
                bail!(
                    "relay {} has an empty auth_token; set a token or remove the key",
                    relay.url
                );
            }
        }
        // `RelayMap` is keyed by url, so duplicates would silently discard one
        // entry along with its token.
        let mut seen = BTreeSet::new();
        for relay in &raw.relay {
            if !seen.insert(&relay.url) {
                bail!("relay {} is listed more than once", relay.url);
            }
        }
        for relay in &raw.relay {
            let auth = match relay.auth_token {
                Some(_) => " (with auth token)",
                None => "",
            };
            println!("using relay: {}{auth}", relay.url);
        }
        Ok(Self {
            relay_mode: RelayMode::Custom(RelayMap::from_iter(raw.relay)),
            bind_port: raw.bind_port,
        })
    }
}

#[cfg(test)]
#[path = "tests/config.rs"]
mod tests;
