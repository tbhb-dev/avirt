---
type: Agent Instructions
title: "Agent instructions: avirt"
description: "Guidance for AI coding agents working in the `avirt` crate."
tags: [avirt, crate, cli]
generated: { by: claude-code/claude-sonnet-5-5, at: 2026-10-17T19:16:42Z }
---

# Agent instructions: avirt

The `avirt` binary, in `src/main.rs`.

## Layout

| Item | Holds |
| --- | --- |
| `Command::Launch` | The `launch` subcommand and its flags (`--agent`, `--cwd`, `--follow`, `--port`, `--url`) |
| `launch` | Builds a `LaunchRequest`, makes `--cwd` absolute, posts it to `/launch`, and prints the actor id and log path |
| `follow` | Reads the actor log as it grows and probes `/health` when it stalls |

## Invariants

- Request and response types, routes and the default port come from `avirt_core::api`. Don't redefine them here, because avirthostd and the CLI must agree on them.
- `--follow` stops only on `api::is_terminal_event` (the `container_exit` meta event, parsed as JSON). Never pattern-match raw log text.
- After 5 s without log growth, `follow` probes `POST /health` with a 3 s timeout, and gives up only when avirthostd does not answer.
- Print only the actor id, the log path and the log lines. Never print request bodies or environment values.

## Testing

```bash
cargo test -p avirt
```

The unit test checks that an unset `--cwd` is omitted from the request. End-to-end behavior needs avirthostd running (see the root `AGENTS.md`).
