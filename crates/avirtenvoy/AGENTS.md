---
type: Agent Instructions
title: "Agent instructions: avirtenvoy"
description: "Guidance for AI coding agents working in the `avirtenvoy` crate."
tags: [avirt, crate, avirtenvoy]
generated: { by: claude-code/claude-sonnet-5-5, at: 2026-10-17T19:16:42Z }
---

# Agent instructions: avirtenvoy

The crate owns the CONNECT authority parsing and destination checks shared with Pingora. The `dynamic-module` feature builds the Envoy v1.40.0 HTTP filter, which checks resolved addresses and emits connection snapshots to avirtmediatord. Use `cargo build --locked -p avirtenvoy --features dynamic-module` for a loadable cdylib; the default build has no Envoy entrypoint. Cedar policy, credential injection and use in the host's front-door Envoy are planned. See the crate README for build constraints and runtime validation.

See [docs/components.md](../../docs/components.md) for the component design and the root [AGENTS.md](../../AGENTS.md) for repository conventions.
