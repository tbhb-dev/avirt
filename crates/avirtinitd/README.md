---
type: README
title: avirtinitd
description: "The workload entrypoint for avirt, built as the `avirtinitd` binary."
tags: [avirt, crate, avirtinitd]
implementation: implemented
generated: { by: codex/gpt-6, at: 2026-10-18T02:56:31Z }
---

# avirtinitd

The workload entrypoint for avirt, built as the `avirtinitd` binary. It's a Linux guest binary only, built into each sandbox image. It runs as root, opens its HTTP/2 connection to its mediator on `mediator.sock` and posts `POST /workloads/<id>/stream`, which today sends only log envelopes. It runs a TCP proxy from the agent to `proxy.sock` and starts the ACP adapter as uid 1000 under seccomp. `AVIRT_AGENT=claude|codex`, set in each image, selects the agent.

```bash
mise run build-images
```

See [ACP](../../docs/acp.md) and [workload hardening](../../docs/workload-hardening.md).
