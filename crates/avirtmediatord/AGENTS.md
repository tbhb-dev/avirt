---
type: Agent Instructions
title: "Agent instructions: avirtmediatord"
description: "Guidance for AI coding agents working in the `avirtmediatord` crate."
tags: [avirt, crate, avirtmediatord]
generated: { by: codex/gpt-6, at: 2026-10-18T05:11:39Z }
---

# Agent instructions: avirtmediatord

The mediator binary. It runs one Pingora `Server` with two Unix-socket services in the workload's directory. The services are the CONNECT proxy and the TCP proxy on `mediator.sock`. In the planned mediator container, `avirtmediatord` handles workload traffic and lifecycle in one process.

## Layout

| Module | Holds |
| --- | --- |
| `main.rs` | Flags (`--dir`, `--hostd-sock`, `--workload`, and the secret from `AVIRT_MEDIATOR_SECRET`), the `ConnectApp` CONNECT proxy, the `SpliceApp` TCP proxy that forwards `mediator.sock` to avirthostd unchanged. Also includes the connection records and shutdown watch |
| `policy.rs` | Re-exports of authority parsing and destination checks from `avirt_connect` |
| `events.rs` | The event thread that writes one NDJSON line per proxied connection to the `EGRESS` connection |

## Conditions

- Accept only `CONNECT host:port` with an explicit port and a hostname or canonical IP literal. Refuse anything else with the status in `docs/egress.md`.
- Apply `is_private` and the host-address check to every resolved address before dialing. Never relax them through a rule. They cover IPv4-mapped, 6to4, and NAT64 forms.
- Produce exactly one event per proxied connection. Flush connections still open at shutdown with `error: "shutdown"`.
- Create both sockets with mode 0600. Their mode is the access control, so don't loosen it.
- The secret goes only into the `WORKLOAD` and `EGRESS` connection headers. Never log it or put it on argv.
- Shut down on stdin EOF so the mediator doesn't stay running after avirthostd exits.

## Testing

```bash
cargo test -p avirtmediatord
```

`crates/avirt-connect/src/lib.rs` holds the tests for private destinations, host addresses, CONNECT authorities, and absolute URIs. Add a case there for any new address range or authority form. The supervision integration tests use a fake avirthostd. They don't launch workloads. Coverage includes Pingora SIGTERM with stdin held open.
