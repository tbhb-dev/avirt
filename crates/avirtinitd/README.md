---
type: README
title: avirtinitd
description: "The workload entrypoint for avirt, built as the `avirtinitd` binary."
tags: [avirt, crate, avirtinitd]
implementation: implemented
generated: { by: claude-code/claude-sonnet-5-5, at: 2026-10-17T19:16:42Z }
---

# avirtinitd

The workload entrypoint for avirt, built as the `avirtinitd` binary. It is a Linux guest binary only, built into each workload image. It runs as root, opens its HTTP/2 connection to its mediator on `uplink.sock` and posts `POST /workloads/<id>/stream`, which today carries only log envelopes, runs a TCP proxy from the agent to `proxy.sock`, and starts the ACP adapter as uid 1000 under seccomp. `AVIRT_AGENT=claude|codex`, set in each image, selects the agent.

```bash
mise run build-images
```

See [ACP](../../docs/acp.md) and [workload hardening](../../docs/workload-hardening.md).
