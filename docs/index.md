---
okf_version: "0.2"
---

# API Reference

* [avirthostd API and log format](hostd-api.md) - Starting avirthostd and using its control API, `POST /workloads/<id>/stream` route, environment variables, and working-directory mount. The NDJSON workload log format and its meta events.

# Architecture Overview

* [Components](components.md) - The avirt components on the host and in the mediator container, sandbox container, and all-in-one server image. Describes where components run and their state and connections, along with startup, failure, status, and the mapping from current binaries.
* [Networking](networking.md) - avirt's whole network model, including topology, names and IDs, connection types and their hops, enforcement points and identities, runtime transports, TCP, UDP, ICMP and DNS protocols, and listeners. Host enforcement of the restriction that only avirtedgeproxyd connects off the host, failure behavior, and observability, with links to each detailed specification.
* [avirt architecture](architecture.md) - What avirt is and its components. The implemented and planned workload topology, with a specification for each part.

# Build Reference

* [Build and platforms](build-and-platforms.md) - Build targets for guest and host binaries, Swift Containerization shim build gates and linking, and the Rust data-plane stack. Signing, the CI matrix, toolchain, host privileges per runtime, and guest artifact pins.

# Command reference

* [Operator CLI](operator-cli.md) - The avirt operator CLI and TUI (including the implemented avirt launch and log follow). Planned workload commands and commands for labels, namespaces, ports, services, sessions, logs, workspaces, peers, contexts, and host authentication. Output formats, exit codes, and each command's API call and Cedar action.
* [Admin CLI](admin-cli.md) - The avirtadmin tool connects directly to avirthostd over its admin socket with peer-credential authentication. Commands cover host status, daemon health and draining, namespaces, workloads, and policy through avirtpolicyd. Other commands cover peers and enrollment, the trust domain and CA, secrets providers and bindings, audit, and break-glass. Flags, output formats, exit codes, and each command's API call and Cedar action.
* [Guest CLI](guest-cli.md) - The avirtguest command-line tool runs as uid 1000 inside a sandbox container and connects to avirtapid through avirtnetd, avirtmediatord, and avirtedgeproxyd as the guest-cli principal. Its commands manage other workloads, sessions, logs, and ports under authorization and escalation constraints. Includes a comparison with avirt, output formats, exit codes, and the API call and Cedar action for each command.

# Component Design

* [1Password secrets provider](secrets-1password.md) - The 1Password provider for avirtsecretsd, a plugin built on the official 1Password Go SDK. Reference syntax, service account and desktop app authentication, caching and rate limiting, 1Password item mapping to avirt secrets, egress, and failure modes.
* [Compute runtimes](compute-runtimes.md) - The runtime interface avirthostd uses for its per-workload primitives. Containerization hosts Apple VMs inside avirthostd. Other runtimes use Docker Desktop and the current code's Apple container CLI runtime.
* [DNS](dns.md) - The avirt DNS zone that avirtdnsd serves and the local and internal names under a peer's host name for workloads and services. Label syntax and length caps, connections to avirtedgeproxyd, and use of names by the workload resolver and DNS stream.
* [Egress and TLS termination](egress.md) - Authorization of TCP, UDP, and ICMP flows, protocol inspection, `private-dest` checks, synthetic addresses, websockets, TLS certificates, and events in avirtmediatord and avirtedgeproxyd.
* [Lifecycle and state](lifecycle.md) - The order in which avirthostd launches and tears down a workload, avirthostd shutdown, the on-disk runtime directory, the startup cleanup after a crash, and path-length limits.
* [Observability](observability.md) - How avirt records and exposes its behavior through logs, audit, metrics, and traces. Workload and host logs, stdout and stderr per runtime, severity and fields, content and access, OCSF delivery, integrity, and retention. Metric attributes and cardinality, metrics from avirtmediatord and avirtedgeproxyd, trace propagation and spans, sampling, and trace context privacy. avirtoteld delivery and backpressure, and bundled dashboards and alerts.
* [Secrets management](secrets.md) - How avirtsecretsd brokers secrets, using references, providers, versions, and scopes. The provider plugin interface, memory caching and refresh, write-back, and per-caller Cedar authorization. The broker, registration, service, and administration APIs and their identities. Rotation and revocation, audit events, failures, storage at rest, bootstrapping, and the all-in-one image versus the Mac host.
* [Workload lifecycle and layers](workload-lifecycle.md) - The workload's root filesystem includes avirt base, harness base, workspace, configuration, and per-workload writable layers. Read-only and writable layers, their producers and creation times, caching, and assembly on each runtime. Filesystem contents during boot, running, and teardown.
* [Workspace builds](workspace-build.md) - avirtbuilderd and the three workspace build kinds, sandboxed jobs, the build pipeline and promotion, caching, and build stages with hooks and the Containerfile. Immutable and mutable tags, prebuild configurations and their triggers and retention, Cedar actions, trust boundaries, and open questions.
* [Workspace registry](workspace-registry.md) - avirtregistryd and its supervised zot child process, registry storage and repository names, the secondary index, and producers. The WorkspaceRegistry search and resolve API, Cedar actions, trust boundaries, and open questions.
* [Workspaces](workspaces.md) - Workspace OCI artifacts compose onto a harness base image. Image format, base references, and content rules. The `.avirt-workspace.toml` sources, includes, lockfile, secret exclusion, and zero-config defaults. Images built from local sources, dev.avirt annotations, workspace references and resolution, root filesystem composition per runtime, avirt workspace commands, and Cedar entities and launch action.
* [macOS Keychain secrets provider](secrets-keychain.md) - The macOS Keychain provider in avirtsecretsd uses the login keychain through the security-framework crate. Item classes and attributes, code signing and entitlements, prompting and user presence for a background daemon, and unavailability in the all-in-one Linux image. Storage locations for avirt's durable keys.

# Metrics Inventory

* [Performance metrics](performance.md) - The metrics avirt measures by area. Each metric's exact start and end points, unit, measuring component, collection method, applicable runtimes, and classification as an SLO candidate or a diagnostic.

# Protocol Specification

* [Agent Client Protocol in avirt](acp.md) - avirt's use of ACP: client and agent roles, the stdio adapter, JSON-RPC tap and log envelope, session lifecycle, and client capabilities. Permissions, errors and timeouts, `harness.toml`, session control through the mediator and avirtapid, the avirtacp split, and multi-session workloads.
* [Workload topology and connections](workload-topology.md) - Connections within and between workloads, including the sandbox container, mediator, avirthostd, host services, and avirtedgeproxyd. Runtime-provided Unix sockets, avirthostd SNI routing, HTTP/2 streams to the mediator, the launch handshake, freeze and reconnect, envelope resumption, and port publication.

# Reference

* [Claude harness](acp-claude.md) - avirt runs Claude Code through claude-agent-acp. The pinned adapter and bundled versions, avirtinitd's ACP v2 launch, OAuth token and placeholder authentication, and configuration sources. Managed settings and MCP configuration in the configuration layer, quirks, and required egress destinations.
* [Codex harness](acp-codex.md) - avirt runs Codex through codex-acp. The pinned adapter and bundled Codex, avirtinitd's ACP v1 launch, auth.json seed, refresh and write-back, planned placeholders, and Codex modes. Configuration sources and managed paths, quirks, and required egress destinations.
* [Glossary](glossary.md) - Definitions of avirt terms and the standard terms its docs use, with links to their specifications. Covers workload, mediator, avirtedgeproxyd, the control plane, SPIFFE, OCI referrers, and the other project terms.

# Roadmap

* [Roadmap](roadmap.md) - Planned phases from the implemented system to the planned design, with changes, removals, and verification checklists. Regression checks, the Rust data plane and parity gates, workload process and image splits, and the configuration layer. Unscheduled runtimes, components and operations, and open questions to resolve first.

# Security Design

* [Egress credentials](egress-credentials.md) - Current Claude and Codex credential storage and delivery to the agent, including Codex auth.json seed and write-back. Planned egress injection, bindings, injectors, avirtsecretsd broker, placeholders and misuse checks, and avirtmediatord HTTP and gRPC injection. Model and MCP gateways, Claude Code and Codex handling, Codex refresh, generation fencing, rotation and revocation, and workload visibility. Credentials for build jobs and workloads that create workloads.
* [Egress policies](egress-policies.md) - Cedar policy for egress, ingress, and the control plane, including schema, entities, actions, context, and tags. Enforcement in avirtmediatord, avirtedgeproxyd, and avirthostd. Name resolution and canonicalization, restrictions on like, and connect, intercept, passthrough, resolve, and request semantics. Audit and enforce modes and exemptions, snapshot builds and distribution, versioning and expiry, and revocation of open flows. The policy prover and derived MITM CA name constraints, failure behavior, and examples.
* [Identity and mTLS](identity.md) - Authentication among sandbox containers, mediators, avirthostd, host services, and peers. SPIFFE trust domain and IDs, durable root and bundle, SVID bootstrap and workload-authenticated rotation, federation, and separation of the SPIFFE and MITM CAs.
* [Peering](peering.md) - Peering between avirt hosts, using fully qualified peers' host names, enrollment files, and fingerprints exchanged out of band. Trust bundle exchange, root rotation, revocation and removal, discovery, and transport over avirtedgeproxyd's peer listener. What crosses a peering, policy on both hosts, cross-host delegation, and `Workloads/KeepAlive` calls that bound workloads created from peers. Pairwise topology, failures, observability and audit, and avirtadmin and avirt commands.
* [Landlock and seccomp](landlock.md) - Landlock inside sandbox containers and mediators, including the ABI of each runtime's kernel, mandatory and best-effort rights, the landlock crate, and launcher pre-exec steps. Security-context rulesets and their paths, ports, and scopes. Ruleset generation from harness.toml, workspace destinations, configuration, and policy, with limits on widening. Ruleset records, logs, verification, enforcement by layer, denial reporting, and regression checks.
* [Security principles](security.md) - avirt's security thesis and scope, expectations of external layers, extension points, and core security principles. Each concrete property has a status of Met, In Progress, Future, or Out of Scope against the current code.
* [Trust boundaries and reachability](trust-boundaries.md) - Each planned avirt listener with its transport, caller identity and allowed operations, the local listeners exempt from mutual TLS, what a compromised agent, root in the sandbox container, mediator or peer can reach, and the `src` field of each logged event.
* [Workload hardening](workload-hardening.md) - How avirtinitd confines the agent inside the sandbox container, covering its user, capabilities, environment and seccomp filters, the reserved paths, the security contexts of avirtnetd, avirtacp, avirtagent, avirtexec, and avirtguestd with links to their Landlock rulesets, and what runs as root.
