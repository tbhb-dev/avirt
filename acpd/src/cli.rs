//! Command line: `serve` runs the daemon, `launch` asks a running one to start a session.

use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::PathBuf;
use std::process::ExitCode;
use std::time::Duration;

use std::collections::HashMap;

use acp_tap::daemon::{self, Config, Profile};
use acp_tap::h2c;
use anyhow::{Result, bail};
use clap::{Args, Parser, Subcommand, ValueEnum};
use serde_json::{Value, json};

use crate::profiles;

#[derive(Parser)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Run the host daemon.
    Serve(Serve),
    /// Ask a running daemon to start a session.
    Launch(Launch),
}

#[derive(Args)]
struct Serve {
    #[arg(long, env = "PORT", default_value_t = profiles::DEFAULT_PORT)]
    port: u16,
    #[arg(long, env = "LOGS_DIR", default_value = "logs")]
    logs_dir: PathBuf,
    /// The per-session supervisor [default: acp-supervisor next to this binary]
    #[arg(long, env = "ACP_SUPERVISOR_BIN")]
    supervisor_bin: Option<PathBuf>,
}

#[derive(Clone, Copy, ValueEnum)]
enum Agent {
    Claude,
    Codex,
}

#[derive(Args)]
struct Launch {
    #[arg(long, value_enum)]
    agent: Agent,
    prompt: String,
    /// Host directory to mount at /workspace in the container
    #[arg(long)]
    cwd: Option<PathBuf>,
    /// Print the session log until the container exits
    #[arg(short, long)]
    follow: bool,
    #[arg(long, env = "PORT", default_value_t = profiles::DEFAULT_PORT)]
    port: u16,
    /// Daemon URL [default: the local daemon on --port]
    #[arg(long, env = "DAEMON_LOCAL_URL")]
    url: Option<String>,
}

pub async fn main() -> ExitCode {
    let result = match Cli::parse().command {
        Command::Serve(args) => match profiles::all() {
            Ok(profiles) => serve(profiles, args).await.map(|()| ExitCode::SUCCESS),
            Err(err) => Err(err),
        },
        Command::Launch(args) => launch(args).await,
    };
    result.unwrap_or_else(|err| {
        eprintln!("{err:#}");
        ExitCode::FAILURE
    })
}

async fn serve(profiles: HashMap<String, Box<dyn Profile>>, args: Serve) -> Result<()> {
    let supervisor_bin = match args.supervisor_bin {
        Some(bin) => bin,
        None => std::env::current_exe()?.with_file_name("acp-supervisor"),
    };
    if !supervisor_bin.is_file() {
        bail!(
            "{} not found; build it with `cargo build -p acp-supervisor` or set ACP_SUPERVISOR_BIN",
            supervisor_bin.display()
        );
    }
    daemon::serve(profiles, Config { port: args.port, logs_dir: args.logs_dir, supervisor_bin }).await
}

async fn launch(args: Launch) -> Result<ExitCode> {
    let url = args.url.unwrap_or_else(|| format!("http://127.0.0.1:{}", args.port));
    let agent = args.agent.to_possible_value().expect("every variant has a name");
    let mut body = json!({ "agent": agent.get_name(), "prompt": args.prompt });
    if let Some(cwd) = &args.cwd {
        body["cwd"] = json!(std::path::absolute(cwd)?);
    }
    let (status, reply) = h2c::post_json(&url, "/launch", &body).await?;
    if !status.is_success() {
        eprintln!("launch failed ({}): {}", status.as_u16(), reply["error"].as_str().unwrap_or("unknown error"));
        return Ok(ExitCode::FAILURE);
    }
    let log_path = PathBuf::from(reply["logPath"].as_str().unwrap_or_default());
    println!("session {}", reply["sessionId"].as_str().unwrap_or_default());
    println!("log     {}", log_path.display());
    if args.follow {
        follow(&log_path, &url).await?;
    }
    Ok(ExitCode::SUCCESS)
}

/// Polls the log and prints new lines until the daemon writes `container_exit`, which is always
/// the last event of a session. Each line is parsed as JSON; raw text is never pattern-matched.
/// If the log stops growing and the daemon's control API stops answering, the daemon is gone and
/// the follow ends with an error instead of waiting forever.
async fn follow(path: &std::path::Path, url: &str) -> Result<()> {
    let mut offset = 0u64;
    let mut last_growth = std::time::Instant::now();
    loop {
        // Read only what was appended since the last pass.
        let mut fresh = Vec::new();
        if let Ok(mut file) = File::open(path)
            && file.seek(SeekFrom::Start(offset)).is_ok()
        {
            let _ = file.read_to_end(&mut fresh);
        }
        if let Some(end) = fresh.iter().rposition(|&b| b == b'\n').map(|i| i + 1) {
            let text = String::from_utf8_lossy(&fresh[..end]);
            print!("{text}");
            offset += end as u64;
            last_growth = std::time::Instant::now();
            if text.lines().any(is_terminal_event) {
                return Ok(());
            }
        } else if last_growth.elapsed() > Duration::from_secs(5) {
            let ping = json!({});
            let answered = tokio::time::timeout(Duration::from_secs(3), h2c::post_json(url, "/health", &ping));
            if !matches!(answered.await, Ok(Ok((status, _))) if status.is_success()) {
                bail!("the daemon stopped answering and the log has stopped growing; giving up");
            }
            last_growth = std::time::Instant::now();
        }
        tokio::time::sleep(Duration::from_millis(300)).await;
    }
}

fn is_terminal_event(line: &str) -> bool {
    serde_json::from_str::<Value>(line).is_ok_and(|e| e["dir"] == "meta" && e["msg"]["event"] == "container_exit")
}

#[cfg(test)]
mod tests {
    use super::is_terminal_event;

    #[test]
    fn follow_stops_on_meta_events_only() {
        assert!(is_terminal_event(r#"{"ts":"t","dir":"meta","msg":{"event":"container_exit","code":0}}"#));
        // An error is followed by the exit event, which is the only terminal one.
        assert!(!is_terminal_event(r#"{"ts":"t","dir":"meta","msg":{"event":"container_error"}}"#));
        // The same text inside agent traffic must not stop the follow.
        assert!(!is_terminal_event(r#"{"ts":"t","dir":"agent->client","msg":{"text":"container_exit"}}"#));
        assert!(!is_terminal_event("container_exit"));
    }
}
