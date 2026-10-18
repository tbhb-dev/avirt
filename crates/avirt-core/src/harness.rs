//! The container-side harness shared by both agents.
//!
//! The container has no network. Its only way out is two Unix sockets that avirthostd's
//! per-workload mediator listens on, mounted at `/run/avirt`: `mediator.sock` for the avirthostd
//! connection, which only root may open, and `proxy.sock` for the egress proxy. avirtinitd (root)
//! maintains the mediator connection and relays a loopback port to the proxy socket. It runs the
//! adapter as an unprivileged user with every capability gone. The caller supplies the ACP
//! conversation because the two adapters speak different protocol versions.

#[cfg(test)]
mod tests;

#[cfg(target_os = "linux")]
use alloc::collections::BTreeMap;
use alloc::sync::Arc;
use core::future;
use core::net::SocketAddr;
use core::sync::atomic::{AtomicBool, Ordering};
use core::time::Duration;
use std::path::Path;
use std::process::ExitCode;
use std::{env, io};

use agent_client_protocol::{AcpAgent, AcpAgentConfig, Error, LineDirection};
#[cfg(target_os = "linux")]
use seccompiler::{
    BackendError, BpfProgram, SeccompAction, SeccompCmpArgLen, SeccompCmpOp, SeccompCondition, SeccompFilter,
    SeccompRule, TargetArch,
};
use serde_json::json;
use tokio::io::{self as tokio_io, AsyncReadExt as _, AsyncWriteExt as _};
use tokio::net::tcp::OwnedWriteHalf;
use tokio::net::unix::OwnedReadHalf;
use tokio::net::{TcpListener, TcpStream, UnixStream};
use tokio::signal::unix::{SignalKind, signal};
use tokio::sync::{OwnedSemaphorePermit, Semaphore};
use tokio::time;

use crate::connection::LogStream;
use crate::console;
use crate::envelope::{Dir, MAX_LINE, parse_line};

/// How long to keep trying to reach the mediator before giving up.
const CONNECTION_WAIT: Duration = Duration::from_secs(15);
/// Exit code when the mediator or the relay is unreachable.
const EXIT_UNREACHABLE: u8 = 3;
/// Exit code when a required environment variable is missing.
const EXIT_USAGE: u8 = 64;
/// Exit code when the workload fails.
const EXIT_WORKLOAD_FAILED: u8 = 2;
/// JSON-RPC internal error code, used for failures that didn't come from the agent.
const INTERNAL_ERROR: i32 = -32603;
/// The mediator's workload connection socket (host mode 0600, so root only in here).
const MEDIATOR_SOCKET: &str = "/run/avirt/mediator.sock";
/// The mediator's CONNECT proxy socket, also root only. The agent connects through a relay.
const PROXY_SOCKET: &str = "/run/avirt/proxy.sock";
/// Connections the relay handles at once. The rest wait in the listen backlog.
const RELAY_CONNECTIONS: usize = 128;
/// How long the downstream direction may remain idle once the client has finished sending. Before
/// that there is no limit, because model API streams are long-lived and quiet.
const RELAY_IDLE_AFTER_CLIENT_DONE: Duration = Duration::from_secs(30);
/// The x32 ABI reaches the same syscalls under numbers with this bit set, which a filter keyed on
/// the plain numbers would not see. It doesn't exist on the aarch64 images.
#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
const X32: i64 = 0x4000_0000;

/// What differs between agents: the adapter and its environment.
#[derive(Clone, Copy, Debug)]
pub struct Spec<'spec> {
    /// Agent name recorded in the `start` event.
    pub agent: &'spec str,
    /// Adapter executable on `PATH`.
    pub agent_command: &'spec str,
    /// Extra environment for the adapter, on top of what avirtinitd inherited.
    pub agent_env: &'spec [(&'spec str, &'spec str)],
}

/// Everything the caller's ACP conversation needs.
#[derive(Debug)]
pub struct Workload {
    /// The adapter subprocess with the tap installed. Hand it to `connect_with`.
    pub agent: AcpAgent,
    /// The connection to avirthostd, for `meta` events.
    pub connection: LogStream,
    /// The working directory the agent runs in.
    pub cwd: String,
    /// The prompt to send.
    pub prompt: String,
}

/// The environment avirtinitd reads on startup.
struct Env {
    /// The working directory, `/workspace` unless `AVIRT_CWD` says otherwise.
    cwd: String,
    /// The prompt to send.
    prompt: String,
    /// The token avirtinitd presents on its stream POST.
    token: String,
    /// The workload id.
    workload_id: String,
}

/// Reports why the run stopped to avirthostd and the operator, closes the connection, and
/// returns the exit code.
async fn abort(connection: &LogStream, summary: &str, cause: String) -> ExitCode {
    console::err_line(format_args!("{summary}: {cause}"));
    connection.meta(json!({ "event": "error", "message": summary }));
    connection.close(Duration::from_secs(5)).await;
    ExitCode::from(EXIT_UNREACHABLE)
}

/// Accepts connections on the relay's loopback port, one relay task per connection.
async fn accept_relay(listener: TcpListener, slots: Arc<Semaphore>) {
    loop {
        let Ok(slot) = Arc::clone(&slots).acquire_owned().await else { return };
        let Ok((client, _)) = listener.accept().await else {
            // For errors such as running out of file descriptors, back off instead of spinning.
            time::sleep(Duration::from_millis(100)).await;
            continue;
        };
        tokio::spawn(relay_connection(client, slot));
    }
}

/// Configures the adapter to run as the agent user with no capabilities.
///
/// Builds the adapter's environment here and removes avirtinitd's own variables (workload id,
/// prompt, working directory). The rest, such as credentials and `CODEX_HOME`, pass through. All
/// egress goes to the relay.
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
    // curl and git honour only the lowercase `http_proxy`. Node needs NODE_USE_ENV_PROXY.
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
    for (key, value) in spec.agent_env {
        config = config.env(*key, *value);
    }
    config
}

/// Builds the filter that makes `io_uring_setup`, `io_uring_enter` and `io_uring_register` fail
/// with `EPERM`.
///
/// # Errors
///
/// Returns the seccomp error text if building the filter fails.
#[cfg(target_os = "linux")]
fn io_uring_filter(arch: TargetArch) -> Result<SeccompFilter, String> {
    // An empty rule list matches the syscall unconditionally.
    let rules = BTreeMap::from([
        (libc::SYS_io_uring_setup, vec![]),
        (libc::SYS_io_uring_enter, vec![]),
        (libc::SYS_io_uring_register, vec![]),
    ]);
    SeccompFilter::new(with_x32(rules), SeccompAction::Allow, SeccompAction::Errno(libc::EPERM.cast_unsigned()), arch)
        .map_err(|err| err.to_string())
}

/// Picks the option that auto-approval selects.
///
/// That's the first `allow_once` option, falling back to `allow_always` and then the first
/// option. Returns `None` (cancel) only for an empty option list. `kind` names an option's kind in
/// the wire spelling.
pub fn pick_option<T, F>(options: &[T], kind: F) -> Option<&T>
where
    F: Fn(&T) -> &'static str,
{
    ["allow_once", "allow_always"]
        .iter()
        .find_map(|want| options.iter().find(|option| kind(option) == *want))
        .or_else(|| options.first())
}

/// Reads the environment avirtinitd needs, naming every missing variable on stderr.
///
/// # Errors
///
/// Returns the usage exit code when a required variable is missing or empty.
fn read_env() -> Result<Env, ExitCode> {
    match (required_env("AVIRT_WORKLOAD_ID"), required_env("AVIRT_PROMPT"), required_env("AVIRT_CONNECTION_TOKEN")) {
        (Ok(workload_id), Ok(prompt), Ok(token)) => {
            let cwd = env::var("AVIRT_CWD").ok().filter(|cwd| !cwd.is_empty()).unwrap_or_else(|| "/workspace".into());
            Ok(Env { cwd, prompt, token, workload_id })
        }
        (Err(code), ..) | (_, Err(code), _) | (.., Err(code)) => Err(code),
    }
}

/// Copies from the proxy to the client until the proxy ends or the idle limit passes, then
/// forwards the end-of-stream to the client.
async fn relay_downstream(from_proxy: &mut OwnedReadHalf, to_client: &mut OwnedWriteHalf, client_done: &AtomicBool) {
    let mut buf = vec![0u8; 16 * 1024];
    loop {
        let idle =
            if client_done.load(Ordering::SeqCst) { RELAY_IDLE_AFTER_CLIENT_DONE } else { Duration::from_hours(24) };
        match time::timeout(idle, from_proxy.read(&mut buf)).await {
            Ok(Ok(count)) if count > 0 => {
                let Some(chunk) = buf.get(..count) else { break };
                if to_client.write_all(chunk).await.is_err() {
                    break;
                }
            }
            _ => break,
        }
    }
    drop(to_client.shutdown().await);
}

/// Relays one client connection to the proxy's Unix socket and holds its slot until either side ends.
async fn relay_connection(client: TcpStream, _slot: OwnedSemaphorePermit) {
    let Ok(proxy) = UnixStream::connect(PROXY_SOCKET).await else { return };
    let (mut from_client, mut to_client) = client.into_split();
    let (mut from_proxy, mut to_proxy) = proxy.into_split();
    let client_done = AtomicBool::new(false);
    let upstream = async {
        drop(tokio_io::copy(&mut from_client, &mut to_proxy).await);
        client_done.store(true, Ordering::SeqCst);
        future::pending::<()>().await;
    };
    let downstream = relay_downstream(&mut from_proxy, &mut to_client, &client_done);
    tokio::select! { () = upstream => {}, () = downstream => {} }
}

/// Reads a required environment variable and reports a missing or empty one on stderr.
///
/// # Errors
///
/// Returns the usage exit code when the variable is missing or empty.
fn required_env(name: &str) -> Result<String, ExitCode> {
    match env::var(name) {
        Ok(value) if !value.is_empty() => Ok(value),
        _ => {
            console::err_line(format_args!("{name} is required"));
            Err(ExitCode::from(EXIT_USAGE))
        }
    }
}

/// Restricts what the agent subtree can reach beyond loopback.
///
/// avirtinitd applies two seccomp filters to this process (all threads), and everything it starts
/// inherits them. The runtime doesn't apply seccomp inside the container.
///
/// - `socket()` with domain `AF_VSOCK` fails with `EAFNOSUPPORT`. Any process in the VM can dial
///   the runtime's vsock relays, which would skip the sockets' permissions. `AF_NETLINK`
///   and `AF_PACKET` also fail. The agent doesn't need them, and neither has a capability or NIC
///   that would make it useful. Denying them provides another check. `AF_UNIX`, `AF_INET` and `AF_INET6` stay, because the
///   agent reaches the proxy relay over loopback.
/// - `io_uring_setup`, `io_uring_enter` and `io_uring_register` fail with `EPERM`, because
///   `IORING_OP_SOCKET` creates sockets without calling `socket()`. Node falls back to its
///   thread pool when `io_uring` is unavailable.
///
/// # Errors
///
/// Returns the seccomp error text if building or applying a filter fails.
#[cfg(target_os = "linux")]
fn restrict_sockets() -> Result<(), String> {
    let arch = TargetArch::try_from(env::consts::ARCH).map_err(|err| err.to_string())?;
    for filter in [socket_filter(arch)?, io_uring_filter(arch)?] {
        let program: BpfProgram = filter.try_into().map_err(|err: BackendError| err.to_string())?;
        seccompiler::apply_filter_all_threads(&program).map_err(|err| err.to_string())?;
    }
    Ok(())
}

/// Opens the connection to avirthostd and sends the `start` event.
///
/// # Errors
///
/// Returns the unreachable exit code when avirthostd doesn't accept the stream.
async fn open_connection(spec: &Spec<'_>, env: &Env) -> Result<LogStream, ExitCode> {
    // Don't start anything until avirthostd has accepted the stream.
    let connection =
        match LogStream::open(Path::new(MEDIATOR_SOCKET), &env.workload_id, &env.token, CONNECTION_WAIT).await {
            Ok(connection) => connection,
            Err(err) => {
                console::err_line(format_args!("cannot open the workload stream: {err}"));
                return Err(ExitCode::from(EXIT_UNREACHABLE));
            }
        };
    connection.meta(json!({
        "event": "start",
        "agent": spec.agent,
        "workloadId": env.workload_id,
        "cwd": env.cwd,
        "prompt": env.prompt,
    }));
    Ok(connection)
}

/// Runs one workload. `workload` returns the stop reason (`end_turn` exits 0, anything else 1).
pub async fn run<F, Fut>(spec: Spec<'_>, workload: F) -> ExitCode
where
    F: FnOnce(Workload) -> Fut,
    Fut: Future<Output = Result<String, Box<Error>>>,
{
    let env = match read_env() {
        Ok(env) => env,
        Err(code) => return code,
    };
    let (connection, agent) = match start_agent(&spec, &env).await {
        Ok(started) => started,
        Err(code) => return code,
    };
    let work = workload(Workload { agent, connection: connection.clone(), cwd: env.cwd, prompt: env.prompt });
    let result = race(work, &connection).await;
    let code = report(&connection, result);
    connection.close(Duration::from_secs(10)).await;
    ExitCode::from(code)
}

/// Opens the connection and starts the proxy relay. Then it restricts sockets and builds the adapter.
///
/// # Errors
///
/// Returns the exit code to stop with when any step fails.
async fn start_agent(spec: &Spec<'_>, env: &Env) -> Result<(LogStream, AcpAgent), ExitCode> {
    let connection = open_connection(spec, env).await?;
    let proxy = match start_relay().await {
        Ok(addr) => format!("http://{addr}"),
        Err(err) => return Err(abort(&connection, "cannot start the proxy relay", err.to_string()).await),
    };
    // The agent shares the VM's network namespace with the runtime's vsock relays, and nothing
    // stops it from opening a vsock socket and dialing the host ports behind the mounted
    // sockets. From here on, this process and everything it starts can't (see the function).
    #[cfg(target_os = "linux")]
    if let Err(why) = restrict_sockets() {
        return Err(abort(&connection, "cannot restrict sockets", why).await);
    }
    // The SDK's line hook sees every line it writes to or reads from the adapter, in both
    // directions, without altering them.
    let tap = connection.clone();
    let agent =
        AcpAgent::new(agent_config(spec, &proxy)).with_debug(move |line, direction| tap_line(&tap, line, direction));
    Ok((connection, agent))
}

/// Waits for the workload, a refused or dropped stream, or SIGTERM, whichever comes first.
///
/// An adapter exit ends the workload inside the SDK. A refused or dropped stream ends it here.
/// This process is a direct child of the runtime's init and receives SIGTERM on `container stop`.
///
/// # Errors
///
/// Returns the workload's error, or an internal error when avirthostd refuses the stream, SIGTERM
/// arrives, or the process can't register a SIGTERM handler.
async fn race<Fut>(work: Fut, connection: &LogStream) -> Result<String, Box<Error>>
where
    Fut: Future<Output = Result<String, Box<Error>>>,
{
    let mut sigterm = match signal(SignalKind::terminate()) {
        Ok(handler) => handler,
        Err(err) => {
            return Err(Box::new(Error::new(INTERNAL_ERROR, format!("cannot register a SIGTERM handler: {err}"))));
        }
    };
    tokio::select! {
        outcome = work => outcome,
        reason = connection.rejected() => Err(Box::new(Error::new(INTERNAL_ERROR, reason))),
        _ = sigterm.recv() => Err(Box::new(Error::new(INTERNAL_ERROR, "terminated by SIGTERM"))),
    }
}

/// Writes the workload's outcome to avirthostd and returns the exit code.
fn report(connection: &LogStream, result: Result<String, Box<Error>>) -> u8 {
    match result {
        Ok(stop_reason) => {
            console::out_line(format_args!(""));
            connection.meta(json!({ "event": "exit", "stopReason": stop_reason }));
            u8::from(stop_reason != "end_turn")
        }
        Err(err) => {
            let message = err.to_string();
            console::err_line(format_args!("workload failed: {message}"));
            connection.meta(json!({ "event": "error", "message": message }));
            connection.meta(json!({ "event": "exit", "stopReason": "error" }));
            EXIT_WORKLOAD_FAILED
        }
    }
}

/// Builds the filter that makes `socket()` with the `AF_VSOCK`, `AF_NETLINK` and `AF_PACKET`
/// domains fail with `EAFNOSUPPORT`.
///
/// # Errors
///
/// Returns the seccomp error text if building the filter fails.
#[cfg(target_os = "linux")]
fn socket_filter(arch: TargetArch) -> Result<SeccompFilter, String> {
    const AF_NETLINK: u64 = 16;
    const AF_PACKET: u64 = 17;
    const AF_VSOCK: u64 = 40;
    let domain_rules = [AF_VSOCK, AF_NETLINK, AF_PACKET]
        .into_iter()
        .map(|domain| {
            SeccompCondition::new(0, SeccompCmpArgLen::Dword, SeccompCmpOp::Eq, domain)
                .and_then(|condition| SeccompRule::new(vec![condition]))
                .map_err(|err| err.to_string())
        })
        .collect::<Result<Vec<_>, _>>()?;
    let rules = BTreeMap::from([(libc::SYS_socket, domain_rules)]);
    SeccompFilter::new(
        with_x32(rules),
        SeccompAction::Allow,
        SeccompAction::Errno(libc::EAFNOSUPPORT.cast_unsigned()),
        arch,
    )
    .map_err(|err| err.to_string())
}

/// Listens on an ephemeral loopback port and relays each connection to the proxy's Unix socket.
///
/// Only root can open the proxy socket. The agent connects to the proxy only through this port.
/// The relay never forwards a client's end-of-stream to the proxy: over a `-v` socket-file mount the
/// runtime relay drops the peer's reply after an immediate `SHUT_WR`, so the host side's end
/// decides. Once the client has finished sending, the relay lets the response continue as long as it
/// keeps moving data (30 s idle at most), so it never cuts an active download. When the host side
/// ends, the relay forwards its end-of-stream to the client and closes.
///
/// # Errors
///
/// Returns an error if binding the loopback port fails.
async fn start_relay() -> io::Result<SocketAddr> {
    let listener = TcpListener::bind(("127.0.0.1", 0)).await?;
    let addr = listener.local_addr()?;
    tokio::spawn(accept_relay(listener, Arc::new(Semaphore::new(RELAY_CONNECTIONS))));
    Ok(addr)
}

/// Forwards one line from the adapter's stdio to avirthostd.
fn tap_line(connection: &LogStream, raw: &str, direction: LineDirection) {
    let line = raw.trim();
    if line.is_empty() {
        return;
    }
    // Don't forward a line past the limit in any form. The avirthostd-side reader also drops
    // lines past the limit, but this keeps them out of memory on the way.
    if line.len() > MAX_LINE {
        connection.meta(json!({ "event": "line_truncated", "bytes": line.len() }));
        return;
    }
    match direction {
        LineDirection::Stdin => connection.send(Dir::ClientToAgent, parse_line(line)),
        LineDirection::Stdout => connection.send(Dir::AgentToClient, parse_line(line)),
        LineDirection::Stderr => {
            console::err_line(format_args!("{line}"));
            connection.meta(json!({ "event": "agent_stderr", "line": line }));
        }
    }
}

/// Adds the x32 syscall interface's copy of each syscall, which a filter keyed on the plain numbers would miss.
#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
fn with_x32<T>(mut rules: BTreeMap<i64, T>) -> BTreeMap<i64, T>
where
    T: Clone,
{
    let x32: Vec<_> = rules.iter().map(|(number, rule)| (*number | X32, rule.clone())).collect();
    rules.extend(x32);
    rules
}

/// Leaves the syscall numbers alone: the x32 ABI exists only on `x86_64`.
#[cfg(all(target_os = "linux", not(target_arch = "x86_64")))]
const fn with_x32<T>(rules: BTreeMap<i64, T>) -> BTreeMap<i64, T> {
    rules
}
