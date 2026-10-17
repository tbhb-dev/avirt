---
type: Agent Instructions
title: "Agent instructions: avirtdnsd"
description: "Guidance for AI coding agents working in the `avirtdnsd` crate."
tags: [avirt, crate, avirtdnsd]
generated: { by: claude-code/claude-sonnet-5-5, at: 2026-10-17T19:16:42Z }
---

# Agent instructions: avirtdnsd

The DNS resolver for `<name>.<ns>.svc.host.avirt.internal` and `<name>.<ns>.workload.host.avirt.internal`, which answers with the local front-door Envoy and, for peered names, with their host's front-door Envoy. It runs on the host and has no implementation yet.

See [docs/components.md](../../docs/components.md) for the component design and the root [AGENTS.md](../../AGENTS.md) for repository conventions.
