---
type: README
title: avirthostd
description: "The host daemon for avirt, built as the `avirthostd` binary."
tags: [avirt, crate, avirthostd]
implementation: implemented
generated: { by: claude-code/claude-sonnet-5-5, at: 2026-10-17T19:16:42Z }
---

# avirthostd

The host daemon for avirt, built as the `avirthostd` binary. Running it starts the server: the control API on `127.0.0.1:8787`, the per-workload mediators, the workload containers and the workload logs in `logs/`.

```bash
cargo build -p avirthostd -p avirtmediatord
mise run serve
```

See [the avirthostd API and log format](../../docs/hostd-api.md), [lifecycle and state](../../docs/lifecycle.md) and [credentials](../../docs/egress-credentials.md).
