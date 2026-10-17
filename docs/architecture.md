---
type: Architecture Overview
title: agentvmm-poc architecture
description: What agentvmm-poc is, its components, the actor topology as implemented and as planned, and where each part is specified.
tags: [agentvmm, architecture, overview]
implementation: partial
generated: { by: claude-code/claude-opus-5-5, at: 2026-10-17T14:23:31Z }
---

# Overview

agentvmm-poc runs ACP agents (Claude through claude-agent-acp, Codex through codex-acp) inside isolated containers on an Apple silicon Mac, with all network egress mediated and logged. Everything is Rust and uses the official `agent-client-protocol` crate.

| Component | Binary | Runs | Role |
| --- | --- | --- | --- |
| Controller | `agentvmm-controller` | On the host, one per user | Serves the control API, launches actors, owns real credentials, writes logs |
| CLI | `agentvmm` | On the host, per invocation | The operator command: `agentvmm launch` asks the controller to start an actor and can follow its log |
| Supervisor | `agentvmm-supervisor` | One per actor | Pingora egress proxy, link endpoint, and uplink relay to the controller |
| Runner | `agentvmm-runner` | As root inside the workload, one per actor | ACP client that spawns the agent adapter, taps every JSON-RPC line, relays the agent's egress to the supervisor, and applies the agent's sandbox |
| Agent | The adapter plus its tools | Inside the workload as uid 1000 | The ACP agent itself (`AGENTVMM_AGENT=claude\|codex`) |

An **actor** is one supervisor plus one workload. The **workload** is the container that runs the runner and the agent. One actor runs one ACP session. The controller launches actors on request (`agentvmm launch`) and writes each actor's JSON-RPC stream and egress events to `logs/<actor-id>.ndjson`.

# Topology

## Status: implemented

The supervisor is a host process. The workload has no network. Its only connections are two Unix socket files that the container runtime relays to the supervisor.

```mermaid
flowchart LR
    subgraph workload["workload (--network none, --init)"]
        agent["agent (uid 1000, no caps, seccomp)"]
        relay["runner proxy relay 127.0.0.1:PORT"]
        runner["runner (root)"]
        agent -->|HTTPS_PROXY| relay
    end
    subgraph host["macOS host"]
        sup["agentvmm-supervisor (one per actor)"]
        ctl["agentvmm-controller"]
        logs[("logs/ACTOR-ID.ndjson")]
        upstream(("upstream"))
        cli["agentvmm launch"]
    end
    relay -->|"/run/agentvmm/proxy.sock"| psock{{"proxy.sock (0600)"}}
    psock --> sup
    sup -->|"CONNECT"| upstream
    runner -->|"/run/agentvmm/uplink.sock"| usock{{"uplink.sock (0600)"}}
    usock --> sup
    sup -->|"controller.sock, L4 splice (HTTP/2)"| ctl
    sup -.->|"controller.sock, egress events"| ctl
    ctl --> logs
    cli -->|"POST /launch, 127.0.0.1:8787"| ctl
```

## Status: planned

The supervisor runs in its own container on its own per-actor network. The workload exposes one link endpoint, and the supervisor reaches it through a stream the compute runtime provides. Both streams carry mutual TLS with SPIFFE identities end to end. The supervisor terminates the agent's TLS, substitutes credentials for placeholders, and enforces policy.

```mermaid
flowchart LR
    subgraph host["macOS host"]
        ctl["agentvmm-controller: SPIFFE CA, real credentials, MITM CA, Codex refresh"]
        logs[("logs/ACTOR-ID.*")]
        hostauth[("~/.codex/auth.json")]
        ctl --> logs
        ctl <--> hostauth
    end
    subgraph supvm["supervisor (own network, NAT, nft)"]
        sup["agentvmm-supervisor: TLS client on the link, Pingora, MITM, policy"]
    end
    subgraph wl["workload (no network)"]
        runner["runner (root): TLS server on the link, h2 client, relay, freezer"]
        agent["agent (uid 1000): placeholders and the MITM CA cert only"]
        agent -->|"HTTPS_PROXY 127.0.0.1:PORT"| runner
    end
    link{{"link stream (runtime-provided)"}}
    cchan{{"controller channel (runtime-provided)"}}
    sup <-->|"mTLS end to end"| link
    link <--> runner
    sup <-->|"mTLS end to end"| cchan
    cchan <--> ctl
    sup -->|"TLS or opaque TCP"| upstream(("internet"))
```

# Design invariants

Status: planned. These hold for every runtime and every phase once the planned design is in place.

1. No SPIFFE Workload API is exposed to either the supervisor or the workload. The controller delivers each workload's bootstrap SVID at launch and pushes every later SVID and trust bundle over an authenticated channel.
2. Long-lived private keys are generated inside the workload or supervisor that uses them. The controller mints only a short-lived bootstrap key per workload and does not keep it.
3. TLS on the link terminates only at the runner and the supervisor, and TLS on the controller channel only at the supervisor and the controller. Runtime relays, shared volumes and controller-side vsock splicing carry ciphertext.
4. Every listener a guest can reach requires mutual TLS from the first byte.
5. uid 1000 in the workload never holds an SVID or its key, the SPIFFE bundle, a CA key, or a real credential.

# Documents

* [Actor topology and link protocol](./actor-topology.md) - the streams between runner, supervisor and controller, gated launch, freeze and reconnect
* [Identity and mTLS](./identity.md) - authentication on both hops, the SPIFFE trust domain, SVID issuance and rotation
* [Egress, TLS termination and policy](./egress.md) - the CONNECT proxy, destination checks, TLS termination, the policy file and the supervisor's network
* [Credentials](./credentials.md) - how Claude and Codex credentials reach the agent, placeholders and Codex token refresh
* [Workload hardening](./workload-hardening.md) - seccomp, Landlock, capabilities and the agent's environment
* [Lifecycle and state](./lifecycle.md) - launch and teardown order, the state root, crash cleanup and path limits
* [Compute runtimes](./compute-runtimes.md) - the runtime interface, the Containerization runtime for Apple, Docker Desktop, and the Apple container CLI runtime the current code uses
* [Build and platforms](./build-and-platforms.md) - build targets, Swift gating, linking, signing, the CI matrix, the toolchain and host privileges per runtime
* [Controller API and log format](./controller-api.md) - starting the controller, the control API, environment variables and the NDJSON log
* [agentvmm CLI](./cli.md) - the operator command that launches actors and follows their logs
* [Agents](./agents.md) - Claude and Codex specifics: ACP versions, modes, approvals and known limitations
* [Roadmap](./roadmap.md) - the planned phases and each phase's verification checklist
* [Performance metrics](./performance.md) - what to measure in each area, where, how, and which metrics are SLO candidates
