---
type: Agent Instructions
title: "Agent instructions: avirtinitd"
description: "Guidance for AI coding agents working in the `avirtinitd` crate."
tags: [avirt, crate, avirtinitd]
generated: { by: codex/gpt-6, at: 2026-10-18T02:56:31Z }
---

# Agent instructions: avirtinitd

The `avirtinitd` binary, which runs only inside a Linux workload. The shared mechanics are in `avirt_core::harness`. This crate defines the two ACP conversations.

## Layout

| Module | Holds |
| --- | --- |
| `main.rs` | Dispatch on `AVIRT_AGENT` to `claude::run` or `codex::run` |
| `claude.rs` | ACP v2 (`schema::v2`, `CLAUDE_AGENT_ACP_EXPERIMENTAL_V2=1`): initialize, ACP session, prompt, permission approvals, update printing |
| `codex.rs` | ACP v1 follows the same steps. It consumes session updates concurrently with the prompt and skips the `authenticate` call |

## Conditions

- Verify the `protocolVersion` in the initialize response and fail when it differs from the one sent.
- Permission approval goes through `harness::pick_option` (`allow_once`, else `allow_always`, else the first option), and logs every approval as `auto_approve`.
- Never call Codex's `authenticate`, because it forces a token refresh on every launch.
- The harness enforces this order: `POST /workloads/<id>/stream` accepted, then the TCP proxy to `proxy.sock`, then seccomp, then the adapter as uid 1000 with no capabilities and `no_new_privs`. Don't spawn anything from this crate outside the harness, or it escapes that order and the filters.
- The seccomp filter (`AF_VSOCK`, `AF_NETLINK`, `AF_PACKET` and io_uring denied) exists only on Linux targets. The non-Linux build is a no-op stub, so avirtinitd only works as a Linux binary.

## Testing

This crate doesn't have unit tests. `cargo test -p avirt-core` covers the harness helpers. The behavior is verified end to end: `mise run build-images`, then the regression suite in `docs/roadmap.md`, with bypass checks run inside the agent's own process tree.
