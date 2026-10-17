---
okf_version: "0.2"
---

# Architecture Overview

* [avirt architecture](architecture.md) - What avirt is, its components, the actor topology as implemented and as planned, and where each part is specified.
* [Components](components.md) - The avirt component inventory across the host, the mediator container, the workload container and the all-in-one server image, with where each component runs, what it owns, what it talks to, its startup and failure semantics, its status, and how the current binaries map onto it.

# API Reference

* [avirthostd API and log format](hostd-api.md) - How avirthostd is started, its control API and stream route, its environment variables, the working-directory mount, and the NDJSON actor log format with its meta events.

# Build Reference

* [Build and platforms](build-and-platforms.md) - Build targets for the guest and host binaries, how the Swift Containerization shim is gated and linked, signing, the CI matrix, the toolchain, host privileges per runtime and guest artifact pins.

# CLI Reference

* [avirt CLI](cli.md) - The avirt operator command, which asks a running avirthostd to launch an actor and can follow the actor's log until it ends.

# Component Design

* [Compute runtimes](compute-runtimes.md) - The runtime interface avirthostd uses for its per-actor primitives, the Containerization runtime that hosts Apple VMs inside avirthostd, the Docker Desktop runtime, and the Apple container CLI runtime the current code uses.
* [Egress, TLS termination and policy](egress.md) - How the agent's network traffic leaves the workload, the mediator's CONNECT proxy and destination checks, the planned mediator Envoy pipeline, the front-door Envoy as policy enforcement point, workload DNS, TLS termination, Cedar egress policy with name resolution and failure behavior, the event format and the mediator's lack of a network.
* [Lifecycle and state](lifecycle.md) - The order in which avirthostd launches and tears down an actor, avirthostd shutdown, the on-disk state root, the startup sweep after a crash, and path-length limits.
* [Workspaces](workspaces.md) - Workspace images as OCI artifacts overlaid onto a workload's base image, the .avirt-workspace.toml config and its zero-config defaults, the dev.avirt annotation set and workspace references, avirtregistryd and its search API, sandboxed builds and warm builds in avirtbuilderd, overlay application per runtime, the avirt workspace commands, and the Cedar actions that govern build, push, pull and launch.

# Metrics Inventory

* [Performance metrics](performance.md) - The metrics avirt measures, grouped by area, with each metric's exact start and end points, unit, measuring component, collection method, applicable runtimes and whether it is an SLO candidate or a diagnostic.

# Protocol Specification

* [Actor topology and link protocol](actor-topology.md) - The connections inside one actor and between actors (workload, mediator, avirthostd, host components, front-door Envoy), the runtime-provided channels and the service relay, the link streams, gated launch, freeze, reconnect, the uplink resume and port publication.

# Reference

* [Agents](agents.md) - How avirtinitd drives Claude and Codex over ACP, the pinned adapter versions, protocol versions, Codex modes, automatic approvals and known limitations.

# Roadmap

* [Roadmap](roadmap.md) - The planned phases from the implemented system to the planned design, each with its changes, what it removes and its verification checklist, plus the regression suite, the Envoy transition and its parity gates, the workload process split, unscheduled runtimes, components and operational work, and the open questions to resolve first.

# Security Design

* [Credentials](credentials.md) - Where Claude and Codex credentials live, how they reach the agent, the Codex auth.json seed and write-back, the planned credential ownership in avirtsecretsd, injection at the gateways and the mediator Envoy, placeholders, and avirtsecretsd-owned Codex token refresh.
* [Identity and mTLS](identity.md) - How the workload, the mediator, avirthostd, the host services and peer hosts authenticate each other, the SPIFFE trust domain and IDs, the durable trust anchors, SVID bootstrap and workload-authenticated rotation, federation, and the separation of the SPIFFE CA from the MITM CA.
* [Trust boundaries and reachability](trust-boundaries.md) - Every planned avirt listener with its transport, caller identity and allowed operations, the local listeners exempt from mutual TLS, what a compromised agent, guest root, mediator or peer host can reach, and the provenance labels on logged events.
* [Workload hardening](workload-hardening.md) - How avirtinitd confines the agent inside the workload, covering its user, capabilities, environment, seccomp filters and Landlock rules, the per-process profiles of the workload helpers, and what runs as root.
