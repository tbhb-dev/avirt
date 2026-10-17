//! `avirthostd`: serves `/launch` and `/actors/:id/stream` over h2c, starts one container
//! per actor and writes each actor's NDJSON log.

use std::collections::{HashMap, VecDeque};
use std::convert::Infallible;
use std::ffi::OsString;
use std::fs::{File, OpenOptions};
use std::io::Write;
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::os::unix::process::ExitStatusExt;
use std::path::{Path, PathBuf};
use std::process::{ExitStatus, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use anyhow::{Result, bail};
use bytes::Bytes;
use futures_util::StreamExt;
use http_body_util::{BodyExt, Full, Limited, StreamBody};
use hyper::body::{Frame, Incoming};
use hyper::service::service_fn;
use hyper::{Method, Request, Response, StatusCode};
use hyper_util::rt::{TokioExecutor, TokioIo};
use serde_json::{Value, json};
use tokio::io::AsyncReadExt;
use tokio::net::{TcpListener, UnixListener, UnixStream};
use tokio::process::{Child, Command};
use tokio::signal::unix::{SignalKind, signal};
use tokio_stream::wrappers::UnboundedReceiverStream;
use tokio_util::sync::CancellationToken;
use tokio_util::task::TaskTracker;
use tokio_util::task::task_tracker::TaskTrackerToken;

use crate::api::{HEALTH_PATH, LAUNCH_PATH, LaunchRequest, LaunchResponse};
use crate::egress::{ActorDir, Proxy};
use crate::envelope::{Dir, Envelope, Item, LineSplitter};
use crate::h2c::Body;
use crate::state::{CONTAINER_LABEL, StateRoot, WORKLOAD_LABEL, private_dir};

/// A launch avirthostd refuses for a reason the caller can fix (missing credentials, say);
/// reported as a 400 with this message. Any other `prepare` error is a 500.
#[derive(Debug)]
pub struct Refused(pub String);

impl std::fmt::Display for Refused {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for Refused {}

/// Returned by `launch` while avirthostd is shutting down; reported as a 503.
#[derive(Debug)]
struct ShuttingDown;

impl std::fmt::Display for ShuttingDown {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("avirthostd is shutting down")
    }
}

impl std::error::Error for ShuttingDown {}

/// What differs between agents: the image and the per-launch container setup.
pub trait Profile: Send + Sync {
    fn image(&self) -> &str;

    /// Per-launch setup: extra `container run` arguments plus the cleanup to run once the
    /// container is gone. Dropping the returned [`Prepared`] without launching runs the cleanup.
    ///
    /// `scratch` is a directory under avirthostd's state root for per-launch temp files, so that
    /// an avirthostd process killed uncleanly leaves them where the next one sweeps them.
    fn prepare(&self, scratch: &Path) -> Result<Prepared>;
}

pub struct Prepared {
    pub args: Vec<OsString>,
    /// Names of avirthostd environment variables to hand to `container run` (the `-e NAME` args
    /// above read them from there). Nothing else of avirthostd's environment is inherited.
    pub inherit: Vec<&'static str>,
    pub cleanup: Cleanup,
}

/// A one-shot cleanup that runs exactly once: explicitly through [`Cleanup::run`] or, failing
/// that, when dropped (setup failure partway, avirthostd unwinding).
pub struct Cleanup(Option<Box<dyn FnOnce() -> Option<Value> + Send>>);

impl Cleanup {
    pub fn new(f: impl FnOnce() -> Option<Value> + Send + 'static) -> Self {
        Self(Some(Box::new(f)))
    }

    #[must_use]
    pub fn none() -> Self {
        Self(None)
    }

    /// Gives up on the cleanup without running it, and without dropping what it owns, so a
    /// temp directory it holds stays on disk for the next startup's sweep. Used when the
    /// container could not be confirmed gone: a write-back then could race a live agent.
    pub fn abandon(&mut self) {
        if let Some(f) = self.0.take() {
            std::mem::forget(f);
        }
    }

    /// Runs the cleanup; the returned meta message, if any, belongs in the actor log.
    pub fn run(&mut self) -> Option<Value> {
        self.0.take().and_then(|f| f())
    }
}

impl Drop for Cleanup {
    fn drop(&mut self) {
        self.run();
    }
}

#[derive(Debug)]
pub struct Config {
    pub port: u16,
    pub logs_dir: PathBuf,
    /// The `avirtmediatord` binary started for each actor.
    pub mediator_bin: PathBuf,
}

enum Entry {
    Live(Arc<Actor>),
    /// Exit note written and stream drained; kept so a late stream POST gets 410, not 404.
    Exited,
}

struct Actor {
    /// Presented by avirtinitd on its stream POST. The runtime's vsock relays can be dialed
    /// by anything in the container, so the sockets' permissions alone do not prove who is on
    /// the other end; this secret is only ever in avirtinitd's environment.
    token: String,
    /// Presented by this actor's mediator in its connection preambles. It is passed to the
    /// mediator's environment only, never to the container.
    mediator_secret: String,
    /// Set by the first (and only) accepted event connection.
    egress_taken: AtomicBool,
    log: Mutex<File>,
    state: Mutex<State>,
    /// Cancelled when the mediator's event connection has closed: every egress line is in the
    /// log, so the exit notes may follow.
    egress_eof: CancellationToken,
}

#[derive(Default)]
struct State {
    streamed: bool,
    streaming: bool,
    exited: bool,
    /// Errors, the cleanup result (`auth_writeback`) and finally `container_exit`, all held back
    /// until the stream has drained so they follow every streamed line, in that order.
    /// `container_exit` is always the last event of an actor.
    notes: Vec<Value>,
    cleanup_note: Option<Value>,
    exit_note: Option<Value>,
}

impl Actor {
    fn write(&self, line: &str) {
        let mut log = self.log.lock().expect("log lock");
        if let Err(err) = log.write_all(line.as_bytes()) {
            eprintln!("log write failed: {err}");
        }
    }

    /// Holds a note until the container has exited and the stream has drained.
    fn hold(&self, msg: Value) {
        self.state.lock().expect("state lock").notes.push(msg);
    }

    fn note(&self, msg: Value) {
        self.write(&Envelope::new(Dir::Meta, msg).to_line());
    }

    /// Writes the held exit note once the container has exited and the stream is done.
    /// Returns true if this call completed the actor.
    fn flush_exit(&self, state: &mut State) -> bool {
        if state.exited && !state.streaming {
            let notes = std::mem::take(&mut state.notes);
            for note in notes.into_iter().chain(state.cleanup_note.take()).chain(state.exit_note.take()) {
                self.note(note);
            }
            return true;
        }
        false
    }
}

/// What `command` needs to know about one launch.
#[derive(Clone, Copy)]
struct Launch<'a> {
    id: &'a str,
    profile: &'a dyn Profile,
    proxy_socket: &'a Path,
    uplink_socket: &'a Path,
    token: &'a str,
    prompt: &'a str,
    cwd: Option<&'a Path>,
}

struct Hostd {
    profiles: HashMap<String, Box<dyn Profile>>,
    config: Config,
    /// The socket mediators dial (in the state root's `hostd/` directory).
    hostd_sock: PathBuf,
    state: StateRoot,
    actors: Mutex<HashMap<String, Entry>>,
    tombstones: Mutex<VecDeque<String>>,
    tracker: TaskTracker,
    shutdown: CancellationToken,
}

/// Runs avirthostd until SIGINT or SIGTERM, then stops the running containers, waits for their
/// cleanup and exits.
#[allow(clippy::implicit_hasher)]
pub async fn serve(profiles: HashMap<String, Box<dyn Profile>>, config: Config) -> Result<()> {
    let logs_dir = std::path::absolute(&config.logs_dir)?;
    private_dir(&logs_dir)?;
    let config = Config { logs_dir, ..config };

    // One avirthostd per user. Holding the state root's lock lets this one sweep whatever a previous
    // avirthostd left behind without racing a live one.
    let state = StateRoot::open()?;
    let swept = state.sweep();
    if !swept.containers.is_empty() || swept.dirs > 0 {
        println!("swept leftovers of a previous run: containers {:?}, {} directories", swept.containers, swept.dirs);
    }
    for failure in &swept.failures {
        eprintln!("sweep could not remove {failure}");
    }

    // The control API (`/launch`) is for the CLI on this machine only. Containers have no
    // network and cannot reach it; an actor's mediator dials avirthostd's own socket instead.
    let listener = TcpListener::bind(("127.0.0.1", config.port)).await?;
    let hostd_sock = state.hostd().join("hostd.sock");
    let mediators = UnixListener::bind(&hostd_sock)?;
    std::fs::set_permissions(&hostd_sock, std::fs::Permissions::from_mode(0o600))?;
    println!("avirthostd listening on 127.0.0.1:{}; logs in {}", config.port, config.logs_dir.display());
    let hostd = Arc::new(Hostd {
        profiles,
        config,
        hostd_sock,
        state,
        actors: Mutex::default(),
        tombstones: Mutex::default(),
        tracker: TaskTracker::new(),
        shutdown: CancellationToken::new(),
    });

    let mut sigint = signal(SignalKind::interrupt())?;
    let mut sigterm = signal(SignalKind::terminate())?;
    let accept = tokio::spawn(accept_loop(Arc::clone(&hostd), listener));
    let accept_mediators = tokio::spawn(mediator_loop(Arc::clone(&hostd), mediators));
    tokio::select! { _ = sigint.recv() => {}, _ = sigterm.recv() => {} }

    println!("shutting down: stopping containers");
    hostd.shutdown.cancel();
    hostd.tracker.close();
    // Keep serving so the stopped containers' streams drain into their logs.
    if tokio::time::timeout(Duration::from_mins(1), hostd.tracker.wait()).await.is_err() {
        eprintln!("timed out waiting for containers to stop");
    }
    accept.abort();
    accept_mediators.abort();
    Ok(())
}

async fn accept_loop(hostd: Arc<Hostd>, listener: TcpListener) {
    loop {
        let Ok((stream, _)) = listener.accept().await else {
            // E.g. out of file descriptors: back off instead of spinning.
            tokio::time::sleep(Duration::from_millis(100)).await;
            continue;
        };
        let hostd = Arc::clone(&hostd);
        tokio::spawn(async move {
            let service = service_fn(move |req| handle_control(Arc::clone(&hostd), req));
            let conn = hyper::server::conn::http2::Builder::new(TokioExecutor::new())
                .serve_connection(TokioIo::new(stream), service);
            if let Err(err) = conn.await {
                eprintln!("h2 connection error: {err}");
            }
        });
    }
}

fn reply(status: StatusCode, body: &Value) -> Response<Body> {
    let mut response = Response::new(Full::new(Bytes::from(body.to_string())).boxed());
    *response.status_mut() = status;
    response.headers_mut().insert("content-type", "application/json".parse().expect("static header"));
    response
}

fn error(status: StatusCode, message: &str) -> Response<Body> {
    reply(status, &json!({ "error": message }))
}

/// The control API, served on the loopback port for the CLI.
async fn handle_control(hostd: Arc<Hostd>, req: Request<Incoming>) -> Result<Response<Body>, Infallible> {
    Ok(match (req.method(), req.uri().path()) {
        (&Method::POST, LAUNCH_PATH) => launch_route(&hostd, req).await,
        (&Method::POST, HEALTH_PATH) => reply(StatusCode::OK, &json!({ "ok": true })),
        _ => error(StatusCode::NOT_FOUND, "not found"),
    })
}

/// What an actor's uplink connection may ask for: that actor's own stream and nothing else,
/// so a container cannot write into another actor's log or launch anything.
fn handle_actor(hostd: &Arc<Hostd>, id: &str, req: Request<Incoming>) -> Response<Body> {
    match (req.method(), req.uri().path()) {
        (&Method::POST, p) if p == format!("/actors/{id}/stream") => stream_route(hostd, id, req),
        _ => error(StatusCode::NOT_FOUND, "not found"),
    }
}

/// Upper bound on a `/launch` request body.
const MAX_LAUNCH_BODY: usize = 64 * 1024;

async fn launch_route(hostd: &Arc<Hostd>, req: Request<Incoming>) -> Response<Body> {
    // An aborted or oversized request body surfaces as an error here, so no handler is left
    // pending and nothing unbounded is buffered.
    let Ok(body) = Limited::new(req.into_body(), MAX_LAUNCH_BODY).collect().await else {
        return error(StatusCode::BAD_REQUEST, "request body aborted or too large");
    };
    let Ok(body) = serde_json::from_slice::<LaunchRequest>(&body.to_bytes()) else {
        return error(StatusCode::BAD_REQUEST, "body must be JSON");
    };
    let agent = body.agent.as_str();
    if !hostd.profiles.contains_key(agent) {
        let mut names: Vec<_> = hostd.profiles.keys().map(String::as_str).collect();
        names.sort_unstable();
        return error(StatusCode::BAD_REQUEST, &format!("agent must be one of: {}", names.join(", ")));
    }
    if body.prompt.is_empty() {
        return error(StatusCode::BAD_REQUEST, "prompt is required");
    }
    let prompt = body.prompt.as_str();
    let cwd = match &body.cwd {
        None => None,
        // `:` would end the host side of the `-v host:container` mount spec.
        Some(c) if c.starts_with('/') && !c.contains(':') => Some(PathBuf::from(c)),
        Some(_) => return error(StatusCode::BAD_REQUEST, "cwd must be an absolute path without ':'"),
    };
    let launched = hostd.launch(agent, prompt, cwd.as_deref()).await;
    match launched {
        Ok(id) => {
            let log_path = hostd.log_path(&id);
            let response = LaunchResponse { actor_id: id, log_path: log_path.to_string_lossy().into_owned() };
            reply(StatusCode::OK, &serde_json::to_value(response).expect("serializable"))
        }
        Err(err) if err.downcast_ref::<Refused>().is_some() => error(StatusCode::BAD_REQUEST, &err.to_string()),
        Err(err) if err.downcast_ref::<ShuttingDown>().is_some() => {
            error(StatusCode::SERVICE_UNAVAILABLE, "avirthostd is shutting down")
        }
        Err(err) => {
            eprintln!("launch failed: {err:#}");
            error(StatusCode::INTERNAL_SERVER_ERROR, "launch failed; see avirthostd's output")
        }
    }
}

fn stream_route(hostd: &Arc<Hostd>, id: &str, req: Request<Incoming>) -> Response<Body> {
    let actor = match hostd.actors.lock().expect("actors lock").get(id) {
        None => return error(StatusCode::NOT_FOUND, "unknown actor"),
        Some(Entry::Exited) => return error(StatusCode::GONE, "actor already exited"),
        Some(Entry::Live(actor)) => Arc::clone(actor),
    };
    let presented = req.headers().get("x-avirt-token").and_then(|v| v.to_str().ok()).unwrap_or_default();
    if !constant_time_eq(presented.as_bytes(), actor.token.as_bytes()) {
        return error(StatusCode::FORBIDDEN, "bad actor token");
    }
    {
        let mut state = actor.state.lock().expect("state lock");
        if state.streamed {
            return error(StatusCode::CONFLICT, "actor already streamed");
        }
        if state.exited {
            return error(StatusCode::GONE, "actor already exited");
        }
        state.streamed = true;
        state.streaming = true;
    }
    // Keeps shutdown waiting until this stream has drained into the log.
    let drain = hostd.tracker.token();
    let hostd = Arc::clone(hostd);
    let id = id.to_owned();
    let (done_tx, done_rx) = tokio::sync::mpsc::unbounded_channel::<Bytes>();

    // Response headers go out now, so avirtinitd learns at once that the stream was
    // accepted; the body (`{"lines": N}`) follows once the request body has drained.
    tokio::spawn(async move {
        let _drain = drain;
        let mut body = req.into_body();
        let mut splitter = LineSplitter::default();
        let mut lines = 0usize;
        let mut emit = |item: Item| match item {
            Item::Line(line) => {
                actor.write(&(line + "\n"));
                lines += 1;
            }
            // Written by avirthostd, so an agent cannot make it look like anything else.
            Item::Truncated(bytes) => actor.note(json!({ "event": "line_truncated", "bytes": bytes })),
        };
        // An aborted stream (RST_STREAM, killed container) ends in an error rather than a clean
        // end; either way the trailing partial line is flushed below.
        while let Some(Ok(frame)) = body.frame().await {
            if let Ok(data) = frame.into_data() {
                splitter.push(&data).into_iter().for_each(&mut emit);
            }
        }
        splitter.finish().into_iter().for_each(&mut emit);

        let completed = {
            let mut state = actor.state.lock().expect("state lock");
            state.streaming = false;
            actor.flush_exit(&mut state)
        };
        if completed {
            hostd.mark_exited(&id);
        }
        let _ = done_tx.send(Bytes::from(json!({ "lines": lines }).to_string()));
    });

    let frames = UnboundedReceiverStream::new(done_rx).map(|b| Ok::<_, Infallible>(Frame::data(b)));
    let mut response = Response::new(BodyExt::boxed(StreamBody::new(frames)));
    response.headers_mut().insert("content-type", "application/json".parse().expect("static header"));
    response
}

impl Hostd {
    fn log_path(&self, id: &str) -> PathBuf {
        self.config.logs_dir.join(format!("{id}.ndjson"))
    }

    fn create_private(path: &Path) -> std::io::Result<File> {
        OpenOptions::new().create_new(true).append(true).mode(0o600).open(path)
    }

    async fn launch(self: &Arc<Self>, agent: &str, prompt: &str, cwd: Option<&Path>) -> Result<String> {
        // Take the token before checking for shutdown: `serve` cancels, then closes the tracker,
        // so a launch that passes the check is always waited for.
        let token = self.tracker.token();
        if self.shutdown.is_cancelled() {
            return Err(ShuttingDown.into());
        }
        if cwd.is_some_and(|c| !c.is_dir()) {
            return Err(Refused("cwd must be an existing directory on the host".into()).into());
        }
        if !self.profiles.contains_key(agent) {
            bail!("unknown agent {agent}");
        }
        // Credential and file work blocks, so it runs off the async workers. If anything below
        // fails, dropping `prepared` (also off the workers) runs its cleanup.
        let prepared = {
            let (hostd, agent) = (Arc::clone(self), agent.to_owned());
            tokio::task::spawn_blocking(move || hostd.profiles[&agent].prepare(&hostd.state.scratch())).await??
        };
        let mut prepared = Some(prepared);
        let result = self.start(agent, prompt, cwd, &mut prepared, token).await;
        if let Some(unused) = prepared {
            let _ = tokio::task::spawn_blocking(move || drop(unused)).await;
        }
        result
    }

    /// Everything after `prepare`: log, network, proxy, container, mediator. On error it
    /// unwinds what it created and leaves `prepared` for the caller to discard.
    #[allow(clippy::too_many_lines)]
    async fn start(
        self: &Arc<Self>,
        agent: &str,
        prompt: &str,
        cwd: Option<&Path>,
        prepared: &mut Option<Prepared>,
        token: TaskTrackerToken,
    ) -> Result<String> {
        let (id, actor) = self.register()?;
        let uplink_token = actor.token.clone();
        let dir = match ActorDir::create(&self.state.actors(), &id) {
            Ok(dir) => dir,
            Err(err) => {
                self.forget(&id);
                return Err(err);
            }
        };
        let mediator_log = self.config.logs_dir.join(format!("{id}.mediator.log"));
        let mut proxy = match Proxy::spawn(
            &self.config.mediator_bin,
            &id,
            &dir,
            &self.hostd_sock,
            &actor.mediator_secret,
            &mediator_log,
        )
        .await
        {
            Ok(proxy) => proxy,
            Err(err) => {
                self.forget(&id);
                return Err(err);
            }
        };
        let profile = &self.profiles[agent];
        let (proxy_socket, uplink_socket) = (dir.proxy_socket(), dir.uplink_socket());
        let launch = Launch {
            id: &id,
            profile: profile.as_ref(),
            proxy_socket: &proxy_socket,
            uplink_socket: &uplink_socket,
            token: &uplink_token,
            prompt,
            cwd,
        };
        let (extra_args, inherit) = {
            let p = prepared.as_ref().expect("prepared");
            (p.args.clone(), p.inherit.clone())
        };
        let mut cmd = match self.command(&launch, &extra_args, &inherit) {
            Ok(cmd) => cmd,
            Err(err) => {
                proxy.stop().await;
                self.forget(&id);
                return Err(err.into());
            }
        };
        let mut cleanup = prepared.take().expect("prepared").cleanup;
        let child = cmd.spawn();
        println!("launched {id}");
        let hostd = Arc::clone(self);
        let task_id = id.clone();
        self.tracker.spawn(async move {
            let _token = token;
            let mut gone = true;
            let status = match child {
                Ok(mut child) => {
                    let status = tokio::select! {
                        s = child.wait() => s,
                        died = proxy.exited() => {
                            // Without its mediator the container has no uplink and no proxy.
                            let how = died.map_or_else(|e| e.to_string(), |s| s.to_string());
                            actor.hold(json!({
                                "event": "container_error",
                                "message": format!("the actor mediator exited unexpectedly ({how}); stopping the container"),
                            }));
                            let (status, confirmed) = stop_and_wait(&task_id, &mut child).await;
                            gone = confirmed;
                            status
                        }
                        () = hostd.shutdown.cancelled() => {
                            let (status, confirmed) = stop_and_wait(&task_id, &mut child).await;
                            gone = confirmed;
                            status
                        }
                    };
                    status.map(|s| json!({ "event": "container_exit", "code": s.code(), "signal": s.signal() }))
                }
                Err(err) => Err(err),
            };
            // While the mediator still splices: give a stream the container opened just before
            // dying time to be routed (it would be refused with 410 otherwise), and an open stream
            // time to drain into the log.
            if !actor.state.lock().expect("state lock").streamed {
                tokio::time::sleep(Duration::from_secs(1)).await;
            }
            for _ in 0..200 {
                if !actor.state.lock().expect("state lock").streaming {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
            // Then stop the mediator, which flushes its last events to avirthostd, and wait for
            // them to reach the log before the exit notes follow.
            proxy.stop().await;
            let _ = tokio::time::timeout(Duration::from_secs(5), actor.egress_eof.cancelled()).await;
            let cleanup_note = if gone {
                // Cleanup may block on a file lock.
                tokio::task::spawn_blocking(move || cleanup.run()).await.ok().flatten()
            } else {
                // A write-back could race an agent that is still running; leave the temp
                // directory for the next startup's sweep.
                cleanup.abandon();
                actor.hold(json!({
                    "event": "container_error",
                    "message": "the container could not be confirmed gone; cleanup and write-back were skipped",
                }));
                None
            };
            actor.state.lock().expect("state lock").cleanup_note = cleanup_note;
            hostd.finish(&task_id, &actor, status);
            // Removes the actor directory with both sockets in it, last.
            drop(dir);
        });
        Ok(id)
    }

    fn command(&self, launch: &Launch<'_>, extra_args: &[OsString], inherit: &[&str]) -> std::io::Result<Command> {
        let Launch { id, profile, proxy_socket, uplink_socket, token, prompt, cwd } = *launch;
        let container_log = Self::create_private(&self.config.logs_dir.join(format!("{id}.container.log")))?;
        let mut cmd = Command::new("container");
        // Start from nothing: avirthostd's environment holds credentials for every agent, and a
        // launch gets only the variables its profile names plus what the CLI itself needs.
        cmd.env_clear();
        for name in ["HOME", "PATH", "TMPDIR", "USER", "LOGNAME"].iter().chain(inherit) {
            if let Some(value) = std::env::var_os(name) {
                cmd.env(name, value);
            }
        }
        // No network at all. The runtime relays a mounted socket *file* to its host listener, so
        // the two mediator sockets, which must already exist, are the container's only way out.
        // (Mounting their directory instead would go over virtiofs, which cannot connect them.)
        let mount = |host: &Path, guest: &str| {
            let mut spec = OsString::from(host);
            spec.push(":");
            spec.push(guest);
            spec
        };
        // `--init` makes the runtime's init PID 1: it reaps orphans (avirtinitd would not) and
        // forwards `container stop` to avirtinitd as SIGTERM.
        cmd.args([
            "run",
            "--rm",
            "--init",
            "--name",
            &format!("avirt-{id}-workload"),
            "--label",
            CONTAINER_LABEL,
            "--label",
            WORKLOAD_LABEL,
            "--network",
            "none",
        ])
        .arg("-v")
        .arg(mount(proxy_socket, "/run/avirt/proxy.sock"))
        .arg("-v")
        .arg(mount(uplink_socket, "/run/avirt/uplink.sock"))
        .args(extra_args);
        if let Some(cwd) = cwd {
            cmd.arg("-v").arg(mount(cwd, "/workspace"));
        }
        // `-e NAME` with no value makes the container CLI read it from its own environment, so
        // the prompt never appears on argv.
        cmd.args(["-e", "AVIRT_PROMPT", "-e", "AVIRT_CWD", "-e", "AVIRT_UPLINK_TOKEN"])
            .arg("-e")
            .arg(format!("AVIRT_ACTOR_ID={id}"))
            .arg(profile.image())
            .env("AVIRT_PROMPT", prompt)
            .env("AVIRT_UPLINK_TOKEN", token)
            .env("AVIRT_CWD", if cwd.is_some() { "/workspace" } else { "" })
            .stdin(Stdio::null())
            .stdout(container_log.try_clone()?)
            .stderr(container_log)
            // Own process group, so a terminal Ctrl-C reaches avirthostd and not the CLI.
            .process_group(0);
        Ok(cmd)
    }

    /// Picks an unused id (redrawn on collision) and registers the actor with its log.
    fn register(&self) -> Result<(String, Arc<Actor>)> {
        let mut actors = self.actors.lock().expect("actors lock");
        // A fresh id that is also unused on disk (earlier runs leave logs behind).
        let (id, log) = loop {
            let id = format!("{:08x}", rand::random::<u32>());
            if actors.contains_key(&id) {
                continue;
            }
            match Self::create_private(&self.log_path(&id)) {
                Ok(log) => break (id, log),
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {}
                Err(e) => return Err(e.into()),
            }
        };
        let actor = Arc::new(Actor {
            token: format!("{:032x}", rand::random::<u128>()),
            mediator_secret: format!("{:032x}", rand::random::<u128>()),
            egress_taken: AtomicBool::new(false),
            log: Mutex::new(log),
            state: Mutex::default(),
            egress_eof: CancellationToken::new(),
        });
        actors.insert(id.clone(), Entry::Live(Arc::clone(&actor)));
        Ok((id, actor))
    }

    fn forget(&self, id: &str) {
        self.actors.lock().expect("actors lock").remove(id);
    }

    /// Records the container's exit. `container_error` is written at once; `container_exit` is
    /// held until the stream has drained (or written now if none was ever accepted).
    fn finish(&self, id: &str, actor: &Actor, status: std::io::Result<Value>) {
        let completed = {
            let mut state = actor.state.lock().expect("state lock");
            state.exited = true;
            match status {
                Ok(exit) => state.exit_note = Some(exit),
                Err(err) => {
                    state.notes.push(json!({ "event": "container_error", "message": err.to_string() }));
                    // Still the last event of the actor, so followers have one thing to wait for.
                    state.exit_note = Some(json!({ "event": "container_exit", "code": null, "signal": null }));
                }
            }
            actor.flush_exit(&mut state)
        };
        if completed {
            self.mark_exited(id);
        }
    }

    /// Replaces a finished actor with a tombstone (so a late stream gets 410), keeping at most
    /// `MAX_TOMBSTONES` of them.
    fn mark_exited(&self, id: &str) {
        let mut actors = self.actors.lock().expect("actors lock");
        actors.insert(id.to_owned(), Entry::Exited);
        let mut order = self.tombstones.lock().expect("tombstones lock");
        order.push_back(id.to_owned());
        while order.len() > MAX_TOMBSTONES {
            if let Some(old) = order.pop_front() {
                actors.remove(&old);
            }
        }
    }
}

/// How many finished actors are remembered for the 410 answer.
const MAX_TOMBSTONES: usize = 1024;

/// Runs a `container` subcommand with a time limit; true if it succeeded.
async fn container_cmd(args: &[&str], limit: Duration) -> bool {
    let run = Command::new("container")
        .args(args)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        // Own process group, so a second Ctrl-C does not kill it; dropped (killed) on timeout.
        .process_group(0)
        .kill_on_drop(true)
        .status();
    matches!(tokio::time::timeout(limit, run).await, Ok(Ok(status)) if status.success())
}

/// Stops the container and waits for the `container run` CLI to exit. A stop that arrives before
/// the container exists fails fast, so it is retried at a short interval. As a last resort the
/// container is killed and deleted by name before the CLI client is killed. The flag says whether
/// the container is confirmed gone; if not, the caller must not touch what the agent could still
/// be using.
async fn stop_and_wait(id: &str, child: &mut Child) -> (std::io::Result<ExitStatus>, bool) {
    let name = format!("avirt-{id}-workload");
    for _ in 0..40 {
        let stopped = container_cmd(&["stop", "--time", "5", &name], Duration::from_secs(15)).await;
        let grace = if stopped { Duration::from_secs(10) } else { Duration::from_millis(300) };
        if let Ok(status) = tokio::time::timeout(grace, child.wait()).await {
            return (status, true);
        }
    }
    container_cmd(&["kill", &name], Duration::from_secs(10)).await;
    container_cmd(&["delete", "--force", &name], Duration::from_secs(10)).await;
    let _ = child.start_kill();
    let status = match tokio::time::timeout(Duration::from_secs(5), child.wait()).await {
        Ok(status) => status,
        Err(_) => Err(std::io::Error::other("the container CLI did not exit")),
    };
    // `inspect` fails once the container no longer exists.
    let gone = !container_cmd(&["inspect", &name], Duration::from_secs(10)).await;
    (status, gone)
}

/// Accepts mediator connections on avirthostd's socket. Each starts with one line naming its
/// kind and the actor it belongs to (the mediator was started for that actor, so the
/// container cannot pick another): `ACTOR <id>` is followed by the actor's HTTP/2 stream,
/// spliced through from avirtinitd, and `EGRESS <id>` by NDJSON proxy events.
async fn mediator_loop(hostd: Arc<Hostd>, listener: UnixListener) {
    loop {
        let Ok((conn, _)) = listener.accept().await else {
            tokio::time::sleep(Duration::from_millis(100)).await;
            continue;
        };
        tokio::spawn(mediator_connection(Arc::clone(&hostd), conn));
    }
}

async fn mediator_connection(hostd: Arc<Hostd>, mut conn: UnixStream) {
    let Some(preamble) = read_preamble(&mut conn).await else { return };
    let mut parts = preamble.split(' ');
    let (Some(kind), Some(id), Some(secret), None) = (parts.next(), parts.next(), parts.next(), parts.next()) else {
        return;
    };
    let actor = match hostd.actors.lock().expect("actors lock").get(id) {
        Some(Entry::Live(actor)) => Arc::clone(actor),
        _ => return,
    };
    // Only this actor's mediator knows the secret; a same-user process that merely knows
    // the actor id cannot inject events or end the event stream early.
    if !constant_time_eq(secret.as_bytes(), actor.mediator_secret.as_bytes()) {
        return;
    }
    match kind {
        "ACTOR" => {
            let id = Arc::new(id.to_owned());
            let service = service_fn(move |req| {
                let (hostd, id) = (Arc::clone(&hostd), Arc::clone(&id));
                async move { Ok::<_, Infallible>(handle_actor(&hostd, &id, req)) }
            });
            let conn = hyper::server::conn::http2::Builder::new(TokioExecutor::new())
                .serve_connection(TokioIo::new(conn), service);
            if let Err(err) = conn.await {
                eprintln!("h2 connection error: {err}");
            }
        }
        "EGRESS" => {
            if actor.egress_taken.swap(true, Ordering::SeqCst) {
                return;
            }
            let mut splitter = LineSplitter::default();
            let mut buf = vec![0u8; 16 * 1024];
            let mut emit = |item: Item| match item {
                Item::Line(line) => {
                    let event = serde_json::from_str(&line).unwrap_or_else(|_| json!({ "raw": line }));
                    actor.write(&Envelope::new(Dir::Egress, event).to_line());
                }
                Item::Truncated(bytes) => actor.note(json!({ "event": "line_truncated", "bytes": bytes })),
            };
            while let Ok(n) = conn.read(&mut buf).await {
                if n == 0 {
                    break;
                }
                splitter.push(&buf[..n]).into_iter().for_each(&mut emit);
            }
            splitter.finish().into_iter().for_each(&mut emit);
            actor.egress_eof.cancel();
        }
        _ => {}
    }
}

/// Reads the preamble line one byte at a time, so nothing after it is consumed.
async fn read_preamble(conn: &mut UnixStream) -> Option<String> {
    let read = async {
        let mut line = Vec::new();
        loop {
            let byte = conn.read_u8().await.ok()?;
            if byte == b'\n' {
                return String::from_utf8(line).ok();
            }
            line.push(byte);
            if line.len() > 128 {
                return None;
            }
        }
    };
    tokio::time::timeout(Duration::from_secs(5), read).await.ok().flatten()
}

/// Compares two byte strings without stopping at the first difference. `black_box` keeps the
/// optimizer from turning the loop back into an early-exit comparison.
fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    a.len() == b.len() && std::hint::black_box(a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y))) == 0
}
