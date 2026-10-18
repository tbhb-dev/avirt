//! The container-side harness shared by both agents. The container has no network. Its only
//! way out is two Unix sockets that avirthostd's per-workload mediator listens on, mounted at
//! `/run/avirt`: `mediator.sock` for the avirthostd connection, which only root may open, and `proxy.sock`
//! for the egress proxy. avirtinitd (root) holds the mediator connection, relays a loopback port to the
//! proxy socket, and runs the adapter as an unprivileged user with every capability gone. The
//! ACP conversation itself is supplied by the caller because the two adapters speak different
//! protocol versions.

use std::future::Future;
use std::path::Path;
use std::process::ExitCode;
use std::sync::Arc;
use std::time::Duration;

use agent_client_protocol::{AcpAgent, AcpAgentConfig, Error, LineDirection};
use serde_json::json;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, UnixStream};
use tokio::sync::Semaphore;

use crate::connection::MediatorConnection;
use crate::envelope::{Dir, MAX_LINE, parse_line};

pub struct Workload {
    /// The adapter subprocess with the tap installed; hand it to `connect_with`.
    pub agent: AcpAgent,
    pub connection: MediatorConnection,
    pub prompt: String,
    pub cwd: String,
}

pub struct Spec<'a> {
    /// Agent name recorded in the `start` event.
    pub agent: &'a str,
    /// Adapter executable on `PATH`.
    pub agent_command: &'a str,
    /// Extra environment for the adapter, on top of what avirtinitd inherited.
    pub agent_env: &'a [(&'a str, &'a str)],
}

/// JSON-RPC internal error code, used for failures that did not come from the agent.
const INTERNAL_ERROR: i32 = -32603;
const EXIT_USAGE: u8 = 64;
const EXIT_UNREACHABLE: u8 = 3;

/// The mediator's workload connection socket (host mode 0600, so root only in here).
const MEDIATOR_SOCKET: &str = "/run/avirt/mediator.sock";
/// The mediator's CONNECT proxy socket, also root only; the agent reaches it through a relay.
const PROXY_SOCKET: &str = "/run/avirt/proxy.sock";
/// How long to keep trying to reach the mediator before giving up.
const CONNECTION_WAIT: Duration = Duration::from_secs(15);
const EXIT_WORKLOAD_FAILED: u8 = 2;

fn required_env(name: &str) -> Result<String, ExitCode> {
    match std::env::var(name) {
        Ok(v) if !v.is_empty() => Ok(v),
        _ => {
            eprintln!("{name} is required");
            Err(ExitCode::from(EXIT_USAGE))
        }
    }
}

/// Runs one workload. `workload` returns the stop reason (`end_turn` exits 0, anything else 1).
pub async fn run<F, Fut>(spec: Spec<'_>, workload: F) -> ExitCode
where
    F: FnOnce(Workload) -> Fut,
    Fut: Future<Output = Result<String, Error>>,
{
    let (workload_id, prompt, token) =
        match (required_env("AVIRT_WORKLOAD_ID"), required_env("AVIRT_PROMPT"), required_env("AVIRT_CONNECTION_TOKEN"))
        {
            (Ok(s), Ok(p), Ok(t)) => (s, p, t),
            (Err(code), ..) | (_, Err(code), _) | (.., Err(code)) => return code,
        };
    let cwd = std::env::var("AVIRT_CWD").ok().filter(|c| !c.is_empty()).unwrap_or_else(|| "/workspace".into());

    // Nothing starts until avirthostd has accepted the stream.
    let connection =
        match MediatorConnection::open(Path::new(MEDIATOR_SOCKET), &workload_id, &token, CONNECTION_WAIT).await {
            Ok(connection) => connection,
            Err(err) => {
                eprintln!("cannot open the workload stream: {err}");
                return ExitCode::from(EXIT_UNREACHABLE);
            }
        };
    connection.meta(
        json!({ "event": "start", "agent": spec.agent, "workloadId": workload_id, "cwd": cwd, "prompt": prompt }),
    );

    let proxy = match start_relay().await {
        Ok(addr) => format!("http://{addr}"),
        Err(err) => {
            eprintln!("cannot start the proxy relay: {err}");
            connection.meta(json!({ "event": "error", "message": "cannot start the proxy relay" }));
            connection.close(Duration::from_secs(5)).await;
            return ExitCode::from(EXIT_UNREACHABLE);
        }
    };
    // The agent shares the VM's network namespace with the runtime's vsock relays, and nothing
    // stops it from opening a vsock socket and dialing the host ports behind the mounted
    // sockets. From here on, this process and everything it starts cannot (see the function).
    if let Err(why) = restrict_sockets() {
        eprintln!("cannot restrict sockets: {why}");
        connection.meta(json!({ "event": "error", "message": "cannot restrict sockets" }));
        connection.close(Duration::from_secs(5)).await;
        return ExitCode::from(EXIT_UNREACHABLE);
    }
    let config = agent_config(&spec, &proxy);
    // The SDK's line hook sees every line it writes to or reads from the adapter, in both
    // directions, without altering them.
    let tap = connection.clone();
    let agent = AcpAgent::new(config).with_debug(move |line, direction| {
        let line = line.trim();
        if line.is_empty() {
            return;
        }
        // A line past the limit is not forwarded in any form; the avirthostd-side reader drops
        // oversized lines too, but this keeps them out of memory on the way.
        if line.len() > MAX_LINE {
            tap.meta(json!({ "event": "line_truncated", "bytes": line.len() }));
            return;
        }
        match direction {
            LineDirection::Stdin => tap.send(Dir::ClientToAgent, parse_line(line)),
            LineDirection::Stdout => tap.send(Dir::AgentToClient, parse_line(line)),
            LineDirection::Stderr => {
                eprintln!("{line}");
                tap.meta(json!({ "event": "agent_stderr", "line": line }));
            }
        }
    });

    // A dead adapter ends the workload inside the SDK; a refused or dropped stream ends it here.
    // As the runtime's init's direct child, a `container stop` reaches this process as SIGTERM.
    let mut sigterm =
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()).expect("SIGTERM handler");
    let result = tokio::select! {
        r = workload(Workload { agent, connection: connection.clone(), prompt, cwd }) => r,
        reason = connection.rejected() => Err(Error::new(INTERNAL_ERROR, reason)),
        _ = sigterm.recv() => Err(Error::new(INTERNAL_ERROR, "terminated by SIGTERM")),
    };

    let code = match result {
        Ok(stop_reason) => {
            println!();
            connection.meta(json!({ "event": "exit", "stopReason": stop_reason }));
            u8::from(stop_reason != "end_turn")
        }
        Err(err) => {
            let message = err.to_string();
            eprintln!("workload failed: {message}");
            connection.meta(json!({ "event": "error", "message": message }));
            connection.meta(json!({ "event": "exit", "stopReason": "error" }));
            EXIT_WORKLOAD_FAILED
        }
    };
    connection.close(Duration::from_secs(10)).await;
    ExitCode::from(code)
}

/// Connections the relay carries at once; the rest wait in the listen backlog.
const RELAY_CONNECTIONS: usize = 128;

/// How long the downstream direction may sit idle once the client has finished sending. Before
/// that there is no limit, because model API streams are long-lived and quiet.
const RELAY_IDLE_AFTER_CLIENT_DONE: Duration = Duration::from_secs(30);

/// Listens on an ephemeral loopback port and relays each connection to the proxy's Unix socket,
/// which only root can open. The agent reaches the proxy through this port and nothing else.
///
/// A client's end-of-stream is not forwarded to the proxy: over a `-v` socket-file mount the
/// runtime relay drops the peer's reply after an immediate `SHUT_WR`, so the host side's end
/// decides. Once the client has finished sending, the response direction is given as long as it
/// keeps moving data (30 s idle at most), so an active download is never cut; when the host side
/// ends, the client is told (its end-of-stream is forwarded) and the relay closes.
async fn start_relay() -> std::io::Result<std::net::SocketAddr> {
    let listener = TcpListener::bind(("127.0.0.1", 0)).await?;
    let addr = listener.local_addr()?;
    let slots = Arc::new(Semaphore::new(RELAY_CONNECTIONS));
    tokio::spawn(async move {
        loop {
            let Ok(slot) = Arc::clone(&slots).acquire_owned().await else { return };
            let Ok((client, _)) = listener.accept().await else {
                // E.g. out of file descriptors: back off instead of spinning.
                tokio::time::sleep(Duration::from_millis(100)).await;
                continue;
            };
            tokio::spawn(async move {
                let _slot = slot;
                let Ok(proxy) = UnixStream::connect(PROXY_SOCKET).await else { return };
                let (mut from_client, mut to_client) = client.into_split();
                let (mut from_proxy, mut to_proxy) = proxy.into_split();
                let client_done = std::sync::atomic::AtomicBool::new(false);
                let upstream = async {
                    let _ = tokio::io::copy(&mut from_client, &mut to_proxy).await;
                    client_done.store(true, std::sync::atomic::Ordering::SeqCst);
                    std::future::pending::<()>().await;
                };
                let downstream = async {
                    let mut buf = vec![0u8; 16 * 1024];
                    loop {
                        let idle = if client_done.load(std::sync::atomic::Ordering::SeqCst) {
                            RELAY_IDLE_AFTER_CLIENT_DONE
                        } else {
                            Duration::from_hours(24)
                        };
                        match tokio::time::timeout(idle, from_proxy.read(&mut buf)).await {
                            Ok(Ok(n)) if n > 0 => {
                                if to_client.write_all(&buf[..n]).await.is_err() {
                                    break;
                                }
                            }
                            _ => break,
                        }
                    }
                    let _ = to_client.shutdown().await;
                };
                tokio::select! { () = upstream => {}, () = downstream => {} }
            });
        }
    });
    Ok(addr)
}

/// Restricts what the agent subtree can reach beyond loopback, with two seccomp filters applied
/// to this process (all threads) and inherited by everything it starts. They are
/// avirtinitd's own: the runtime applies no seccomp inside the container.
///
/// - `socket()` with domain `AF_VSOCK` fails with `EAFNOSUPPORT`. The runtime's vsock relays can
///   be dialed by any process in the VM, which would skip the sockets' permissions. `AF_NETLINK`
///   and `AF_PACKET` are denied too: neither is needed, and no capability or NIC makes them
///   useful, so this is belt and braces. `AF_UNIX`, `AF_INET` and `AF_INET6` stay, because the
///   agent reaches the proxy relay over loopback.
/// - `io_uring_setup`, `io_uring_enter` and `io_uring_register` fail with `EPERM`, because
///   `IORING_OP_SOCKET` creates sockets without calling `socket()`. Node falls back to its
///   thread pool when io_uring is unavailable.
#[cfg(target_os = "linux")]
fn restrict_sockets() -> Result<(), String> {
    use seccompiler::{
        BpfProgram, SeccompAction, SeccompCmpArgLen, SeccompCmpOp, SeccompCondition, SeccompFilter, SeccompRule,
        TargetArch,
    };
    use std::collections::BTreeMap;
    const AF_NETLINK: u64 = 16;
    const AF_PACKET: u64 = 17;
    const AF_VSOCK: u64 = 40;
    let err = |e: &dyn std::fmt::Display| e.to_string();
    let arch = TargetArch::try_from(std::env::consts::ARCH).map_err(|e| err(&e))?;

    let domain_rules = [AF_VSOCK, AF_NETLINK, AF_PACKET]
        .into_iter()
        .map(|domain| {
            SeccompCondition::new(0, SeccompCmpArgLen::Dword, SeccompCmpOp::Eq, domain)
                .and_then(|c| SeccompRule::new(vec![c]))
                .map_err(|e| err(&e))
        })
        .collect::<Result<Vec<_>, _>>()?;
    // On x86_64 the x32 ABI reaches the same syscalls under numbers with this bit set, which a
    // filter keyed on the plain numbers would not see. It does not exist on the aarch64 images.
    #[cfg(target_arch = "x86_64")]
    const X32: i64 = 0x4000_0000;
    #[cfg(target_arch = "x86_64")]
    let (x32_rules, x32_io_uring) =
        (domain_rules.clone(), vec![libc::SYS_io_uring_setup, libc::SYS_io_uring_enter, libc::SYS_io_uring_register]);
    #[allow(unused_mut)]
    let mut socket_map = BTreeMap::from([(libc::SYS_socket, domain_rules)]);
    #[cfg(target_arch = "x86_64")]
    socket_map.insert(libc::SYS_socket | X32, x32_rules);
    let sockets =
        SeccompFilter::new(socket_map, SeccompAction::Allow, SeccompAction::Errno(libc::EAFNOSUPPORT as u32), arch)
            .map_err(|e| err(&e))?;

    // An empty rule list matches the syscall unconditionally.
    #[allow(unused_mut)]
    let mut io_uring_map = BTreeMap::from([
        (libc::SYS_io_uring_setup, vec![]),
        (libc::SYS_io_uring_enter, vec![]),
        (libc::SYS_io_uring_register, vec![]),
    ]);
    #[cfg(target_arch = "x86_64")]
    for nr in x32_io_uring {
        io_uring_map.insert(nr | X32, vec![]);
    }
    let io_uring =
        SeccompFilter::new(io_uring_map, SeccompAction::Allow, SeccompAction::Errno(libc::EPERM as u32), arch)
            .map_err(|e| err(&e))?;

    for filter in [sockets, io_uring] {
        let program: BpfProgram = filter.try_into().map_err(|e: seccompiler::BackendError| err(&e))?;
        seccompiler::apply_filter_all_threads(&program).map_err(|e| err(&e))?;
    }
    Ok(())
}

#[cfg(not(target_os = "linux"))]
#[allow(clippy::unnecessary_wraps)]
fn restrict_sockets() -> Result<(), String> {
    Ok(())
}

/// How the adapter is started: as the agent user with every capability gone, in an environment
/// built here. avirtinitd's own variables (workload id, prompt, working directory) are
/// removed; the rest, such as credentials and `CODEX_HOME`, pass through. All egress goes to
/// the relay.
fn agent_config(spec: &Spec<'_>, proxy: &str) -> AcpAgentConfig {
    let mut args: Vec<&str> =
        ["AVIRT_WORKLOAD_ID", "AVIRT_PROMPT", "AVIRT_CWD", "AVIRT_CONNECTION_TOKEN", "AVIRT_AGENT"]
            .iter()
            .flat_map(|name| ["-u", name])
            .collect();
    args.extend([
        "setpriv",
        "--reuid=agent",
        "--regid=agent",
        "--init-groups",
        "--inh-caps=-all",
        "--bounding-set=-all",
        "--no-new-privs",
        "--",
        spec.agent_command,
    ]);
    let mut config = AcpAgentConfig::new("env").args(args).env("HOME", "/home/agent");
    // curl and git honour only the lowercase `http_proxy`; Node needs NODE_USE_ENV_PROXY.
    for name in [
        "HTTPS_PROXY",
        "https_proxy",
        "HTTP_PROXY",
        "http_proxy",
        "ALL_PROXY",
        "npm_config_proxy",
        "npm_config_https_proxy",
    ] {
        config = config.env(name, proxy);
    }
    for name in ["NO_PROXY", "no_proxy"] {
        config = config.env(name, "localhost,127.0.0.1");
    }
    config = config.env("NODE_USE_ENV_PROXY", "1");
    for (k, v) in spec.agent_env {
        config = config.env(*k, *v);
    }
    config
}

/// Auto-approval policy: the first `allow_once` option, else `allow_always`, else the first
/// option; `None` (cancel) only when there are no options. `kind` names an option's kind in
/// the wire spelling.
pub fn pick_option<T>(options: &[T], kind: impl Fn(&T) -> &'static str) -> Option<&T> {
    ["allow_once", "allow_always"]
        .iter()
        .find_map(|want| options.iter().find(|o| kind(o) == *want))
        .or_else(|| options.first())
}

#[cfg(test)]
mod tests {
    use super::pick_option;

    #[test]
    fn approval_prefers_allow_once_then_always_then_first() {
        let opts = [("a", "reject_once"), ("b", "allow_always"), ("c", "allow_once")];
        assert_eq!(pick_option(&opts, |o| o.1).unwrap().0, "c");
        assert_eq!(pick_option(&opts[..2], |o| o.1).unwrap().0, "b");
        assert_eq!(pick_option(&opts[..1], |o| o.1).unwrap().0, "a");
        assert!(pick_option::<(&str, &str)>(&[], |o| o.1).is_none());
    }
}
