# agentvmm-core

The shared library for agentvmm-poc. It holds the control API types, the controller server, the per-actor supervisor process handle, the runner harness, the workload uplink, the NDJSON log envelope and the controller's state root. The controller, the CLI and the runner depend on it.

```bash
cargo test -p agentvmm-core
```

See [architecture](../../docs/architecture.md) and [the controller API and log format](../../docs/controller-api.md).
