---
type: Agent Instructions
title: "Agent instructions: avirtlogd"
description: "Guidance for AI coding agents working in the `avirtlogd` crate."
tags: [avirt, crate, avirtlogd]
generated: { by: codex/gpt-6, at: 2026-10-18T02:56:31Z }
---

# Agent instructions: avirtlogd

The log collector, which gathers workload stdout and stderr, transforms it and sends it to avirtoteld as OpenTelemetry Logs. It runs on the host and doesn't provide this service yet.

See [docs/components.md](../../docs/components.md) for the component design and the root [AGENTS.md](../../AGENTS.md) for repository conventions.
