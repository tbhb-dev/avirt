//! Unit tests for the `StartWorkload` request.

use super::{Inputs, build};
use crate::egress::WorkloadDir;
use crate::hostd::profile::Share;
use crate::runtime::WorkloadSpec;
use anyhow::{Result, ensure};
use std::path::{Path, PathBuf};

/// A spec for workload `00000001` in `base`, with one secret variable.
///
/// # Errors
///
/// Fails when making the workload directory fails.
fn spec(base: &Path, secret: &str) -> Result<WorkloadSpec> {
    Ok(WorkloadSpec {
        dir: WorkloadDir::create(base, "00000001")?,
        env: vec![("TOKEN".to_owned(), secret.to_owned()), ("MODE".to_owned(), "fast".to_owned())],
        id: "00000001".to_owned(),
        image: "image:dev".to_owned(),
        logs_dir: base.join("logs"),
        shares: vec![Share { guest: "/workspace", host: PathBuf::from("/home/work") }],
    })
}

/// The paths a request names.
fn inputs() -> Inputs<'static> {
    Inputs {
        container_dir: Path::new("/state/containers/00000001"),
        image_config: Path::new("/images/blobs/sha256/config"),
        initfs: Path::new("/cache/initfs/initfs.ext4"),
        kernel: Path::new("/cache/kernel/vmlinux"),
        rootfs_base: Path::new("/cache/rootfs/rootfs.ext4"),
        stdio_log: Path::new("/logs/00000001.stdio.ndjson"),
    }
}

/// The request lists the sockets and the shares, and writes each variable as `NAME=value`.
///
/// # Errors
///
/// Fails when a field differs from the spec.
#[test]
fn the_request_names_the_sockets_the_shares_and_the_variables() -> Result<()> {
    let base = tempfile::tempdir()?;
    let request = build(&spec(base.path(), "hunter2")?, &inputs())?;
    ensure!(request.env == ["TOKEN=hunter2", "MODE=fast"], "the environment is {:?}", request.env);
    ensure!(request.shares.len() == 1 && request.shares.iter().all(|share| share.guest_path == "/workspace"), "shares");
    let guests: Vec<_> = request.sockets.iter().map(|socket| (socket.guest_path.as_str(), socket.mode)).collect();
    ensure!(guests == [("/run/avirt/proxy.sock", 0o600), ("/run/avirt/mediator.sock", 0o600)], "{guests:?}");
    ensure!(
        request.sockets.iter().all(|socket| socket.host_path.contains("/00000001/")),
        "a host path is outside the workload directory"
    );
    Ok(())
}

/// The digest ignores the environment. A changed secret leaves it unchanged.
///
/// # Errors
///
/// Fails when changing a secret changes the digest.
#[test]
fn the_digest_never_covers_the_environment() -> Result<()> {
    let base = tempfile::tempdir()?;
    let mut workload = spec(base.path(), "one")?;
    let first = build(&workload, &inputs())?;
    for (name, value) in &mut workload.env {
        if name == "TOKEN" {
            "two".clone_into(value);
        }
    }
    let second = build(&workload, &inputs())?;
    ensure!(first.spec_digest.len() == 64, "the digest is {}", first.spec_digest);
    ensure!(first.spec_digest == second.spec_digest, "the secret changed the digest");
    ensure!(first.env != second.env, "the environment didn't change");
    Ok(())
}
