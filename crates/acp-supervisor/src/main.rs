//! Per-session supervisor, spawned by the daemon. It owns the two Unix sockets that are mounted
//! into one container and is the container's only way out:
//!
//! - `proxy.sock`: a CONNECT proxy. It accepts only `CONNECT host:port`, refuses private
//!   destinations, and tunnels everything else. Phase 1 is log-only: the only denies are
//!   `private-dest` and `plain-http`.
//! - `session.sock`: the container's HTTP/2 session uplink, spliced byte for byte to the
//!   daemon's own socket, so HTTP/2 stays end to end between the entrypoint and the daemon.
//!
//! One NDJSON event per proxied connection goes to the daemon over a second daemon connection.

mod events;
mod policy;

use std::collections::HashMap;
use std::net::SocketAddr;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, LazyLock, Mutex};
use std::time::{Duration, Instant};

use async_trait::async_trait;
use clap::Parser;
use pingora_core::apps::ServerApp;
use pingora_core::protocols::Stream;
use pingora_core::server::configuration::ServerConf;
use pingora_core::server::{RunArgs, Server, ShutdownSignal, ShutdownSignalWatch, ShutdownWatch};
use pingora_core::services::listening::Service;
use serde_json::json;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpStream, UnixStream};
use tokio::sync::Semaphore;

use policy::{Target, is_private};

const MAX_HEAD: usize = 8 * 1024;
const HEAD_TIMEOUT: Duration = Duration::from_secs(10);
const DIAL_TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Parser)]
struct Args {
    /// The session's private directory; `proxy.sock` and `session.sock` are created in it, 0600.
    #[arg(long)]
    dir: PathBuf,
    /// The daemon's Unix socket.
    #[arg(long)]
    daemon_sock: PathBuf,
    /// Session id, recorded in every event and sent to the daemon as the connections' binding.
    #[arg(long)]
    session: String,
    /// What this supervisor presents to the daemon in its connection preambles. It comes from the
    /// environment so that it is not on the command line, and the container never sees it.
    #[arg(long, env = "ACP_SUPERVISOR_SECRET", hide_env_values = true)]
    secret: String,
}

/// One tunnel. The byte counters are shared with the copy loop so a shutdown flush can report
/// what moved so far.
struct Conn {
    id: u64,
    method: String,
    host: String,
    port: u16,
    started: chrono::DateTime<chrono::Utc>,
    began: Instant,
    up: Arc<AtomicU64>,
    down: Arc<AtomicU64>,
    emitted: AtomicBool,
}

static OPEN: LazyLock<Mutex<HashMap<u64, Arc<Conn>>>> = LazyLock::new(Mutex::default);
/// Concurrent connections each service will carry; the rest wait.
static LIMIT: Semaphore = Semaphore::const_new(MAX_CONNECTIONS);
const MAX_CONNECTIONS: usize = 128;
static SESSION: LazyLock<Mutex<String>> = LazyLock::new(Mutex::default);

impl Conn {
    fn new(method: &str, host: &str, port: u16) -> Arc<Self> {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        Arc::new(Self {
            id: NEXT.fetch_add(1, Ordering::Relaxed),
            method: method.to_owned(),
            host: host.to_owned(),
            port,
            started: chrono::Utc::now(),
            began: Instant::now(),
            up: Arc::default(),
            down: Arc::default(),
            emitted: AtomicBool::new(false),
        })
    }

    /// Starts tracking the connection for the shutdown flush. Only called once the destination
    /// has passed the policy, so a refused request is never reported as allowed.
    fn register(self: &Arc<Self>) {
        OPEN.lock().expect("open lock").insert(self.id, Arc::clone(self));
    }

    /// Writes the connection's event, once.
    fn emit(&self, decision: &str, rule: &str, error: Option<&str>) {
        if self.emitted.swap(true, Ordering::SeqCst) {
            return;
        }
        OPEN.lock().expect("open lock").remove(&self.id);
        let event = json!({
            "event": "conn",
            "session": *SESSION.lock().expect("session lock"),
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
}

/// Wraps a stream and counts the bytes written to it.
struct Counting<S> {
    inner: S,
    written: Arc<AtomicU64>,
}

impl<S: tokio::io::AsyncRead + Unpin> tokio::io::AsyncRead for Counting<S> {
    fn poll_read(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &mut tokio::io::ReadBuf<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        std::pin::Pin::new(&mut self.inner).poll_read(cx, buf)
    }
}

impl<S: tokio::io::AsyncWrite + Unpin> tokio::io::AsyncWrite for Counting<S> {
    fn poll_write(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &[u8],
    ) -> std::task::Poll<std::io::Result<usize>> {
        let poll = std::pin::Pin::new(&mut self.inner).poll_write(cx, buf);
        if let std::task::Poll::Ready(Ok(n)) = &poll {
            self.written.fetch_add(*n as u64, Ordering::Relaxed);
        }
        poll
    }

    fn poll_flush(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        std::pin::Pin::new(&mut self.inner).poll_flush(cx)
    }

    fn poll_shutdown(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        std::pin::Pin::new(&mut self.inner).poll_shutdown(cx)
    }
}

struct ConnectApp;

/// Replies with a short text body and closes.
async fn refuse(stream: &mut Stream, status: u16, reason: &str, body: &str) {
    let response = format!(
        "HTTP/1.1 {status} {reason}\r\nConnection: close\r\nContent-Type: text/plain\r\nContent-Length: {}\r\n\r\n{body}",
        body.len()
    );
    let _ = stream.write_all(response.as_bytes()).await;
    let _ = stream.flush().await;
    let _ = stream.shutdown().await;
}

/// Reads up to the end of the request head. Returns the head's length and everything read.
async fn read_head(stream: &mut Stream) -> Result<(usize, Vec<u8>), (u16, &'static str)> {
    let read = async {
        let mut buf = Vec::with_capacity(1024);
        loop {
            let mut chunk = [0u8; 1024];
            let n = stream.read(&mut chunk).await.map_err(|_| (400, "read failed"))?;
            if n == 0 {
                // A readiness probe (or a client that gave up) closes without a byte: not an event.
                return Err((0, ""));
            }
            buf.extend_from_slice(&chunk[..n]);
            let mut headers = [httparse::EMPTY_HEADER; 32];
            match httparse::Request::new(&mut headers).parse(&buf) {
                Ok(httparse::Status::Complete(len)) => return Ok((len, buf)),
                Ok(httparse::Status::Partial) if buf.len() < MAX_HEAD => {}
                Ok(httparse::Status::Partial) => return Err((400, "request head too large")),
                Err(_) => return Err((400, "malformed request")),
            }
        }
    };
    tokio::time::timeout(HEAD_TIMEOUT, read).await.unwrap_or(Err((400, "timed out reading the request")))
}

#[async_trait]
impl ServerApp for ConnectApp {
    async fn process_new(self: &Arc<Self>, mut stream: Stream, shutdown: &ShutdownWatch) -> Option<Stream> {
        let _slot = LIMIT.acquire().await.ok()?;
        let (head_len, buf) = match read_head(&mut stream).await {
            Ok(v) => v,
            Err((0, _)) => return None,
            Err((status, why)) => {
                Conn::new("", "", 0).emit("deny", "bad-request", Some(why));
                refuse(&mut stream, status, "Bad Request", why).await;
                return None;
            }
        };
        let mut headers = [httparse::EMPTY_HEADER; 32];
        let mut req = httparse::Request::new(&mut headers);
        let _ = req.parse(&buf);
        let method = req.method.unwrap_or_default().to_owned();
        let path = req.path.unwrap_or_default().to_owned();
        let early = &buf[head_len..];

        if method != "CONNECT" {
            // Plain HTTP is a deliberate exception to log-only: it would need a second proxy
            // service, and phase 2 would mostly deny it anyway.
            let (host, port) = policy::absolute_uri_authority(&path).unwrap_or_default();
            let conn = Conn::new(&method, &host, port);
            let rule = if path.starts_with("http://") { "plain-http" } else { "unsupported-method" };
            conn.emit("deny", rule, None);
            refuse(&mut stream, 501, "Not Implemented", "only CONNECT is supported").await;
            return None;
        }
        let Some(Target { host, port }) = Target::parse(&path) else {
            Conn::new(&method, &path, 0).emit("deny", "bad-request", Some("bad authority"));
            refuse(&mut stream, 400, "Bad Request", "bad CONNECT authority").await;
            return None;
        };
        let conn = Conn::new(&method, &host, port);

        // Name resolution runs on the blocking pool and can take as long as the resolver's own
        // timeouts, so it gets the same limit as dialing.
        let addrs: Vec<SocketAddr> =
            match tokio::time::timeout(DIAL_TIMEOUT, tokio::net::lookup_host((host.as_str(), port))).await {
                Ok(Ok(addrs)) => addrs.collect(),
                _ => Vec::new(),
            };
        if addrs.is_empty() {
            conn.emit("allow", "phase1-allow-all", Some("dns"));
            refuse(&mut stream, 502, "Bad Gateway", "name resolution failed").await;
            return None;
        }
        // Without this the proxy would bridge to the daemon's /launch and every host service.
        // The host's own addresses count too, global ones included: a wildcard listener answers on
        // all of them.
        let own = policy::host_addresses();
        if addrs.iter().any(|a| is_private(a.ip()) || policy::is_host_address(a.ip(), &own)) {
            conn.emit("deny", "private-dest", None);
            refuse(&mut stream, 403, "Forbidden", &format!("egress denied: {host}:{port}")).await;
            return None;
        }

        conn.register();
        let dial = async {
            let mut last = None;
            for addr in &addrs {
                match TcpStream::connect(addr).await {
                    Ok(s) => return Ok(s),
                    Err(e) => last = Some(e),
                }
            }
            Err(last.expect("at least one address"))
        };
        let mut upstream = match tokio::time::timeout(DIAL_TIMEOUT, dial).await {
            Ok(Ok(s)) => s,
            Ok(Err(_)) => {
                conn.emit("allow", "phase1-allow-all", Some("dial"));
                refuse(&mut stream, 502, "Bad Gateway", "connect failed").await;
                return None;
            }
            Err(_) => {
                conn.emit("allow", "phase1-allow-all", Some("connect timeout"));
                refuse(&mut stream, 504, "Gateway Timeout", "connect timed out").await;
                return None;
            }
        };

        if stream.write_all(b"HTTP/1.1 200 Connection Established\r\n\r\n").await.is_err()
            || stream.flush().await.is_err()
        {
            conn.emit("allow", "phase1-allow-all", Some("client closed"));
            return None;
        }
        // Clients may send the TLS ClientHello with the CONNECT.
        if !early.is_empty() {
            if !matches!(tokio::time::timeout(DIAL_TIMEOUT, upstream.write_all(early)).await, Ok(Ok(()))) {
                conn.emit("allow", "phase1-allow-all", Some("upstream closed"));
                return None;
            }
            conn.up.fetch_add(early.len() as u64, Ordering::Relaxed);
        }
        let mut client = Counting { inner: stream, written: Arc::clone(&conn.down) };
        let mut server = Counting { inner: upstream, written: Arc::clone(&conn.up) };
        let mut shutdown = shutdown.clone();
        let error = tokio::select! {
            r = tokio::io::copy_bidirectional(&mut client, &mut server) => r.err().map(|e| e.kind().to_string()),
            _ = shutdown.changed() => Some("shutdown".to_owned()),
        };
        conn.emit("allow", "phase1-allow-all", error.as_deref());
        None
    }
}

/// Shuts down on stdin EOF (a crashed or killed daemon takes its proxies with it) or SIGINT,
/// and gracefully on SIGTERM.
struct Shutdown;

#[async_trait]
impl ShutdownSignalWatch for Shutdown {
    async fn recv(&self) -> ShutdownSignal {
        use tokio::signal::unix::{SignalKind, signal};
        let stdin_eof = async {
            let mut sink = [0u8; 64];
            let mut stdin = tokio::io::stdin();
            while matches!(stdin.read(&mut sink).await, Ok(n) if n > 0) {}
        };
        let mut sigint = signal(SignalKind::interrupt()).expect("sigint handler");
        let mut sigterm = signal(SignalKind::terminate()).expect("sigterm handler");
        let signal = tokio::select! {
            () = stdin_eof => ShutdownSignal::FastShutdown,
            _ = sigint.recv() => ShutdownSignal::FastShutdown,
            _ = sigterm.recv() => ShutdownSignal::GracefulTerminate,
        };
        signal
    }
}

/// Splices `session.sock` to the daemon. Nothing is dialed until the first bytes arrive, so a
/// readiness probe (connect and close) costs the daemon nothing. Either direction ending ends
/// the splice: HTTP/2 closes both ways when it is done.
struct SpliceApp {
    daemon_sock: PathBuf,
    session: String,
    secret: String,
}

/// How long one direction of the splice may keep going after the other has ended.
const DRAIN: Duration = Duration::from_secs(5);

#[async_trait]
impl ServerApp for SpliceApp {
    async fn process_new(self: &Arc<Self>, mut stream: Stream, _shutdown: &ShutdownWatch) -> Option<Stream> {
        let _slot = LIMIT.acquire().await.ok()?;
        let mut first = [0u8; 4096];
        let n = tokio::time::timeout(HEAD_TIMEOUT, stream.read(&mut first)).await.ok()?.ok().filter(|n| *n > 0)?;
        let mut daemon = UnixStream::connect(&self.daemon_sock).await.ok()?;
        daemon.write_all(format!("SESSION {} {}\n", self.session, self.secret).as_bytes()).await.ok()?;
        daemon.write_all(&first[..n]).await.ok()?;
        let (mut from_daemon, mut to_daemon) = daemon.split();
        let (mut from_container, mut to_container) = tokio::io::split(&mut stream);
        // Each direction passes its end-of-stream on, and an end on one side gives the other a
        // bounded time to finish, so a reply that is still in flight is not cut off.
        let up = async {
            let _ = tokio::io::copy(&mut from_container, &mut to_daemon).await;
            let _ = to_daemon.shutdown().await;
        };
        let down = async {
            let _ = tokio::io::copy(&mut from_daemon, &mut to_container).await;
            let _ = to_container.shutdown().await;
        };
        tokio::pin!(up, down);
        tokio::select! {
            () = &mut up => { let _ = tokio::time::timeout(DRAIN, &mut down).await; }
            () = &mut down => { let _ = tokio::time::timeout(DRAIN, &mut up).await; }
        }
        None
    }
}

fn main() {
    let args = Args::parse();
    SESSION.lock().expect("session lock").clone_from(&args.session);
    if let Err(err) = events::connect(&args.daemon_sock, &args.session, &args.secret) {
        eprintln!("cannot reach the daemon at {}: {err}", args.daemon_sock.display());
        std::process::exit(1);
    }
    let conf = ServerConf { threads: 1, ..ServerConf::default() };
    let mut server = Server::new_with_opt_and_conf(None, conf);
    let private = || Some(std::fs::Permissions::from_mode(0o600));
    let mut proxy = Service::new("proxy".into(), ConnectApp);
    proxy.add_uds(&args.dir.join("proxy.sock").to_string_lossy(), private());
    let mut session = Service::new(
        "session".into(),
        SpliceApp { daemon_sock: args.daemon_sock, session: args.session, secret: args.secret },
    );
    session.add_uds(&args.dir.join("session.sock").to_string_lossy(), private());
    server.add_service(proxy);
    server.add_service(session);
    server.bootstrap();
    server.run(RunArgs { shutdown_signal: Box::new(Shutdown) });
    // Connections still open when the server stopped are reported, with what moved so far.
    let open: Vec<_> = OPEN.lock().expect("open lock").values().cloned().collect();
    for conn in open {
        conn.emit("allow", "phase1-allow-all", Some("shutdown"));
    }
    // Every event is on the daemon's socket before the process exits.
    events::close();
}
