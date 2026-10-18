---
type: README
title: avirt
description: "`avirt` runs ACP agents in isolated containers on an Apple silicon Mac."
tags: [avirt, readme]
generated: { by: codex/gpt-6, at: 2026-10-18T04:57:34Z }
---

# avirt

`avirt` runs ACP agents in isolated containers on an Apple silicon Mac. The supported agents are Claude, through [claude-agent-acp](https://github.com/agentclientprotocol/claude-agent-acp), and Codex, through [codex-acp](https://github.com/agentclientprotocol/codex-acp). `avirthostd`, the host daemon, launches one workload per request. A workload is a workload container plus its egress mediator (`avirtmediatord`). Inside the workload container, `avirtinitd` drives the agent over ACP and streams every JSON-RPC message to `avirthostd`, which writes it to `logs/<workload-id>.ndjson`. Without a workload network, the agent's only way out is the mediator's logged CONNECT proxy. All code is Rust and uses the official [`agent-client-protocol`](https://github.com/agentclientprotocol/rust-sdk) crate.

Agents working in this repository should read [AGENTS.md](AGENTS.md). See [docs/architecture.md](docs/architecture.md) for the architecture, both the implemented system and the planned design, and [docs/build-and-platforms.md](docs/build-and-platforms.md) for build targets and platforms.

## Layout

- `crates/avirthostd/` contains the `avirthostd` binary (running it starts the server) and the per-agent launch profiles. It also handles Codex's credentials.
- `crates/avirt/` contains the `avirt` operator CLI (`avirt launch`).
- `crates/avirtmediatord/` contains the `avirtmediatord` binary, the per-workload egress proxy. Pingora is the default, with an opt-in Envoy engine.
- `crates/avirtmedproxyd/` contains the `avirtmedproxyd` binary, the mediator proxy: supervises the mediator's Envoy with libavirtenvoy and serves it local xDS and SDS (not implemented).
- `crates/avirtinitd/` contains the `avirtinitd` binary, which runs in the workload (`AVIRT_AGENT=claude|codex`).
- `crates/avirt-connect/` contains the `avirt-connect` library, the CONNECT authority parsing and destination checks that both of avirtmediatord's engines share.
- `crates/avirt-core/` contains the shared library. It contains the control API types (`api`) and log envelope, along with the h2c client and the client for the workload's HTTP/2 connection to its mediator. It includes the `avirthostd` server and workload registry, plus the `avirtinitd` harness and runtime directory.
- `crates/avirtadmin/` contains the `avirtadmin` binary, the local admin CLI (not implemented).
- `crates/avirtwebd/` contains the `avirtwebd` binary, the backend for the avirt web UI (not implemented).
- `crates/avirtapid/` contains the `avirtapid` binary, the REST and gRPC proxy to the local avirthostd and to peers' avirtapid (not implemented).
- `crates/avirtdnsd/` contains the `avirtdnsd` binary, the DNS resolver for `<name>.<ns>.svc.host.avirt.internal` and `<name>.<ns>.workload.host.avirt.internal`, which answers with the local avirtedgeproxyd or, for peered names, their host's avirtedgeproxyd (not implemented).
- `crates/avirtmcpgwd/` contains the `avirtmcpgwd` binary, the MCP gateway (not implemented).
- `crates/avirtmodelgwd/` contains the `avirtmodelgwd` binary. The model gateway injects model credentials (not implemented).
- `crates/avirtbuilderd/` contains the `avirtbuilderd` binary. The builder watches VCS and builds repository OCI artifacts for fast workload startup (not implemented).
- `crates/avirtpolicyd/` contains the `avirtpolicyd` binary. The policy service stores, compiles, proves, and distributes Cedar policy snapshots (not implemented).
- `crates/avirtregistryd/` contains the `avirtregistryd` binary. The workspace image registry wraps an OCI registry and adds avirt's index and search API for workspace images (not implemented).
- `crates/avirtsecretsd/` contains the `avirtsecretsd` binary. The secrets broker fetches secrets from providers and serves the credential broker API. It serves mediator-injected bindings over the Envoy secret discovery service (not implemented).
- `crates/avirtlogd/` contains the `avirtlogd` binary. The log collector gathers workload stdout and stderr, transforms it and sends it to avirtoteld as OpenTelemetry Logs (not implemented).
- `crates/avirtauditd/` contains the `avirtauditd` binary. The audit logger delivers audit events as OCSF (not implemented).
- `crates/avirtoteld/` contains the `avirtoteld` binary, the OpenTelemetry Collector for the host (not implemented).
- `crates/avirtenvoy/` contains the `avirtenvoy` library. This Envoy <!-- vale ai-tells.OverusedVocabulary = NO -->dynamic<!-- vale ai-tells.OverusedVocabulary = YES --> module builds outside the root workspace and checks CONNECT destinations and reports connection events. It doesn't support Cedar policy or credential injection.
- `crates/avirtacp/` contains the `avirtacp` binary, the workload process that runs an agent through the Agent Client Protocol (not implemented).
- `crates/avirtagent/` contains the `avirtagent` binary, the workload process that runs an agent through its native harness (not implemented).
- `crates/avirtexec/` contains the `avirtexec` binary, the workload process for generic execution (not implemented).
- `crates/avirtnetd/` contains the `avirtnetd` binary, the guest networking daemon. It provides the client end of the workload's HTTP/2 connection to its mediator and the forward and reverse proxies, plus the stub DNS resolver and guest control API (not implemented).
- `crates/avirtguestd/` contains the `avirtguestd` binary, the workload services daemon. It provides a file server and snapshots, plus the guest MCP tool surface (not implemented).
- `crates/avirtmcp/` contains the `avirtmcp` binary, the MCP stdio proxy to avirtmcpgwd, launched on demand by the agent (not implemented).
- `crates/avirtguest/` contains the `avirtguest` binary, the guest CLI that's in the image and connects to avirtapid through the mediator (not implemented).
- `workloads/claude/` and `workloads/codex/` contain the image build directories, each with a Containerfile and `image/` holding the pinned adapter. The images are `avirt-workload-claude:dev` and `avirt-workload-codex:dev`.

## Prerequisites

- An Apple silicon Mac with Apple `container` 1.5.0 or later. mise can't install `container`.
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

Extra flags go after `--`, such as `-- --cwd DIR --follow`. `--cwd` mounts that host directory read-write at `/workspace`. [docs/operator-cli.md](docs/operator-cli.md) describes the command-line interface. [docs/hostd-api.md](docs/hostd-api.md) describes the `avirthostd` environment variables and log format.

## Caveats

- avirt approves permission requests automatically. The agent can run any tool inside the workload with no human in the loop.
- The control API on `127.0.0.1:8787` has no authentication. Any local process can start workloads that use your Claude token or ChatGPT login and mount host directories.
- The Claude token is in the agent's environment, and the agent can read Codex's `auth.json`. With automatic approval, either can end up in the workload log. Logs also contain prompts and tool output. They include the Codex account email and plan. Treat logs as secret.
- avirt allows and logs egress. It filters only private and host destinations.
- codex-acp 2.2.2 omits the first chunk of each command's terminal output from Codex workload logs.
