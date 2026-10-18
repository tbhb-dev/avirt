---
type: README
title: avirtmediatord
description: "The per-workload egress mediator for avirt, built as the `avirtmediatord` binary on Pingora 0.9.0."
tags: [avirt, crate, avirtmediatord]
implementation: implemented
generated: { by: claude-code/claude-sonnet-5-5, at: 2026-10-17T19:16:42Z }
---

# avirtmediatord

The per-workload egress mediator for avirt, built as the `avirtmediatord` binary on Pingora 0.9.0. `avirthostd` starts one per workload. It serves the workload's CONNECT proxy and forwards avirtinitd's HTTP/2 connection on `mediator.sock` to `avirthostd` with the bytes unchanged.

```bash
cargo build -p avirtmediatord
```

`avirthostd` finds it next to its own binary, or at `AVIRT_MEDIATOR_BIN`. See [egress and TLS termination](../../docs/egress.md), [egress policies](../../docs/egress-policies.md) and [workload topology](../../docs/workload-topology.md).
