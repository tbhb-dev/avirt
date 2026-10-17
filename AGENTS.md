---
type: Agent Instructions
title: Agent instructions
description: Guidance for AI coding agents working in this repository.
tags: [avirt, agents]
generated: { by: claude-code/claude-sonnet-5-5, at: 2026-10-17T19:16:42Z }
---

# Agent instructions

Guidance for AI coding agents working in this repository. Read it alongside the per-crate `AGENTS.md` files and any memory files the harness loads.

## What this is

`avirt` runs ACP agents (Claude and Codex) in isolated workloads on an Apple silicon Mac. The host daemon, `avirthostd`, launches one actor per request, and every byte the agent sends out passes through a per-actor egress mediator. [docs/architecture.md](docs/architecture.md) is the entry point to the design, and it marks what is implemented and what is planned.

## Crate map

Every crate lives directly under `crates/`, and each directory is named after its package.

| Crate | Kind | Runs | Role |
| --- | --- | --- | --- |
| `avirthostd` | Binary | Host | The host daemon: control API, actor launch and teardown, logs, Codex `auth.json` write-back |
| `avirt` | Binary `avirt` | Host | The operator CLI: `avirt launch` |
| `avirtmediatord` | Binary | Host (one per actor) | The CONNECT proxy (Pingora by default, optional Envoy) and the uplink splice |
| `avirtinitd` | Binary | Linux guest only (the workload) | The ACP client: spawns the adapter, taps JSON-RPC, drops to uid 1000 under seccomp |
| `avirt-connect` | Library | Both | CONNECT authority parsing and destination checks, shared by both CONNECT engines |
| `avirt-core` | Library | Both | Shared code: the `api` module, the `avirthostd` server, the `avirtinitd` harness, the uplink, the log envelope, the state root |
| `avirtadmin` | Binary | Host | The local admin CLI (not implemented) |
| `avirtwebd` | Binary | Host | The backend for the avirt web UI (not implemented) |
| `avirtproxyd` | Binary | Host | The REST and gRPC proxy to the local avirthostd and to peers' avirtproxyd (not implemented) |
| `avirtdnsd` | Binary | Host | The DNS resolver for `<name>.<ns>.svc.host.avirt.internal` and `<name>.<ns>.workload.host.avirt.internal`, which answers with the local front-door Envoy and, for peered names, with their host's front-door Envoy (not implemented) |
| `avirtmcpgwd` | Binary | Host | The MCP gateway (not implemented) |
| `avirtmodelgwd` | Binary | Host | The model gateway, which injects model credentials (not implemented) |
| `avirtbuilderd` | Binary | Host | The builder, which watches VCS and builds repo OCI artifacts for fast workload startup (not implemented) |
| `avirtpolicyd` | Binary | Host | The policy service, which stores, compiles, proves and distributes Cedar policy snapshots (not implemented) |
| `avirtregistryd` | Binary | Host | The workspace image registry, which wraps an OCI registry and adds avirt's index and search API for workspace images (not implemented) |
| `avirtsecretsd` | Binary | Host | The secrets broker, which fetches secrets from providers and serves them as the Envoy secret discovery service (not implemented) |
| `avirtlogd` | Binary | Host | The log collector, which gathers workload stdout and stderr, transforms it and sends it to avirtoteld as OpenTelemetry Logs (not implemented) |
| `avirtauditd` | Binary | Host | The audit logger, which delivers audit events as OCSF (not implemented) |
| `avirtoteld` | Binary | Host | The OpenTelemetry Collector for the host (not implemented) |
| `avirtenvoy` | Library (cdylib) | Mediator, host | The Envoy dynamic module with CONNECT destination checks and events; Cedar policy and credential injection are planned |
| `avirtacp` | Binary | Workload guest | The workload process that runs an agent through the Agent Client Protocol (not implemented) |
| `avirtagent` | Binary | Workload guest | The workload process that runs an agent through its native harness (not implemented) |
| `avirtexec` | Binary | Workload guest | The workload process for generic execution (not implemented) |
| `avirtnetd` | Binary | Workload guest | The guest networking daemon: link endpoint, egress, ingress, stub DNS resolver and guest control API (not implemented) |
| `avirtguestd` | Binary | Workload guest | The workload services daemon: file server, snapshots and the guest MCP tool surface (not implemented) |
| `avirtmcp` | Binary | Workload guest | The stdio MCP relay to avirtmcpgwd, launched on demand by the agent (not implemented) |
| `avirtguest` | Binary | Workload guest | The in-image CLI that reaches avirtproxyd through the mediator (not implemented) |

`avirt-connect` is part of the root workspace. `avirtenvoy` is an excluded standalone workspace with its own lockfile; build it with `cargo build --locked --manifest-path crates/avirtenvoy/Cargo.toml --target-dir target/envoy-module`. Root builds do not include the Envoy SDK or libclang build dependencies.

`workloads/claude/` and `workloads/codex/` hold only the workload image build directories.

## Commands

```bash
mise install                                                      # pins Rust
mise run check                                                    # fmt --check, clippy -D warnings, tests
mise run build-images                                             # both workload images (needs Apple container)
mise run serve                                                    # avirthostd
mise run launch claude "Reply with the word pong" -- --follow     # avirt launch, in a second terminal
```

`mise run check` is the gate for every change. Changes to the excluded `avirtenvoy` module also require `mise run check-envoy`, which needs the cached SDK git source and libclang. The images rebuild `avirtinitd` from the workspace, so an `avirtinitd` or `avirt-core` change needs `mise run build-images` before an end-to-end run sees it.

## Conventions

- **Terms.** Use the project's terms consistently. The project name is always lowercase **avirt**. **avirthostd** is the host daemon, and the host side as a whole is the **control plane**. The **mediator** (avirtmediatord) is the per-actor egress proxy. **avirtinitd** is the root process inside the **workload**, which is the container that runs the agent. An **actor** is one mediator plus one workload. An ACP session stays an ACP session.
- **Docs format.** Every Markdown file in the repo carries OKF v0.2 frontmatter, and `docs/` is an OKF v0.2 bundle:
  - every document has YAML frontmatter with a non-empty `type`, plus `title`, `description`, `tags` and `generated`, and `implementation` where it applies (every document in `docs/`, and crate READMEs);
  - `docs/index.md` lists every document under `docs/`, and Markdown files outside `docs/` need no index entry;
  - after editing, run `mise run check-docs`, which runs `scripts/okf-check.py docs` and `scripts/okf-check.py --repo .`.
- **Docs content.** Docs and code comments describe what the system is and does. They hold no history and no decision rationale. A fact that constrains behavior is stated as a constraint. Planned work is marked with a `Status: planned` section, never with version names.
- **Markdown.** Diagrams are Mermaid. Each paragraph is a single line, with no hard wrapping. Run `guard-markdown <file>` on every Markdown file you touch.

## Secrets

- Never print, log or echo `CLAUDE_CODE_OAUTH_TOKEN` or the contents of any `auth.json`.
- The token lives in `mise.local.toml`, which is gitignored and loaded by mise on top of `mise.toml`. Don't commit it, copy it elsewhere, or read it into output.
- Don't modify `~/.codex/` yourself. `avirthostd`'s own write-back is the only thing that changes `~/.codex/auth.json`.
- Actor logs (`logs/`) can contain prompts, tool output, credentials and the Codex account email and plan. Treat them as secret, and quote only the lines you need.

## End-to-end checks

An end-to-end run launches a real actor:

1. Start avirthostd with `mise run serve`.
2. Launch with `mise run launch <agent> "<prompt>" -- --follow`.
3. Read `logs/<actor-id>.ndjson`. It ends with a `container_exit` meta event, and the `dir: "egress"` lines record every proxied connection.

The regression suite, with its bypass, positive and lifecycle checks, is in [docs/roadmap.md](docs/roadmap.md#regression-suite). Run bypass checks inside the agent's own process tree, by prompting the agent to run the script. A process started from outside the agent's tree is not under its restrictions, and checking from there proves nothing.

## Signing

Once the Containerization runtime lands, `avirthostd` on macOS must carry the `com.apple.security.virtualization` entitlement. `cargo run` and `cargo test` then go through a signing runner. [docs/build-and-platforms.md](docs/build-and-platforms.md) describes the targets, the signing setup and the CI matrix.
