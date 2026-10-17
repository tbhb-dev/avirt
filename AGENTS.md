# Agent instructions

Guidance for AI coding agents working in this repository. Read it alongside the per-crate `AGENTS.md` files and any memory files the harness loads.

## What this is

agentvmm-poc runs ACP agents (Claude and Codex) in isolated workloads on an Apple silicon Mac. A host controller launches one actor per request, and every byte the agent sends out passes through a per-actor egress supervisor. [docs/architecture.md](docs/architecture.md) is the entry point to the design, and it marks what is implemented and what is planned.

## Crate map

Every crate lives directly under `crates/`, and each directory is named after its package.

| Crate | Kind | Runs | Role |
| --- | --- | --- | --- |
| `agentvmm-controller` | Binary | Host | The controller: control API, actor launch and teardown, logs, Codex `auth.json` write-back |
| `agentvmm-cli` | Binary `agentvmm` | Host | The operator CLI: `agentvmm launch` |
| `agentvmm-supervisor` | Binary | Host (one per actor) | The Pingora CONNECT proxy and the uplink splice |
| `agentvmm-runner` | Binary | Linux guest only (the workload) | The ACP client: spawns the adapter, taps JSON-RPC, drops to uid 1000 under seccomp |
| `agentvmm-core` | Library | Both | Shared code: the `api` module, the controller server, the runner harness, the uplink, the log envelope, the state root |

`workloads/claude/` and `workloads/codex/` hold only the workload image build directories.

## Commands

```bash
mise install                                                      # pins Rust
mise run check                                                    # fmt --check, clippy -D warnings, tests
mise run build-images                                             # both workload images (needs Apple container)
mise run serve                                                    # agentvmm-controller
mise run launch claude "Reply with the word pong" -- --follow     # agentvmm launch, in a second terminal
```

`mise run check` is the gate for every change. The images rebuild the runner from the workspace, so a runner or core change needs `mise run build-images` before an end-to-end run sees it.

## Conventions

- **Terms.** Use the project's terms consistently. The **controller** is the host process. The **supervisor** is the per-actor egress proxy. The **runner** is the root process inside the **workload**, which is the container that runs the agent. An **actor** is one supervisor plus one workload. An ACP session stays an ACP session.
- **Docs format.** `docs/` is an OKF v0.2 bundle:
  - every document has YAML frontmatter with a non-empty `type`, plus `title`, `description`, `tags`, `implementation` and `generated`;
  - `docs/index.md` lists every document;
  - after editing, run `uv run --no-project --with pyyaml python scripts/okf-check.py docs`.
- **Docs content.** Docs and code comments describe what the system is and does. They hold no history and no decision rationale. A fact that constrains behavior is stated as a constraint. Planned work is marked with a `Status: planned` section, never with version names.
- **Markdown.** Diagrams are Mermaid. Each paragraph is a single line, with no hard wrapping. Run `guard-markdown <file>` on every Markdown file you touch.

## Secrets

- Never print, log or echo `CLAUDE_CODE_OAUTH_TOKEN` or the contents of any `auth.json`.
- The token lives in `mise.local.toml`, which is gitignored and loaded by mise on top of `mise.toml`. Don't commit it, copy it elsewhere, or read it into output.
- Don't modify `~/.codex/` yourself. The controller's own write-back is the only thing that changes `~/.codex/auth.json`.
- Actor logs (`logs/`) can contain prompts, tool output, credentials and the Codex account email and plan. Treat them as secret, and quote only the lines you need.

## End-to-end checks

An end-to-end run launches a real actor:

1. Start the controller with `mise run serve`.
2. Launch with `mise run launch <agent> "<prompt>" -- --follow`.
3. Read `logs/<actor-id>.ndjson`. It ends with a `container_exit` meta event, and the `dir: "egress"` lines record every proxied connection.

The regression suite, with its bypass, positive and lifecycle checks, is in [docs/roadmap.md](docs/roadmap.md#regression-suite). Run bypass checks inside the agent's own process tree, by prompting the agent to run the script. A process started from outside the agent's tree is not under its restrictions, and checking from there proves nothing.

## Signing

Once the Containerization runtime lands, the macOS controller must carry the `com.apple.security.virtualization` entitlement. `cargo run` and `cargo test` then go through a signing runner. [docs/build-and-platforms.md](docs/build-and-platforms.md) describes the targets, the signing setup and the CI matrix.
