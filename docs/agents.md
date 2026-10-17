---
type: Reference
title: Agents
description: How avirtinitd drives Claude and Codex over ACP, the pinned adapter versions, protocol versions, Codex modes, automatic approvals and known limitations.
tags: [avirt, acp, claude, codex, avirtinitd]
implementation: implemented
generated: { by: claude-code/claude-opus-5-5, at: 2026-10-17T14:23:31Z }
---

# Status: implemented

## avirtinitd

`avirtinitd` is one binary for both agents, selected with `AVIRT_AGENT=claude|codex`. It is an ACP client built on `agent-client-protocol` with the `process` feature. It spawns the adapter as a stdio subprocess, runs one ACP session, and streams every JSON-RPC line to avirthostd.

The tap is the SDK's own hook, `AcpAgent::with_debug`, which reports every line written to the adapter's stdin and read from its stdout and stderr without altering it. `AcpAgent` owns the adapter's lifetime: it races the protocol against the adapter's exit so a dead adapter cannot hang pending calls. After the protocol ends, it waits up to 1 s for the adapter to exit and then SIGKILLs its whole process group. Protocol lines are all reported before the ACP session returns. Stderr bytes still unread when the adapter exits cleanly can be lost.

`avirtinitd` verifies that the `protocolVersion` in the initialize response equals the one it sent, and fails otherwise.

## Permission requests

Every permission request is approved automatically, choosing the first `allow_once` option, else `allow_always`, else the first option, else cancelling. Each approval is logged as an `auto_approve` meta event.

## Workload images

`workloads/claude/` and `workloads/codex/` each hold a Containerfile and `image/` with the pinned adapter, installed with `npm ci`. The build context is the repository root. A Rust stage compiles avirtinitd for linux arm64, and the final `ubuntu:26.04` stage adds Node 24, the adapter, avirtinitd and the `agent` user. The images are `avirt-workload-claude:dev` and `avirt-workload-codex:dev`.

## Claude

- **Adapter.** claude-agent-acp 0.93.0, bundling agent-sdk 0.3.300 and Claude Code 2.1.300.
- **Protocol.** ACP v2, which is a draft. `avirtinitd` sets `CLAUDE_AGENT_ACP_EXPERIMENTAL_V2=1`, sends `protocolVersion: 2`, and uses `Client.v2()` with the `schema::v2` types. Both the SDK feature (`unstable_protocol_v2`) and the adapter flag are experimental, so versions are pinned exactly.
- **Session end.** The ACP session ends when it goes idle after having been active.

## Codex

- **Adapter.** codex-acp 2.2.2, bundling Codex 0.160.1.
- **Protocol.** ACP v1, which codex-acp hardcodes. `avirtinitd` uses `Client.builder()` and `schema::v1`, and prints session updates as they arrive, concurrently with the prompt request.
- **Authentication.** `avirtinitd` makes no `authenticate` call, because the adapter's `chat-gpt` authenticate forces a token refresh on every launch. Codex reads its `auth.json` itself (see [credentials](./credentials.md)), and the advertised method ids are logged as `auth_methods`.
- **Modes.** The default mode is `agent` (workspace-write sandbox, approvals on request). `INITIAL_AGENT_MODE` in avirthostd's environment selects `read-only`, `workspace-write`, `agent` or `agent-full-access`. codex-acp reads it at startup. In `read-only`, Codex requests permission for writes, and avirtinitd approves them.
- **Tool sandbox.** Codex's own sandbox blocks the network for tool commands in `workspace-write` and makes `/tmp` read-only in `read-only`, so bypass and positive egress checks for Codex need `agent-full-access`. The container is the security boundary, not Codex's sandbox.
- **Startup downloads.** At startup Codex downloads large artifacts from several `*.oaiusercontent.com` hosts.
- **Known limitation.** codex-acp 2.2.2 never sends the first `_meta.terminal_output_delta` of a command's output, so that first chunk is absent from Codex actor logs. A command that prints three lines yields only the second and third.
