---
type: README
title: avirt
description: "The operator CLI for avirt, built as the `avirt` binary."
tags: [avirt, crate, cli]
implementation: implemented
generated: { by: claude-code/claude-sonnet-5-5, at: 2026-10-17T19:16:42Z }
---

# avirt

The operator CLI for avirt, built as the `avirt` binary. `avirt launch --agent claude|codex "<prompt>" [--cwd DIR] [--follow]` asks a running `avirthostd` to start a workload and can print its log until the workload ends.

```bash
cargo build -p avirt
mise run launch claude "Reply with the word pong" -- --follow
```

See [the operator CLI](../../docs/operator-cli.md).
