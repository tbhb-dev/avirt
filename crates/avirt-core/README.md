# avirt-core

The shared library for avirt. It holds the control API types, the `avirthostd` server, the per-actor mediator process handle, the `avirtinitd` harness, the workload uplink, the NDJSON log envelope and the `avirthostd` state root. `avirthostd`, the CLI and `avirtinitd` depend on it.

```bash
cargo test -p avirt-core
```

See [architecture](../../docs/architecture.md) and [the avirthostd API and log format](../../docs/hostd-api.md).
