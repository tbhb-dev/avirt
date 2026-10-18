---
type: Metrics Inventory
title: Performance metrics
description: The metrics avirt measures, grouped by area, with each metric's exact start and end points, unit, measuring component, collection method, applicable runtimes and its classification as an SLO candidate or a diagnostic.
tags: [avirt, performance, metrics, observability]
implementation: planned
generated: { by: codex/gpt-6, at: 2026-10-18T05:14:22Z }
---

# Status: planned

Each table lists metrics with these columns:

| Column | Meaning |
| --- | --- |
| Definition | The exact start and end points |
| Where | The component that records the metric |
| How | Span, histogram, counter, gauge, log event, or external benchmark |
| Runtimes | Containerization, the Apple container CLI, Docker Desktop, or all three |
| Class | **SLO** for a candidate <!-- Required SLO term. --><!-- vale write-good.TooWordy = NO -->service-level objective<!-- vale write-good.TooWordy = YES -->, **diag** for a diagnostic |

Stages that exist only in the planned design (launch handshake, the workload's HTTP/2 connection to its mediator, SVIDs, TLS termination) apply to Containerization and Docker.

# Workload lifecycle

| Metric | Definition | Unit | Where | How | Runtimes | Class |
| --- | --- | --- | --- | --- | --- | --- |
| `launch.accept` | `POST /launch` received → 200 response sent | ms | avirthostd | Span | All | diag |
| `launch.workload_boot` | `run_workload` called → the workload container's init process running | ms | avirthostd | Span | All | diag |
| `launch.mediator_ready` | `run_mediator` called → the mediator's first `GET /workload` | ms | avirthostd | Span | Containerization, Docker | diag |
| `launch.connection_attached` | Mediator ready → `attached` received by avirtinitd | ms | avirtinitd | Span | Containerization, Docker | diag |
| `launch.agent_started` | `POST /launch` received → `started` sent by avirtinitd (adapter spawned, on the Apple container CLI runtime) | ms | avirthostd | Span using the workload log's timestamps | All | SLO |
| `teardown.duration` | Workload container exit observed → `container_exit` written | ms | avirthostd | Span | All | diag |
| `teardown.release` | `runtime.release` called → returned | ms | avirthostd | Span | All | diag |
| `sweep.duration` | Startup cleanup started → avirthostd listening | ms | avirthostd | Span | All | diag |
| `sweep.items` | VMs, containers, networks, volumes and directories removed by the startup cleanup, by kind | count | avirthostd | Counter | All | diag |

```mermaid
flowchart LR
    a["POST /launch"] -->|launch.workload_boot| b["workload container running"]
    b -->|launch.mediator_ready| c["mediator GET /workload"]
    c -->|launch.connection_attached| d["attached"]
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

# Workload connection to the mediator

| Metric | Definition | Unit | Where | How | Runtimes | Class |
| --- | --- | --- | --- | --- | --- | --- |
| `connection.rtt` | Echo round trip of a small payload across the workload's connection to its mediator, including avirthostd's vsock forwarding where present | µs | Bench harness | External benchmark | Containerization, Docker | diag |
| `connection.throughput` | Sustained bytes per second per direction on one CONNECT stream across that connection | MB/s | Bench harness | External benchmark | Containerization, Docker | diag |
| `connection.stream_setup` | CONNECT stream opened by avirtinitd → response headers received. On the Apple container CLI runtime: `proxy.sock` connect → `200` read. | ms | avirtinitd | Histogram | All | diag |
| `connection.mtls_handshake` | Transport connected → TLS 1.3 handshake complete with the peer ID authorized, per side | ms | avirtinitd, mediator, avirthostd | Histogram | Containerization, Docker | diag |
| `svid.issue` | CSR sent → SVID received, end to end, plus avirthostd's signing time | ms | Mediator, avirtinitd, avirthostd | Histogram | Containerization, Docker | diag |
| `svid.rotation` | `rotate` pushed → new connection in use with the new SVID and GOAWAY sent on the old one | ms | avirthostd, mediator | Span | Containerization, Docker | diag |
| `connection.reconnect` | Mediator loss detected by avirtinitd (freeze) → the agent resumed after the new generation's `confirm` | ms | avirtinitd | Histogram | Containerization, Docker | SLO |
| `connection.frozen_total` | Total time the agent spent frozen during one workload | ms | avirtinitd | Counter | Containerization, Docker | diag |

# Egress

| Metric | Definition | Unit | Where | How | Runtimes | Class |
| --- | --- | --- | --- | --- | --- | --- |
| `egress.connect_setup` | CONNECT head received by the mediator → `200` written | ms | Mediator and edge proxy | Histogram | All | SLO |
| `egress.dns` | Name lookup started → addresses returned | ms | Mediator, or avirtedgeproxyd where it resolves | Histogram | All | diag |
| `egress.dial` | First upstream dial started → connected | ms | Mediator, or avirtedgeproxyd where it dials | Histogram | All | diag |
| `egress.tls_accept` | `200` written → downstream TLS handshake complete | ms | Mediator | Histogram | Containerization, Docker | diag |
| `egress.leaf` | Leaf certificate cache miss → leaf ready, plus the cache hit ratio | µs, ratio | Mediator | Histogram, counter | Containerization, Docker | diag |
| `egress.tls_added_latency` | Request completion time through termination minus through TLS passthrough, for the same request | ms | Bench harness | External benchmark | Containerization, Docker | diag |
| `egress.tls_added_throughput` | Throughput through termination relative to TLS passthrough, for the same transfer | ratio | Bench harness | External benchmark | Containerization, Docker | diag |
| `egress.policy_eval` | Rule evaluation started → decision for each connection and each request | µs | Mediator and edge proxy | Histogram | All | diag |
| `egress.substitution` | Credential substitution and placeholder misuse check, per request, including body scanning | µs | Mediator | Histogram | Containerization, Docker | diag |
| `egress.codex_refresh` | Intercepted refresh request received → synthesized response sent, plus avirtmodelgwd's and avirtsecretsd's refresh time | ms | Mediator, avirtmodelgwd, avirtsecretsd | Span | Containerization, Docker | diag |
| `egress.decisions` | Connections and requests by `decision` (`allow`, `deny`, `would-deny`) and by `rule` | count | Mediator and edge proxy | Counter, from the egress events | All | diag |
| `egress.errors` | Failures by kind: `bad-request`, `private-dest`, `plain-http`, DNS, dial, timeout, `shutdown`, `credential-misplaced` | count | Mediator and edge proxy | Counter, from the egress events | All | diag |
| `egress.bytes` | `bytes_up` and `bytes_down` per connection and per workload | bytes | Mediator and edge proxy | Counter, from the egress events | All | diag |

# Logging

| Metric | Definition | Unit | Where | How | Runtimes | Class |
| --- | --- | --- | --- | --- | --- | --- |
| `envelopes.latency` | Line read by avirtinitd's tap (envelope `ts`) → line written to `logs/<workload-id>.ndjson` | ms | avirtinitd, avirthostd | Histogram | All | SLO |
| `log.write_rate` | Envelopes and bytes written per second for each workload log and in total | count/s, bytes/s | avirthostd | Counter | All | diag |
| `envelopes.backpressure` | Time avirtinitd's envelope sender on the workload's HTTP/2 connection to its mediator waits for flow-control window, and its queue depth | ms, count | avirtinitd | Histogram, gauge | All | diag |
| `log.line_truncated` | `line_truncated` events, by writer (avirtinitd or avirthostd) | count | avirtinitd, avirthostd | Counter, from the log | All | diag |
| `envelopes.replay` | Envelopes replayed on resume, deduplicated by `seq`, and `envelope_gap` events | count | avirtinitd, avirthostd | Counter | Containerization, Docker | diag |

# Resources per workload

| Metric | Definition | Unit | Where | How | Runtimes | Class |
| --- | --- | --- | --- | --- | --- | --- |
| `mem.workload` | Workload container memory: guest `memory.current` for a VM, the container's cgroup on Docker | MiB | avirtinitd, avirthostd | Gauge for peak and steady values | All | diag |
| `mem.mediator` | Mediator memory, measured the same way | MiB | Mediator, avirthostd | Gauge | All | diag |
| `cpu.workload`, `cpu.mediator` | CPU seconds consumed per workload, per container | s | avirthostd | Counter | All | diag |
| `mem.hostd`, `cpu.hostd` | avirthostd RSS and CPU, including the host-side cost of in-process VMs on Containerization | MiB, s | avirthostd | Gauge, counter | All | diag |
| `disk.workload` | Bytes on disk per workload: writable root filesystem, `containers/<id>/`, `workloads/<id>/` and its logs | MiB | avirthostd | Gauge | All | diag |
| `disk.caches` | Sizes of the kernel cache, the initfs and the content-addressed cache | MiB | avirthostd | Gauge | Containerization | diag |
| `concurrency.max_workloads` | The largest number of simultaneous workloads for which `launch.agent_started`, `acp.first_update`, `egress.connect_setup` and `envelopes.latency` stay within their objectives | count | Bench harness | External benchmark | All | SLO |

# Build and artifacts

| Metric | Definition | Unit | Where | How | Runtimes | Class |
| --- | --- | --- | --- | --- | --- | --- |
| `build.rust_cold` | Clean `cargo build --workspace` start → finish | s | CI, bench harness | External benchmark | All | diag |
| `build.rust_incremental` | `cargo build --workspace` after a one-file change in `avirt-core` | s | CI, bench harness | External benchmark | All | diag |
| `build.swift_shim` | Shim build from source (including Containerization) start → finish, and the same with `AVIRT_SWIFTSHIM_DIR` set | s | CI (macOS) | External benchmark | Containerization | diag |
| `build.images` | `mise run build-images` both cold and with cached layers | s | CI, bench harness | External benchmark | All | diag |
| `size.images` | Workload container and mediator image sizes | MiB | CI | External benchmark | All | diag |
| `size.binaries` | `avirthostd`, `avirt`, `avirtmediatord` and `avirtinitd` sizes, per target | MiB | CI | External benchmark | All | diag |
| `size.dylib` | Swift shim dylib size | MiB | CI (macOS) | External benchmark | Containerization | diag |

# Reliability counters

| Metric | Definition | Unit | Where | How | Runtimes | Class |
| --- | --- | --- | --- | --- | --- | --- |
| `runtime.create_failures` | VM or container creation failures by cause: XPC and Virtualization.framework create or start errors, non-zero `container` or `docker` exits | count | avirthostd | Counter | All | diag |
| `runtime.retries` | Dials of the workload's connection to its mediator, dials of the mediator's connection to avirthostd and `mediator.sock` connect attempts beyond the first, per workload | count | Mediator, avirtinitd | Counter | All | diag |
| `mediator.restarts` | Mediator restarts per workload, and workloads ended because restarts ran out | count | avirthostd | Counter | Containerization, Docker | diag |
| `workload.errors` | `container_error` events by reason | count | avirthostd | Counter, from the log | All | diag |
| `sweep.findings` | Items found at startup that a previous avirthostd left, by kind | count | avirthostd | Counter | All | diag |
| `codex.writeback` | `auth_writeback` outcomes by reason string (implemented), and refresh outcomes (planned) | count | avirthostd | Counter, from the log | All | diag |

# Collection

Status: planned.

- Each binary records the listed spans, histograms, and counters with `tracing` (see [observability](./observability.md#metrics)).
- When the environment includes `OTEL_EXPORTER_OTLP_ENDPOINT`, each binary exports its metrics as OTLP.
- Without that variable, each binary writes a workload's summaries into its log as `meta` `metric` events. avirtinitd uses the workload's HTTP/2 connection to its mediator. The mediator uses the events stream, and avirthostd writes directly.
- Existing workload log events also provide the data for "from the log" counters.
- A `mise run bench` harness runs the external benchmarks: the workload connection's RTT and throughput, the TLS termination overhead, `concurrency.max_workloads`, and the build and size measurements.

See [roadmap](./roadmap.md) for the phases that introduce the planned stages.
