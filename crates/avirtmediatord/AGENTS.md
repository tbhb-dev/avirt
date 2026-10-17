# Agent instructions: avirtmediatord

The mediator binary. It runs one Pingora `Server` with two Unix-socket services in the actor's directory.

## Layout

| Module | Holds |
| --- | --- |
| `main.rs` | Flags (`--dir`, `--hostd-sock`, `--actor`, and the secret from `AVIRT_MEDIATOR_SECRET`), the `ConnectApp` CONNECT proxy, the `SpliceApp` uplink splice, the connection records, and the shutdown watch |
| `policy.rs` | Parsing CONNECT authorities and absolute URIs, `is_private` and the host-address check |
| `events.rs` | The event thread that writes one NDJSON line per proxied connection to the `EGRESS` connection |

## Invariants

- Accept only `CONNECT host:port` with an explicit port and a hostname or canonical IP literal. Anything else is refused with the status in `docs/egress.md`.
- Apply `is_private` and the host-address check to every resolved address before dialing. They are never relaxed by a rule, and they cover IPv4-mapped, 6to4 and NAT64 forms.
- Every proxied connection produces exactly one event, including connections still open at shutdown, which are flushed with `error: "shutdown"`.
- Both sockets are created 0600. Their mode is the access control, so don't loosen it.
- The secret goes only into the `ACTOR` and `EGRESS` preambles. It is never logged or put on argv.
- Stdin EOF means shut down, so the mediator never outlives avirthostd.

## Testing

```bash
cargo test -p avirtmediatord
```

`policy.rs` holds the tests for private destinations, host addresses, CONNECT authorities and absolute URIs. Add a case there for any new address range or authority form.
