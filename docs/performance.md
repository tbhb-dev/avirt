---
type: Metrics Inventory
title: Performance metrics
description: The metrics avirt measures, grouped by area, with each metric's exact start and end points, unit, measuring component, collection method, applicable runtimes and whether it is an SLO candidate or a diagnostic.
tags: [avirt, performance, metrics, observability]
implementation: planned
generated: { by: claude-code/claude-opus-5-5, at: 2026-10-17T14:23:31Z }
---

# Status: planned

Each table lists metrics with these columns:

| Column | Meaning |
| --- | --- |
| Definition | The exact start and end points |
| Where | The component that records the metric |
| How | Span, histogram, counter, gauge, log event, or external benchmark |
| Runtimes | Containerization, the Apple container CLI, Docker Desktop, or all three |
| Class | **SLO** for a candidate service-level objective, **diag** for a diagnostic |

Stages that exist only in the planned design (gated launch, the link, SVIDs, TLS termination) apply to Containerization and Docker.

# Actor lifecycle

| Metric | Definition | Unit | Where | How | Runtimes | Class |
| --- | --- | --- | --- | --- | --- | --- |
| `launch.accept` | `POST /launch` received → 200 response sent | ms | avirthostd | Span | All | diag |
| `launch.workload_boot` | `run_workload` called → the workload's init process running | ms | avirthostd | Span | All | diag |
| `launch.mediator_ready` | `run_mediator` called → the mediator's first `GET /actor` | ms | avirthostd | Span | Containerization, Docker | diag |
| `launch.link_attached` | Mediator ready → `attached` received by avirtinitd | ms | avirtinitd | Span | Containerization, Docker | diag |
| `launch.agent_started` | `POST /launch` received → `started` sent by avirtinitd (adapter spawned, on the CLI runtime) | ms | avirthostd | Span across the actor log's timestamps | All | SLO |
| `teardown.duration` | Workload exit observed → `container_exit` written | ms | avirthostd | Span | All | diag |
| `teardown.release` | `runtime.release` called → returned | ms | avirthostd | Span | All | diag |
| `sweep.duration` | Startup sweep started → avirthostd listening | ms | avirthostd | Span | All | diag |
| `sweep.items` | VMs, containers, networks, volumes and directories removed by the startup sweep, by kind | count | avirthostd | Counter | All | diag |

```mermaid
flowchart LR
    a["POST /launch"] -->|launch.workload_boot| b["workload running"]
    b -->|launch.mediator_ready| c["mediator GET /actor"]
    c -->|launch.link_attached| d["attached"]
    d --> e["confirm"]
    e --> f["started"]
    a -.->|launch.agent_started| f
```

# ACP session

| Metric | Definition | Unit | Where | How | Runtimes | Class |
| --- | --- | --- | --- | --- | --- | --- |
| `acp.initialize` | `initialize` request written to the adapter → response read | ms | avirtinitd | Span | All | diag |
| `acp.first_update` | `session/prompt` written → first `session/update` read | ms | avirtinitd | Span, or derived from the tap envelopes' `ts` | All | SLO |
| `acp.prompt_turn` | `session/prompt` written → turn end (the prompt response under ACP v1, idle after activity under ACP v2) | s | avirtinitd | Histogram | All | diag |
| `acp.tool_call` | A `tool_call` update → the matching `tool_call_update` with a terminal status | ms | avirtinitd | Histogram, or derived from the log | All | diag |
| `acp.permission` | `session/request_permission` read → the approval written | ms | avirtinitd | Histogram | All | diag |

# Link

| Metric | Definition | Unit | Where | How | Runtimes | Class |
| --- | --- | --- | --- | --- | --- | --- |
| `link.rtt` | Echo round trip of a small payload across the link, including avirthostd's vsock splice where present | µs | Bench harness | External benchmark | Containerization, Docker | diag |
| `link.throughput` | Sustained bytes per second per direction on one CONNECT stream across the link | MB/s | Bench harness | External benchmark | Containerization, Docker | diag |
| `link.stream_setup` | CONNECT stream opened by avirtinitd → response headers received. On the CLI runtime: `proxy.sock` connect → `200` read. | ms | avirtinitd | Histogram | All | diag |
| `link.mtls_handshake` | Transport connected → TLS 1.3 handshake complete with the peer ID authorized, per side | ms | avirtinitd, mediator, avirthostd | Histogram | Containerization, Docker | diag |
| `svid.issue` | CSR sent → SVID received, end to end, plus avirthostd's signing time | ms | Mediator, avirtinitd, avirthostd | Histogram | Containerization, Docker | diag |
| `svid.rotation` | `rotate` pushed → new connection carrying the new SVID in use and GOAWAY sent on the old one | ms | avirthostd, mediator | Span | Containerization, Docker | diag |
| `link.reconnect` | Mediator loss detected by avirtinitd (freeze) → SIGCONT after the new generation's `confirm` | ms | avirtinitd | Histogram | Containerization, Docker | SLO |
| `link.frozen_total` | Total time the agent spent frozen during one actor | ms | avirtinitd | Counter | Containerization, Docker | diag |

# Egress

| Metric | Definition | Unit | Where | How | Runtimes | Class |
| --- | --- | --- | --- | --- | --- | --- |
| `egress.connect_setup` | CONNECT head received by the mediator → `200` written | ms | Mediator | Histogram | All | SLO |
| `egress.dns` | Name lookup started → addresses returned | ms | Mediator | Histogram | All | diag |
| `egress.dial` | First upstream dial started → connected | ms | Mediator | Histogram | All | diag |
| `egress.tls_accept` | `200` written → downstream TLS handshake complete | ms | Mediator | Histogram | Containerization, Docker | diag |
| `egress.leaf` | Leaf certificate cache miss → leaf ready, plus the cache hit ratio | µs, ratio | Mediator | Histogram, counter | Containerization, Docker | diag |
| `egress.tls_added_latency` | Request completion time through termination minus through the opaque path, for the same request | ms | Bench harness | External benchmark | Containerization, Docker | diag |
| `egress.tls_added_throughput` | Throughput through termination relative to the opaque path, for the same transfer | ratio | Bench harness | External benchmark | Containerization, Docker | diag |
| `egress.policy_eval` | Rule evaluation started → decision, per connection and per request | µs | Mediator | Histogram | All | diag |
| `egress.substitution` | Credential substitution and leak scan, per request, including body scanning | µs | Mediator | Histogram | Containerization, Docker | diag |
| `egress.codex_refresh` | Intercepted refresh request received → synthesized response sent, plus avirthostd's `POST /actor/codex/refresh` time | ms | Mediator, avirthostd | Span | Containerization, Docker | diag |
| `egress.decisions` | Connections and requests by `decision` (`allow`, `deny`, `would-deny`) and by `rule` | count | Mediator | Counter, from the egress events | All | diag |
| `egress.errors` | Failures by kind: `bad-request`, `private-dest`, `plain-http`, DNS, dial, timeout, `shutdown`, `credential-misplaced` | count | Mediator | Counter, from the egress events | All | diag |
| `egress.bytes` | `bytes_up` and `bytes_down` per connection and per actor | bytes | Mediator | Counter, from the egress events | All | diag |

# Uplink and logging

| Metric | Definition | Unit | Where | How | Runtimes | Class |
| --- | --- | --- | --- | --- | --- | --- |
| `uplink.latency` | Line read by avirtinitd's tap (envelope `ts`) → line written to `logs/<actor-id>.ndjson` | ms | avirtinitd, avirthostd | Histogram | All | SLO |
| `log.write_rate` | Envelopes and bytes written per second, per actor log and in total | count/s, bytes/s | avirthostd | Counter | All | diag |
| `uplink.backpressure` | Time avirtinitd's uplink sender waits for flow-control window, and its queue depth | ms, count | avirtinitd | Histogram, gauge | All | diag |
| `log.line_truncated` | `line_truncated` events, by writer (avirtinitd or avirthostd) | count | avirtinitd, avirthostd | Counter, from the log | All | diag |
| `uplink.replay` | Envelopes replayed on resume, deduplicated by `seq`, and `uplink_gap` events | count | avirtinitd, avirthostd | Counter | Containerization, Docker | diag |

# Resources per actor

| Metric | Definition | Unit | Where | How | Runtimes | Class |
| --- | --- | --- | --- | --- | --- | --- |
| `mem.workload` | Workload memory: guest `memory.current` for a VM, the container's cgroup on Docker | MiB | avirtinitd, avirthostd | Gauge, peak and steady | All | diag |
| `mem.mediator` | Mediator memory, measured the same way | MiB | Mediator, avirthostd | Gauge | All | diag |
| `cpu.workload`, `cpu.mediator` | CPU seconds consumed per actor, per role | s | avirthostd | Counter | All | diag |
| `mem.hostd`, `cpu.hostd` | avirthostd RSS and CPU, including the host-side cost of in-process VMs on Containerization | MiB, s | avirthostd | Gauge, counter | All | diag |
| `disk.actor` | Bytes on disk per actor: writable root filesystem, `containers/<id>/`, `actors/<id>/` and its logs | MiB | avirthostd | Gauge | All | diag |
| `disk.caches` | Sizes of the kernel cache, the initfs and the image store | MiB | avirthostd | Gauge | Containerization | diag |
| `concurrency.max_actors` | The largest number of simultaneous actors for which `launch.agent_started`, `acp.first_update`, `egress.connect_setup` and `uplink.latency` stay within their objectives | count | Bench harness | External benchmark | All | SLO |

# Build and artifacts

| Metric | Definition | Unit | Where | How | Runtimes | Class |
| --- | --- | --- | --- | --- | --- | --- |
| `build.rust_cold` | Clean `cargo build --workspace` start → finish | s | CI, bench harness | External benchmark | All | diag |
| `build.rust_incremental` | `cargo build --workspace` after a one-file change in `avirt-core` | s | CI, bench harness | External benchmark | All | diag |
| `build.swift_shim` | Shim build from source (including Containerization) start → finish, and the same with `AVIRT_SWIFTSHIM_DIR` set | s | CI (macOS) | External benchmark | Containerization | diag |
| `build.images` | `mise run build-images`, cold and with cached layers | s | CI, bench harness | External benchmark | All | diag |
| `size.images` | Workload and mediator image sizes | MiB | CI | External benchmark | All | diag |
| `size.binaries` | `avirthostd`, `avirt`, `avirtmediatord` and `avirtinitd` sizes, per target | MiB | CI | External benchmark | All | diag |
| `size.dylib` | Swift shim dylib size | MiB | CI (macOS) | External benchmark | Containerization | diag |

# Reliability counters

| Metric | Definition | Unit | Where | How | Runtimes | Class |
| --- | --- | --- | --- | --- | --- | --- |
| `runtime.create_failures` | VM or container creation failures by cause: XPC and Virtualization.framework create or start errors, non-zero `container` or `docker` exits | count | avirthostd | Counter | All | diag |
| `runtime.retries` | Link dials, avirthostd-channel dials and uplink connect attempts beyond the first, per actor | count | Mediator, avirtinitd | Counter | All | diag |
| `mediator.restarts` | Mediator restarts per actor, and actors ended because restarts ran out | count | avirthostd | Counter | Containerization, Docker | diag |
| `actor.errors` | `container_error` events by reason | count | avirthostd | Counter, from the log | All | diag |
| `sweep.findings` | Items found at startup that a previous avirthostd left, by kind | count | avirthostd | Counter | All | diag |
| `codex.writeback` | `auth_writeback` outcomes by reason string (implemented), and refresh outcomes (planned) | count | avirthostd | Counter, from the log | All | diag |

# Collection

Status: planned.

- Each binary records the spans, histograms and counters above with `tracing`.
- When `OTEL_EXPORTER_OTLP_ENDPOINT` is set, they are exported as OTLP.
- Otherwise each actor's summaries are written into its log as `meta` `metric` events: through the uplink for avirtinitd, through the events stream for the mediator, and directly for avirthostd.
- Counters marked "from the log" can also be derived from existing actor log events.
- A `mise run bench` harness runs the external benchmarks: link RTT and throughput, the TLS termination overhead, `concurrency.max_actors`, and the build and size measurements.

See [roadmap](./roadmap.md) for the phases that introduce the planned stages.
