---
type: README
title: avirtmediatord
description: "The per-workload egress mediator for avirt, built as the `avirtmediatord` binary on Pingora 0.9.0."
tags: [avirt, crate, avirtmediatord]
implementation: implemented
generated: { by: codex/gpt-6, at: 2026-10-18T02:56:31Z }
---

# avirtmediatord

The per-workload egress mediator for avirt, built as the `avirtmediatord` binary on Pingora 0.9.0. `avirthostd` starts one per workload. Pingora serves the workload's CONNECT proxy and forwards avirtinitd's HTTP/2 connection on `mediator.sock` to `avirthostd` with the bytes unchanged.

```bash
cargo build -p avirtmediatord
```

`avirthostd` finds it next to its own binary, or at `AVIRT_MEDIATOR_BIN`.
