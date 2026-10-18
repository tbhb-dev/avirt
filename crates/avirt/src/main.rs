//! `avirt launch` asks a running avirthostd to start a workload.

use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::PathBuf;
use std::process::ExitCode;
use std::time::Duration;

use anyhow::{Context, Result, bail};
use avirt_core::api::{
    DEFAULT_PORT, HEALTH_PATH, LAUNCH_PATH, LaunchRequest, LaunchResponse, default_url, is_terminal_event,
};
use avirt_core::h2c;
use clap::{Args, Parser, Subcommand, ValueEnum};
use serde_json::json;

#[derive(Parser)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Ask a running avirthostd to start a workload.
    Launch(Launch),
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
    /// Print the workload log until the container exits
    #[arg(short, long)]
    follow: bool,
    #[arg(long, env = "PORT", default_value_t = DEFAULT_PORT)]
    port: u16,
    /// avirthostd URL [default: the local avirthostd on --port]
    #[arg(long, env = "AVIRT_HOSTD_URL")]
    url: Option<String>,
}

#[tokio::main]
async fn main() -> ExitCode {
    let Command::Launch(args) = Cli::parse().command;
    launch(args).await.unwrap_or_else(|err| {
        eprintln!("{err:#}");
        ExitCode::FAILURE
    })
}

async fn launch(args: Launch) -> Result<ExitCode> {
    let url = args.url.unwrap_or_else(|| default_url(args.port));
    let agent = args.agent.to_possible_value().expect("every variant has a name");
    let request = LaunchRequest {
        agent: agent.get_name().to_owned(),
        prompt: args.prompt,
        cwd: args.cwd.as_deref().map(std::path::absolute).transpose()?.map(|p| p.to_string_lossy().into_owned()),
    };
    let body = serde_json::to_value(request)?;
    let (status, reply) = h2c::post_json(&url, LAUNCH_PATH, &body)
        .await
        .with_context(|| format!("cannot reach avirthostd at {url}; is `avirthostd` running?"))?;
    if !status.is_success() {
        eprintln!("launch failed ({}): {}", status.as_u16(), reply["error"].as_str().unwrap_or("unknown error"));
        return Ok(ExitCode::FAILURE);
    }
    let reply: LaunchResponse = serde_json::from_value(reply).context("unexpected launch response")?;
    let log_path = PathBuf::from(&reply.log_path);
    println!("workload {}", reply.workload_id);
    println!("log     {}", log_path.display());
    if args.follow {
        follow(&log_path, &url).await?;
    }
    Ok(ExitCode::SUCCESS)
}

/// Polls the log and prints new lines until avirthostd writes `container_exit`, which is
/// always the last event of a workload. If the log stops growing and avirthostd's control API
/// stops answering, avirthostd is gone and the follow ends with an error instead of waiting
/// forever.
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
            let answered = tokio::time::timeout(Duration::from_secs(3), h2c::post_json(url, HEALTH_PATH, &ping));
            if !matches!(answered.await, Ok(Ok((status, _))) if status.is_success()) {
                bail!("avirthostd stopped answering and the log has stopped growing; giving up");
            }
            last_growth = std::time::Instant::now();
        }
        tokio::time::sleep(Duration::from_millis(300)).await;
    }
}

#[cfg(test)]
mod tests {
    use avirt_core::api::LaunchRequest;

    #[test]
    fn request_omits_an_unset_cwd() {
        let body =
            serde_json::to_value(LaunchRequest { agent: "claude".into(), prompt: "hi".into(), cwd: None }).unwrap();
        assert_eq!(body, serde_json::json!({ "agent": "claude", "prompt": "hi" }));
    }
}
