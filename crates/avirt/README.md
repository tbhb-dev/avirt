---
type: README
title: avirt
description: "The operator command-line interface for avirt, built as the `avirt` binary."
tags: [avirt, crate, cli]
implementation: implemented
generated: { by: codex/gpt-6, at: 2026-10-18T03:13:20Z }
---

# avirt

The <!-- Preserve the exact term from the glossary. --><!-- vale project.WordList = NO -->operator CLI<!-- vale project.WordList = YES --> for avirt, built as the `avirt` binary. `avirt launch --agent claude|codex "<prompt>" [--cwd DIR] [--follow]` asks a running `avirthostd` to start a workload and can print its log until the workload ends.

```bash
cargo build -p avirt
mise run launch claude "Reply with the word pong" -- --follow
```

See [the <!-- Preserve the exact term from the glossary. --><!-- vale project.WordList = NO -->operator CLI<!-- vale project.WordList = YES -->](../../docs/operator-cli.md).
