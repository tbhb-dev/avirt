---
type: Agent Instructions
title: "Agent instructions: avirt-core"
description: "Guidance for AI coding agents working in the `avirt-core` crate."
tags: [avirt, crate, avirt-core]
generated: { by: claude-code/claude-sonnet-5-5, at: 2026-10-17T19:16:42Z }
---

# Agent instructions: avirt-core

The shared library. The avirthostd server and the avirtinitd harness both live here, so a change can affect the host and the workload at once.

## Layout

| Module | Holds |
| --- | --- |
| `api` | The control API contract shared by avirthostd and the CLI: `DEFAULT_PORT`, `LAUNCH_PATH`, `HEALTH_PATH`, `LaunchRequest`, `LaunchResponse`, `is_terminal_event` |
| `hostd` | The server: `/launch` and `/health` on TCP, `/workloads/:id/stream` on avirthostd's socket, the `WORKLOAD` and `EGRESS` connection headers, launch, teardown and shutdown |
| `egress` | `WorkloadDir` (the 0700 workload directory) and `Proxy`, the mediator process with a cleared environment and a stdin-bound lifetime |
| `state` | The runtime directory `$TMPDIR/avirt-<uid>/`, its lock, the startup cleanup, and `private_dir`, `make_removable` and `remove_tree` |
| `harness` | The avirtinitd harness: the workload's HTTP/2 connection to its mediator first, the TCP proxy to `proxy.sock`, seccomp (Linux only), the scrubbed agent environment, `setpriv` |
| `uplink` | The workload's HTTP/2 connection to its mediator, whose one long-lived POST carries NDJSON envelopes with `x-avirt-token` |
| `envelope` | `Dir`, `Envelope`, `parse_line`, `MAX_LINE` (16 MiB) and `LineSplitter` |
| `h2c` | Cleartext HTTP/2 client helpers |

## Invariants

- **Workload startup order.** Nothing starts in the workload container until avirthostd has accepted `POST /workloads/<id>/stream`. Seccomp is installed before the adapter starts, and the agent's environment never carries `AVIRT_WORKLOAD_ID`, `AVIRT_PROMPT`, `AVIRT_CWD`, `AVIRT_UPLINK_TOKEN` or `AVIRT_AGENT`.
- **Proxy half-close.** The TCP proxy to `proxy.sock` never forwards a client's end-of-stream to the socket, because nothing on a forwarded socket may half-close.
- **Log ordering.** `container_exit` is always the last event of a workload. Errors are held and written before it.
- **Oversized lines.** A line over `MAX_LINE` is dropped whole and replaced by `line_truncated`. A fragment of it is never written.
- **`POST /workloads/<id>/stream`.** A request with a missing or wrong token gets 403 before any workload state changes. The mediator secret is compared in constant time.
- **File modes.** Logs are 0700 directories and 0600 files. The runtime directory and workload directories are 0700.
- **Sweep.** The sweep never writes anything back to the host. Removal restores permissions first and never follows symlinks.
- **No secrets on argv.** No secret is ever placed on argv or in a log line.

## Testing

```bash
cargo test -p avirt-core
```

The tests cover the envelope splitter, the runtime directory (lock, sweep, unremovable homes), `api::is_terminal_event`, and option picking. The seccomp filter compiles only for Linux targets, so it is exercised end to end inside a workload.
