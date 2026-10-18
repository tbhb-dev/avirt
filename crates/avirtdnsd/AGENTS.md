---
type: Agent Instructions
title: "Agent instructions: avirtdnsd"
description: "Guidance for AI coding agents working in the `avirtdnsd` crate."
tags: [avirt, crate, avirtdnsd]
generated: { by: codex/gpt-6, at: 2026-10-18T04:56:05Z }
---

# Agent instructions: avirtdnsd

The DNS resolver for `<name>.<ns>.svc.host.avirt.internal` and `<name>.<ns>.workload.host.avirt.internal`, which answers with the local avirtedgeproxyd or, for peered names, their host's avirtedgeproxyd. It runs on the host and is a stub binary for now.

See [docs/components.md](../../docs/components.md) for the component design and the root [AGENTS.md](../../AGENTS.md) for repository conventions.
