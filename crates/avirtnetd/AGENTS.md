---
type: Agent Instructions
title: "Agent instructions: avirtnetd"
description: "Guidance for AI coding agents working in the `avirtnetd` crate."
tags: [avirt, crate, avirtnetd]
generated: { by: codex/gpt-6, at: 2026-10-18T04:29:26Z }
---

# Agent instructions: avirtnetd

The guest networking daemon provides the client end of the workload's HTTP/2 connection to its mediator. It also provides the forward and reverse proxies, the stub DNS resolver, and the guest control API. It runs in the sandbox container and is a stub binary for now.

See [docs/components.md](../../docs/components.md) for the component design and the root [AGENTS.md](../../AGENTS.md) for repository conventions.
