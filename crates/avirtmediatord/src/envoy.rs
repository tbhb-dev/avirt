//! Static Envoy configuration and a child tied to the mediator's lifetime.
use std::collections::HashMap;
use std::fs::OpenOptions;
use std::io::{self, BufRead, Read, Write};
use std::os::unix::fs::OpenOptionsExt;
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use async_trait::async_trait;
use pingora_core::apps::ServerApp;
use pingora_core::protocols::Stream;
use pingora_core::server::ShutdownWatch;
use serde_json::{Value, json};

use crate::events;

pub fn bootstrap(socket: &Path, module: &Path) -> Value {
    let cache = json!({"name": "avirt-dns", "dns_lookup_family": "ALL"});
    json!({
        "static_resources": {
            "listeners": [{
                "name": "connect", "address": {"pipe": {"path": socket, "mode": 384}},
                "filter_chains": [{"filters": [{
                    "name": "envoy.filters.network.http_connection_manager",
                    "typed_config": {
                        "@type": "type.googleapis.com/envoy.extensions.filters.network.http_connection_manager.v3.HttpConnectionManager",
                        "stat_prefix": "connect", "codec_type": "HTTP1",
                        "http_protocol_options": {"accept_http_10": false},
                        "max_request_headers_kb": 8, "request_headers_timeout": "10s",
                        "stream_idle_timeout": "0s", "request_timeout": "0s",
                        "upgrade_configs": [{"upgrade_type": "CONNECT"}],
                        "route_config": {"name": "connect", "virtual_hosts": [{
                            "name": "all", "domains": ["*"], "routes": [
                                {"match": {"connect_matcher": {}}, "route": {
                                    "cluster": "forward", "timeout": "0s",
                                    "upgrade_configs": [{"upgrade_type": "CONNECT", "connect_config": {}}]
                                }},
                                {"match": {"prefix": "/"}, "direct_response": {"status": 501}}
                            ]
                        }]},
                        "http_filters": [
                            {"name": "envoy.filters.http.dynamic_modules", "typed_config": {
                                "@type": "type.googleapis.com/envoy.extensions.filters.http.dynamic_modules.v3.DynamicModuleFilter",
                                "dynamic_module_config": {"module": {"local": {"filename": module}}, "do_not_close": true},
                                "filter_name": "avirt-connect"
                            }},
                            {"name": "envoy.filters.http.dynamic_forward_proxy", "typed_config": {
                                "@type": "type.googleapis.com/envoy.extensions.filters.http.dynamic_forward_proxy.v3.FilterConfig",
                                "dns_cache_config": cache
                            }},
                            {"name": "envoy.filters.http.router", "typed_config": {
                                "@type": "type.googleapis.com/envoy.extensions.filters.http.router.v3.Router"
                            }}
                        ]
                    }
                }]}]
            }],
            "clusters": [{
                "name": "forward", "connect_timeout": "10s", "lb_policy": "CLUSTER_PROVIDED",
                "cluster_type": {"name": "envoy.clusters.dynamic_forward_proxy", "typed_config": {
                    "@type": "type.googleapis.com/envoy.extensions.clusters.dynamic_forward_proxy.v3.ClusterConfig",
                    "dns_cache_config": cache
                }}
            }]
        }
    })
}

/// A small guardian process gives Envoy a parent-death lifeline on macOS as well as Linux.
/// Its stdin is held only by avirtmediatord; it never inherits the mediator secret.
pub fn supervise() -> i32 {
    let mut args = std::env::args_os().skip(2);
    let (Some(binary), Some(config)) = (args.next(), args.next()) else { return 2 };
    if args.next().is_some() {
        return 2;
    }
    let mut child = match Command::new(binary)
        .args(["--concurrency", "1", "--disable-hot-restart", "-c"])
        .arg(config)
        .stdin(Stdio::null())
        .spawn()
    {
        Ok(child) => child,
        Err(error) => {
            eprintln!("cannot execute Envoy: {error}");
            return 1;
        }
    };
    let eof = Arc::new(AtomicBool::new(false));
    let notify = Arc::clone(&eof);
    std::thread::spawn(move || {
        let mut buf = [0; 64];
        while matches!(std::io::stdin().read(&mut buf), Ok(n) if n > 0) {}
        notify.store(true, Ordering::Release);
    });
    loop {
        match child.try_wait() {
            Ok(Some(status)) => return status.code().unwrap_or(1),
            Ok(None) if !eof.load(Ordering::Acquire) => std::thread::sleep(Duration::from_millis(20)),
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                return 1;
            }
        }
    }
}

pub struct Envoy {
    child: Child,
    guardian_pid: i32,
    stopped: bool,
    reader: Option<JoinHandle<()>>,
    config: PathBuf,
    socket: PathBuf,
}

impl Envoy {
    pub fn start(dir: &Path, binary: &Path, module: &Path, actor: String) -> io::Result<Self> {
        let socket = dir.join("envoy.sock");
        let config = dir.join("envoy.json");
        let module = module.canonicalize()?;
        let mut file = OpenOptions::new().write(true).create_new(true).mode(0o600).open(&config)?;
        writeln!(file, "{}", bootstrap(&socket, &module))?;
        let mut child = Command::new(std::env::current_exe()?)
            .env_clear()
            .envs(["PATH", "TMPDIR"].iter().filter_map(|n| std::env::var_os(n).map(|v| (*n, v))))
            .arg("--supervise-envoy")
            .arg(binary)
            .arg(&config)
            .process_group(0)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()?;
        let stdout = child.stdout.take().expect("piped stdout");
        let reader = std::thread::spawn(move || {
            let mut pending = HashMap::new();
            for line in io::BufReader::new(stdout).lines().map_while(Result::ok) {
                if let Ok(record) = serde_json::from_str(&line)
                    && let Some(event) = accept_event(&record, &actor, &mut pending)
                {
                    events::send(event.to_string());
                }
            }
            // The pipe closes when Envoy exits, including a forced shutdown or a crash.
            for mut event in pending.into_values() {
                event["error"] = json!("shutdown");
                events::send(event.to_string());
            }
        });
        let guardian_pid = i32::try_from(child.id()).expect("guardian pid fits i32");
        let mut envoy = Self { child, guardian_pid, stopped: false, reader: Some(reader), config, socket };
        let deadline = Instant::now() + Duration::from_secs(3);
        loop {
            if envoy.exited() {
                return Err(io::Error::other("Envoy exited before its listener was ready"));
            }
            if std::os::unix::net::UnixStream::connect(&envoy.socket).is_ok() {
                return Ok(envoy);
            }
            if Instant::now() >= deadline {
                return Err(io::Error::new(io::ErrorKind::TimedOut, "Envoy listener not ready"));
            }
            std::thread::sleep(Duration::from_millis(20));
        }
    }

    pub fn forward_app(&self) -> ForwardApp {
        ForwardApp(self.socket.clone())
    }

    pub fn exited(&mut self) -> bool {
        !matches!(self.child.try_wait(), Ok(None))
    }

    pub fn stop(&mut self) {
        if self.stopped {
            return;
        }
        self.stopped = true;
        drop(self.child.stdin.take());
        // The group survives the guardian, so this also kills an orphaned Envoy.
        match nix::sys::signal::killpg(nix::unistd::Pid::from_raw(self.guardian_pid), nix::sys::signal::Signal::SIGKILL)
        {
            Ok(()) | Err(nix::errno::Errno::ESRCH) => {}
            Err(error) => eprintln!("cannot kill Envoy process group: {error}"),
        }
        let _ = self.child.wait();
        if let Some(reader) = self.reader.take() {
            let _ = reader.join();
        }
        let _ = std::fs::remove_file(&self.socket);
        let _ = std::fs::remove_file(&self.config);
    }
}

impl Drop for Envoy {
    fn drop(&mut self) {
        self.stop();
    }
}

fn accept_event(record: &Value, actor: &str, pending: &mut HashMap<u64, Value>) -> Option<Value> {
    let id = record.get("avirt_conn")?.as_u64()?;
    let mut event = record.get("event")?.as_object()?.clone();
    event.insert("actor".into(), json!(actor));
    let event = Value::Object(event);
    if record.get("complete")?.as_bool()? {
        pending.remove(&id);
        Some(event)
    } else {
        pending.insert(id, event);
        None
    }
}

/// Pingora supplies only the existing socket accept loop in Envoy mode; HTTP stays in Envoy.
pub struct ForwardApp(pub PathBuf);
#[async_trait]
impl ServerApp for ForwardApp {
    async fn process_new(self: &Arc<Self>, mut stream: Stream, shutdown: &ShutdownWatch) -> Option<Stream> {
        let _slot = crate::LIMIT.acquire().await.ok()?;
        let mut upstream = tokio::net::UnixStream::connect(&self.0).await.ok()?;
        let mut shutdown = shutdown.clone();
        tokio::select! {
            _ = tokio::io::copy_bidirectional(&mut stream, &mut upstream) => {},
            _ = shutdown.changed() => {},
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn connect_bootstrap() {
        let config = bootstrap(Path::new("/actor/envoy.sock"), Path::new("/build/libavirtenvoy.so"));
        let resources = &config["static_resources"];
        assert_eq!(resources["listeners"][0]["address"]["pipe"]["mode"], 0o600);
        let hcm = &resources["listeners"][0]["filter_chains"][0]["filters"][0]["typed_config"];
        assert_eq!(hcm["codec_type"], "HTTP1");
        assert!(hcm.get("http2_protocol_options").is_none());
        assert_eq!(hcm["stream_idle_timeout"], "0s");
        assert_eq!(
            hcm["route_config"]["virtual_hosts"][0]["routes"][0]["route"]["upgrade_configs"][0]["connect_config"],
            json!({})
        );
        let filters = hcm["http_filters"].as_array().unwrap();
        assert_eq!(
            filters.iter().map(|f| f["name"].as_str().unwrap()).collect::<Vec<_>>(),
            [
                "envoy.filters.http.dynamic_modules",
                "envoy.filters.http.dynamic_forward_proxy",
                "envoy.filters.http.router"
            ]
        );
        assert_eq!(
            filters[1]["typed_config"]["dns_cache_config"],
            resources["clusters"][0]["cluster_type"]["typed_config"]["dns_cache_config"]
        );
        assert!(config.get("admin").is_none());
        assert!(!config.to_string().contains("secret"));
    }

    #[test]
    fn events_preserve_envelope_payload_and_actor_binding() {
        let mut pending = HashMap::new();
        let event = json!({"event": "conn", "actor": "untrusted", "host": "example.com", "bytes_up": 12});
        assert!(
            accept_event(&json!({"avirt_conn": 1, "complete": false, "event": event}), "actor", &mut pending).is_none()
        );
        assert_eq!(pending.len(), 1);
        let result =
            accept_event(&json!({"avirt_conn": 1, "complete": true, "event": event}), "actor", &mut pending).unwrap();
        assert_eq!(result["actor"], "actor");
        assert_eq!(result["bytes_up"], 12);
        assert!(pending.is_empty());
    }

    #[test]
    fn envoy_validates_bootstrap_when_available() {
        let (Some(binary), Some(module)) =
            (std::env::var_os("AVIRT_ENVOY_BIN"), std::env::var_os("AVIRT_ENVOY_MODULE"))
        else {
            eprintln!("skipped: set AVIRT_ENVOY_BIN and AVIRT_ENVOY_MODULE");
            return;
        };
        let config = bootstrap(Path::new("/unused/envoy.sock"), Path::new(&module));
        let status =
            Command::new(binary).args(["--mode", "validate", "--config-yaml", &config.to_string()]).status().unwrap();
        assert!(status.success());
    }
}
