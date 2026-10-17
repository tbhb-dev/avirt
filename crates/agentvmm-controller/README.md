# agentvmm-controller

The host controller for agentvmm-poc, built as the `agentvmm-controller` binary. Running it starts the server: the control API on `127.0.0.1:8787`, the per-actor supervisors, the workload containers and the actor logs in `logs/`.

```bash
cargo build -p agentvmm-controller -p agentvmm-supervisor
mise run serve
```

See [the controller API and log format](../../docs/controller-api.md), [lifecycle and state](../../docs/lifecycle.md) and [credentials](../../docs/credentials.md).
