use std::{
    env, fs,
    net::{Ipv4Addr, SocketAddr},
    path::{Path, PathBuf},
    process,
};

use iroh::{Endpoint, EndpointAddr, RelayMode, endpoint::presets};

/// A unique directory under the system temp dir, deleted when the guard drops.
pub struct TempDir(PathBuf);

impl TempDir {
    /// Creates the directory. `label` keeps concurrent tests from colliding.
    pub fn new(label: &str) -> Self {
        let path = env::temp_dir().join(format!("iroh-app-{}-{label}", process::id()));
        fs::create_dir_all(&path).unwrap();
        Self(path)
    }

    pub fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

/// An endpoint on an ephemeral port with no relay, for tests that run both
/// halves of a transfer in one process.
pub async fn test_endpoint() -> Endpoint {
    Endpoint::builder(presets::N0)
        .relay_mode(RelayMode::Disabled)
        .bind()
        .await
        .unwrap()
}

/// The address to dial `endpoint` on from this machine.
///
/// `Endpoint::online` would wait forever with the relay disabled, and
/// `Endpoint::addr` needs it to have settled first, so the loopback address is
/// built from the bound socket instead.
pub fn local_addr(endpoint: &Endpoint) -> EndpointAddr {
    let port = endpoint.bound_sockets()[0].port();
    EndpointAddr::new(endpoint.id()).with_ip_addr(SocketAddr::from((Ipv4Addr::LOCALHOST, port)))
}
