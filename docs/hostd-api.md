---
type: API Reference
title: avirthostd API and log format
description: How avirthostd is started, its control API and the `POST /workloads/<id>/stream` route, its environment variables, the working-directory mount, and the NDJSON workload log format with its meta events.
tags: [avirt, avirthostd, api, logging]
implementation: partial
generated: { by: claude-code/claude-opus-5-5, at: 2026-10-17T14:23:31Z }
---

# Status: implemented

## Starting avirthostd

```text
avirthostd [--port PORT] [--logs-dir DIR] [--mediator-bin PATH]
```

`avirthostd` has no subcommands: running it starts the server. Workloads are launched with the [`avirt` CLI](./operator-cli.md#status-implemented). The request and response types, the routes and the default port are shared with the CLI through the `api` module of `avirt-core`.

## Control API

`avirthostd` serves cleartext HTTP/2 on `127.0.0.1:<PORT>` (default 8787).

| Route | Request | Responses |
| --- | --- | --- |
| `POST /launch` | `{"agent":"claude"\|"codex","prompt":"…","cwd":"/abs/host/dir"}` (`cwd` optional) | 200 `{"workloadId","logPath"}`. 400 for an unknown agent, a missing prompt, a bad `cwd`, or missing credentials. 503 while shutting down. |
| `POST /health` | Any JSON body | 200 `{"ok":true}` |

The control API has no authentication. Any local process can start workloads that use the user's Claude token or ChatGPT login and mount host directories with `cwd`.

The route `POST /workloads/<id>/stream`, the one request on the workload's HTTP/2 connection to its mediator, is served only on a mediator's `WORKLOAD <id> <secret>` connection over avirthostd's socket (see [workload topology](./workload-topology.md)), never on the TCP port. Its responses:

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
| `CLAUDE_IMAGE` | `avirthostd` | `avirt-workload-claude:dev` | Claude workload image |
| `CODEX_IMAGE` | `avirthostd` | `avirt-workload-codex:dev` | Codex workload image |
| `CODEX_HOST_HOME` | `avirthostd` | `~/.codex` | Directory of the host `auth.json` |
| `INITIAL_AGENT_MODE` | `avirthostd`, passed to Codex | Codex's own (`agent`) | `read-only`, `workspace-write`, `agent` or `agent-full-access` |
| `CLAUDE_CODE_OAUTH_TOKEN` | `avirthostd`, passed to Claude | none | Claude credential (see [credentials](./egress-credentials.md)) |
| `AVIRT_HOSTD_URL` | `avirt` | the local avirthostd on `--port` | avirthostd URL |

`avirtinitd` reads `AVIRT_WORKLOAD_ID`, `AVIRT_PROMPT`, `AVIRT_CWD` and `AVIRT_CONNECTION_TOKEN`, set by avirthostd, and `AVIRT_AGENT`, set in each workload image. The mediator reads `AVIRT_MEDIATOR_SECRET`.

## Log files

The logs directory is created 0700 and every file in it 0600, since logs hold prompts, tool output and possibly credentials. The planned workload log is in [observability](./observability.md#workload-log-1).

| File | Contents |
| --- | --- |
| `logs/<workload-id>.ndjson` | One envelope per line, as below |
| `logs/<workload-id>.container.log` | The workload container's stdout and stderr |
| `logs/<workload-id>.mediator.log` | The mediator's diagnostics |

## Envelope

```json
{"ts": "2026-10-17T06:50:14.181Z", "dir": "client->agent", "msg": {"jsonrpc": "2.0", "id": 0, "method": "initialize", "params": {}}}
```

`dir` is `client->agent`, `agent->client`, `meta` or `egress`. For the two agent directions, `msg` is the raw JSON-RPC message, or `{"raw": "..."}` if a line did not parse. The tap is byte-transparent: bytes pass to the adapter unchanged, and each complete line is also reported. `egress` lines are described in [egress](./egress.md). The planned envelope fields are in [observability](./observability.md#workload-log-1).

## Meta events

| Event | Written by | Meaning |
| --- | --- | --- |
| `start` | avirtinitd | The workload started, with `agent`, `workloadId`, `cwd` and `prompt` |
| `agent_stderr` | avirtinitd | One line of the adapter's stderr |
| `auto_approve` | avirtinitd | A permission request was approved, with `optionId` and `name` |
| `auth_methods` | avirtinitd (Codex) | The adapter's advertised authentication method ids |
| `error` | avirtinitd | A failure, including adapter crashes, with a `message` |
| `exit` | avirtinitd | The ACP session ended |
| `line_truncated` | avirtinitd or avirthostd | A line longer than 16 MiB was dropped whole |
| `auth_writeback` | avirthostd (Codex) | The outcome of the `auth.json` write-back |
| `container_error` | avirthostd | A workload container or mediator failure |
| `container_exit` | avirthostd | The workload container exited. Always the last line. |

`container_exit` is written only after the stream has drained, or at once if no stream was ever accepted. Errors are held and written before it, including when the workload container could not be started.

A line longer than 16 MiB is dropped whole, through its newline, on both the avirtinitd side and the avirthostd side, and replaced by a `line_truncated` event. A fragment of an oversized line is never written as a record. One oversized line on the adapter's stdout normally yields two events: the Rust SDK answers an unparseable line by writing it back to the adapter's stdin, and the tap drops that copy too. Constraint: there is no adapter exit-status event. The SDK owns the adapter process and does not report its exit status.

# Status: planned

## Meta events

| Event | Written by | Meaning |
| --- | --- | --- |
| `sandbox` | avirtinitd | The Landlock ruleset of one process it launches, written before the process execs (see [sandbox](./sandbox.md#recording-and-logging)) |
| `landlock_denial` | avirtinitd | One Landlock denial reported by the kernel's audit records (see [sandbox](./sandbox.md#denials)) |
| `landlock_denials_dropped` | avirtinitd | The count of denials past the per-workload rate in the last second (see [sandbox](./sandbox.md#denials)) |
