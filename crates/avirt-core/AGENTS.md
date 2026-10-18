---
type: Agent Instructions
title: "Agent instructions: avirt-core"
description: "Guidance for AI coding agents working in the `avirt-core` crate."
tags: [avirt, crate, avirt-core]
generated: { by: codex/gpt-6, at: 2026-10-18T04:29:26Z }
---

# Agent instructions: avirt-core

The shared library. The avirthostd server and the avirtinitd harness both live here, so a change can affect the host and the workload at once.

## Layout

| Module | Holds |
| --- | --- |
| `api` | The control API contract shared by avirthostd and the command-line interface: `DEFAULT_PORT`, `LAUNCH_PATH`, `HEALTH_PATH`, `LaunchRequest`, `LaunchResponse`, `is_terminal_event` |
| `hostd` | The server: `/launch` and `/health` on TCP, `/workloads/:id/stream` on avirthostd's socket, the `WORKLOAD` and `EGRESS` connection headers, launch, teardown, and shutdown |
| `egress` | `WorkloadDir` (the 0700 workload directory) and `Proxy`, the mediator process with a cleared environment and a stdin-bound lifetime |
| `state` | The runtime directory `$TMPDIR/avirt-<uid>/`, its lock, the startup cleanup, and `private_dir`, `make_removable` and `remove_tree` |
| `harness` | The avirtinitd harness starts the workload's HTTP/2 connection to its mediator first. It provides the TCP proxy to `proxy.sock` and seccomp (Linux only), plus the scrubbed agent environment and `setpriv` |
| `connection` | The workload's HTTP/2 connection to its mediator, whose one long-lived POST sends NDJSON envelopes with `x-avirt-token` |
| `envelope` | `Dir`, `Envelope`, `parse_line`, `MAX_LINE` (16 MiB) and `LineSplitter` |
| `h2c` | Cleartext HTTP/2 client functions |

## Invariants

- **Workload startup order.** Don't start anything in the workload container until avirthostd has accepted `POST /workloads/<id>/stream`. Install seccomp before the adapter starts. The agent's environment must never contain `AVIRT_WORKLOAD_ID`, `AVIRT_PROMPT`, `AVIRT_CWD`, `AVIRT_CONNECTION_TOKEN` or `AVIRT_AGENT`.
- **Proxy half-close.** The TCP proxy to `proxy.sock` never forwards a client's end-of-stream to the socket, because nothing on a forwarded socket may half-close.
- **Log ordering.** `container_exit` is always the last event of a workload. Buffer errors and write them before it.
- **Long lines.** Drop a line over `MAX_LINE` whole and replace it with `line_truncated`. Never write a fragment of it.
- **`POST /workloads/<id>/stream`.** A request with a missing or wrong token gets 403 before any workload state changes. Compare the mediator secret in constant time.
- **File modes.** Logs are 0700 directories and 0600 files. The runtime directory and workload directories are 0700.
- **Sweep.** The sweep never writes anything back to the host. Removal restores permissions first and never follows symlinks.
- **Mediator environment.** Forward only `PATH`, `HOME` and `TMPDIR` from the host environment, plus the per-workload `AVIRT_MEDIATOR_SECRET`.
- **Secrets.** Never place a secret on argv or in a log line.

## Testing

```bash
cargo test -p avirt-core
```

The tests cover the envelope splitter, the runtime directory (lock, sweep, unremovable homes), `api::is_terminal_event`, and option picking. The seccomp filter compiles only for Linux targets, so exercise it end to end inside a workload.
