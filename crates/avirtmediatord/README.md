---
type: README
title: avirtmediatord
description: "The per-actor egress mediator for avirt, built as the `avirtmediatord` binary on Pingora 0.9.0."
tags: [avirt, crate, avirtmediatord]
implementation: implemented
generated: { by: claude-code/claude-sonnet-5-5, at: 2026-10-17T19:16:42Z }
---

# avirtmediatord

The per-actor egress mediator for avirt, built as the `avirtmediatord` binary on Pingora 0.9.0. `avirthostd` starts one per actor. It serves the workload's CONNECT proxy and splices the workload's uplink to `avirthostd`.

```bash
cargo build -p avirtmediatord
```

`avirthostd` finds it next to its own binary, or at `AVIRT_MEDIATOR_BIN`. See [egress, TLS termination and policy](../../docs/egress.md) and [actor topology](../../docs/actor-topology.md).
