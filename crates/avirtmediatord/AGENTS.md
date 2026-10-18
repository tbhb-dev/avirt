---
type: Agent Instructions
title: "Agent instructions: avirtmediatord"
description: "Guidance for AI coding agents working in the `avirtmediatord` crate."
tags: [avirt, crate, avirtmediatord]
generated: { by: claude-code/claude-sonnet-5-5, at: 2026-10-17T19:16:42Z }
---

# Agent instructions: avirtmediatord

The mediator binary. It runs one Pingora `Server` with two Unix-socket services in the workload's directory. Pingora is the default CONNECT engine; `--proxy-engine envoy` relays proxy streams to a supervised Envoy child while keeping the TCP proxy on `uplink.sock`. See [egress](../../docs/egress.md#experimental-host-envoy-mode) for Envoy mode's unverified parity and event limitations. avirtmedproxyd is planned to take over supervising the mediator's Envoy.

## Layout

| Module | Holds |
| --- | --- |
| `main.rs` | Flags (`--dir`, `--hostd-sock`, `--workload`, and the secret from `AVIRT_MEDIATOR_SECRET`), the `ConnectApp` CONNECT proxy, the `SpliceApp` TCP proxy that forwards `uplink.sock` to avirthostd unchanged, the connection records, and the shutdown watch |
| `policy.rs` | Re-exports of authority parsing and destination checks from `avirt_connect` |
| `envoy.rs` | Static CONNECT bootstrap, the guardian process, Envoy supervision, stream forwarding and conversion of module snapshots to connection events |
| `events.rs` | The event thread that writes one NDJSON line per proxied connection to the `EGRESS` connection |

## Invariants

- Accept only `CONNECT host:port` with an explicit port and a hostname or canonical IP literal. Anything else is refused with the status in `docs/egress.md`.
- Apply `is_private` and the host-address check to every resolved address before dialing. They are never relaxed by a rule, and they cover IPv4-mapped, 6to4 and NAT64 forms.
- Every proxied connection produces exactly one event, including connections still open at shutdown, which are flushed with `error: "shutdown"`.
- Both sockets are created 0600. Their mode is the access control, so don't loosen it.
- The secret goes only into the `WORKLOAD` and `EGRESS` connection headers. It is never logged or put on argv.
- Stdin EOF means shut down, so the mediator never outlives avirthostd.

## Testing

```bash
cargo test -p avirtmediatord
```

`crates/avirt-connect/src/lib.rs` holds the tests for private destinations, host addresses, CONNECT authorities and absolute URIs. Add a case there for any new address range or authority form. `envoy.rs` tests the bootstrap structure and event conversion and validates the bootstrap when an Envoy binary and module are supplied. The supervision integration tests use `/bin/sh` and `/usr/bin/perl` as a fake Envoy, with a fake avirthostd. They cover guardian cleanup, Envoy exit, process groups, open-stream shutdown flushing and Pingora SIGTERM with stdin held open, and launch no workload.
