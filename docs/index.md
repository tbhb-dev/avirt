---
okf_version: "0.2"
---

# Architecture Overview

* [agentvmm-poc architecture](architecture.md) - What agentvmm-poc is, its components, the actor topology as implemented and as planned, and where each part is specified.

# API Reference

* [Controller API and log format](controller-api.md) - How agentvmm-controller is started, its control API and stream route, its environment variables, the working-directory mount, and the NDJSON actor log format with its meta events.

# Build Reference

* [Build and platforms](build-and-platforms.md) - Build targets for the guest and host binaries, how the Swift Containerization shim is gated and linked, signing, the CI matrix, the toolchain, host privileges per runtime and guest artifact pins.

# CLI Reference

* [agentvmm CLI](cli.md) - The agentvmm operator command, which asks a running controller to launch an actor and can follow the actor's log until it ends.

# Component Design

* [Compute runtimes](compute-runtimes.md) - The runtime interface the controller uses for its per-actor primitives, the Containerization runtime that hosts Apple VMs inside the controller, the Docker Desktop runtime, and the Apple container CLI runtime the current code uses.
* [Egress, TLS termination and policy](egress.md) - How the agent's network traffic leaves the workload, the supervisor's CONNECT proxy and destination checks, TLS termination in Pingora, the egress policy file, the event format and the supervisor's network.
* [Lifecycle and state](lifecycle.md) - The order in which the controller launches and tears down an actor, controller shutdown, the on-disk state root, the startup sweep after a crash, and path-length limits.

# Metrics Inventory

* [Performance metrics](performance.md) - The metrics agentvmm-poc measures, grouped by area, with each metric's exact start and end points, unit, measuring component, collection method, applicable runtimes and whether it is an SLO candidate or a diagnostic.

# Protocol Specification

* [Actor topology and link protocol](actor-topology.md) - The connections inside one actor (runner, supervisor, controller), the streams they carry, gated launch, freeze, reconnect and the uplink resume.

# Reference

* [Agents](agents.md) - How the runner drives Claude and Codex over ACP, the pinned adapter versions, protocol versions, Codex modes, automatic approvals and known limitations.

# Roadmap

* [Roadmap](roadmap.md) - The planned phases from the implemented system to the planned design, each with its changes, what it removes and its verification checklist, plus the regression suite, unscheduled runtimes and the open questions to resolve first.

# Security Design

* [Credentials](credentials.md) - Where Claude and Codex credentials live, how they reach the agent, the Codex auth.json seed and write-back, placeholder substitution in the supervisor, and controller-owned Codex token refresh.
* [Identity and mTLS](identity.md) - How the runner, the supervisor and the controller authenticate each other, the SPIFFE trust domain and IDs, SVID bootstrap and rotation, and the separation of the SPIFFE CA from the MITM CA.
* [Workload hardening](workload-hardening.md) - How the runner confines the agent inside the workload, covering its user, capabilities, environment, seccomp filters and Landlock rules, plus what runs as root.
