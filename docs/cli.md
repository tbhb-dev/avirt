---
type: CLI Reference
title: agentvmm CLI
description: The agentvmm operator command, which asks a running controller to launch an actor and can follow the actor's log until it ends.
tags: [agentvmm, cli]
implementation: implemented
generated: { by: claude-code/claude-opus-5-5, at: 2026-10-17T14:23:31Z }
---

# Status: implemented

`agentvmm` is the operator command, from `crates/agentvmm-cli`. It talks to a running [controller](./controller-api.md) over its control API, using the request and response types from the `api` module of `agentvmm-core`.

## launch

```text
agentvmm launch --agent claude|codex "<prompt>" [--cwd DIR] [--follow] [--port PORT] [--url URL]
```

| Option | Default | Meaning |
| --- | --- | --- |
| `--agent` | required | `claude` or `codex` |
| `<prompt>` | required | The prompt for the ACP session |
| `--cwd DIR` | none | Host directory to mount read-write at `/workspace`. A relative path is made absolute against the current directory. |
| `--follow`, `-f` | off | Print the actor log until the actor ends |
| `--port PORT` | `PORT`, else 8787 | Port of the local controller |
| `--url URL` | `AGENTVMM_CONTROLLER_URL`, else the local controller on `--port` | Controller URL |

`launch` posts `POST /launch` and prints the actor id and the log path. A refused launch prints the controller's status and error message and exits non-zero.

## Following

With `--follow`, `agentvmm` reads `logs/<actor-id>.ndjson` as it grows. It parses each line as JSON and stops only on the `container_exit` meta event, which is always the last line. If the log stops growing for 5 s, it calls `POST /health` with a 3 s timeout. If the controller does not answer, it exits with an error.

```mermaid
sequenceDiagram
    participant U as operator
    participant A as agentvmm
    participant C as agentvmm-controller
    participant L as logs/ACTOR-ID.ndjson
    U->>A: agentvmm launch --agent claude "prompt" --follow
    A->>C: POST /launch
    C-->>A: 200 {actorId, logPath}
    loop until container_exit
        A->>L: read new lines
        A->>U: print lines
        opt log idle for 5 s
            A->>C: POST /health
        end
    end
```

Through mise, `mise run launch <agent> "<prompt>"` runs `agentvmm launch`, and extra flags go after `--`, for example `mise run launch claude "Reply with pong" -- --cwd DIR --follow`.
