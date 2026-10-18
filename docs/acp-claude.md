---
type: Reference
title: Claude harness
description: How avirt runs Claude Code through claude-agent-acp, with the pinned adapter and its bundled versions, how avirtinitd launches it over ACP v2, authentication with the OAuth token and its placeholder, the configuration sources Claude Code reads, the managed settings and managed MCP configuration in the configuration layer, quirks and the egress destinations the harness needs.
tags: [avirt, acp, claude, harness, claude-agent-acp]
implementation: partial
generated: { by: codex/gpt-6, at: 2026-10-18T05:14:22Z }
---

# Status: implemented

## Adapter

| Component | Version | Pinned in |
| --- | --- | --- |
| `@agentclientprotocol/claude-agent-acp` | 0.93.0 | `workloads/claude/image/package.json` and its `package-lock.json` |
| `@anthropic-ai/claude-agent-sdk` | 0.3.300 | The adapter's own dependency, locked in `package-lock.json` |
| Claude Code | 2.1.300 | Bundled by the agent SDK |
| Node | 24.21.0 | `NODE_VERSION` in `workloads/claude/Containerfile` |

`workloads/claude/Containerfile` installs the adapter with `npm ci --omit=dev` into `/opt/agent` and puts `/opt/agent/node_modules/.bin` first on `PATH`. The image is `avirt-workload-claude:dev`, or `CLAUDE_IMAGE` in avirthostd's environment, and sets `AVIRT_AGENT=claude`. The image pins the adapter and its versions exactly, because both the ACP v2 support in the Rust SDK and the adapter's v2 flag are experimental.

## Launch

avirtinitd starts `claude-agent-acp` as uid 1000 through the common launch path in [ACP](./acp.md#adapter-process), with `CLAUDE_AGENT_ACP_EXPERIMENTAL_V2=1` added to its environment. The adapter speaks ACP v2 only with that variable set.

- **Protocol.** avirtinitd uses the SDK's `Client.v2()` with the `schema::v2` types and sends `protocolVersion: 2`.
- **Session.** One `session/new` with `cwd` and no MCP servers, then one `session/prompt`.
- **Turn end.** The adapter sets `CLAUDE_CODE_EMIT_SESSION_STATE_EVENTS=1` for Claude Code, which makes it report `state_update` notifications. avirtinitd ends the turn on the `idle` state as [ACP](./acp.md#session-lifecycle) describes.
- **Permissions.** Claude Code asks for permission through the adapter's tool-permission callback, which the adapter sends as `session/request_permission`, and avirtinitd approves it (see [permission requests](./acp.md#permission-requests)).
- **Shutdown.** The adapter exits when its ACP connection closes, and on SIGTERM or SIGINT.

## Authentication

avirthostd refuses a Claude launch with a 400 unless its environment sets `CLAUDE_CODE_OAUTH_TOKEN`. It passes the variable to the sandbox container with `-e CLAUDE_CODE_OAUTH_TOKEN` and no value, so the real token is in the agent's environment and never on argv. avirtinitd skips the `authenticate` call. The mechanics and their exposure are in [credentials](./egress-credentials.md#claude).

## Configuration sources

The adapter starts each Claude Code session with `settingSources: ["user", "project", "local"]`. With the managed tier, which Claude Code always reads, these are the sources in a sandbox container, highest precedence first:

| Scope | Path in the sandbox container | Present in the implemented image |
| --- | --- | --- |
| Managed | `/etc/claude-code/managed-settings.json` and `/etc/claude-code/managed-mcp.json` | No |
| Project local | `/workspace/.claude/settings.local.json` | Only if the `cwd` directory holds it |
| Shared project | `/workspace/.claude/settings.json`, and `/workspace/.mcp.json` for MCP servers | Only if the `cwd` directory holds it |
| User | `/home/agent/.claude/settings.json` (`CLAUDE_CONFIG_DIR` overrides the directory) | No |

Claude Code loads project-scoped MCP servers from `.mcp.json` without an approval prompt in Agent SDK sessions, so a `cwd` directory with a `.mcp.json` starts its stdio servers as uid 1000 inside the sandbox container. `CLAUDE.md` files are instructions, not configuration, and no setting stops Claude Code from reading them.

## Quirks

- **Draft protocol.** ACP v2 is a draft. The `session/prompt` response contains only a message id, and the turn's outcome arrives as a `state_update`.
- **Immediate refusal.** A turn that ends before any work starts sends `idle` with a stop reason and no preceding `running`.
- **Diagnostics.** The adapter redirects `console.log` and its other console methods to stderr, so stdout sends only ACP. The log records its stderr as `agent_stderr` events. `CLAUDE_AGENT_LOGS` names a directory for its own log file, and the environment leaves it unset.

## Egress destinations

| Destination | Used for |
| --- | --- |
| `api.anthropic.com:443` | Model requests (`/v1/*`) and Claude Code's other API calls (`/api/*`) |

Tools the agent runs need their own destinations, such as `registry.npmjs.org` and `github.com` in the regression suite's positive checks (see [roadmap](./roadmap.md#regression-suite)). The mediator allows every public destination, so none of these needs configuration (see [egress](./egress.md#status-implemented)).

# Status: planned

## Harness base image

The Claude harness base is `FROM` the avirt base and adds only Node, the adapter and `harness.toml` under `/opt/agent` (see [workload lifecycle](./workload-lifecycle.md#base-images)).

```toml
name = "claude"
main_process = "avirtacp"

[adapter]
command = "/opt/agent/node_modules/.bin/claude-agent-acp"
protocol = 2
env = { CLAUDE_AGENT_ACP_EXPERIMENTAL_V2 = "1" }

[config]
managed_paths = ["/etc/claude-code"]
user_paths = ["/home/agent/.claude"]
```

The keys of `harness.toml` are in [ACP](./acp.md#harnesstoml).

## Authentication

The agent's environment contains the placeholder `sk-ant-oat01-avirtph<32>` in `CLAUDE_CODE_OAUTH_TOKEN`, never the real token. `/etc/avirt/launch.toml` names the variable, avirtinitd sets it from the placeholder that arrives in `confirm`, and avirtmodelgwd replaces it with the real token in the `authorization` header of requests to `api.anthropic.com`. Claude Code never refreshes this token, so no refresh path exists. The binding, the placeholder format and the `credential-misplaced` check are in [credentials](./egress-credentials.md#claude-code).

## Managed settings

The configuration layer writes `/etc/claude-code/managed-settings.json` for each launch. It holds:

| Key | Value | Effect |
| --- | --- | --- |
| `permissions.defaultMode` | The launch's mode, `default` unless the launch sets another | The session's initial permission mode |
| `allowManagedPermissionRulesOnly` | `true` | Claude Code ignores permission rules from project, local, and user settings. Without it, `permissions.allow` lists merge across every scope. |
| `allowManagedHooksOnly` | `true` | Only managed hooks run, so a workspace's `.claude/settings.json` can't add hooks |
| `allowManagedMcpServersOnly` | `true` | Only the managed MCP allowlist applies |
| `env` | `CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC=1` and `DISABLE_AUTOUPDATER=1` | No telemetry, error reporting or update checks, so model traffic is the harness's only egress |

A managed key overrides the same key in every other scope. List keys merge across scopes unless a managed-only key such as `allowManagedPermissionRulesOnly` locks it, and the security keys Claude Code lists as exceptions keep the strictest value from any scope.

## Managed MCP configuration

The overlay writes `/etc/claude-code/managed-mcp.json` with one server:

```json
{ "mcpServers": { "avirt": { "type": "stdio", "command": "/usr/local/bin/avirtmcp" } } }
```

`managed-mcp.json` takes exclusive control of MCP servers. Claude Code loads only the servers it defines. It ignores servers from `/workspace/.mcp.json`, user settings, and plugins. avirtmcp relays MCP to avirtmcpgwd (see [components](./components.md#sandbox-container)).

## Workspace configuration

The overlay limits a workspace's Claude Code configuration:

- `/workspace/.claude/settings.json` and `settings.local.json` still set keys that no managed key sets, such as output style or `CLAUDE.md` imports. They can't add permission rules, hooks, or MCP servers.
- Claude Code ignores `/workspace/.mcp.json`.
- `/home/agent/.claude` is a user path in `harness.toml`, so at launch avirthostd refuses a workspace layer that writes this path.

# Open questions

1. **`managed-mcp.json` and the adapter.** The agent SDK passes the adapter's MCP server map to Claude Code as <!-- Preserve the named SDK feature. --><!-- vale ai-tells.OverusedVocabulary = NO -->dynamic MCP configuration<!-- vale ai-tells.OverusedVocabulary = YES -->, and Claude Code refuses <!-- Preserve the named SDK feature. --><!-- vale ai-tells.OverusedVocabulary = NO -->dynamic MCP configuration<!-- vale ai-tells.OverusedVocabulary = YES --> on a workstation when `managed-mcp.json` is present. Whether claude-agent-acp 0.93.0 starts with an empty map under `managed-mcp.json`, or avirtmcp has to arrive in `session/new` instead, is open.
2. **Plugin and skill sources.** Whether the overlay also sets `strictPluginOnlyCustomization` is open. This setting blocks skills, agents, and MCP servers from user and project sources.
