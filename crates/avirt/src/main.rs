//! `avirt launch` asks a running avirthostd to start a workload.

#[cfg(test)]
mod tests;

use core::time::Duration;
use std::fs::File;
use std::io::{BufReader, Read as _, Seek as _, SeekFrom};
use std::path::{Path, PathBuf, absolute};
use std::process::ExitCode;
use std::time::Instant;

use tokio::time::{sleep, timeout};

use anyhow::{Context as _, Result, bail};
use avirt_core::api::{
    DEFAULT_PORT, HEALTH_PATH, LAUNCH_PATH, LaunchRequest, LaunchResponse, default_url, is_terminal_event,
};
use avirt_core::console;
use avirt_core::h2c;
use clap::{Args, Parser, Subcommand, ValueEnum};
use serde_json::{Value, json};

/// The `avirt` command line.
#[derive(Parser)]
struct Cli {
    /// The subcommand to run.
    #[command(subcommand)]
    command: Command,
}

/// The subcommands of `avirt`.
#[derive(Subcommand)]
enum Command {
    /// Ask a running avirthostd to start a workload.
    Launch(Launch),
}

/// The agents a workload can run.
#[derive(Clone, Copy, ValueEnum)]
enum Agent {
    /// Claude through its ACP adapter.
    Claude,
    /// Codex through its ACP adapter.
    Codex,
}

/// Arguments of `avirt launch`.
#[derive(Args)]
struct Launch {
    /// Agent to run in the workload.
    #[arg(long, value_enum)]
    agent: Agent,
    /// Host directory to mount at /workspace in the container.
    #[arg(long)]
    cwd: Option<PathBuf>,
    /// Print the workload log until the container exits.
    #[arg(short, long)]
    follow: bool,
    /// Port of the local avirthostd.
    #[arg(long, env = "PORT", default_value_t = DEFAULT_PORT)]
    port: u16,
    /// Prompt to send to the agent.
    prompt: String,
    /// avirthostd URL [default: the local avirthostd on `--port`].
    #[arg(long, env = "AVIRT_HOSTD_URL")]
    url: Option<String>,
}

/// Parses the command line and runs it.
///
/// # Panics
///
/// Panics if the async runtime can't start.
#[tokio::main]
async fn main() -> ExitCode {
    let Command::Launch(args) = Cli::parse().command;
    launch(args).await.unwrap_or_else(|err| {
        console::err_line(format_args!("{err:#}"));
        ExitCode::FAILURE
    })
}

/// Posts the launch request and prints where the workload's log is.
///
/// # Errors
///
/// Returns an error if avirthostd is unreachable, replies with something unexpected, or the
/// followed log ends without `container_exit`.
async fn launch(args: Launch) -> Result<ExitCode> {
    let url = args.url.unwrap_or_else(|| default_url(args.port));
    let agent = args.agent.to_possible_value().context("every agent variant has a name")?;
    let request = LaunchRequest {
        agent: agent.get_name().to_owned(),
        prompt: args.prompt,
        cwd: args.cwd.as_deref().map(absolute).transpose()?.map(|path| path.to_string_lossy().into_owned()),
    };
    let body = serde_json::to_value(request)?;
    let (status, response) = h2c::post_json(&url, LAUNCH_PATH, &body)
        .await
        .with_context(|| format!("cannot reach avirthostd at {url}; is `avirthostd` running?"))?;
    if !status.is_success() {
        console::err_line(format_args!(
            "launch failed ({}): {}",
            status.as_u16(),
            response.get("error").and_then(Value::as_str).unwrap_or("unknown error")
        ));
        return Ok(ExitCode::FAILURE);
    }
    let reply: LaunchResponse = serde_json::from_value(response).context("unexpected launch response")?;
    let log_path = PathBuf::from(&reply.log_path);
    console::out_line(format_args!("workload {}", reply.workload_id));
    console::out_line(format_args!("log     {}", log_path.display()));
    if args.follow {
        follow(&log_path, &url).await?;
    }
    Ok(ExitCode::SUCCESS)
}

/// Reads the bytes appended to the log since `offset`, or nothing if the log isn't readable yet.
fn read_new(path: &Path, offset: u64) -> Vec<u8> {
    let mut fresh = Vec::new();
    if let Ok(mut file) = File::open(path)
        && file.seek(SeekFrom::Start(offset)).is_ok()
    {
        let _read = BufReader::new(file).read_to_end(&mut fresh);
    }
    fresh
}

/// Returns whether avirthostd's control API answers its health check.
async fn hostd_answers(url: &str) -> bool {
    let ping = json!({});
    let answered = timeout(Duration::from_secs(3), h2c::post_json(url, HEALTH_PATH, &ping));
    matches!(answered.await, Ok(Ok((status, _))) if status.is_success())
}

/// Polls the log and prints new lines until avirthostd writes `container_exit`.
///
/// `container_exit` is always the last event of a workload. If the log stops growing and
/// avirthostd's control API stops answering, avirthostd is unavailable and the follow ends with an
/// error instead of waiting forever.
///
/// # Errors
///
/// Returns an error if the log stops growing and avirthostd stops answering.
async fn follow(path: &Path, url: &str) -> Result<()> {
    let mut offset = 0u64;
    let mut last_growth = Instant::now();
    loop {
        // Read only the bytes added since the last pass.
        let fresh = read_new(path, offset);
        let complete = fresh
            .iter()
            .rposition(|&byte| byte == b'\n')
            .map(|index| index.saturating_add(1))
            .and_then(|end| Some((end, fresh.get(..end)?)));
        match complete {
            Some((end, lines)) => {
                let text = String::from_utf8_lossy(lines);
                console::out(format_args!("{text}"));
                offset = offset.saturating_add(u64::try_from(end)?);
                last_growth = Instant::now();
                if text.lines().any(is_terminal_event) {
                    return Ok(());
                }
            }
            None if last_growth.elapsed() > Duration::from_secs(5) => {
                if !hostd_answers(url).await {
                    bail!("avirthostd stopped answering and the log has stopped growing; giving up");
                }
                last_growth = Instant::now();
            }
            None => {}
        }
        sleep(Duration::from_millis(300)).await;
    }
}
