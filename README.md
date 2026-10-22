---
type: README
title: avirt
description: "`avirt` runs ACP agents in isolated containers on an Apple silicon Mac."
tags: [avirt, readme]
generated: { by: codex/gpt-6, at: 2026-10-18T04:57:34Z }
---

# avirt

`avirt` runs ACP agents in isolated containers on an Apple silicon Mac. The supported agents are Claude, through [claude-agent-acp](https://github.com/agentclientprotocol/claude-agent-acp), and Codex, through [codex-acp](https://github.com/agentclientprotocol/codex-acp). `avirthostd`, the host daemon, launches one workload per request. A workload is a sandbox container plus its egress mediator (`avirtmediatord`). Inside the sandbox container, `avirtinitd` drives the agent over ACP and streams every JSON-RPC message to `avirthostd`, which writes it to `logs/<workload-id>.ndjson`. In the implemented system, the agent's only way out is the mediator's logged CONNECT proxy. Everything except `avirtvmd`, the Swift VM helper, is Rust, and the ACP client uses the official [`agent-client-protocol`](https://github.com/agentclientprotocol/rust-sdk) crate.

Agents working in this repository should read [AGENTS.md](AGENTS.md).

## Layout

- `crates/avirthostd/` contains the `avirthostd` binary (running it starts the server) and the per-agent launch profiles. It also handles Codex's credentials.
- `crates/avirt/` contains the `avirt` operator CLI (`avirt launch`).
- `crates/avirtmediatord/` contains the `avirtmediatord` binary, the per-workload egress proxy. It uses the Pingora CONNECT app.
- `crates/avirtinitd/` contains the `avirtinitd` binary, which runs in the workload (`AVIRT_AGENT=claude|codex`).
- `crates/avirt-connect/` contains the `avirt-connect` library, CONNECT authority parsing and destination checks for the Pingora CONNECT app.
- `crates/avirt-core/` contains the shared library. It contains the control API types (`api`) and log envelope, along with the h2c client and the client for the workload's HTTP/2 connection to its mediator. It includes the `avirthostd` server and workload registry, plus the `avirtinitd` harness and runtime directory.
- `workloads/claude/` and `workloads/codex/` contain the image build directories, each with a Containerfile and `image/` holding the pinned adapter. The images are `avirt-workload-claude:dev` and `avirt-workload-codex:dev`.

## Prerequisites

- An Apple silicon Mac with Apple `container` 1.5.0 or later. mise can't install `container`.
- [mise](https://mise.jdx.dev). `mise.toml` pins Rust 1.99.0 and defines the tasks.
- For Claude, put `CLAUDE_CODE_OAUTH_TOKEN` in the uncommitted `mise.local.toml` under `[env]`. mise loads that file on top of `mise.toml`.
- For Codex, run `codex login` once, which writes `~/.codex/auth.json`.

## Quickstart

```bash
mise install                                                      # the Rust toolchain
mise run build-images                                             # both sandbox images
mise run serve                                                    # terminal 1: avirthostd
mise run launch claude "Reply with the word pong" -- --follow     # terminal 2: avirt launch
mise run launch codex "Reply with the word pong" -- --follow
mise run check                                                    # fmt --check, clippy -D warnings, tests
```

Extra flags go after `--`, such as `-- --cwd DIR --follow`. `--cwd` mounts that host directory read-write at `/workspace`.

## Caveats

- avirt approves permission requests automatically. The agent can run any tool inside the workload with no human in the loop.
- The control API on `127.0.0.1:8787` has no authentication. Any local process can start workloads that use your Claude token or ChatGPT login and mount host directories.
- The Claude token is in the agent's environment, and the agent can read Codex's `auth.json`. With automatic approval, either can end up in the workload log. Logs also contain prompts and tool output. They include the Codex account email and plan. Treat logs as secret.
- avirt allows and logs egress. It filters only private and host destinations.
- codex-acp 2.2.2 omits the first chunk of each command's terminal output from Codex workload logs.
