---
type: Agent Instructions
title: "Agent instructions: avirthostd"
description: "Guidance for AI coding agents working in the `avirthostd` crate."
tags: [avirt, crate, avirthostd]
generated: { by: codex/gpt-6, at: 2026-10-18T02:56:31Z }
---

# Agent instructions: avirthostd

The `avirthostd` binary. Most of the server is in `avirt-core` (`hostd`, `runtime`, `egress`, `state`), and this crate wires it to the two agent profiles.

## Layout

| Module | Holds |
| --- | --- |
| `main.rs` | Flag parsing (`--port`, `--logs-dir`, `--mediator-bin`, `--runtime`, `--vmd-bin`, `--image-archives`) and the call into `avirt_core::hostd::serve` with the chosen runtime |
| `profiles.rs` | The per-agent launch profiles: image names (`CLAUDE_IMAGE`, `CODEX_IMAGE`), the variables passed to the workload, and the per-launch hook with its cleanup |
| `codex_auth.rs` | The Codex `CODEX_HOME` seed and the `auth.json` write-back |

## Conditions

- Pass secrets to the workload as `EnvVar::Inherit`, so no runtime puts a value on argv. A profile only names variables, it never formats their values into arguments.
- Refuse a launch whose credentials are missing with a `Refused` message (a 400). Never start it without them.
- Treat everything read back from the workload's `CODEX_HOME` as untrusted, because the agent can write to it. Open it without following symlinks, accept only a regular file of at most 1 MiB, and never log or return text that may quote its contents. Use fixed reason strings for outcomes.
- Write-back requires the `auth.json.lock` flock and a workload `last_refresh` strictly newer than the host's. The keys, `auth_mode`, and account must also match the seed. It goes through an exclusive 0600 temp file (`auth.json.avirt-tmp-<random>`), fsync, and rename. Back up the first host file once as `auth.json.pre-avirt-backup`. Never overwrite that backup.
- Run cleanup exactly once on every path. Always remove the temp home afterwards.

## Testing

```bash
cargo test -p avirthostd
```

The `codex_auth` tests build host and workload `auth.json` fixtures in temp directories and cover every write-back outcome. Never point a test at the real `~/.codex/`.
