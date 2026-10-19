---
type: API Reference
title: avirthostd API and log format
description: Starting avirthostd and using its control API, `POST /workloads/<id>/stream` route, environment variables, and working-directory mount. The NDJSON workload log format and its meta events.
tags: [avirt, avirthostd, api, logging]
implementation: partial
generated: { by: codex/gpt-6, at: 2026-10-18T03:05:39Z }
---

# Status: implemented

## Starting avirthostd

```text
avirthostd [--port PORT] [--logs-dir DIR] [--mediator-bin PATH]
```

`avirthostd` has no subcommands: running it starts the server. Launch workloads with the [`avirt` command-line tool](./operator-cli.md#status-implemented). The `api` module of `avirt-core` defines the request and response types, routes, and default port for both the server and the command-line tool.

## Control API

`avirthostd` serves cleartext HTTP/2 on `127.0.0.1:<PORT>` (default 8787).

| Route | Request | Responses |
| --- | --- | --- |
| `POST /launch` | `{"agent":"claude"\|"codex","prompt":"…","cwd":"/abs/host/dir"}` (`cwd` optional) | 200 `{"workloadId","logPath"}`. 400 for an unknown agent, a missing prompt, a bad `cwd`, or missing credentials. 503 while shutting down. |
| `POST /health` | Any JSON body | 200 `{"ok":true}` |

The control API has no authentication. Any local process can start workloads that use the user's Claude token or ChatGPT login and mount host directories with `cwd`.

avirthostd serves `POST /workloads/<id>/stream`, the one request on the workload's HTTP/2 connection to its mediator, only on a mediator's `WORKLOAD <id> <secret>` connection over its socket (see [workload topology](./workload-topology.md)). The TCP port excludes this route. Its responses:

| Status | When |
| --- | --- |
| 403 | The `x-avirt-token` header is missing or wrong. Sent before the workload's state changes. |
| 404 | No such workload |
| 409 | The workload already has a stream |
| 410 | The workload has exited |

A stream ends on either end-of-stream or reset. When it ends, avirthostd flushes the trailing partial line and the decoder's remaining bytes into the log.

## Working directory

Without `cwd`, the agent works in the image's empty `/workspace`. With `cwd`, avirthostd bind-mounts that host directory read-write at `/workspace` and sets `AVIRT_CWD=/workspace`. `cwd` must be an absolute path to an existing directory and must not contain `:`, which would break the mount spec. `avirt launch --cwd` accepts a path relative to where it runs.

## Environment

| Variable | Read by | Default | Meaning |
| --- | --- | --- | --- |
| `PORT` | `avirthostd`, `avirt` | 8787 | Control API port |
| `LOGS_DIR` | `avirthostd` | `logs` | Log directory |
| `AVIRT_MEDIATOR_BIN` | `avirthostd` | `avirtmediatord` next to avirthostd | The mediator binary |
| `AVIRT_RUNTIME` | `avirthostd` | `containerization` | The runtime, `container` or `containerization` |
| `AVIRT_VMD_BIN` | `avirthostd` | `avirtvmd` next to avirthostd | The per-workload VM helper on Containerization |
| `AVIRT_IMAGE_ARCHIVES` | `avirthostd` | `target/images` | The directory of OCI archives that `mise run build-image` writes |
| `CLAUDE_IMAGE` | `avirthostd` | `avirt-workload-claude:dev` | Claude sandbox image |
| `CODEX_IMAGE` | `avirthostd` | `avirt-workload-codex:dev` | Codex sandbox image |
| `CODEX_HOST_HOME` | `avirthostd` | `~/.codex` | Directory of the host `auth.json` |
| `INITIAL_AGENT_MODE` | `avirthostd`, passed to Codex | Codex's own (`agent`) | `read-only`, `workspace-write`, `agent` or `agent-full-access` |
| `CLAUDE_CODE_OAUTH_TOKEN` | `avirthostd`, passed to Claude | none | Claude credential (see [credentials](./egress-credentials.md)) |
| `AVIRT_HOSTD_URL` | `avirt` | the local avirthostd on `--port` | avirthostd URL |

`avirtinitd` reads `AVIRT_WORKLOAD_ID`, `AVIRT_PROMPT`, `AVIRT_CWD` and `AVIRT_CONNECTION_TOKEN`, set by avirthostd, and `AVIRT_AGENT`, set in each sandbox image. The mediator reads `AVIRT_MEDIATOR_SECRET`.

## Log files

avirthostd creates the logs directory with mode 0700 and each file in it with mode 0600. Logs contain prompts, tool output, and possibly credentials. The planned workload log is in [observability](./observability.md#workload-log-1).

| File | Contents |
| --- | --- |
| `logs/<workload-id>.ndjson` | One envelope per line, as below |
| `logs/<workload-id>.container.log` | The sandbox container's stdout and stderr |
| `logs/<workload-id>.mediator.log` | The mediator's diagnostics |

## Envelope

```json
{"ts": "2026-10-17T06:50:14.181Z", "dir": "client->agent", "msg": {"jsonrpc": "2.0", "id": 0, "method": "initialize", "params": {}}}
```

`dir` is `client->agent`, `agent->client`, `meta` or `egress`. For the two agent directions, `msg` is the raw JSON-RPC message, or `{"raw": "..."}` if a line didn't parse. The tap is byte-transparent: bytes pass to the adapter unchanged, and the tap also reports each complete line. See [egress](./egress.md) for `egress` lines. The planned envelope fields are in [observability](./observability.md#workload-log-1).

## Meta events

| Event | Written by | Meaning |
| --- | --- | --- |
| `start` | avirtinitd | The workload started, with `agent`, `workloadId`, `cwd` and `prompt` |
| `agent_stderr` | avirtinitd | One line of the adapter's stderr |
| `auto_approve` | avirtinitd | avirtinitd approved a permission request, with `optionId` and `name` |
| `auth_methods` | avirtinitd (Codex) | The adapter's advertised authentication method ids |
| `error` | avirtinitd | A failure, including adapter crashes, with a `message` |
| `exit` | avirtinitd | The ACP session ended |
| `line_truncated` | avirtinitd or avirthostd | avirtinitd or avirthostd dropped a whole line longer than 16 MiB |
| `auth_writeback` | avirthostd (Codex) | The outcome of the `auth.json` write-back |
| `container_error` | avirthostd | A sandbox container or mediator failure |
| `container_exit` | avirthostd | The sandbox container exited. Always the last line. |

avirthostd writes `container_exit` only after the stream has drained, or at once if it never accepted a stream. It retains errors and writes them before this event, including when starting the sandbox container failed.

Both avirtinitd and avirthostd drop a whole line longer than 16 MiB through its newline, then write a `line_truncated` event in its place. They never write a fragment of that line as a record. One such line on the adapter's stdout normally yields two events: the Rust SDK responds to an unparseable line by writing it back to the adapter's stdin, and the tap drops that copy too. The SDK manages the adapter process without reporting its exit status, so the log has no adapter exit-status event.

# Status: planned

## Meta events

| Event | Written by | Meaning |
| --- | --- | --- |
| `sandbox` | avirtinitd | The Landlock ruleset of one process it launches, written before the process execs (see [Landlock](./landlock.md#recording-and-logging)) |
| `landlock_denial` | avirtinitd | One Landlock denial reported by the kernel's audit records (see [Landlock](./landlock.md#denials)) |
| `landlock_denials_dropped` | avirtinitd | The count of denials past the per-workload rate in the last second (see [Landlock](./landlock.md#denials)) |
