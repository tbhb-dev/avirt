---
type: Agent Instructions
title: "Agent instructions: avirtedgeproxyd"
description: "Guidance for AI coding agents working in the `avirtedgeproxyd` crate."
tags: [avirt, crate, avirtedgeproxyd]
generated: { by: claude-code/claude-sonnet-5-5, at: 2026-10-17T23:04:29Z }
---

# Agent instructions: avirtedgeproxyd

The edge proxy: supervises the host's edge Envoy with libavirtenvoy and serves it local xDS and SDS. It is the only component that opens connections off the host, and the policy enforcement point for traffic between workloads and between hosts. It runs on the host and has no implementation yet.

See [docs/components.md](../../docs/components.md) for the component design and the root [AGENTS.md](../../AGENTS.md) for repository conventions.
