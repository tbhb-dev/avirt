//! The runtime that drives Apple's `container` command: each workload container is one
//! `container run` process, and the mediator is a host process.

#[cfg(test)]
mod tests;

use super::{Exit, MediatorSpec, Runtime, Stopped, Swept, WorkloadExit, WorkloadHandle, WorkloadSpec, spawn_mediator};
use crate::egress::Proxy;
use crate::state;
use anyhow::{Result, bail};
use async_trait::async_trait;
use core::time::Duration;
use serde_json::Value;
use std::env;
use std::ffi::OsString;
use std::fs::OpenOptions;
use std::io;
use std::os::unix::fs::OpenOptionsExt as _;
use std::os::unix::process::ExitStatusExt as _;
use std::path::Path;
use std::process::{self, ExitStatus, Stdio};
use tokio::process::{Child, Command};
use tokio::task;
use tokio::time;

/// Label put on every container avirthostd starts. A sweep uses it to find them without guessing at names.
pub const CONTAINER_LABEL: &str = "avirt=1";
/// Marks the container as a workload.
pub const WORKLOAD_LABEL: &str = "avirt.role=workload";

/// The runtime that runs each workload container through the `container` command.
#[derive(Clone, Copy, Debug)]
pub struct ContainerCli;

/// One workload under [`ContainerCli`].
struct Handle {
    /// The `container run` process, once started.
    child: Option<Child>,
    /// The workload's mediator, once started and until `stop_mediator`.
    proxy: Option<Proxy>,
    /// Why `container run` failed to start. `wait` reports it as the workload's exit.
    spawn_error: Option<io::Error>,
    /// The workload to run.
    spec: WorkloadSpec,
}

#[async_trait]
impl Runtime for ContainerCli {
    async fn open(&self, spec: WorkloadSpec) -> Result<Box<dyn WorkloadHandle>> {
        Ok(Box::new(Handle { child: None, proxy: None, spawn_error: None, spec }))
    }

    async fn sweep(&self, _state: &state::Root) -> Swept {
        let removed = task::spawn_blocking(sweep_containers).await.unwrap_or_default();
        Swept { removed, ..Swept::default() }
    }
}

#[async_trait]
impl WorkloadHandle for Handle {
    async fn release(self: Box<Self>, gone: bool) {
        let Self { spec, .. } = *self;
        if gone {
            // Removes the workload directory with both sockets in it.
            drop(spec.dir);
        } else {
            spec.dir.leave();
        }
    }

    async fn start_mediator(&mut self, spec: MediatorSpec) -> Result<()> {
        if self.proxy.is_some() {
            bail!("the workload already has a mediator");
        }
        // The mediator starts before the container because `container run -v` mounts only a
        // socket file that already exists.
        self.proxy = Some(spawn_mediator(&self.spec.dir, &self.spec.id, &spec).await?);
        Ok(())
    }

    async fn start_workload(&mut self) -> Result<()> {
        if self.proxy.is_none() {
            bail!("the workload's mediator is not running");
        }
        if self.child.is_some() || self.spawn_error.is_some() {
            bail!("the workload container was already started");
        }
        match command(&self.spec)?.spawn() {
            Ok(child) => self.child = Some(child),
            Err(err) => self.spawn_error = Some(err),
        }
        Ok(())
    }

    async fn stop(&mut self, grace: Duration) -> Stopped {
        let Some(child) = self.child.as_mut() else {
            return Stopped { exit: Err(io::Error::other("the container never started")), gone: true };
        };
        let (status, gone) = stop_and_wait(&self.spec.id, child, grace).await;
        Stopped { exit: status.map(exit_of), gone }
    }

    async fn stop_mediator(&mut self) {
        if let Some(proxy) = self.proxy.take() {
            proxy.stop().await;
        }
    }

    async fn wait(&mut self) -> Exit {
        if let Some(err) = self.spawn_error.take() {
            return Exit::Workload(Err(err));
        }
        let (Some(child), Some(proxy)) = (self.child.as_mut(), self.proxy.as_mut()) else {
            return Exit::Lost("the workload is not running".to_owned());
        };
        tokio::select! {
            status = child.wait() => Exit::Workload(status.map(exit_of)),
            died = proxy.exited() => Exit::Mediator(died.map_or_else(|err| err.to_string(), |exit| exit.to_string())),
        }
    }
}

/// The name of a workload's container.
fn container_name(id: &str) -> String {
    format!("avirt-{id}-workload")
}

/// The mount spec for a socket or directory: `host:guest`.
fn mount(host: &Path, guest: &str) -> OsString {
    let mut spec = OsString::from(host);
    spec.push(":");
    spec.push(guest);
    spec
}

/// How a `container run` process ended.
fn exit_of(status: ExitStatus) -> WorkloadExit {
    WorkloadExit { code: status.code(), signal: status.signal() }
}

/// Builds the `container run` command for one workload.
///
/// # Errors
///
/// Returns an error if creating the container log fails.
fn command(spec: &WorkloadSpec) -> io::Result<Command> {
    let id = &spec.id;
    let log_path = spec.logs_dir.join(format!("{id}.container.log"));
    let container_log = OpenOptions::new().create_new(true).append(true).mode(0o600).open(log_path)?;
    let mut cmd = Command::new("container");
    // Start from nothing: avirthostd's environment holds credentials for every agent, and a
    // launch gets only the variables its profile names plus what the command-line interface itself needs.
    cmd.env_clear();
    for name in ["HOME", "PATH", "TMPDIR", "USER", "LOGNAME"] {
        if let Some(value) = env::var_os(name) {
            cmd.env(name, value);
        }
    }
    // No network at all. The runtime relays a mounted socket *file* to its host listener, so
    // the two mediator sockets, which must already exist, are the container's only way out.
    // (Mounting their directory instead would go over virtiofs, which can't connect them.)
    //
    // `--init` makes the runtime's init PID 1: it reaps orphans (avirtinitd would not) and
    // forwards `container stop` to avirtinitd as SIGTERM.
    cmd.args([
        "run",
        "--rm",
        "--init",
        "--name",
        &container_name(id),
        "--label",
        CONTAINER_LABEL,
        "--label",
        WORKLOAD_LABEL,
        "--network",
        "none",
    ])
    .arg("-v")
    .arg(mount(&spec.dir.proxy_socket(), "/run/avirt/proxy.sock"))
    .arg("-v")
    .arg(mount(&spec.dir.mediator_socket(), "/run/avirt/mediator.sock"));
    for share in &spec.shares {
        cmd.arg("-v").arg(mount(&share.host, share.guest));
    }
    // `-e NAME` with no value makes the `container` command read it from its own environment, so
    // argv holds names and never a profile secret or the prompt.
    for (name, value) in &spec.env {
        cmd.arg("-e").arg(name).env(name, value);
    }
    cmd.arg(&spec.image)
        .stdin(Stdio::null())
        .stdout(container_log.try_clone()?)
        .stderr(container_log)
        // A separate process group prevents terminal Ctrl-C from reaching the command-line interface while avirthostd receives it.
        .process_group(0);
    Ok(cmd)
}

/// Runs a `container` subcommand with a time limit. Returns true if it succeeded.
async fn container_cmd(args: &[&str], limit: Duration) -> bool {
    let run = Command::new("container")
        .args(args)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        // A separate process group prevents a second Ctrl-C from stopping it. Drop and stop it on timeout.
        .process_group(0)
        .kill_on_drop(true)
        .status();
    matches!(time::timeout(limit, run).await, Ok(Ok(status)) if status.success())
}

/// Stops the container and waits for the `container run` command-line interface to exit.
///
/// A stop that arrives before the container exists fails fast, so this function retries it at a
/// short interval. As a last resort, it kills and deletes the container by name before killing the
/// `container` process. The flag confirms whether the container no longer exists.
/// Without that confirmation, the caller must not change what the agent could still be using.
async fn stop_and_wait(id: &str, child: &mut Child, grace: Duration) -> (io::Result<ExitStatus>, bool) {
    let name = container_name(id);
    let seconds = grace.as_secs().to_string();
    for _ in 0u8..40 {
        let stopped = container_cmd(&["stop", "--time", &seconds, &name], Duration::from_secs(15)).await;
        let wait = if stopped { grace.saturating_add(Duration::from_secs(5)) } else { Duration::from_millis(300) };
        if let Ok(status) = time::timeout(wait, child.wait()).await {
            return (status, true);
        }
    }
    container_cmd(&["kill", &name], Duration::from_secs(10)).await;
    container_cmd(&["delete", "--force", &name], Duration::from_secs(10)).await;
    drop(child.start_kill());
    let status = time::timeout(Duration::from_secs(5), child.wait())
        .await
        .unwrap_or_else(|_| Err(io::Error::other("the container CLI did not exit")));
    // `inspect` fails once the container no longer exists.
    let gone = !container_cmd(&["inspect", &name], Duration::from_secs(10)).await;
    (status, gone)
}

/// Stops and deletes the container if it has avirthostd's label, returning its id.
fn sweep_container(container: &Value, key: &str, value: &str) -> Option<String> {
    let config = container.get("configuration")?;
    if config.get("labels")?.get(key)?.as_str() != Some(value) {
        return None;
    }
    let id = config.get("id")?.as_str()?;
    for args in [["kill", id], ["delete", id]] {
        drop(process::Command::new("container").args(args).output());
    }
    Some(id.to_owned())
}

/// Stops and deletes every container with avirthostd's label, whatever its state.
fn sweep_containers() -> Vec<String> {
    let Ok(out) = process::Command::new("container").args(["ls", "--all", "--format", "json"]).output() else {
        return Vec::new();
    };
    let Ok(list) = serde_json::from_slice::<Value>(&out.stdout) else { return Vec::new() };
    // The label constant always has the form `key=value`.
    let Some((key, value)) = CONTAINER_LABEL.split_once('=') else { return Vec::new() };
    list.as_array().into_iter().flatten().filter_map(|container| sweep_container(container, key, value)).collect()
}
