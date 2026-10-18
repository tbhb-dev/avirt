//! `avirthostd`: serves `/launch` and `/workloads/:id/stream` over h2c, starts one container
//! per workload and writes each workload's NDJSON log.

mod container;
mod mediator;
pub mod profile;
mod routes;

use crate::console;
use crate::egress::{Proxy, ProxySpec, WorkloadDir};
use crate::envelope::{Dir, Envelope, Item, LineSplitter};
use crate::state::{self, CONTAINER_LABEL, WORKLOAD_LABEL, private_dir};
use alloc::collections::VecDeque;
use alloc::sync::Arc;
use anyhow::{Result, anyhow, bail};
use bytes::Bytes;
use container::{container_name, first_signal, mount, run_cleanup, settle_stream, watch_container};
use core::error::Error;
use core::fmt;
use core::hash::BuildHasher;
use core::mem;
use core::sync::atomic::AtomicBool;
use core::time::Duration;
use http_body_util::BodyExt as _;
use hyper::StatusCode;
use hyper::body::Incoming;
use hyper::server::conn::http2;
use hyper::service::service_fn;
use hyper_util::rt::{TokioExecutor, TokioIo};
use mediator::mediator_loop;
use profile::{Cleanup, Config, Prepared, Profile, Refused};
use routes::handle_control;
use serde_json::{Value, json};
use std::collections::HashMap;
use std::collections::hash_map::RandomState;
use std::env;
use std::ffi::OsString;
use std::fs::{self, File, OpenOptions, Permissions};
use std::io::{self, Write as _};
use std::os::unix::fs::{OpenOptionsExt as _, PermissionsExt as _};
use std::path::{self, Path, PathBuf};
use std::process::Stdio;
use std::sync::{Mutex, MutexGuard, PoisonError};
use tokio::net::{TcpListener, UnixListener};
use tokio::process::{Child, Command};
use tokio::signal::unix::{SignalKind, signal};
use tokio::sync::mpsc::UnboundedSender;
use tokio::task;
use tokio::time;
use tokio_util::sync::CancellationToken;
use tokio_util::task::TaskTracker;
use tokio_util::task::task_tracker::TaskTrackerToken;

/// How many finished workloads avirthostd remembers for the 410 answer.
const MAX_TOMBSTONES: usize = 1024;

/// The agent profiles by name.
type ProfileMap<S = RandomState> = HashMap<String, Box<dyn Profile>, S>;

/// A workload's id and its shared state, as returned by `register`.
type Registered = (String, Arc<Workload>);

/// The status and error message for a request the router refuses.
type Rejection = (StatusCode, &'static str);

/// Where a workload stands in the registry.
enum Entry {
    /// The log has the exit note and the stream has drained. Keep this state so a late stream POST gets 410, not 404.
    Exited,
    /// The workload is running, or its stream hasn't finished draining.
    Live(Arc<Workload>),
}

/// Whether the workload's stream POST has arrived and finished.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum StreamPhase {
    /// The stream ended.
    Drained,
    /// The stream is open.
    Streaming,
    /// No stream has arrived.
    #[default]
    Unclaimed,
}

/// What the launch route needs from one request after validation.
#[derive(Clone, Copy)]
struct LaunchArgs<'args> {
    /// The agent profile's name.
    agent: &'args str,
    /// A host directory to mount at `/workspace`.
    cwd: Option<&'args Path>,
    /// The prompt sent to the agent.
    prompt: &'args str,
}

/// What `command` needs to know about one launch.
#[derive(Clone, Copy)]
struct Launch<'launch> {
    /// A host directory to mount at `/workspace`.
    cwd: Option<&'launch Path>,
    /// The workload id.
    id: &'launch str,
    /// The mediator's workload connection socket.
    mediator_socket: &'launch Path,
    /// The agent's profile.
    profile: &'launch dyn Profile,
    /// The prompt sent to the agent.
    prompt: &'launch str,
    /// The mediator's CONNECT proxy socket.
    proxy_socket: &'launch Path,
    /// The token avirtinitd presents on its stream POST.
    token: &'launch str,
}

/// The mutable state of one workload.
#[derive(Default)]
struct State {
    /// The cleanup result (`auth_writeback`), held back with the other notes.
    cleanup_note: Option<Value>,
    /// The `container_exit` event, written last.
    exit_note: Option<Value>,
    /// Whether the container has exited.
    exited: bool,
    /// Errors, the cleanup result (`auth_writeback`) and finally `container_exit`, all held back
    /// until the stream has drained so they follow every streamed line, in that order.
    /// `container_exit` is always the last event of a workload.
    notes: Vec<Value>,
    /// Whether the stream POST has arrived, and whether it has finished.
    stream: StreamPhase,
}

/// One workload's credentials, log, and state.
struct Workload {
    /// Cancelled when the mediator's event connection has closed: every egress line is in the
    /// log, so the exit notes may follow.
    egress_eof: CancellationToken,
    /// Set by the first (and only) accepted event connection.
    egress_taken: AtomicBool,
    /// The workload's NDJSON log.
    log: Mutex<File>,
    /// Presented by this workload's mediator in its connection preambles. It's passed to the
    /// mediator's environment only, never to the container.
    mediator_secret: String,
    /// The workload's mutable state.
    state: Mutex<State>,
    /// avirtinitd presents this on its stream POST. Anything in the container can dial
    /// the runtime's vsock relays. The sockets' permissions alone don't prove who is on
    /// the other end. This secret is only ever in avirtinitd's environment.
    token: String,
}

/// The daemon's profiles, state root, and registry of workloads.
struct Hostd {
    /// The daemon's configuration.
    config: Config,
    /// The agent profiles by name.
    profiles: ProfileMap,
    /// Cancelled when avirthostd shuts down.
    shutdown: CancellationToken,
    /// The socket mediators dial (in the state root's `hostd/` directory).
    socket: PathBuf,
    /// The state root, which holds the lock.
    state: state::Root,
    /// The order in which finished workloads became tombstones.
    tombstones: Mutex<VecDeque<String>>,
    /// Tracks the tasks that shutdown waits for.
    tracker: TaskTracker,
    /// The registry of workloads by id.
    workloads: Mutex<HashMap<String, Entry>>,
}

/// Returned by `launch` while avirthostd is shutting down. Reports a 503.
#[derive(Debug)]
struct ShuttingDown;

/// Everything the task that supervises one running workload owns.
struct Supervised {
    /// The cleanup to run once the container no longer exists.
    cleanup: Cleanup,
    /// The workload directory, removed last.
    dir: WorkloadDir,
    /// Keeps shutdown waiting until the workload finishes.
    drain: TaskTrackerToken,
    /// The workload id.
    id: String,
    /// The workload's mediator.
    proxy: Proxy,
    /// The workload's log and state.
    workload: Arc<Workload>,
}

/// The task that copies one workload's stream POST into its log.
struct StreamTask {
    /// Receives the final `{"lines": N}` body.
    done_tx: UnboundedSender<Bytes>,
    /// Keeps shutdown waiting until this stream has drained into the log.
    drain: TaskTrackerToken,
    /// The daemon.
    hostd: Arc<Hostd>,
    /// The workload id.
    id: String,
    /// The workload whose log receives the lines.
    workload: Arc<Workload>,
}

impl fmt::Display for ShuttingDown {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("avirthostd is shutting down")
    }
}

impl Error for ShuttingDown {}

impl Workload {
    /// Claims the workload's one stream POST.
    ///
    /// # Errors
    ///
    /// Returns the rejection when the workload already streamed or already exited.
    fn claim_stream(&self) -> Result<(), Rejection> {
        let mut state = lock(&self.state);
        if state.stream != StreamPhase::Unclaimed {
            return Err((StatusCode::CONFLICT, "workload already streamed"));
        }
        if state.exited {
            return Err((StatusCode::GONE, "workload already exited"));
        }
        state.stream = StreamPhase::Streaming;
        drop(state);
        Ok(())
    }

    /// Marks the stream drained. Returns true if that completed the workload.
    fn drained(&self) -> bool {
        let mut state = lock(&self.state);
        state.stream = StreamPhase::Drained;
        let completed = self.flush_exit(&mut state);
        drop(state);
        completed
    }

    /// Writes the held exit note once the container has exited and the stream has ended.
    /// Returns true if this call completed the workload.
    fn flush_exit(&self, state: &mut State) -> bool {
        if state.exited && state.stream != StreamPhase::Streaming {
            let notes = mem::take(&mut state.notes);
            for note in notes.into_iter().chain(state.cleanup_note.take()).chain(state.exit_note.take()) {
                self.note(note);
            }
            return true;
        }
        false
    }

    /// Holds a note until the container has exited and the stream has drained.
    fn hold(&self, msg: Value) {
        lock(&self.state).notes.push(msg);
    }

    /// Writes a note into the log now, as a `meta` envelope.
    fn note(&self, msg: Value) {
        self.write(&Envelope::new(Dir::Meta, msg).to_line());
    }

    /// Notes a line that was longer than the limit and dropped whole.
    fn note_truncated(&self, bytes: usize) {
        // Written by avirthostd, so an agent can't make it look like anything else.
        self.note(json!({ "event": "line_truncated", "bytes": bytes }));
    }

    /// Appends text to the log.
    fn write(&self, line: &str) {
        let mut log = lock(&self.log);
        if let Err(err) = log.write_all(line.as_bytes()) {
            console::err_line(format_args!("log write failed: {err}"));
        }
    }
}

impl StreamTask {
    /// Writes one streamed item to the log and counts the lines.
    fn record(&self, item: Item, lines: &mut usize) {
        match item {
            Item::Line(mut line) => {
                line.push('\n');
                self.workload.write(&line);
                *lines = lines.saturating_add(1);
            }
            Item::Truncated(bytes) => self.workload.note_truncated(bytes),
        }
    }

    /// Writes every item to the log and counts the lines.
    fn record_all(&self, items: impl IntoIterator<Item = Item>, lines: &mut usize) {
        for item in items {
            self.record(item, lines);
        }
    }

    /// Copies the request body into the log, then completes the workload if the container has
    /// already exited, and answers with the line count.
    async fn run(self, mut body: Incoming) {
        let mut splitter = LineSplitter::default();
        let mut lines = 0usize;
        // An aborted stream (RST_STREAM, killed container) ends in an error rather than a clean
        // end. Either way, flush the trailing partial line.
        while let Some(Ok(frame)) = body.frame().await {
            if let Ok(data) = frame.into_data() {
                self.record_all(splitter.push(&data), &mut lines);
            }
        }
        self.record_all(splitter.finish(), &mut lines);

        if self.workload.drained() {
            self.hostd.mark_exited(&self.id);
        }
        drop(self.done_tx.send(Bytes::from(json!({ "lines": lines }).to_string())));
        drop(self.drain);
    }
}

impl Hostd {
    /// Builds the `container run` command for one launch.
    ///
    /// # Errors
    ///
    /// Returns an error if creating the container log fails.
    fn command(&self, launch: &Launch<'_>, extra_args: &[OsString], inherit: &[&str]) -> io::Result<Command> {
        let Launch { cwd, id, mediator_socket, profile, prompt, proxy_socket, token } = *launch;
        let container_log = Self::create_private(&self.config.logs_dir.join(format!("{id}.container.log")))?;
        let mut cmd = Command::new("container");
        // Start from nothing: avirthostd's environment holds credentials for every agent, and a
        // launch gets only the variables its profile names plus what the command-line interface itself needs.
        cmd.env_clear();
        for name in ["HOME", "PATH", "TMPDIR", "USER", "LOGNAME"].iter().chain(inherit) {
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
        .arg(mount(proxy_socket, "/run/avirt/proxy.sock"))
        .arg("-v")
        .arg(mount(mediator_socket, "/run/avirt/mediator.sock"))
        .args(extra_args);
        if let Some(host_dir) = cwd {
            cmd.arg("-v").arg(mount(host_dir, "/workspace"));
        }
        // `-e NAME` with no value makes the `container` command read it from its own environment, so
        // the prompt never appears on argv.
        cmd.args(["-e", "AVIRT_PROMPT", "-e", "AVIRT_CWD", "-e", "AVIRT_CONNECTION_TOKEN"])
            .arg("-e")
            .arg(format!("AVIRT_WORKLOAD_ID={id}"))
            .arg(profile.image())
            .env("AVIRT_PROMPT", prompt)
            .env("AVIRT_CONNECTION_TOKEN", token)
            .env("AVIRT_CWD", if cwd.is_some() { "/workspace" } else { "" })
            .stdin(Stdio::null())
            .stdout(container_log.try_clone()?)
            .stderr(container_log)
            // A separate process group prevents terminal Ctrl-C from reaching the command-line interface while avirthostd receives it.
            .process_group(0);
        Ok(cmd)
    }

    /// Creates a new file that only this user can read or write.
    ///
    /// # Errors
    ///
    /// Returns an error if the file already exists or creating it fails.
    fn create_private(path: &Path) -> io::Result<File> {
        OpenOptions::new().create_new(true).append(true).mode(0o600).open(path)
    }

    /// Records the container's exit. Writes `container_error` at once and holds `container_exit`
    /// until the stream has drained (or writes it now if avirthostd never accepted a stream).
    fn finish(&self, id: &str, workload: &Workload, status: io::Result<Value>) {
        let completed = {
            let mut state = lock(&workload.state);
            state.exited = true;
            match status {
                Ok(exit) => state.exit_note = Some(exit),
                Err(err) => {
                    state.notes.push(json!({ "event": "container_error", "message": err.to_string() }));
                    // Still the last event of the workload, so followers have one thing to wait for.
                    state.exit_note = Some(json!({ "event": "container_exit", "code": null, "signal": null }));
                }
            }
            let completed = workload.flush_exit(&mut state);
            drop(state);
            completed
        };
        if completed {
            self.mark_exited(id);
        }
    }

    /// Drops a registered workload whose launch failed.
    fn forget(&self, id: &str) {
        lock(&self.workloads).remove(id);
    }

    /// Starts a workload for `agent` and returns its id.
    ///
    /// # Errors
    ///
    /// Returns [`ShuttingDown`] during shutdown, [`Refused`] for a launch the caller can fix, and
    /// any other error when setup fails.
    async fn launch(self: &Arc<Self>, agent: &str, prompt: &str, cwd: Option<&Path>) -> Result<String> {
        // Take the token before checking for shutdown: `serve` cancels, then closes the tracker,
        // so it always waits for a launch that passes the check.
        let token = self.tracker.token();
        if self.shutdown.is_cancelled() {
            return Err(ShuttingDown.into());
        }
        if cwd.is_some_and(|dir| !dir.is_dir()) {
            return Err(Refused("cwd must be an existing directory on the host".into()).into());
        }
        if !self.profiles.contains_key(agent) {
            bail!("unknown agent {agent}");
        }
        let mut prepared = self.prepare(agent).await?;
        let result = self.start(&LaunchArgs { agent, cwd, prompt }, &mut prepared, token).await;
        // Whatever `start` left unused runs its cleanup when dropped, off the async workers.
        drop(task::spawn_blocking(move || drop(prepared)).await);
        result
    }

    /// The workload's log file.
    fn log_path(&self, id: &str) -> PathBuf {
        self.config.logs_dir.join(format!("{id}.ndjson"))
    }

    /// Looks up a workload that's still live.
    ///
    /// # Errors
    ///
    /// Returns the rejection for an unknown workload (404) or one that already exited (410).
    fn lookup(&self, id: &str) -> Result<Arc<Workload>, Rejection> {
        match lock(&self.workloads).get(id) {
            None => Err((StatusCode::NOT_FOUND, "unknown workload")),
            Some(Entry::Exited) => Err((StatusCode::GONE, "workload already exited")),
            Some(Entry::Live(workload)) => Ok(Arc::clone(workload)),
        }
    }

    /// Replaces a finished workload with a tombstone (so a late stream gets 410), keeping at most
    /// `MAX_TOMBSTONES` of them.
    fn mark_exited(&self, id: &str) {
        let mut workloads = lock(&self.workloads);
        workloads.insert(id.to_owned(), Entry::Exited);
        let mut order = lock(&self.tombstones);
        order.push_back(id.to_owned());
        while order.len() > MAX_TOMBSTONES {
            if let Some(old) = order.pop_front() {
                workloads.remove(&old);
            }
        }
        drop(order);
        drop(workloads);
    }

    /// Runs the profile's per-launch setup. Credential and file work blocks, so it runs off the
    /// async workers.
    ///
    /// # Errors
    ///
    /// Returns an error if the agent is unknown or the profile's setup fails.
    async fn prepare(self: &Arc<Self>, agent: &str) -> Result<Prepared> {
        let (shared, name) = (Arc::clone(self), agent.to_owned());
        task::spawn_blocking(move || {
            let profile = shared.profiles.get(&name).ok_or_else(|| anyhow!("unknown agent {name}"))?;
            profile.prepare(&shared.state.scratch())
        })
        .await?
    }

    /// Picks an unused id (redrawn on collision) and registers the workload with its log.
    ///
    /// # Errors
    ///
    /// Returns an error if creating the log file fails.
    fn register(&self) -> Result<Registered> {
        let mut workloads = lock(&self.workloads);
        // A fresh id that's also unused on disk (earlier runs leave logs behind).
        let (id, log) = loop {
            let id = format!("{:08x}", rand::random::<u32>());
            if workloads.contains_key(&id) {
                continue;
            }
            match Self::create_private(&self.log_path(&id)) {
                Ok(log) => break (id, log),
                Err(err) if err.kind() == io::ErrorKind::AlreadyExists => {}
                Err(err) => return Err(err.into()),
            }
        };
        let workload = Arc::new(Workload {
            egress_eof: CancellationToken::new(),
            egress_taken: AtomicBool::new(false),
            log: Mutex::new(log),
            mediator_secret: format!("{:032x}", rand::random::<u128>()),
            state: Mutex::default(),
            token: format!("{:032x}", rand::random::<u128>()),
        });
        workloads.insert(id.clone(), Entry::Live(Arc::clone(&workload)));
        drop(workloads);
        Ok((id, workload))
    }

    /// Everything after `prepare`: log, network, proxy, container, mediator. On error it
    /// unwinds what it created and leaves `prepared` for the caller to discard.
    ///
    /// # Errors
    ///
    /// Returns an error if the workload's directory, mediator or container can't start.
    async fn start(
        self: &Arc<Self>,
        args: &LaunchArgs<'_>,
        prepared: &mut Prepared,
        drain: TaskTrackerToken,
    ) -> Result<String> {
        let profile = self.profiles.get(args.agent).ok_or_else(|| anyhow!("unknown agent {}", args.agent))?;
        let (id, workload) = self.register()?;
        let dir = WorkloadDir::create(&self.state.workloads(), &id).inspect_err(|_| self.forget(&id))?;
        let mediator_log = self.config.logs_dir.join(format!("{id}.mediator.log"));
        let proxy = Proxy::spawn(&ProxySpec {
            bin: &self.config.mediator_bin,
            dir: &dir,
            hostd_sock: &self.socket,
            id: &id,
            log_path: &mediator_log,
            secret: &workload.mediator_secret,
        })
        .await
        .inspect_err(|_| self.forget(&id))?;
        let (proxy_socket, mediator_socket) = (dir.proxy_socket(), dir.mediator_socket());
        let launch = Launch {
            cwd: args.cwd,
            id: &id,
            mediator_socket: &mediator_socket,
            profile: profile.as_ref(),
            prompt: args.prompt,
            proxy_socket: &proxy_socket,
            token: &workload.token,
        };
        let mut cmd = match self.command(&launch, &prepared.args, &prepared.inherit) {
            Ok(cmd) => cmd,
            Err(err) => {
                proxy.stop().await;
                self.forget(&id);
                return Err(err.into());
            }
        };
        let cleanup = mem::replace(&mut prepared.cleanup, Cleanup::none());
        let child = cmd.spawn();
        console::out_line(format_args!("launched {id}"));
        let run = Supervised { cleanup, dir, drain, id: id.clone(), proxy, workload };
        self.tracker.spawn(Arc::clone(self).supervise(child, run));
        Ok(id)
    }

    /// Waits for the container to exit and then winds the workload down.
    ///
    /// That drains its stream and stops the mediator. The cleanup runs next, and the exit notes
    /// come last.
    async fn supervise(self: Arc<Self>, spawned: io::Result<Child>, mut run: Supervised) {
        let (status, gone) = match spawned {
            Ok(child) => watch_container(child, &mut run, &self.shutdown).await,
            Err(err) => (Err(err), true),
        };
        settle_stream(&run.workload).await;
        // Then stop the mediator, which flushes its last events to avirthostd, and wait for
        // them to reach the log before the exit notes follow.
        run.proxy.stop().await;
        // The mediator may never close the event connection, so the wait has a limit.
        time::timeout(Duration::from_secs(5), run.workload.egress_eof.cancelled()).await.unwrap_or_default();
        let cleanup_note = run_cleanup(&run.workload, run.cleanup, gone).await;
        lock(&run.workload.state).cleanup_note = cleanup_note;
        self.finish(&run.id, &run.workload, status);
        // Removes the workload directory with both sockets in it, last.
        drop(run.dir);
        drop(run.drain);
    }
}

/// Accepts control API connections on the loopback port.
async fn accept_loop(hostd: Arc<Hostd>, listener: TcpListener) -> ! {
    loop {
        let Ok((stream, _)) = listener.accept().await else {
            // For errors such as running out of file descriptors, back off instead of spinning.
            time::sleep(Duration::from_millis(100)).await;
            continue;
        };
        let shared = Arc::clone(&hostd);
        tokio::spawn(async move {
            let service = service_fn(move |req| handle_control(Arc::clone(&shared), req));
            let served = http2::Builder::new(TokioExecutor::new()).serve_connection(TokioIo::new(stream), service);
            if let Err(err) = served.await {
                console::err_line(format_args!("h2 connection error: {err}"));
            }
        });
    }
}

/// Takes the lock even when a panicking thread poisoned it. Every guarded value is a plain collection or
/// field that one call updates. A panic never leaves one half written.
fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Runs avirthostd until SIGINT or SIGTERM, then stops the running containers, waits for their
/// cleanup and exits.
///
/// # Errors
///
/// Returns an error if setting up the logs directory or state root fails, or if a listener or
/// signal handler can't start.
pub async fn serve<S>(profiles: ProfileMap<S>, requested: Config) -> Result<()>
where
    S: BuildHasher,
{
    let logs_dir = path::absolute(&requested.logs_dir)?;
    private_dir(&logs_dir)?;
    let config = Config { logs_dir, ..requested };

    // One avirthostd per user. Holding the state root's lock lets this one sweep whatever a previous
    // avirthostd left behind without racing a live one.
    let state = state::Root::open()?;
    let swept = state.sweep();
    if !swept.containers.is_empty() || swept.dirs > 0 {
        let names: Vec<String> = swept.containers.iter().map(|name| format!("\"{name}\"")).collect();
        console::out_line(format_args!(
            "swept leftovers of a previous run: containers [{}], {} directories",
            names.join(", "),
            swept.dirs
        ));
    }
    for failure in &swept.failures {
        console::err_line(format_args!("sweep could not remove {failure}"));
    }

    // The control API (`/launch`) is for the command-line interface on this machine only. Containers have no
    // network and can't reach it. A workload's mediator dials avirthostd's own socket instead.
    let listener = TcpListener::bind(("127.0.0.1", config.port)).await?;
    let hostd_sock = state.hostd().join("hostd.sock");
    let mediators = UnixListener::bind(&hostd_sock)?;
    fs::set_permissions(&hostd_sock, Permissions::from_mode(0o600))?;
    console::out_line(format_args!(
        "avirthostd listening on 127.0.0.1:{}; logs in {}",
        config.port,
        config.logs_dir.display()
    ));
    let hostd = Arc::new(Hostd {
        config,
        profiles: profiles.into_iter().collect(),
        shutdown: CancellationToken::new(),
        socket: hostd_sock,
        state,
        tombstones: Mutex::default(),
        tracker: TaskTracker::new(),
        workloads: Mutex::default(),
    });

    let mut sigint = signal(SignalKind::interrupt())?;
    let mut sigterm = signal(SignalKind::terminate())?;
    let accept = tokio::spawn(accept_loop(Arc::clone(&hostd), listener));
    let accept_mediators = tokio::spawn(mediator_loop(Arc::clone(&hostd), mediators));
    first_signal(&mut sigint, &mut sigterm).await;

    console::out_line(format_args!("shutting down: stopping containers"));
    hostd.shutdown.cancel();
    hostd.tracker.close();
    // Keep serving so the stopped containers' streams drain into their logs.
    if time::timeout(Duration::from_mins(1), hostd.tracker.wait()).await.is_err() {
        console::err_line(format_args!("timed out waiting for containers to stop"));
    }
    accept.abort();
    accept_mediators.abort();
    Ok(())
}
