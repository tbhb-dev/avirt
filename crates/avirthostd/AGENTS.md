---
type: Agent Instructions
title: "Agent instructions: avirthostd"
description: "Guidance for AI coding agents working in the `avirthostd` crate."
tags: [avirt, crate, avirthostd]
generated: { by: claude-code/claude-sonnet-5-5, at: 2026-10-17T19:16:42Z }
---

# Agent instructions: avirthostd

The `avirthostd` binary. Most of the server lives in `avirt-core` (`hostd`, `egress`, `state`), and this crate wires it to the two agent profiles.

## Layout

| Module | Holds |
| --- | --- |
| `main.rs` | Flag parsing (`--port`, `--logs-dir`, `--mediator-bin`) and the call into `avirt_core::hostd::serve` |
| `profiles.rs` | The per-agent launch profiles: image names (`CLAUDE_IMAGE`, `CODEX_IMAGE`), the variables passed to the workload, and the per-launch hook with its cleanup |
| `codex_auth.rs` | The Codex `CODEX_HOME` seed and the `auth.json` write-back |

## Invariants

- Secrets reach the workload as `-e NAME` with no value, so they never appear on argv. A profile only names variables, it never formats their values into arguments.
- A launch whose credentials are missing is refused with a `Refused` message (a 400). It is never started without them.
- Everything read back from the workload's `CODEX_HOME` is untrusted, because the agent can write to it. Open it without following symlinks, accept only a regular file of at most 1 MiB, and never log or return text that may quote its contents. Outcomes are fixed reason strings.
- A write-back happens only under the `auth.json.lock` flock, only when the workload's `last_refresh` is strictly newer than the host's, and only when the keys, `auth_mode` and account match the seed. It goes through an exclusive 0600 temp file (`auth.json.avirt-tmp-<random>`), fsync and rename. The first host file is backed up once as `auth.json.pre-avirt-backup`, and that backup is never overwritten.
- Cleanup runs exactly once on every path, and the temp home is always removed afterwards.

## Testing

```bash
cargo test -p avirthostd
```

The `codex_auth` tests build host and workload `auth.json` fixtures in temp directories and cover every write-back outcome. Never point a test at the real `~/.codex/`.
