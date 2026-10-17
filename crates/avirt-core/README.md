---
type: README
title: avirt-core
description: The shared library for avirt.
tags: [avirt, crate, avirt-core]
implementation: implemented
generated: { by: claude-code/claude-sonnet-5-5, at: 2026-10-17T19:16:42Z }
---

# avirt-core

The shared library for avirt. It holds the control API types, the `avirthostd` server, the per-workload mediator process handle, the `avirtinitd` harness, the client for the workload's HTTP/2 connection to its mediator, the NDJSON log envelope and the `avirthostd` runtime directory. `avirthostd`, the CLI and `avirtinitd` depend on it.

```bash
cargo test -p avirt-core
```

See [architecture](../../docs/architecture.md) and [the avirthostd API and log format](../../docs/hostd-api.md).
