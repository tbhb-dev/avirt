# avirt

`avirt` runs ACP agents in isolated containers on an Apple silicon Mac. The supported agents are Claude, through [claude-agent-acp](https://github.com/agentclientprotocol/claude-agent-acp), and Codex, through [codex-acp](https://github.com/agentclientprotocol/codex-acp). The host daemon, `avirthostd`, launches one actor per request. An actor is a per-actor egress mediator (`avirtmediatord`) plus a workload container. Inside the workload, `avirtinitd` drives the agent over ACP and streams every JSON-RPC message to `avirthostd`, which writes it to `logs/<actor-id>.ndjson`. The workload has no network: the agent's only way out is the mediator's logged CONNECT proxy. Everything is Rust and uses the official [`agent-client-protocol`](https://github.com/agentclientprotocol/rust-sdk) crate.

Agents working in this repository should read [AGENTS.md](AGENTS.md). See [docs/architecture.md](docs/architecture.md) for the architecture, both the implemented system and the planned design, and [docs/build-and-platforms.md](docs/build-and-platforms.md) for build targets and platforms.

## Layout

- `crates/avirthostd/`: the `avirthostd` binary (running it starts the server), the per-agent launch profiles, and Codex's credential handling.
- `crates/avirt/`: the `avirt` operator CLI (`avirt launch`).
- `crates/avirtmediatord/`: the `avirtmediatord` binary, the per-actor Pingora egress proxy.
- `crates/avirtinitd/`: the `avirtinitd` binary, which runs in the workload (`AVIRT_AGENT=claude|codex`).
- `crates/avirt-core/`: the shared library, covering the control API types (`api`), the log envelope, the h2c client and uplink, the `avirthostd` server and actor registry, the `avirtinitd` harness and the state root.
- `workloads/claude/` and `workloads/codex/`: the image build directories, each with a Containerfile and `image/` holding the pinned adapter. The images are `avirt-workload-claude:dev` and `avirt-workload-codex:dev`.

## Prerequisites

- An Apple silicon Mac with Apple `container` 1.5.0 or later. `container` cannot be installed through mise.
- [mise](https://mise.jdx.dev). `mise.toml` pins Rust 1.97.1 and defines the tasks.
- For Claude, put `CLAUDE_CODE_OAUTH_TOKEN` in the uncommitted `mise.local.toml` under `[env]`. mise loads that file on top of `mise.toml`.
- For Codex, run `codex login` once, which writes `~/.codex/auth.json`.

## Quickstart

```bash
mise install                                                      # the Rust toolchain
mise run build-images                                             # both workload images
mise run serve                                                    # terminal 1: avirthostd
mise run launch claude "Reply with the word pong" -- --follow     # terminal 2: avirt launch
mise run launch codex "Reply with the word pong" -- --follow
mise run check                                                    # fmt --check, clippy -D warnings, tests
```

Extra flags go after `--`, for example `-- --cwd DIR --follow`. `--cwd` mounts that host directory read-write at `/workspace`. The CLI is described in [docs/cli.md](docs/cli.md), and the `avirthostd` environment variables and log format in [docs/hostd-api.md](docs/hostd-api.md).

## Caveats

- Permission requests are approved automatically, so the agent can run any tool inside the workload with no human in the loop.
- The control API on `127.0.0.1:8787` has no authentication. Any local process can start workloads that use your Claude token or ChatGPT login and mount host directories.
- The Claude token is in the agent's environment, and the agent can read Codex's `auth.json`. With automatic approval, either can end up in the actor log, which also holds prompts, tool output, and the Codex account email and plan. Treat logs as secret.
- Egress is allowed and logged, not filtered, apart from private and host destinations.
- codex-acp 2.2.2 omits the first chunk of each command's terminal output from Codex actor logs.
