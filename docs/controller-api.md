---
type: API Reference
title: Controller API and log format
description: How agentvmm-controller is started, its control API and stream route, its environment variables, the working-directory mount, and the NDJSON actor log format with its meta events.
tags: [agentvmm, controller, api, logging]
implementation: implemented
generated: { by: claude-code/claude-opus-5-5, at: 2026-10-17T14:23:31Z }
---

# Status: implemented

## Starting the controller

```text
agentvmm-controller [--port PORT] [--logs-dir DIR] [--supervisor-bin PATH]
```

The controller has no subcommands: running it starts the server. Actors are launched with the [`agentvmm` CLI](./cli.md). The request and response types, the routes and the default port are shared with the CLI through the `api` module of `agentvmm-core`.

## Control API

The controller serves cleartext HTTP/2 on `127.0.0.1:<PORT>` (default 8787).

| Route | Request | Responses |
| --- | --- | --- |
| `POST /launch` | `{"agent":"claude"\|"codex","prompt":"…","cwd":"/abs/host/dir"}` (`cwd` optional) | 200 `{"actorId","logPath"}`. 400 for an unknown agent, a missing prompt, a bad `cwd`, or missing credentials. 503 while shutting down. |
| `POST /health` | Any JSON body | 200 `{"ok":true}` |

The control API has no authentication. Any local process can start workloads that use the user's Claude token or ChatGPT login and mount host directories with `cwd`.

The stream route `POST /actors/<id>/stream` is served only on a supervisor's `ACTOR <id> <secret>` connection over the controller socket (see [actor topology](./actor-topology.md)), never on the TCP port. Its responses:

| Status | When |
| --- | --- |
| 403 | The `x-agentvmm-token` header is missing or wrong. Sent before the actor's state changes. |
| 404 | No such actor |
| 409 | The actor already has a stream |
| 410 | The actor has exited |

A stream ends on either end-of-stream or reset. When it ends, the controller flushes the trailing partial line and the decoder's remaining bytes into the log.

## Working directory

Without `cwd`, the agent works in the image's empty `/workspace`. With `cwd`, the controller bind-mounts that host directory read-write at `/workspace` and sets `AGENTVMM_CWD=/workspace`. `cwd` must be an absolute path to an existing directory and must not contain `:`, which would break the mount spec. `agentvmm launch --cwd` accepts a path relative to where it runs.

## Environment

| Variable | Read by | Default | Meaning |
| --- | --- | --- | --- |
| `PORT` | `agentvmm-controller`, `agentvmm` | 8787 | Control API port |
| `LOGS_DIR` | `agentvmm-controller` | `logs` | Log directory |
| `AGENTVMM_SUPERVISOR_BIN` | `agentvmm-controller` | `agentvmm-supervisor` next to the controller | The supervisor binary |
| `CLAUDE_IMAGE` | `agentvmm-controller` | `agentvmm-workload-claude:dev` | Claude workload image |
| `CODEX_IMAGE` | `agentvmm-controller` | `agentvmm-workload-codex:dev` | Codex workload image |
| `CODEX_HOST_HOME` | `agentvmm-controller` | `~/.codex` | Directory of the host `auth.json` |
| `INITIAL_AGENT_MODE` | `agentvmm-controller`, passed to Codex | Codex's own (`agent`) | `read-only`, `workspace-write`, `agent` or `agent-full-access` |
| `CLAUDE_CODE_OAUTH_TOKEN` | `agentvmm-controller`, passed to Claude | none | Claude credential (see [credentials](./credentials.md)) |
| `AGENTVMM_CONTROLLER_URL` | `agentvmm` | the local controller on `--port` | Controller URL |

The runner reads `AGENTVMM_ACTOR_ID`, `AGENTVMM_PROMPT`, `AGENTVMM_CWD` and `AGENTVMM_UPLINK_TOKEN`, set by the controller, and `AGENTVMM_AGENT`, set in each workload image. The supervisor reads `AGENTVMM_SUPERVISOR_SECRET`.

## Log files

The logs directory is created 0700 and every file in it 0600, since logs hold prompts, tool output and possibly credentials.

| File | Contents |
| --- | --- |
| `logs/<actor-id>.ndjson` | One envelope per line, as below |
| `logs/<actor-id>.container.log` | The workload's stdout and stderr |
| `logs/<actor-id>.supervisor.log` | The supervisor's diagnostics |

## Envelope

```json
{"ts": "2026-10-17T06:50:14.181Z", "dir": "client->agent", "msg": {"jsonrpc": "2.0", "id": 0, "method": "initialize", "params": {}}}
```

`dir` is `client->agent`, `agent->client`, `meta` or `egress`. For the two agent directions, `msg` is the raw JSON-RPC message, or `{"raw": "..."}` if a line did not parse. The tap is byte-transparent: bytes pass to the adapter unchanged, and each complete line is also reported. `egress` lines are described in [egress](./egress.md).

## Meta events

| Event | Written by | Meaning |
| --- | --- | --- |
| `start` | Runner | The actor started, with `agent`, `actorId`, `cwd` and `prompt` |
| `agent_stderr` | Runner | One line of the adapter's stderr |
| `auto_approve` | Runner | A permission request was approved, with `optionId` and `name` |
| `auth_methods` | Runner (Codex) | The adapter's advertised authentication method ids |
| `error` | Runner | A failure, including adapter crashes, with a `message` |
| `exit` | Runner | The ACP session ended |
| `line_truncated` | Runner or controller | A line longer than 16 MiB was dropped whole |
| `auth_writeback` | Controller (Codex) | The outcome of the `auth.json` write-back |
| `container_error` | Controller | A workload or supervisor failure |
| `container_exit` | Controller | The workload exited. Always the last line. |

`container_exit` is written only after the stream has drained, or at once if no stream was ever accepted. Errors are held and written before it, including when the workload could not be started.

A line longer than 16 MiB is dropped whole, through its newline, on both the runner side and the controller side, and replaced by a `line_truncated` event. A fragment of an oversized line is never written as a record. One oversized line on the adapter's stdout normally yields two events: the Rust SDK answers an unparseable line by writing it back to the adapter's stdin, and the tap drops that copy too. Constraint: there is no adapter exit-status event. The SDK owns the adapter process and does not report its exit status.
