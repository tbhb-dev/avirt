---
type: README
title: avirtvmd
description: "The per-workload VM host helper for avirt, a Swift program built on Containerization. It currently serves the control channel."
tags: [avirt, crate, avirtvmd]
implementation: partial
generated: { by: claude-code/claude-sonnet-5-5, at: 2026-10-18T10:45:00Z }
---

# avirtvmd

A Swift package built on Apple's Containerization, `avirtvmd` is the per-workload VM host helper for avirt. `avirthostd` starts one helper for each workload on the Containerization runtime, and the helper builds on macOS only.

At present `avirtvmd run` reads an incarnation from stdin and serves the control channel on `v.sock` in the workload's directory. Its channel supports the version handshake, `Status` and `Stop`, and closing stdin ends the helper. Hosting the workload VM and preparing guest artifacts are still to come.

```bash
mise run build-vmd   # builds target/debug/avirtvmd and signs it with the virtualization entitlement
mise run check-vmd   # builds, lints and tests the package
```

See [compute runtimes](../../docs/compute-runtimes.md) and [components](../../docs/components.md).
