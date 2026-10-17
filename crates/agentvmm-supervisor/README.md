# agentvmm-supervisor

The per-actor egress supervisor for agentvmm-poc, built as the `agentvmm-supervisor` binary on Pingora 0.9.0. The controller starts one per actor. It serves the workload's CONNECT proxy and splices the workload's uplink to the controller.

```bash
cargo build -p agentvmm-supervisor
```

The controller finds it next to its own binary, or at `AGENTVMM_SUPERVISOR_BIN`. See [egress, TLS termination and policy](../../docs/egress.md) and [actor topology](../../docs/actor-topology.md).
