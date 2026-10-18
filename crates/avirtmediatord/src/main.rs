#![recursion_limit = "256"]

//! Per-workload mediator, spawned by avirthostd. It creates two Unix sockets for avirthostd to mount
//! into one container. The mediator is the container's only way out:
//!
//! - `proxy.sock`: a CONNECT proxy. It accepts only `CONNECT host:port`. It refuses private
//!   destinations and tunnels everything else. The only denies are `private-dest`,
//!   `plain-http` and `unsupported-method`.
//! - `mediator.sock` forwards the container's HTTP/2 workload connection byte for byte to
//!   avirthostd's own socket. HTTP/2 stays end to end between avirtinitd and avirthostd.
//!
//! One NDJSON event per proxied connection goes to avirthostd over a second avirthostd connection.

extern crate alloc;

mod events;
mod policy;

use alloc::sync::Arc;
use core::net::SocketAddr;
use core::pin::Pin;
use core::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use core::task::{Context, Poll};
use core::time::Duration;
use std::collections::HashMap;
use std::fs::Permissions;
use std::io::{self, Read as _, Write as _, stderr};
use std::os::unix::fs::PermissionsExt as _;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::{LazyLock, Mutex, MutexGuard, PoisonError};
use std::thread;
use std::time::Instant;

use async_trait::async_trait;
use clap::Parser;
use pingora_core::apps::ServerApp;
use pingora_core::protocols::Stream;
use pingora_core::server::configuration::ServerConf;
use pingora_core::server::{RunArgs, Server, ShutdownSignal, ShutdownSignalWatch, ShutdownWatch};
use pingora_core::services::listening::Service;
use serde_json::json;
use tokio::io::{
    AsyncRead, AsyncReadExt as _, AsyncWrite, AsyncWriteExt as _, ReadBuf, copy, copy_bidirectional, split,
};
use tokio::net::{TcpStream, UnixStream, lookup_host};
use tokio::signal::unix::{SignalKind, signal};
use tokio::sync::{Notify, Semaphore};
use tokio::time::timeout;

use policy::{Target, absolute_uri_authority, destination_allowed, host_addresses};

// The supervision test target uses `nix`. The unit-test build of this binary links it too.
#[cfg(test)]
use nix as _;

/// How long one direction of the splice may keep going after the other has ended.
const DRAIN: Duration = Duration::from_secs(5);
/// How long dialing and name resolution may each take.
const DIAL_TIMEOUT: Duration = Duration::from_secs(10);
/// How long a client may take to send its request head.
const HEAD_TIMEOUT: Duration = Duration::from_secs(10);
/// The largest request head the proxy accepts.
const MAX_HEAD: usize = 8 * 1024;
/// The number of connections each service handles at once.
const MAX_CONNECTIONS: usize = 128;

/// Set when the shutdown watch can't register its signal handlers. The mediator then exits nonzero.
static SIGNALS_FAILED: AtomicBool = AtomicBool::new(false);
/// Concurrent connections each service handles. The rest wait.
static LIMIT: Semaphore = Semaphore::const_new(MAX_CONNECTIONS);
/// Tunnels that passed the policy and haven't emitted their event yet.
static OPEN: LazyLock<OpenConns> = LazyLock::new(Mutex::default);
/// The workload id recorded in every event.
static WORKLOAD: LazyLock<Mutex<String>> = LazyLock::new(Mutex::default);

/// The connections tracked for the shutdown flush, by id.
type OpenConns = Mutex<HashMap<u64, Arc<Conn>>>;
/// The status to send and the reason for a refused request, or status 0 for a silent close.
type Refusal = (u16, &'static str);

/// The command line.
#[derive(Parser)]
#[command(about = None, long_about = None)]
struct Args {
    /// The workload's private directory. Creates `proxy.sock` and `mediator.sock` in it with mode 0600.
    #[arg(long, display_order = 0)]
    dir: PathBuf,
    /// avirthostd's Unix socket.
    #[arg(long, display_order = 1)]
    hostd_sock: PathBuf,
    /// What this mediator presents to avirthostd in its connection preambles. It comes from the
    /// environment to keep it off the command line. The container never sees it.
    #[arg(long, env = "AVIRT_MEDIATOR_SECRET", hide_env_values = true, display_order = 3)]
    secret: String,
    /// Workload id, recorded in every event and sent to avirthostd as the connections' binding.
    #[arg(long, display_order = 2)]
    workload: String,
}

/// One tunnel. Shares byte counters with the copy loop so a shutdown flush can report
/// what moved so far.
struct Conn {
    /// When the tunnel started, for the duration.
    began: Instant,
    /// Bytes sent to the client.
    down: Arc<AtomicU64>,
    /// Whether the mediator already wrote the event.
    emitted: AtomicBool,
    /// The requested host.
    host: String,
    /// A process-wide unique id.
    id: u64,
    /// The request method.
    method: String,
    /// The requested port.
    port: u16,
    /// When the tunnel started, as a wall-clock time.
    started: chrono::DateTime<chrono::Utc>,
    /// Bytes sent to the destination.
    up: Arc<AtomicU64>,
}

/// The CONNECT proxy service.
struct ConnectApp;

/// Wraps a stream and counts the bytes written to it.
struct Counting<S> {
    /// The wrapped stream.
    inner: S,
    /// The counter that grows with each write.
    written: Arc<AtomicU64>,
}

/// A parsed request head.
struct Request {
    /// Bytes read after the head, such as a TLS `ClientHello` sent with the CONNECT.
    early: Vec<u8>,
    /// The request method.
    method: String,
    /// The request target.
    path: String,
}

/// Shuts down on stdin EOF, SIGINT, or SIGTERM.
///
/// A crashed or killed avirthostd closes stdin, so it takes its proxies with it. SIGTERM shuts down
/// gracefully.
struct Shutdown {
    /// Notified when stdin reaches end of file.
    stdin_eof: Arc<Notify>,
}

/// Splices `mediator.sock` to avirthostd.
///
/// It waits for the first bytes before dialing, so a readiness probe (connect and close) costs
/// avirthostd nothing. Either direction ending ends the splice: HTTP/2 closes both ways when it's
/// done.
struct SpliceApp {
    /// avirthostd's Unix socket.
    hostd_sock: PathBuf,
    /// The secret sent in the connection preamble.
    secret: String,
    /// The workload id sent in the connection preamble.
    workload: String,
}

impl Conn {
    /// Writes the connection's event, once.
    fn emit(&self, decision: &str, rule: &str, error: Option<&str>) {
        if self.emitted.swap(true, Ordering::SeqCst) {
            return;
        }
        lock(&OPEN).remove(&self.id);
        let workload = lock(&WORKLOAD).clone();
        let event = json!({
            "event": "conn",
            "workload": workload,
            "started": self.started.to_rfc3339_opts(chrono::SecondsFormat::Millis, true),
            "method": self.method,
            "host": self.host,
            "port": self.port,
            "decision": decision,
            "rule": rule,
            "bytes_up": self.up.load(Ordering::Relaxed),
            "bytes_down": self.down.load(Ordering::Relaxed),
            "duration_ms": u64::try_from(self.began.elapsed().as_millis()).unwrap_or(u64::MAX),
            "error": error,
        });
        events::send(event.to_string());
    }

    /// Creates a connection record with a fresh id and zeroed counters.
    fn new(method: &str, host: &str, port: u16) -> Arc<Self> {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        Arc::new(Self {
            began: Instant::now(),
            down: Arc::default(),
            emitted: AtomicBool::new(false),
            host: host.to_owned(),
            id: NEXT.fetch_add(1, Ordering::Relaxed),
            method: method.to_owned(),
            port,
            started: chrono::Utc::now(),
            up: Arc::default(),
        })
    }

    /// Starts tracking the connection for the shutdown flush. Call only once the destination
    /// has passed the policy, so tracking never reports a refused request as allowed.
    fn register(self: &Arc<Self>) {
        lock(&OPEN).insert(self.id, Arc::clone(self));
    }
}

impl<S: AsyncRead + Unpin> AsyncRead for Counting<S> {
    fn poll_read(mut self: Pin<&mut Self>, cx: &mut Context<'_>, buf: &mut ReadBuf<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.inner).poll_read(cx, buf)
    }
}

impl<S: AsyncWrite + Unpin> AsyncWrite for Counting<S> {
    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.inner).poll_flush(cx)
    }

    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.inner).poll_shutdown(cx)
    }

    fn poll_write(mut self: Pin<&mut Self>, cx: &mut Context<'_>, buf: &[u8]) -> Poll<io::Result<usize>> {
        let poll = Pin::new(&mut self.inner).poll_write(cx, buf);
        if let Poll::Ready(Ok(written)) = &poll {
            count_bytes(&self.written, *written);
        }
        poll
    }
}

#[async_trait]
impl ServerApp for ConnectApp {
    async fn process_new(self: &Arc<Self>, mut session: Stream, shutdown: &ShutdownWatch) -> Option<Stream> {
        let _slot = LIMIT.acquire().await.ok()?;
        let request = accept_request(&mut session).await?;
        let conn = screen(&mut session, &request).await?;
        let addrs = resolve_allowed(&mut session, &conn).await?;
        let mut upstream = dial_upstream(&mut session, &conn, &addrs).await?;
        if open_tunnel(&mut session, &mut upstream, &conn, &request.early).await {
            relay(session, upstream, &conn, shutdown).await;
        }
        None
    }
}

#[async_trait]
impl ShutdownSignalWatch for Shutdown {
    async fn recv(&self) -> ShutdownSignal {
        let interrupt = signal(SignalKind::interrupt());
        let terminate = signal(SignalKind::terminate());
        if let Err(err) = &interrupt {
            drop(writeln!(stderr(), "cannot register the SIGINT handler: {err}"));
        }
        if let Err(err) = &terminate {
            drop(writeln!(stderr(), "cannot register the SIGTERM handler: {err}"));
        }
        let (Ok(mut sigint), Ok(mut sigterm)) = (interrupt, terminate) else {
            // Without signal handlers nothing could stop the mediator later, so it stops now and
            // `main` exits with a failure.
            SIGNALS_FAILED.store(true, Ordering::Release);
            return ShutdownSignal::FastShutdown;
        };
        tokio::select! {
            () = self.stdin_eof.notified() => ShutdownSignal::FastShutdown,
            _ = sigint.recv() => ShutdownSignal::FastShutdown,
            _ = sigterm.recv() => ShutdownSignal::GracefulTerminate,
        }
    }
}

#[async_trait]
impl ServerApp for SpliceApp {
    async fn process_new(self: &Arc<Self>, mut session: Stream, _shutdown: &ShutdownWatch) -> Option<Stream> {
        let _slot = LIMIT.acquire().await.ok()?;
        let mut first = [0u8; 4096];
        let count = timeout(HEAD_TIMEOUT, session.read(&mut first)).await.ok()?.ok().filter(|count| *count > 0)?;
        let mut hostd = UnixStream::connect(&self.hostd_sock).await.ok()?;
        hostd.write_all(format!("WORKLOAD {} {}\n", self.workload, self.secret).as_bytes()).await.ok()?;
        hostd.write_all(first.get(..count)?).await.ok()?;
        splice(&mut session, &mut hostd).await;
        None
    }
}

/// Reads the request head. A refused request gets its reply and its event here.
async fn accept_request(session: &mut Stream) -> Option<Request> {
    match read_head(session).await {
        Ok(request) => Some(request),
        Err((0, _)) => None,
        Err((status, why)) => {
            Conn::new("", "", 0).emit("deny", "bad-request", Some(why));
            refuse(session, status, "Bad Request", why).await;
            None
        }
    }
}

/// Builds the server with the proxy service and the `mediator.sock` splice service.
fn build_server(args: Args) -> Server {
    let conf = ServerConf {
        threads: 1,
        grace_period_seconds: Some(0),
        graceful_shutdown_timeout_seconds: Some(1),
        ..ServerConf::default()
    };
    let mut server = Server::new_with_opt_and_conf(None, conf);
    let proxy_sock = socket_path(&args.dir, "proxy.sock");
    let mut proxy = Service::new("proxy".into(), ConnectApp);
    proxy.add_uds(&proxy_sock, Some(private()));
    server.add_service(proxy);
    let mut workload = Service::new(
        "workload".into(),
        SpliceApp { hostd_sock: args.hostd_sock, secret: args.secret, workload: args.workload },
    );
    workload.add_uds(&socket_path(&args.dir, "mediator.sock"), Some(private()));
    server.add_service(workload);
    server.bootstrap();
    server
}

/// Adds `count` to the counter.
fn count_bytes(counter: &AtomicU64, count: usize) {
    counter.fetch_add(u64::try_from(count).unwrap_or(u64::MAX), Ordering::Relaxed);
}

/// Dials each address in sequence and returns the first connection.
///
/// # Errors
///
/// Returns the last dial error when every address fails.
async fn dial(addrs: &[SocketAddr]) -> io::Result<TcpStream> {
    let mut last = io::Error::other("no address to dial");
    for addr in addrs {
        match TcpStream::connect(addr).await {
            Ok(upstream) => return Ok(upstream),
            Err(error) => last = error,
        }
    }
    Err(last)
}

/// Registers the tunnel and dials the destination. A failure gets its reply and its event here.
async fn dial_upstream(session: &mut Stream, conn: &Arc<Conn>, addrs: &[SocketAddr]) -> Option<TcpStream> {
    conn.register();
    match timeout(DIAL_TIMEOUT, dial(addrs)).await {
        Ok(Ok(upstream)) => Some(upstream),
        Ok(Err(_)) => {
            conn.emit("allow", "allow-all", Some("dial"));
            refuse(session, 502, "Bad Gateway", "connect failed").await;
            None
        }
        Err(_) => {
            conn.emit("allow", "allow-all", Some("connect timeout"));
            refuse(session, 504, "Gateway Timeout", "connect timed out").await;
            None
        }
    }
}

/// Reports the connections still open when the server stopped, with what moved so far.
fn flush_open() {
    let open: Vec<_> = lock(&OPEN).values().cloned().collect();
    for conn in open {
        conn.emit("allow", "allow-all", Some("shutdown"));
    }
}

/// Takes the lock even when a panicking thread poisoned it. Every guarded value is a plain collection or
/// field that one call updates. A panic never leaves one half written.
fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Runs the mediator and returns the exit code.
fn main() -> ExitCode {
    let args = Args::parse();
    lock(&WORKLOAD).clone_from(&args.workload);
    if let Err(err) = events::connect(&args.hostd_sock, &args.workload, &args.secret) {
        drop(writeln!(stderr(), "cannot reach avirthostd at {}: {err}", args.hostd_sock.display()));
        return ExitCode::FAILURE;
    }
    let server = build_server(args);
    let stdin_eof = watch_stdin();
    server.run(RunArgs { shutdown_signal: Box::new(Shutdown { stdin_eof }) });
    flush_open();
    // Every event is on avirthostd's socket before the process exits.
    events::close();
    if SIGNALS_FAILED.load(Ordering::Acquire) { ExitCode::FAILURE } else { ExitCode::SUCCESS }
}

/// Opens the tunnel: sends the 200 reply and forwards bytes that arrived with the CONNECT.
/// Returns whether the relay should start.
async fn open_tunnel(session: &mut Stream, upstream: &mut TcpStream, conn: &Conn, early: &[u8]) -> bool {
    if session.write_all(b"HTTP/1.1 200 Connection Established\r\n\r\n").await.is_err()
        || session.flush().await.is_err()
    {
        conn.emit("allow", "allow-all", Some("client closed"));
        return false;
    }
    // Clients may send the TLS ClientHello with the CONNECT.
    if !early.is_empty() {
        if !matches!(timeout(DIAL_TIMEOUT, upstream.write_all(early)).await, Ok(Ok(()))) {
            conn.emit("allow", "allow-all", Some("upstream closed"));
            return false;
        }
        count_bytes(&conn.up, early.len());
    }
    true
}

/// Returns the mode that keeps a socket private to the mediator's user.
fn private() -> Permissions {
    Permissions::from_mode(0o600)
}

/// Parses the request head out of `buf`, or returns `None` while the head is incomplete.
///
/// # Errors
///
/// Returns the refusal for a malformed head or one larger than the limit.
fn parse_head(buf: &mut Vec<u8>) -> Result<Option<Request>, Refusal> {
    let mut headers = [httparse::EMPTY_HEADER; 32];
    let mut req = httparse::Request::new(&mut headers);
    match req.parse(buf) {
        Ok(httparse::Status::Complete(len)) => {
            let method = req.method.unwrap_or_default().to_owned();
            let path = req.path.unwrap_or_default().to_owned();
            Ok(Some(Request { early: buf.split_off(len), method, path }))
        }
        Ok(httparse::Status::Partial) if buf.len() < MAX_HEAD => Ok(None),
        Ok(httparse::Status::Partial) => Err((400, "request head too large")),
        Err(_) => Err((400, "malformed request")),
    }
}

/// Reads up to the end of the request head. Status 0 in the error means the client closed
/// without a byte.
///
/// # Errors
///
/// Returns the refusal for a malformed, oversize, unread, or late head.
async fn read_head(stream: &mut Stream) -> Result<Request, Refusal> {
    timeout(HEAD_TIMEOUT, read_head_bytes(stream)).await.unwrap_or(Err((400, "timed out reading the request")))
}

/// Reads from the stream until the request head is complete.
///
/// # Errors
///
/// Returns the refusal for a malformed, oversize, or unread head.
async fn read_head_bytes(stream: &mut Stream) -> Result<Request, Refusal> {
    let mut buf = Vec::with_capacity(1024);
    loop {
        let mut chunk = [0u8; 1024];
        let count = stream.read(&mut chunk).await.map_err(|_err| (400, "read failed"))?;
        if count == 0 {
            // A readiness probe (or a client that gave up) closes without a byte: not an event.
            return Err((0, ""));
        }
        buf.extend_from_slice(chunk.get(..count).unwrap_or_default());
        if let Some(request) = parse_head(&mut buf)? {
            return Ok(request);
        }
    }
}

/// Replies with a short text body and closes.
async fn refuse(stream: &mut Stream, status: u16, reason: &str, body: &str) {
    let response = format!(
        "HTTP/1.1 {status} {reason}\r\nConnection: close\r\nContent-Type: text/plain\r\nContent-Length: {}\r\n\r\n{body}",
        body.len()
    );
    stream.write_all(response.as_bytes()).await.unwrap_or_default();
    stream.flush().await.unwrap_or_default();
    stream.shutdown().await.unwrap_or_default();
}

/// Copies both ways until either side ends or the server shuts down, then emits the event.
async fn relay(session: Stream, upstream: TcpStream, conn: &Conn, shutdown: &ShutdownWatch) {
    let mut client = Counting { inner: session, written: Arc::clone(&conn.down) };
    let mut server = Counting { inner: upstream, written: Arc::clone(&conn.up) };
    let mut watch = shutdown.clone();
    let error = tokio::select! {
        result = copy_bidirectional(&mut client, &mut server) => {
            result.err().map(|error| error.kind().to_string())
        },
        _ = watch.changed() => Some("shutdown".to_owned()),
    };
    conn.emit("allow", "allow-all", error.as_deref());
}

/// Resolves the destination and checks every address against the policy. A refusal gets its
/// reply and its event here.
async fn resolve_allowed(session: &mut Stream, conn: &Conn) -> Option<Vec<SocketAddr>> {
    // Name resolution runs on the blocking pool and can take as long as the resolver's own
    // timeouts, so it gets the same limit as dialing.
    let addrs: Vec<SocketAddr> = match timeout(DIAL_TIMEOUT, lookup_host((conn.host.as_str(), conn.port))).await {
        Ok(Ok(found)) => found.collect(),
        _ => Vec::new(),
    };
    if addrs.is_empty() {
        conn.emit("allow", "allow-all", Some("dns"));
        refuse(session, 502, "Bad Gateway", "name resolution failed").await;
        return None;
    }
    // Without this the proxy would bridge to avirthostd's /launch and every host service.
    // The host's own addresses count too, global ones included: a wildcard listener answers on
    // them all.
    let own = host_addresses();
    if !destination_allowed(&addrs.iter().map(SocketAddr::ip).collect::<Vec<_>>(), &own) {
        conn.emit("deny", "private-dest", own.as_ref().err().map(|_| "host addresses"));
        refuse(session, 403, "Forbidden", &format!("egress denied: {}:{}", conn.host, conn.port)).await;
        return None;
    }
    Some(addrs)
}

/// Checks the method and the CONNECT authority. A refusal gets its reply and its event here.
async fn screen(session: &mut Stream, request: &Request) -> Option<Arc<Conn>> {
    if request.method != "CONNECT" {
        // Deny plain HTTP: serving it would need a second proxy service.
        let (host, port) = absolute_uri_authority(&request.path).unwrap_or_default();
        let conn = Conn::new(&request.method, &host, port);
        let rule = if request.path.starts_with("http://") { "plain-http" } else { "unsupported-method" };
        conn.emit("deny", rule, None);
        refuse(session, 501, "Not Implemented", "only CONNECT is supported").await;
        return None;
    }
    let Some(Target { host, port }) = Target::parse(&request.path) else {
        Conn::new(&request.method, &request.path, 0).emit("deny", "bad-request", Some("bad authority"));
        refuse(session, 400, "Bad Request", "bad CONNECT authority").await;
        return None;
    };
    Some(Conn::new(&request.method, &host, port))
}

/// Returns the path of a socket in the workload directory as the string Pingora takes.
fn socket_path(dir: &Path, name: &str) -> String {
    dir.join(name).to_string_lossy().into_owned()
}

/// Copies between the container and avirthostd.
///
/// Each direction passes its end-of-stream on, and an end on one side gives the other a bounded
/// time to finish, so a reply that's still in flight isn't cut off.
async fn splice(stream: &mut Stream, hostd: &mut UnixStream) {
    let (mut from_hostd, mut to_hostd) = hostd.split();
    let (mut from_container, mut to_container) = split(stream);
    let up = async {
        copy(&mut from_container, &mut to_hostd).await.unwrap_or_default();
        to_hostd.shutdown().await.unwrap_or_default();
    };
    let down = async {
        copy(&mut from_hostd, &mut to_container).await.unwrap_or_default();
        to_container.shutdown().await.unwrap_or_default();
    };
    tokio::pin!(up, down);
    tokio::select! {
        () = &mut up => { timeout(DRAIN, &mut down).await.unwrap_or_default(); }
        () = &mut down => { timeout(DRAIN, &mut up).await.unwrap_or_default(); }
    }
}

/// Starts a thread that notifies when stdin reaches end of file.
fn watch_stdin() -> Arc<Notify> {
    let stdin_eof = Arc::new(Notify::new());
    let notify = Arc::clone(&stdin_eof);
    thread::spawn(move || {
        let mut sink = [0u8; 64];
        while matches!(io::stdin().read(&mut sink), Ok(count) if count > 0) {}
        notify.notify_one();
    });
    stdin_eof
}
