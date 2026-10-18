---
type: README
title: avirt
description: "`avirt` runs ACP agents in isolated containers on an Apple silicon Mac."
tags: [avirt, readme]
generated: { by: claude-code/claude-sonnet-5-5, at: 2026-10-17T19:16:42Z }
---

# avirt

`avirt` runs ACP agents in isolated containers on an Apple silicon Mac. The supported agents are Claude, through [claude-agent-acp](https://github.com/agentclientprotocol/claude-agent-acp), and Codex, through [codex-acp](https://github.com/agentclientprotocol/codex-acp). The host daemon, `avirthostd`, launches one workload per request. A workload is a workload container plus its egress mediator (`avirtmediatord`). Inside the workload container, `avirtinitd` drives the agent over ACP and streams every JSON-RPC message to `avirthostd`, which writes it to `logs/<workload-id>.ndjson`. The workload container has no network: the agent's only way out is the mediator's logged CONNECT proxy. Everything is Rust and uses the official [`agent-client-protocol`](https://github.com/agentclientprotocol/rust-sdk) crate.

Agents working in this repository should read [AGENTS.md](AGENTS.md). See [docs/architecture.md](docs/architecture.md) for the architecture, both the implemented system and the planned design, and [docs/build-and-platforms.md](docs/build-and-platforms.md) for build targets and platforms.

## Layout

- `crates/avirthostd/`: the `avirthostd` binary (running it starts the server), the per-agent launch profiles, and Codex's credential handling.
- `crates/avirt/`: the `avirt` operator CLI (`avirt launch`).
- `crates/avirtmediatord/`: the `avirtmediatord` binary, the per-workload egress proxy: Pingora by default, with an opt-in Envoy engine.
- `crates/avirtmedproxyd/`: the `avirtmedproxyd` binary, the mediator proxy: supervises the mediator's Envoy with libavirtenvoy and serves it local xDS and SDS (not implemented).
- `crates/avirtinitd/`: the `avirtinitd` binary, which runs in the workload (`AVIRT_AGENT=claude|codex`).
- `crates/avirt-connect/`: the `avirt-connect` library, the CONNECT authority parsing and destination checks that both of avirtmediatord's engines share.
- `crates/avirt-core/`: the shared library, covering the control API types (`api`), the log envelope, the h2c client and the client for the workload's HTTP/2 connection to its mediator, the `avirthostd` server and workload registry, the `avirtinitd` harness and the runtime directory.
- `crates/avirtadmin/`: the `avirtadmin` binary, the local admin CLI (not implemented).
- `crates/avirtwebd/`: the `avirtwebd` binary, the backend for the avirt web UI (not implemented).
- `crates/avirtapid/`: the `avirtapid` binary, the REST and gRPC proxy to the local avirthostd and to peers' avirtapid (not implemented).
- `crates/avirtdnsd/`: the `avirtdnsd` binary, the DNS resolver for `<name>.<ns>.svc.host.avirt.internal` and `<name>.<ns>.workload.host.avirt.internal`, which answers with the local avirtedgeproxyd and, for peered names, with their host's avirtedgeproxyd (not implemented).
- `crates/avirtmcpgwd/`: the `avirtmcpgwd` binary, the MCP gateway (not implemented).
- `crates/avirtmodelgwd/`: the `avirtmodelgwd` binary, the model gateway, which injects model credentials (not implemented).
- `crates/avirtbuilderd/`: the `avirtbuilderd` binary, the builder, which watches VCS and builds repo OCI artifacts for fast workload startup (not implemented).
- `crates/avirtpolicyd/`: the `avirtpolicyd` binary, the policy service, which stores, compiles, proves and distributes Cedar policy snapshots (not implemented).
- `crates/avirtregistryd/`: the `avirtregistryd` binary, the workspace image registry, which wraps an OCI registry and adds avirt's index and search API for workspace images (not implemented).
- `crates/avirtsecretsd/`: the `avirtsecretsd` binary, the secrets broker, which fetches secrets from providers, serves the credential broker API, and serves mediator-injected bindings over the Envoy secret discovery service (not implemented).
- `crates/avirtlogd/`: the `avirtlogd` binary, the log collector, which gathers workload stdout and stderr, transforms it and sends it to avirtoteld as OpenTelemetry Logs (not implemented).
- `crates/avirtauditd/`: the `avirtauditd` binary, the audit logger, which delivers audit events as OCSF (not implemented).
- `crates/avirtoteld/`: the `avirtoteld` binary, the OpenTelemetry Collector for the host (not implemented).
- `crates/avirtenvoy/`: the `avirtenvoy` library, the Envoy dynamic module, built outside the root workspace, that checks CONNECT destinations and reports connection events. Cedar policy and credential injection are not implemented.
- `crates/avirtacp/`: the `avirtacp` binary, the workload process that runs an agent through the Agent Client Protocol (not implemented).
- `crates/avirtagent/`: the `avirtagent` binary, the workload process that runs an agent through its native harness (not implemented).
- `crates/avirtexec/`: the `avirtexec` binary, the workload process for generic execution (not implemented).
- `crates/avirtnetd/`: the `avirtnetd` binary, the guest networking daemon: the client end of the workload's HTTP/2 connection to its mediator, the forward proxy, the reverse proxy, the stub DNS resolver and the guest control API (not implemented).
- `crates/avirtguestd/`: the `avirtguestd` binary, the workload services daemon: file server, snapshots and the guest MCP tool surface (not implemented).
- `crates/avirtmcp/`: the `avirtmcp` binary, the MCP stdio proxy to avirtmcpgwd, launched on demand by the agent (not implemented).
- `crates/avirtguest/`: the `avirtguest` binary, the in-image CLI that reaches avirtapid through the mediator (not implemented).
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

Extra flags go after `--`, for example `-- --cwd DIR --follow`. `--cwd` mounts that host directory read-write at `/workspace`. The CLI is described in [docs/operator-cli.md](docs/operator-cli.md), and the `avirthostd` environment variables and log format in [docs/hostd-api.md](docs/hostd-api.md).

## Caveats

- Permission requests are approved automatically, so the agent can run any tool inside the workload with no human in the loop.
- The control API on `127.0.0.1:8787` has no authentication. Any local process can start workloads that use your Claude token or ChatGPT login and mount host directories.
- The Claude token is in the agent's environment, and the agent can read Codex's `auth.json`. With automatic approval, either can end up in the workload log, which also holds prompts, tool output, and the Codex account email and plan. Treat logs as secret.
- Egress is allowed and logged, not filtered, apart from private and host destinations.
- codex-acp 2.2.2 omits the first chunk of each command's terminal output from Codex workload logs.
