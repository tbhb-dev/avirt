---
type: Reference
title: Codex harness
description: How avirt runs Codex through codex-acp, with the pinned adapter and its bundled Codex, how avirtinitd launches it over ACP v1, the auth.json seed, refresh and write-back and its planned placeholders, Codex modes, the configuration sources and managed configuration paths Codex reads, quirks and the egress destinations the harness needs.
tags: [avirt, acp, codex, harness, codex-acp]
implementation: partial
generated: { by: codex/gpt-6, at: 2026-10-18T04:29:26Z }
---

# Status: implemented

## Adapter

| Component | Version | Pinned in |
| --- | --- | --- |
| `@agentclientprotocol/codex-acp` | 2.2.2 | `workloads/codex/image/package.json` and its `package-lock.json` |
| `@openai/codex` | 0.160.1 | The adapter's own dependency, locked in `package-lock.json` |
| Node | 24.21.0 | `NODE_VERSION` in `workloads/codex/Containerfile` |

`workloads/codex/Containerfile` installs the adapter with `npm ci --omit=dev` into `/opt/agent` and puts `/opt/agent/node_modules/.bin` first on `PATH`. The image is `avirt-workload-codex:dev`, or `CODEX_IMAGE` in avirthostd's environment, and sets `AVIRT_AGENT=codex`.

codex-acp starts the bundled Codex as `codex app-server`, a child process. It translates between ACP and Codex's app-server JSON-RPC. It restarts a crashed app-server with backoff, up to 5 crashes in 5 minutes (`CODEX_ACP_APP_SERVER_CRASH_LIMIT` and `CODEX_ACP_APP_SERVER_CRASH_WINDOW_MS` override both), and allows the app-server 30 s for its handshake.

## Launch

avirtinitd starts `codex-acp` as uid 1000 through the common launch path in [ACP](./acp.md#adapter-process), with no extra variables. The adapter inherits `CODEX_HOME=/codex-home` and `INITIAL_AGENT_MODE` from avirtinitd's environment.

- **Protocol.** ACP v1, which codex-acp sets in its code. avirtinitd uses the SDK's `Client.builder()` with the `schema::v1` types and sends `protocolVersion: 1` with the default client capabilities.
- **Authentication.** avirtinitd skips the `authenticate` call, because the adapter's `chat-gpt` method forces a token refresh on every launch. Codex reads `auth.json` from `CODEX_HOME` itself. avirtinitd logs the advertised method ids as an `auth_methods` meta event.
- **Session.** One `session/new` with `cwd` and no MCP servers, then one `session/prompt`. The `session/prompt` response ends the turn and contains the stop reason. avirtinitd prints session updates as they arrive, concurrently with the request.
- **Permissions.** Codex's approval requests arrive as `session/request_permission`, and avirtinitd approves them (see [permission requests](./acp.md#permission-requests)).

## Authentication

avirthostd refuses a codex launch with a 400 unless the host `auth.json` exists (`~/.codex/auth.json`, or `CODEX_HOST_HOME/auth.json`). For each launch, it copies the file into a private scratch directory and mounts that directory at `/codex-home`. It sets `CODEX_HOME=/codex-home`. Once the sandbox container no longer exists, avirthostd writes a refreshed copy back to the host under the rules in [credentials](./egress-credentials.md#codex), and logs the outcome as `auth_writeback`. It skips the host `config.toml`, so `CODEX_HOME` contains only `auth.json` and whatever Codex writes there during the session.

## Modes

codex-acp reads `INITIAL_AGENT_MODE` at startup. avirthostd passes it through from its own environment when that environment sets it.

| Mode | Codex's sandbox and approvals |
| --- | --- |
| `read-only` | Codex's read-only sandbox. Codex requests permission for writes, and avirtinitd approves them. |
| `workspace-write` | Writes inside the workspace, no network for tool commands |
| `agent` | Codex's workspace-write sandbox by default, approvals on request |
| `agent-full-access` | Codex runs without its sandbox |

Codex's sandbox blocks the network for tool commands in `workspace-write` and `agent`, and makes `/tmp` read-only in `read-only`, so bypass and positive egress checks for Codex need `agent-full-access`. The sandbox container is the security boundary, not Codex's sandbox (see [workload hardening](./workload-hardening.md)).

## Configuration sources

Codex reads, highest precedence first:

| Layer | Path in the sandbox container | Present in the implemented image |
| --- | --- | --- |
| Requirements | `/etc/codex/requirements.toml` | No |
| Managed defaults | `/etc/codex/managed_config.toml` | No |
| Project | `.codex/config.toml` in `/workspace` and its parents, the closest taking precedence | Only if the `cwd` directory holds it |
| User | `$CODEX_HOME/config.toml` | No |
| System | `/etc/codex/config.toml` | No |

codex-acp also merges `CODEX_CONFIG`, a JSON object in its environment, into each session's config, and it's not set. Requirements are constraints that no other layer can loosen. Managed defaults override the user's `config.toml` and command-line `--config` overrides at every startup. Codex loads project `.codex/` layers, including project config, hooks, and rules, only for a trusted project, and the trust records live in the user `config.toml`. `CODEX_HOME` lacks `config.toml`, so Codex treats `/workspace` as untrusted and ignores its `.codex/config.toml`. `AGENTS.md` files are instructions, not configuration, and Codex reads them regardless of project trust.

## Quirks

- **Missing first output chunk.** codex-acp 2.2.2 never sends the first `_meta.terminal_output_delta` of a command's output, so that first chunk is absent from Codex workload logs. A command that prints three lines yields only the second and third.
- **Startup downloads.** At startup Codex downloads large artifacts from more than one `*.oaiusercontent.com` host.
- **Concurrent refresh.** When two concurrent workloads start from the same stale refresh token, they can't both refresh. The second one's refresh fails (see [credentials](./egress-credentials.md#codex)).

## Egress destinations

| Destination | Used for |
| --- | --- |
| `chatgpt.com:443` | The ChatGPT backend under `/backend-api/*`, including the Responses API over WSS at `/backend-api/codex/responses` |
| `auth.openai.com:443` | Token refresh, `POST /oauth/token` |
| `*.oaiusercontent.com:443` | Startup downloads |
| `api.openai.com:443` | The OpenAI API under `/v1/*`, with an API key login |

The mediator allows every public destination, so none of these needs configuration (see [egress](./egress.md#status-planned)).

# Status: planned

## Harness base image

The Codex harness base is `FROM` the avirt base and adds only Node, the adapter with its bundled Codex, and `harness.toml` under `/opt/agent` (see [workload lifecycle](./workload-lifecycle.md#base-images)).

```toml
name = "codex"
main_process = "avirtacp"

[adapter]
command = "/opt/agent/node_modules/.bin/codex-acp"
protocol = 1

[config]
managed_paths = ["/etc/codex"]
user_paths = ["/home/agent/.codex"]
```

The keys of `harness.toml` are in [ACP](./acp.md#harnesstoml).

## Authentication

`CODEX_HOME` is `/home/agent/.codex`, in the sandbox container's own writable layer, with no host mount. avirtinitd writes `auth.json` there at boot from the placeholder copy of the file that arrives in `confirm`: the `id_token` and `access_token` keep their real header and payload with a placeholder signature, and the `refresh_token` is a placeholder. avirtmodelgwd injects the real tokens on `chatgpt.com` and `api.openai.com`, and avirtmediatord intercepts Codex's refresh on `auth.openai.com` and answers it with new placeholders, so no write-back exists. The token formats, the bindings and the refresh path are in [credentials](./egress-credentials.md#codex-1) and [Codex refresh](./egress-credentials.md#codex-refresh).

## Managed configuration

The configuration layer writes two files under `/etc/codex` for each launch.

`/etc/codex/managed_config.toml` holds the launch's defaults and the agent's MCP server:

```toml
sandbox_mode = "workspace-write"        # from the launch's mode
approval_policy = "on-request"          # from the launch's mode

[mcp_servers.avirt]
command = "/usr/local/bin/avirtmcp"
```

`/etc/codex/requirements.toml` holds what no layer may change:

```toml
[mcp_servers.avirt]
identity = { command = "/usr/local/bin/avirtmcp" }
```

The requirements' `mcp_servers` table is an allowlist, so Codex starts only avirtmcp as an MCP server, whatever a project or user `config.toml` defines. avirtinitd also passes the launch's mode to codex-acp as `INITIAL_AGENT_MODE` in `/etc/avirt/launch.toml`, so the ACP session's mode and Codex's defaults agree.

## Workspace configuration

The overlay limits a workspace's Codex configuration:

- Codex ignores `/workspace/.codex/config.toml`, because it treats the project as untrusted.
- `/home/agent/.codex` is a user path in `harness.toml`, so at launch avirthostd refuses a workspace layer that writes it. This includes a layer with a `config.toml` marking `/workspace` trusted.
- Codex reads `AGENTS.md` files as instructions.

## Egress destinations

| Destination | Path through avirtmediatord |
| --- | --- |
| `chatgpt.com:443`, `api.openai.com:443` | TLS interception, routed to avirtmodelgwd for credential injection |
| `auth.openai.com:443` `POST /oauth/token` | The refresh intercept, answered by avirtmodelgwd |
| `*.oaiusercontent.com:443` | TLS passthrough, under the `oai-content` rules |

The policy rules are in [egress](./egress-policies.md#external-destinations).

# Open questions

1. **Managed defaults and project config.** The managed-configuration documentation gives `managed_config.toml` precedence over the user `config.toml` and command-line overrides, but not relative to a trusted project's `.codex/config.toml`. Whether a managed default also overrides project config matters only when a project has trusted status.
2. **Modes as requirements.** Whether the overlay also constrains Codex's sandbox and approval values in `requirements.toml` (`allowed_permission_profiles` for Codex 0.138.0 and later), so that an operator's mode change through the session control API can't leave the launch's mode, is open.
