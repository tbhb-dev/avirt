# avirthostd

The host daemon for avirt, built as the `avirthostd` binary. Running it starts the server: the control API on `127.0.0.1:8787`, the per-actor mediators, the workload containers and the actor logs in `logs/`.

```bash
cargo build -p avirthostd -p avirtmediatord
mise run serve
```

See [the avirthostd API and log format](../../docs/hostd-api.md), [lifecycle and state](../../docs/lifecycle.md) and [credentials](../../docs/credentials.md).
