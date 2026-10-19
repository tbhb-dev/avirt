//! The `StartWorkload` request, which tells avirtvmd what to run.

#[cfg(test)]
mod tests;

use super::digest;
use super::proto::{Share, Socket, StartWorkload};
use crate::runtime::WorkloadSpec;
use anyhow::{Context as _, Result};
use prost::Message as _;
use std::path::Path;

/// The mode of the sockets that the container sees.
const SOCKET_MODE: u32 = 0o600;

/// The prepared files a request names.
#[derive(Clone, Copy, Debug)]
pub struct Inputs<'paths> {
    /// Where avirtvmd creates the workload's VM state.
    pub container_dir: &'paths Path,
    /// The OCI image config blob.
    pub image_config: &'paths Path,
    /// The init filesystem.
    pub initfs: &'paths Path,
    /// The kernel.
    pub kernel: &'paths Path,
    /// The cached root filesystem, which avirtvmd clones.
    pub rootfs_base: &'paths Path,
    /// Where avirtvmd writes the container's stdout and stderr.
    pub stdio_log: &'paths Path,
}

/// Builds the request for `spec`.
///
/// The `spec_digest` is the sha256 of the request with its environment cleared, so the digest
/// never covers a secret.
///
/// # Errors
///
/// Returns an error if a path isn't valid UTF-8.
pub fn build(spec: &WorkloadSpec, inputs: &Inputs<'_>) -> Result<StartWorkload> {
    let mut request = StartWorkload {
        container_dir: text(inputs.container_dir)?,
        env: Vec::new(),
        image_config_path: text(inputs.image_config)?,
        initfs_path: text(inputs.initfs)?,
        kernel_path: text(inputs.kernel)?,
        rootfs_base_path: text(inputs.rootfs_base)?,
        shares: Vec::new(),
        sockets: vec![
            socket(&spec.dir.proxy_socket(), "/run/avirt/proxy.sock")?,
            socket(&spec.dir.mediator_socket(), "/run/avirt/mediator.sock")?,
        ],
        spec_digest: String::new(),
        stdio_log_path: text(inputs.stdio_log)?,
    };
    for share in &spec.shares {
        request.shares.push(Share { guest_path: share.guest.to_owned(), host_path: text(&share.host)? });
    }
    request.spec_digest = digest::bytes(&request.encode_to_vec());
    request.env = spec.env.iter().map(|(name, value)| format!("{name}={value}")).collect();
    Ok(request)
}

/// A socket on the host that appears in the container at `guest`.
///
/// # Errors
///
/// Returns an error if the host path isn't valid UTF-8.
fn socket(host: &Path, guest: &str) -> Result<Socket> {
    Ok(Socket { guest_path: guest.to_owned(), host_path: text(host)?, mode: SOCKET_MODE })
}

/// A path as text.
///
/// # Errors
///
/// Returns an error if the path isn't valid UTF-8.
fn text(path: &Path) -> Result<String> {
    path.to_str().map(str::to_owned).with_context(|| format!("{} isn't valid UTF-8", path.display()))
}
