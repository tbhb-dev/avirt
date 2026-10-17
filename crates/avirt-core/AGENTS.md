# Agent instructions: avirt-core

The shared library. The avirthostd server and the avirtinitd harness both live here, so a change can affect the host and the workload at once.

## Layout

| Module | Holds |
| --- | --- |
| `api` | The control API contract shared by avirthostd and the CLI: `DEFAULT_PORT`, `LAUNCH_PATH`, `HEALTH_PATH`, `LaunchRequest`, `LaunchResponse`, `is_terminal_event` |
| `hostd` | The server: `/launch` and `/health` on TCP, `/actors/:id/stream` on avirthostd's socket, the `ACTOR` and `EGRESS` preambles, launch, teardown and shutdown |
| `egress` | `ActorDir` (the 0700 actor directory) and `Proxy`, the mediator process with a cleared environment and a stdin-bound lifetime |
| `state` | The state root `$TMPDIR/avirt-<uid>/`, its lock, the startup sweep, and `private_dir`, `make_removable` and `remove_tree` |
| `harness` | The avirtinitd harness: uplink first, the loopback proxy relay, seccomp (Linux only), the scrubbed agent environment, `setpriv` |
| `uplink` | The workload's single long-lived HTTP/2 POST of NDJSON envelopes, carrying `x-avirt-token` |
| `envelope` | `Dir`, `Envelope`, `parse_line`, `MAX_LINE` (16 MiB) and `LineSplitter` |
| `h2c` | Cleartext HTTP/2 client helpers |

## Invariants

- **Workload startup order.** Nothing starts in the workload until avirthostd has accepted the uplink. Seccomp is installed before the adapter starts, and the agent's environment never carries `AVIRT_ACTOR_ID`, `AVIRT_PROMPT`, `AVIRT_CWD`, `AVIRT_UPLINK_TOKEN` or `AVIRT_AGENT`.
- **Relay half-close.** The proxy relay never forwards a client's end-of-stream to the socket, because nothing on a relayed socket may half-close.
- **Log ordering.** `container_exit` is always the last event of an actor. Errors are held and written before it.
- **Oversized lines.** A line over `MAX_LINE` is dropped whole and replaced by `line_truncated`. A fragment of it is never written.
- **Stream route.** A stream POST with a missing or wrong token gets 403 before any actor state changes. The mediator secret is compared in constant time.
- **File modes.** Logs are 0700 directories and 0600 files. The state root and actor directories are 0700.
- **Sweep.** The sweep never writes anything back to the host. Removal restores permissions first and never follows symlinks.
- **No secrets on argv.** No secret is ever placed on argv or in a log line.

## Testing

```bash
cargo test -p avirt-core
```

The tests cover the envelope splitter, the state root (lock, sweep, unremovable homes), `api::is_terminal_event`, and option picking. The seccomp filter compiles only for Linux targets, so it is exercised end to end inside a workload.
