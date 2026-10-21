---
type: README
title: avirt-core
description: The shared library for avirt.
tags: [avirt, crate, avirt-core]
implementation: implemented
generated: { by: codex/gpt-6, at: 2026-10-18T02:56:30Z }
---

# avirt-core

The shared library for avirt. It holds the control API types, the `avirthostd` server, the per-workload mediator process handle, the `avirtinitd` harness, the client for the workload's HTTP/2 connection to its mediator, the NDJSON log envelope and the `avirthostd` runtime directory. `avirthostd`, the command-line interface and `avirtinitd` depend on it.

The mediator process receives only `PATH`, `HOME`, `TMPDIR` and its per-workload secret in its environment.

```bash
cargo test -p avirt-core
```
