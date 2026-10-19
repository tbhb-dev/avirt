---
type: README
title: avirthostd
description: "The host daemon for avirt, built as the `avirthostd` binary."
tags: [avirt, crate, avirthostd]
implementation: implemented
generated: { by: codex/gpt-6, at: 2026-10-18T02:56:31Z }
---

# avirthostd

The host daemon for avirt, built as the `avirthostd` binary. Running it starts the server with the control API on `127.0.0.1:8787`. It manages the per-workload mediators and sandbox containers, plus the workload logs in `logs/`.

```bash
cargo build -p avirthostd -p avirtmediatord
mise run serve
```

See [the avirthostd API and log format](../../docs/hostd-api.md), [lifecycle and state](../../docs/lifecycle.md), and [credentials](../../docs/egress-credentials.md).
