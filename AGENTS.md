---
type: Agent Instructions
title: Agent instructions
description: Guidance for AI coding agents working in this repository.
tags: [avirt, agents]
generated: { by: codex/gpt-6, at: 2026-10-18T04:56:05Z }
---

# Agent instructions

Guidance for AI coding agents working in this repository. Read it alongside the per-crate `AGENTS.md` files and any memory files the harness loads.

## What this is

`avirt` runs ACP agents (Claude and Codex) in isolated workloads on an Apple silicon Mac. The host daemon, `avirthostd`, launches one workload per request, and every byte the agent sends out passes through that workload's mediator. [docs/architecture.md](docs/architecture.md) is the entry point to the design, and it distinguishes working features from plans.

## Crate map

Every crate lives directly under `crates/`, and each directory uses its package name.

| Crate | Kind | Runs | Role |
| --- | --- | --- | --- |
| `avirthostd` | Binary | Host | The host daemon: control API, workload launch and teardown, logs, Codex `auth.json` write-back |
| `avirt` | Binary `avirt` | Host | The operator CLI: `avirt launch` |
| `avirtmediatord` | Binary | Host (one per workload) | The implemented host mediator. Planned: one process for workload traffic and lifecycle. Holds its SVID and handles rotation. |
| `avirtvmd` | Binary (Swift) | Host (one per workload) | The per-workload VM host helper, a Swift package built on Containerization. It hosts the workload VM and prepares the guest artifacts |
| `avirtinitd` | Binary | Linux guest only (the workload) | The ACP client spawns the adapter and taps JSON-RPC. It drops to uid 1000 under seccomp |
| `avirt-connect` | Library | Both | CONNECT authority parsing and destination checks for the Pingora CONNECT app |
| `avirt-core` | Library | Both | Shared code for the `api` module and the `avirthostd` server. Includes the `avirtinitd` harness and the client for the workload's HTTP/2 connection to its mediator. Also includes the log envelope and runtime directory |
| `avirtadmin` | Binary | Host | The local admin CLI (not implemented) |
| `avirtwebd` | Binary | Host | The backend for the avirt web UI (not implemented) |
| `avirtapid` | Binary | Host | The API server: the gRPC and REST API in front of the local avirthostd and peers' API servers, reached through the edge proxy (not implemented) |
| `avirtedgeproxyd` | Binary | Host | The edge proxy: host policy evaluation, external resolution, private-address checks and upstream connections (not implemented) |
| `avirtdnsd` | Binary | Host | The DNS resolver for `<name>.<ns>.svc.host.avirt.internal` and `<name>.<ns>.workload.host.avirt.internal`, which answers with the local avirtedgeproxyd or, for avirt hostnames under a peer's host name, their host's avirtedgeproxyd (not implemented) |
| `avirtmcpgwd` | Binary | Host | The MCP gateway (not implemented) |
| `avirtmodelgwd` | Binary | Host | The model gateway, which injects model credentials (not implemented) |
| `avirtbuilderd` | Binary | Host | The builder, which watches VCS and builds repository OCI artifacts for fast workload startup (not implemented) |
| `avirtpolicyd` | Binary | Host | The policy service, which stores, compiles, proves, and distributes Cedar policy snapshots (not implemented) |
| `avirtregistryd` | Binary | Host | The workspace image registry, which wraps an OCI registry and adds avirt's index and search API for workspace images (not implemented) |
| `avirtsecretsd` | Binary | Host | The secrets broker, which fetches secrets from providers and serves the credential broker API. It serves leases for mediator-injected bindings through the broker API (not implemented) |
| `avirtlogd` | Binary | Host | The log collector, which gathers workload stdout and stderr, transforms it and sends it to avirtoteld as OpenTelemetry Logs (not implemented) |
| `avirtauditd` | Binary | Host | The audit logger, which delivers audit events as OCSF (not implemented) |
| `avirtoteld` | Binary | Host | The OpenTelemetry Collector for the host (not implemented) |
| `avirtacp` | Binary | Workload guest | The workload process that runs an agent through the Agent Client Protocol (not implemented) |
| `avirtagent` | Binary | Workload guest | The workload process that runs an agent through its native harness (not implemented) |
| `avirtexec` | Binary | Workload guest | The workload process for generic execution (not implemented) |
| `avirtnetd` | Binary | Workload guest | The guest networking daemon provides the client end of the workload's HTTP/2 connection to its mediator. It also provides the forward and reverse proxies, the stub DNS resolver, and the guest control API (not implemented) |
| `avirtguestd` | Binary | Workload guest | The workload services daemon provides a file server and snapshots, plus the guest MCP tool surface (not implemented) |
| `avirtmcp` | Binary | Workload guest | The MCP stdio proxy to avirtmcpgwd, launched on demand by the agent (not implemented) |
| `avirtguest` | Binary | Workload guest | The guest CLI that's in the image and connects to avirtapid through the mediator (not implemented) |

`avirt-connect` is part of the root workspace.

`workloads/claude/` and `workloads/codex/` contain only the sandbox image build directories.

## Commands

```bash
mise run bootstrap                                               # first step in a fresh clone or worktree: vendir sync, toolchain, Vale styles, git hooks
mise install                                                      # pins Rust
mise run check                                                    # fmt --check, clippy, rustdoc, shear, deny, nextest, doc tests, docs checks
mise run build-images                                             # both sandbox images and their OCI archives (needs Apple container)
mise run serve                                                    # avirthostd on the Containerization runtime (builds avirtvmd too)
mise run launch claude "Reply with the word pong" -- --follow     # avirt launch, in a second terminal
```

`mise run check` is the gate for every change. The images rebuild `avirtinitd` from the workspace, so an `avirtinitd` or `avirt-core` change needs `mise run build-images` before an end-to-end run sees it.

### Rust toolchain

Any lint in the `all` and `pedantic` groups fails the build, and so does each nursery, cargo and restriction lint that `Cargo.toml` lists. The comment that precedes that list says why each missing lint is missing. Thresholds live in `clippy.toml`. Fix a finding in the code. The tree has no lint attributes and no per-lint exceptions, so a new one is a defect.

```bash
mise run lint-rust       # clippy, all targets, warnings denied
mise run lint-rustdoc    # rustdoc over private items, warnings denied
mise run lint-shear      # cargo-shear: unused dependencies
mise run deny            # cargo-deny: advisories, licenses, bans, sources
mise run test            # nextest, then the doc tests
mise run coverage        # cargo-llvm-cov with the line and region floors
mise run mutants         # cargo-mutants baseline; no gate
mise run fuzz-smoke      # every fuzz target for 20 seconds; needs the pinned nightly
```

Pre-commit hooks run rustfmt, cargo-shear and cargo-deny on the files they cover, and pre-push hooks run clippy, rustdoc, and the tests. Coverage, mutants, and fuzzing run in no hook. CI fails a pull request that falls below the coverage floors, and a daily schedule measures mutants, coverage history, and fuzzing.

## Conventions

- **Terms.** Use the project's terms consistently. The project name is always lowercase **avirt**. **avirthostd** is the host daemon. The **control plane** is avirthostd plus the host services that configure and decide, and the **data plane** is **avirtmediatord** (the mediator) and **avirtedgeproxyd** (the edge proxy), which forward traffic. A **workload** is the whole unit avirthostd launches. It consists of the **sandbox container**, which runs the agent, plus its **mediator**. The mediator is the per-workload container that contains avirtinitd and avirtmediatord. All the workload's traffic passes through it. Today the mediator is the avirtmediatord host process. **avirtinitd** is the root process in each container. **avirtapid** is the API server. An ACP session stays an ACP session. See [docs/glossary.md](docs/glossary.md) for every other term.
- **Docs format.** Every Markdown file in the repository has OKF v0.2 frontmatter, and `docs/` is an OKF v0.2 bundle:
  - every document has YAML frontmatter with a non-empty `type`, plus `title`, `description`, `tags` and `generated`, and `implementation` where it applies (every document in `docs/`, and crate READMEs).
  - `docs/index.md` lists every document under `docs/`, and Markdown files outside `docs/` don't need an index entry.
  - after editing, run `mise run check-docs`, which runs `scripts/okf-check.py docs` and `scripts/okf-check.py --repo .`.
- **Docs content.** Docs and code comments describe what the system is and does. Don't include history or decision rationale. State facts that constrain behavior as constraints. Mark planned work with a `Status: planned` section, never with version names.
- **Markdown.** Diagrams are Mermaid. Each paragraph is one line, with no hard wrapping. Run `guard-markdown <file>` on every Markdown file you edit.
- **Invented terms.** Don't coin terms. Write for the reader in ordinary words, or use terms established by the glossary or a standard. Vale's banned-term rules block lint. Don't add a term to the Vale lists yourself.
- **Jargon report.** `scripts/jargon_check.py` (`mise run lint-jargon`) is an optional report for human review, outside lint and pre-commit. It prints candidate terms from added prose and exits 0 whatever it finds. A usage error such as a bad `--base` exits 1. Judge each candidate in context. Preserve correct grammar and reserve glossary entries for real terms. Its tests are part of `mise run check`. Set `JARGON_BASE` to the parent branch when checking a stacked branch. An explicit `--base` takes precedence. The report skips a term that a glossary heading, a line of `.vale/config/vocabularies/project/accept.txt`, or a line of `jargon-accept.txt` at the repository root names. Each `jargon-accept.txt` line is a term, then `#` and the reason, and the report flags a line with no reason.

## Secrets

- Never print, log or echo `CLAUDE_CODE_OAUTH_TOKEN` or the contents of any `auth.json`.
- The token is in `mise.local.toml`. Git ignores this file, and mise loads it on top of `mise.toml`. Don't commit it, copy it elsewhere, or read it into output.
- Don't change `~/.codex/` yourself. `avirthostd`'s own write-back is the only thing that changes `~/.codex/auth.json`.
- Workload logs (`logs/`) can contain prompts and tool output. They can also contain credentials and the Codex account email and plan. Treat them as secret, and quote only the lines you need.

## End-to-end checks

An end-to-end run launches a real workload:

1. Start avirthostd with `mise run serve`.
2. Launch with `mise run launch <agent> "<prompt>" -- --follow`.
3. Read `logs/<workload-id>.ndjson`. It ends with a `container_exit` meta event, and the `dir: "egress"` lines record every proxied connection. On the Containerization runtime, `logs/<workload-id>.stdio.ndjson` holds the container's stdout, stderr, and console, and `logs/<workload-id>.avirtvmd.log` holds the helper's stderr. If the VM fails to start after its stdio log opens, avirtvmd renames the log to `logs/<workload-id>.stdio.ndjson.failed`, which keeps the boot log.

`AVIRT_RUNTIME` picks the runtime: `containerization` (the default) or `container`, which drives Apple's `container` command. The Containerization runtime runs without a `container` binary.

The regression suite, with its bypass, positive and lifecycle checks, is in [docs/roadmap.md](docs/roadmap.md#regression-suite). Run bypass checks inside the agent's own process tree, by prompting the agent to run the script. A process started from outside the agent's tree isn't under its restrictions, and checking from there doesn't prove the restrictions apply.

## Signing

`avirtvmd` on macOS must have the `com.apple.security.virtualization` entitlement, and `mise run build-vmd` signs it. `avirthostd` doesn't need the entitlement. [docs/build-and-platforms.md](docs/build-and-platforms.md) describes the targets, the signing setup, and the CI matrix.
