---
type: README
title: avirtvmd
description: "The per-workload VM host helper for avirt, a Swift program built on Containerization. It serves the control channel and boots the workload VM, and it writes the stdio log and prepares the guest artifacts."
tags: [avirt, crate, avirtvmd]
implementation: partial
generated: { by: claude-code/claude-sonnet-5-5, at: 2026-10-18T10:45:00Z }
---

# avirtvmd

A Swift package built on Apple's Containerization, `avirtvmd` is the per-workload VM host helper for avirt. `avirthostd` starts one helper for each workload on the Containerization runtime, and the helper builds on macOS only.

At present `avirtvmd run` reads an incarnation from stdin and serves the control channel on `v.sock` in the workload's directory. Its channel supports the version handshake, `Status` and `Stop`, and closing stdin ends the helper. `avirtvmd prepare` makes the guest artifacts from paths that avirthostd names. Its steps download a file and extract an archive member. They also build the init filesystem and unpack image layers into a root filesystem. `StartWorkload` boots the workload VM, and the helper writes the container's stdio to an NDJSON log. When the VM fails to start after the log opens, the helper renames the stdio log to its name plus `.failed`, which keeps the boot log.

```bash
mise run build-vmd   # builds target/debug/avirtvmd and signs it with the virtualization entitlement
mise run check-vmd   # builds, lints and tests the package
```

See [compute runtimes](../../docs/compute-runtimes.md) and [components](../../docs/components.md).
