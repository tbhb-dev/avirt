---
type: Reference
title: Glossary
description: One definition for every avirt term and every standard term the avirt docs rely on, from workload, mediator, avirtedgeproxyd and the control plane to SPIFFE and OCI referrers, each with the document section that specifies it.
tags: [avirt, glossary, terminology, reference]
implementation: partial
generated: { by: codex/gpt-6, at: 2026-10-18T05:20:59Z }
---

# Overview

Each entry gives one definition, what the term isn't where another term is close to it, and the section that specifies it. An entry marked Status: planned names something only the planned design has. A term used unqualified in the docs means what its entry here says, and references to the other senses of the same word always include a qualifier.

# Terms

## ACP

The Agent Client Protocol: newline-delimited JSON-RPC 2.0 between an ACP client and an ACP agent, sent over the agent's stdin and stdout. avirt is always the client. A harness's adapter is always the agent. See [ACP roles](./acp.md#roles).

## ACP session

One conversation that the ACP client opens with `session/new` and drives with `session/prompt` turns. Session alone always means an ACP session. References to web UI login sessions and to MCP or TLS sessions always include the qualifier. See [session lifecycle](./acp.md#session-lifecycle).

## Adapter

The process that speaks ACP for a harness, claude-agent-acp or codex-acp, which avirt starts as uid 1000 and talks to over stdio. See [adapter process](./acp.md#adapter-process).

## Agent

In avirt, the adapter and every tool it starts, running as uid 1000 in the sandbox container, in the agent cgroup in the planned design. A launch's `agent` field and `AVIRT_AGENT` select the harness, and `agent` is also the name of a Codex mode and of the uid 1000 user, which aren't the agent process. See [architecture](./architecture.md#overview).

## Agent cgroup

The cgroup v2 group that holds the agent and every tool it starts, which avirtinitd freezes and kills as a unit. Status: planned. See [freeze and reconnect](./workload-topology.md#freeze-and-reconnect).

## Alias

An optional DNS label that a workload or a service contains beside its ID or name. Name resolution maps it to the same entity, and it never grants anything. Status: planned. See [avirt hostnames](./dns.md#avirt-hostnames).

## All-in-one server image

The deployment mode that runs every host service in one container, started in order by avirtinitd in server mode. Status: planned. See [all-in-one server image](./components.md#all-in-one-server-image).

<!-- The glossary defines Apple container CLI runtime verbatim. -->

## Apple container CLI runtime

The compute runtime the implemented system uses, which drives Apple's `container` command. It's removed once the Containerization runtime matches its behavior. See [Apple container CLI runtime](./compute-runtimes.md#apple-container-cli-runtime).

## Approval policy

The launch option, `auto`, `operator` or `reject`, that decides how avirtacp answers `session/request_permission`. The implemented system approves every request automatically. Status: planned. See [client capabilities](./acp.md#client-capabilities-1).

## Audit log

An append-only NDJSON file of audit events that one component alone writes, in its own state directory and its own `seq` domain, and that avirtauditd reads by offset. The host audit log is avirthostd's. Status: planned. See [audit logs](./observability.md#audit-logs).

## avirt

The project, always written in lowercase, and its operator CLI, the `avirt` binary. See [operator CLI](./operator-cli.md#status-implemented).

## avirt base image

The image from which every planned sandbox image derives. It includes the OS userland and the `agent` user, along with the per-process UIDs, the `avirt-ws` group, and every avirt guest binary. Status: planned. See [layer stack](./workload-lifecycle.md#layer-stack).

## avirt hostname

A DNS name under `host.avirt.internal`, or ending in an active peer's host name, that specifies a workload or a service, which only avirtdnsd answers. avirtdnsd maps it to avirtedgeproxyd on the host that serves it, and the workload receives a synthetic address. Status: planned. See [avirt hostnames](./dns.md#avirt-hostnames).

## avirtacp

The main workload process that drives an ACP adapter, taps every JSON-RPC line and serves the session control API. In the implemented system avirtinitd holds the ACP client instead. Status: planned. See [sandbox container](./components.md#sandbox-container).

## avirtadmin

The local admin CLI, which talks to avirthostd only over its admin socket. It authenticates with peer credentials and uses the `Admin` principal. Status: planned. See [admin CLI](./admin-cli.md#connection-and-authentication).

## avirtagent

The main workload process that drives a harness through its native interface rather than ACP. Status: planned. See [sandbox container](./components.md#sandbox-container).

## avirtapid

The API server: the REST and gRPC API in front of the local avirthostd, peers' avirtapid, and services. avirt, avirtwebd, and avirtguest access it through avirtedgeproxyd. Status: planned. See [host services](./components.md#host-services).

## avirtauditd

The host service that reads the workload logs and audit logs by offset and delivers them as OCSF. Status: planned. See [audit](./observability.md#audit).

## avirtbuilderd

The host service that schedules workspace builds and runs prebuild configurations. It promotes the resulting build artifacts into avirtregistryd. It never runs a build command itself. Status: planned. See [workspace builds and their execution](./workspace-build.md#builds).

## avirtdnsd

The host service that answers avirt hostnames with the address of avirtedgeproxyd on the host that serves them. Status: planned. See [DNS](./dns.md#the-avirtdnsd-zone).

## avirtedgeproxyd

The edge proxy. It evaluates the host policy snapshot and resolves external names. It checks private addresses before opening upstream connections. It's the global enforcement point for workload-to-workload, service, and external connections. Status: planned. See [avirtedgeproxyd](./components.md#avirtedgeproxyd).

## avirtexec

The main workload process for generic execution, which also runs as the build runner in a build job. Status: planned. See [sandbox container](./components.md#sandbox-container).

## avirtguest

The guest CLI, run on demand as uid 1000 inside the sandbox container, which reaches avirtapid under the `guest-cli` principal. Status: planned. See [guest CLI](./guest-cli.md#path).

## avirtguestd

The daemon that serves the file server and the MCP tool surface for workspace roots and workspace snapshots, reachable only through the `guestd` ingress target. Status: planned. See [sandbox container](./components.md#sandbox-container).

## avirthostd

The host daemon and the core of the control plane. It provides the control API and manages workload launch, teardown, and logs. The planned design also assigns it the SPIFFE issuer, SNI routing, and the content-addressed cache. See [avirthostd API](./hostd-api.md#control-api) and [host services](./components.md#host-services).

## avirtinitd

The root process inside the sandbox container. In the implemented system it runs the ACP client, the TCP proxy to `proxy.sock`, and the seccomp launcher. In the planned design it's PID 1 and the launcher and the freeze authority. It also has mediator and server modes. See [ACP roles](./acp.md#roles) and [sandbox container](./components.md#sandbox-container).

## avirtlogd

The host service that tails the host-written workload stdio logs and sends them to avirtoteld as OpenTelemetry Logs. Status: planned. See [workload stdout and stderr](./observability.md#workload-stdout-and-stderr).

## avirtmcp

The MCP stdio proxy that the agent launches on demand, which relays MCP to avirtmcpgwd over the `/mcp` stream of the workload's HTTP/2 connection to its mediator. Status: planned. See [sandbox container](./components.md#sandbox-container).

## avirtmcpgwd

The MCP gateway: the credential injector for MCP upstream credentials, which scopes tool discovery and invocation per workload. Status: planned. See [avirtmcpgwd](./egress-credentials.md#avirtmcpgwd).

## avirtmediatord

The mediator daemon. Its implemented host process runs the Pingora CONNECT app. The planned mediator container runs one avirtmediatord process under one UID, with one SVID. Workload lifecycle and rotation share that process with traffic inspection and credential injection. See [mediator process](./egress.md#mediator-process) and [mediator container](./components.md#mediator-container).

## avirtmodelgwd

The model gateway injects model credentials. It also applies per-workload model rules and budgets and serves the Codex refresh endpoint. Status: planned. See [avirtmodelgwd](./egress-credentials.md#avirtmodelgwd).

## avirtnetd

The daemon at the sandbox container's end of the workload's HTTP/2 connection to its mediator: it holds the workload SVID and handles redirected TCP. It reads UDP and ICMP from its TUN, and serves DNS, the optional forward proxy, and published ports. Status: planned. See [sandbox container](./components.md#sandbox-container).

## avirtoteld

The host's OpenTelemetry Collector: a Rust front end that stamps identity attributes from the caller's SVID, supervising a Collector child process. Status: planned. See [avirtoteld](./observability.md#avirtoteld).

## avirtpolicyd

The host service that stores, validates, proves, compiles, and distributes policy snapshots. Status: planned. See [egress policies](./egress-policies.md#policy-snapshots).

## avirtregistryd

The host service that stores workspace images in a supervised zot and maintains their secondary index. It serves the `WorkspaceRegistry` API and makes every off-host registry connection. Status: planned. See [avirtregistryd](./workspace-registry.md#avirtregistryd).

## avirtsecretsd

The secrets broker is the only component that reads a secret provider or refreshes or writes back a credential. It issues placeholders and leases. Status: planned. See [secrets](./secrets.md#overview).

## avirtwebd

The backend of the avirt web UI, which authenticates web UI users and calls avirtapid over REST. Status: planned. See [web UI](./components.md#web-ui).

## Base image reference

The base that a workspace image's layers depend on, recorded in its config blob as `none` or as a manifest digest and layer `diff_ids`, and in the OCI `base.name` and `base.digest` annotations. Status: planned. See [base image reference](./workspaces.md#base-image-reference).

## Binding

A credential binding is a record in avirtsecretsd that ties one secret to a placeholder and one credential injector. It specifies a header template and the destinations on which the injector may inject the credential. Binding alone always means a credential binding. Status: planned. See [bindings](./egress-credentials.md#bindings).

## Bootstrap SVID

The SVID and key, valid for 10 minutes, that avirthostd issues for each container at creation and hands to the compute runtime. Only this issuance proceeds without proof of a current key. Status: planned. See [bootstrap](./identity.md#bootstrap).

## Break-glass

The avirtadmin commands that avirthostd authorizes by peer credentials alone, without Cedar, for when the operator can't trust policy or the control plane. Status: planned. See [break-glass](./admin-cli.md#break-glass).

## Broker API

avirtsecretsd's API, also called the credential broker API, through which avirtmediatord and the gateways get placeholders and leases for the workload and generation they serve. Status: planned. See [APIs](./secrets.md#apis).

## Build job

A workload that avirthostd creates for avirtbuilderd with the build security context. It runs one build that processes untrusted content. The job has neither host control nor signing authority. Status: planned. See [build jobs](./workspace-build.md#build-jobs).

## Build security context

The security context of a build job's build runner, which runs as root in the build VM and may use the namespaces and mounts BuildKit needs. Status: planned. See [security contexts](./workload-hardening.md#security-contexts).

## Canonical ID

The UUIDv7 that avirthostd assigns to a workload at creation and never reuses, which SPIFFE IDs, Cedar entities, and log records contain. Authorization always uses it, never a short ID, alias, or generated name. Status: planned. See [workload IDs](./components.md#workload-ids).

## Capsule

A length-delimited message inside an HTTP stream. CONNECT-UDP uses `DATAGRAM` capsules with an RFC 9298 Context ID and a size checked before allocation. Status: planned. See [HTTP/2 streams](./networking.md#http2-streams).

## Cedar

The policy language, evaluated with the `cedar-policy` crate, that governs egress, ingress, DNS, TLS interception, workload management, workspaces, and secrets in avirt. Status: planned. See [egress policies](./egress-policies.md#schema).

## CEL

The Common Expression Language, which workspace configs and prebuild configurations use for filters and tag templates. Status: planned. See [expressions](./workspaces.md#expressions).

## Composed root filesystem

A cached root filesystem, or on Docker Desktop a composed image, made of a harness base and one workspace image's layers, from which each sandbox container gets its own writable copy. Status: planned. See [assembly per runtime](./workload-lifecycle.md#assembly-per-runtime).

## Compute runtime

The backend that creates and removes the sandbox container and mediator for a workload and provides their Unix sockets to the host. The Containerization runtime and the Apple container CLI runtime exist today. Status: planned. The Docker Desktop runtime is a third. See [runtime interface](./compute-runtimes.md#runtime-interface).

## Condition on policy sets

A condition that must remain true for every candidate policy set. The prover checks it before avirtpolicyd builds a policy snapshot. A condition can limit the requests a set permits. It can also require permission or denial for specified requests. Status: planned. See [conditions](./egress-policies.md#conditions-on-policy-sets).

## Configuration layer

The read-only OCI layer that avirthostd builds for each launch with the harness's managed settings, its MCP configuration and `/etc/avirt/launch.toml`, layer 4 of the layer stack. It contains no secret or placeholder value. Status: planned. See [configuration layer](./workload-lifecycle.md#configuration-layer).

## `confirm`

The control message with which avirtmediatord releases a sandbox container to start its agent, including the MITM CA certificate, the placeholders, the placeholder copies of files, the policy mode, and the port publications. Status: planned. See [launch handshake](./workload-topology.md#launch-handshake).

## CONNECT

The HTTP method that opens a tunnel to `host:port`. The implemented mediator accepts it through its [CONNECT app](./egress.md#pingora-connect-app). Status: planned. avirtnetd uses one HTTP/2 CONNECT stream per TCP flow received through a transparent listener or the optional forward proxy. See [HTTP/2 streams](./networking.md#http2-streams).

## CONNECT app

The implemented mediator's Pingora CONNECT proxy on `proxy.sock`, which parses the authority, applies the private-address block and dials upstream. See [CONNECT app](./egress.md#pingora-connect-app).

## CONNECT-UDP

An HTTP tunnel for relaying UDP datagrams to one immutable destination per stream, with `DATAGRAM` capsules over HTTP/2. Status: planned. See [HTTP/2 streams](./networking.md#http2-streams).

## Connection header

The first line the implemented mediator writes on each connection to avirthostd's socket, `WORKLOAD <id> <secret>` or `EGRESS <id> <secret>`, naming the connection's kind, the workload and the mediator secret, as in the PROXY protocol. See [sockets](./workload-topology.md#sockets).

## Connection token

The random 128-bit token avirthostd gives only avirtinitd as `AVIRT_CONNECTION_TOKEN`, which the implemented `POST /workloads/<id>/stream` request must include in `x-avirt-token`. Mutual TLS replaces it in the planned design. See [connection token](./identity.md#connection-token).

## Containerization runtime

The Apple compute runtime, the default on macOS, which hosts each sandbox container as its own VM through a per-workload avirtvmd helper and Apple's `containerization` package. Status: implemented. In the planned design each mediator also runs as its own VM. See [Containerization runtime](./compute-runtimes.md#containerization-runtime).

## Content-addressed cache

avirthostd's store of runtime artifacts, keyed by digest: kernels, initfs images, the unpacked avirt base, harness base and mediator images, configuration layers, and the workspace images it pulls. Status: planned. See [operational ownership](./components.md#operational-ownership).

## Context ID

The RFC 9298 identifier in a CONNECT-UDP `DATAGRAM` capsule. Status: planned. See [networking](./networking.md#http2-streams).

## Control plane

avirthostd plus the host services that configure and decide. These services include avirtpolicyd and avirtsecretsd. A control-plane action is one that avirthostd authorizes, such as `create_workload`. avirtedgeproxyd and avirtmediatord form the data plane and forward traffic. See [components](./components.md#overview).

## Credential injector

The one component that may inject a binding's real credential: avirtmodelgwd, avirtmcpgwd, or avirtmediatord. The binding field and Cedar attribute are both `injector`. Status: planned. See [credential injectors](./egress-credentials.md#credential-injectors).

## `credential-misplaced`

avirtmediatord's rejection, with 403 `credential-misplaced` in both policy modes, of any issued placeholder found anywhere in a request except its bound header. Status: planned. See [placeholders](./egress-credentials.md#placeholders).

## Data plane

avirtedgeproxyd and avirtmediatord, which transport and inspect traffic, as distinct from the control plane, which configures them. Status: planned. See [components](./components.md#overview).

## Datagram

One UDP message, kept intact within an HTTP/2 `DATAGRAM` capsule. The receiver checks its capsule length before allocation. Status: planned. See [HTTP/2 streams](./networking.md#http2-streams).

## Delegation chain

The chain of creators that created a workload. Its record uses `avirt.internal/creator` and `avirt.internal/root`, with a depth that host configuration caps. Peers receive it in `x-avirt-delegation`. Status: planned. See [workload management](./components.md#workload-management).

## DNS stub resolver

The avirtnetd code that sends each query to the mediator on its own POST `/dns` stream. Status: planned. See [networking](./networking.md#guest-networking).

## Docker Desktop runtime

The compute runtime that runs each sandbox container and each mediator as a container on Docker Desktop's single shared Linux kernel. Status: planned. See [Docker Desktop runtime](./compute-runtimes.md#docker-desktop-runtime).

## ECH

Encrypted ClientHello. Named flows using TLS or QUIC reject its extension, and synthetic answers omit `ech`. Status: planned. See [policy and endpoint checks](./networking.md#policy-and-endpoint-checks).

## `edgeproxy.sock`

avirtedgeproxyd's user-owned Unix socket `edgeproxy.sock` in the runtime directory, with mode 0600. avirt and local clients access avirtapid through it. Status: planned. See [listeners](./trust-boundaries.md#listeners).

## Egress event

An event about one connection (`conn`), one intercepted request (`http`) or one DNS query (`dns`), written into the workload log with `dir: "egress"`. The implemented system writes only `conn` events. See [events](./egress.md#events) and [planned events](./egress.md#events-1).

## Enforcement point

A component that evaluates policy and enforces the decision on traffic or a call (PEP). avirtedgeproxyd is the only global PEP for traffic that leaves a workload. Each avirtmediatord enforces its own workload policy snapshot. Status: planned. See [enforcement points](./egress-policies.md#enforcement-points).

## Enrollment

The act by which an operator adds a peer to this host from the peer's enrollment file, after comparing its fingerprint out of band. This makes the host trust the peer's trust bundle and avirt hostnames under that peer's host name. Status: planned. See [enrollment](./peering.md#enrollment).

## Enrollment file

The JSON file, written by `avirtadmin peer export`, that contains a host's trust domain, host names, peer listener addresses and trust bundle for enrollment on a peer. It contains no key or secret. Status: planned. See [enrollment file](./peering.md#enrollment-file).

## Entity revision

A numbered set of the Cedar entities avirthostd owns, such as namespaces, workloads, services and their labels, that avirtpolicyd builds policy snapshots against, in the way of a Kubernetes `resourceVersion`. Status: planned. See [building snapshots](./egress-policies.md#building).

## Envelope

One line of the workload log, `{ts, dir, msg}`, where `dir` is `client->agent`, `agent->client`, `meta` or `egress`. See [envelope](./hostd-api.md#envelope).

## External endpoint

A destination that's neither an avirt hostname nor a host service, given by a canonical host name or an IP literal and a port, and reached only through avirtedgeproxyd. Status: planned. See [how avirtmediatord handles egress](./egress.md#how-avirtmediatord-handles-egress).

## Federation

SPIFFE trust between two trust domains that exchange trust bundles. avirt federates only enrolled peers, and peers never share a CA key. Status: planned. See [federation](./identity.md#federation).

## Forward proxy

A proxy that opens connections on behalf of clients. avirtnetd provides an optional HTTP CONNECT proxy on loopback for explicitly configured clients. avirt omits proxy variables by default. Status: planned. See [connection types](./networking.md#connection-types).

## Freeze reason

A reason, `connection` or `workspace-snapshot`, for which avirtinitd keeps the agent cgroup frozen. The agent resumes only once no reason remains. Status: planned. See [workload startup and failure semantics](./components.md#workload).

## Generated name

The `<adverb>-<adjective>-<noun>` name, such as `quietly-brave-otter`, that avirthostd gives every workload at creation, unique in its namespace among live workloads. Status: planned. See [avirt hostnames](./dns.md#avirt-hostnames).

## Generation

The number of one mediator instance of a workload, starting at 1 and incremented on each mediator restart, included in the mediator's SVID as `<gen>` and in events as `gen`. Status: planned. See [freeze and reconnect](./workload-topology.md#freeze-and-reconnect).

## Generation fencing

Each host listener that accepts mediator SVIDs refuses connections from a superseded generation. It closes that generation's open connections after its successor's confirmation. Status: planned. See [authorization](./identity.md#authorization).

## `guest-cli` principal

The principal `spiffe://<td>/ns/<ns>/workload/<uuid>/guest-cli` under which avirtnetd forwards avirtguest's calls. It's never issued as an SVID, and any permission granted to this principal is also available to the agent. Status: planned. See [trust domain and IDs](./identity.md#trust-domain-and-ids).

## Harness

An agent product that avirt runs, Claude Code or Codex, together with its adapter, as one harness base image packages it and as a launch's `agent` selects it. See [`harness.toml`](./acp.md#harnesstoml).

## Harness base image

An image `FROM` the avirt base that adds one harness and its adapter under `/opt/agent` only, and contains no configuration. Status: planned. See [base images](./workload-lifecycle.md#base-images).

## `harness.toml`

The harness config file `/opt/agent/harness.toml` in a harness base image. It specifies the harness and its adapter command and protocol. It also specifies the main workload process and managed configuration paths. Status: planned. See [`harness.toml`](./acp.md#harnesstoml).

## Helper

A workload helper is a process in the sandbox container other than avirtinitd and the agent. The helpers are avirtnetd, the main workload process, and avirtguestd. Each runs under its own UID distinct from 0 and 1000. Status: planned. See [security contexts](./workload-hardening.md#security-contexts).

## Host

The machine that runs avirthostd and its host services. References to the literal DNS label `host`, a peer's host name `<host>`, the `Host` request header, or the host name of an external endpoint always distinguish these meanings. See [architecture](./architecture.md#overview).

## Host CA

The per-workload certificate authority that signs SSH host certificates. Its certificates identify hosts in that workload's set of hosts permitted for SSH interception. Status: planned. See [networking](./networking.md#ssh-and-git).

## Host certificate

An OpenSSH certificate that identifies a host. avirtmediatord presents one from the workload's host CA only for hosts permitted for SSH interception. Status: planned. See [networking](./networking.md#ssh-and-git).

## Host key

The SSH key that identifies a server. An SSH binding pins the upstream host key. Status: planned. See [networking](./networking.md#authentication-and-commands).

## Host policy set

The policy set that applies to every decision on the host, beside the policy sets identified by name that apply only to the workloads that reference them. Status: planned. See [building snapshots](./egress-policies.md#building).

## Host service

An avirt daemon that runs on the host outside every workload, such as avirthostd, avirtsecretsd or avirtedgeproxyd, each with the SPIFFE ID `spiffe://<td>/service/<name>` and the Cedar entity `HostService::"<name>"`. Status: planned. See [host services](./components.md#host-services).

## Host-local service

A service running on the host outside avirt, such as one bound to the host's `127.0.0.1`, reachable only as the backend of a service record. Status: planned. See [avirtedgeproxyd](./components.md#avirtedgeproxyd).

## httparse

The HTTP/1 request-head parser used before hyper normalizes a request. The parser checks ambiguity in each request head. Status: planned. See [HTTP and TLS](./networking.md#http-and-tls).

## ICMP

Internet Control Message Protocol. avirt supports echo through an extended CONNECT stream with `:protocol` `icmp`, with one target per stream. Status: planned. See [ICMP echo](./networking.md#icmp-echo).

## Image record

avirtregistryd's index entry for one workspace image: its repository, tags, digest, annotations, producer, base image reference, provenance, referrers and the live workloads that reference it. Status: planned. See [search API](./workspace-registry.md#search-api).

## Immutable tag

The tag `build-<build id>` that every promoted workspace image gets, which no other build reuses. Status: planned. See [tags](./workspace-build.md#tags).

## Incarnation

The random 16-byte value avirthostd generates for each avirtvmd start, which the helper presents in its hello so a leftover helper of the same workload can't pass for the current one. Status: implemented. See [avirtvmd](./compute-runtimes.md#avirtvmd).

## Ingress listener

avirtmediatord's listener on `/run/avirt/ingress.sock`, on which avirtedgeproxyd, through avirthostd's SNI routing, reaches that workload's published ports. Status: planned. See [runtime-provided sockets](./workload-topology.md#runtime-provided-sockets).

## Issuing CA

The in-memory intermediate SPIFFE CA, valid for 7 days and signed by the root, that signs every SVID. Status: planned. See [issuer](./identity.md#issuer).

## Label

A free-form key and value on a workload or a service, used for selection and visible to Cedar as an entity tag. Only avirthostd sets keys under `avirt.internal/`. A label isn't a trust boundary by itself. Status: planned. See [namespaces and labels](./components.md#namespaces-and-labels).

## Landlock

The Linux security module that confines a process's filesystem access, TCP ports, abstract Unix sockets and signals, which avirtinitd applies per security context. Status: planned. See [Landlock and seccomp](./landlock.md#status-planned).

## Launch handshake

The sequence on the workload's HTTP/2 connection to its mediator, from the mutual TLS handshake through `attach`, the first SVID rotation and the accepted `/envelopes` stream to `confirm`, before which no agent process exists. Status: planned. See [launch handshake](./workload-topology.md#launch-handshake).

## Launch intent

The mode-0600 `intent.json` record that avirthostd writes in a Containerization workload directory before it spawns avirtvmd. It identifies the workload and incarnation, records the spec digest, helper pid, and launch state, and lets startup match a helper to the intended launch. Status: implemented. See [runtime directory](./lifecycle.md#runtime-directory).

## Layer stack

The layers of a sandbox container's root filesystem, with five levels. The avirt base and a harness base form the first two. The workspace layers, the configuration layer, and the per-workload writable layer follow. Status: planned. See [layer stack](./workload-lifecycle.md#layer-stack).

## Lease

A time-bounded grant of one secret value from avirtsecretsd to one caller, for one workload, binding and generation, in the sense of a Vault lease, that expires no later than the secret's cache lifetime. Status: planned. See [scoping](./secrets.md#scoping).

## Main workload process

The one workload helper that drives the workload's work: avirtacp, avirtagent or avirtexec, named by the `main_process` key of `harness.toml`. Status: planned. See [sandbox container](./components.md#sandbox-container).

## Managed configuration paths

The paths, named in `harness.toml`, where a harness reads configuration that overrides project and user configuration, such as `/etc/claude-code`, which only the configuration layer writes. Status: planned. See [base images](./workload-lifecycle.md#base-images).

## MCP

The Model Context Protocol, which the agent speaks only to avirtmcp, and which avirtmcpgwd speaks to upstream MCP servers. Status: planned. See [avirtmcpgwd](./egress-credentials.md#avirtmcpgwd).

## Mediator

The component of a workload through which every byte the agent sends out passes. In the implemented system it's the avirtmediatord host process. It handles the agent's egress and connects `mediator.sock` to avirthostd's `hostd.sock` for the workload's HTTP/2 connection and log envelopes. In the planned design it's a container with no network that runs avirtinitd and avirtmediatord. See [architecture](./architecture.md#topology).

## Mediator secret

The random secret that avirthostd gives each implemented mediator and that the mediator presents in its connection headers, replaced by mutual TLS in the planned design. See [mediator secret](./identity.md#mediator-secret).

## Meta event

A workload log envelope with `dir: "meta"`, such as `start`, `auto_approve`, `exit` or `container_exit`. See [meta events](./hostd-api.md#meta-events).

## MITM CA

The per-workload CA whose leaf certificates let avirtmediatord perform TLS interception of the agent's connections. It's name-constrained to the hosts the workload's policy can intercept, its key never enters the sandbox container, and it's separate from the SPIFFE CA. Status: planned. See [MITM CA and leaf certificates](./egress.md#mitm-ca-and-leaf-certificates).

## MPTCP

Multipath TCP. avirtinitd turns it off with `net.mptcp.enabled=0`, and seccomp denies `IPPROTO_MPTCP`. Status: planned. See [guest networking](./networking.md#guest-networking).

## MTU

The largest IP packet a network interface sends without fragmentation, called its maximum transmission unit. Status: planned. avirt must choose and test its fragmentation strategy and TUN MTU. See [guest networking](./networking.md#guest-networking).

## Mutable tag

A movable OCI tag on a promoted workspace image, from `--tag` or a prebuild configuration's `tag-format`, which no selector reads. Status: planned. See [tags](./workspace-build.md#tags).

## Mutual TLS

TLS in which both sides present and verify certificates. avirt uses TLS 1.3 with SPIFFE X.509-SVIDs on every hop that crosses a container or host boundary. Status: planned. See [listeners](./trust-boundaries.md#listeners).

## Name constraints

The X.509 extension that restricts the set of names for which a CA may sign. avirt applies it to the SPIFFE root and issuing CA and to each MITM CA. Status: planned. See [MITM CA name constraints](./egress-policies.md#mitm-ca-name-constraints).

## Namespace

A grouping of workloads that's a trust and access boundary for policy and a scope for names, local to its host. Every workload is in exactly one namespace, fixed at creation. Status: planned. See [namespaces and labels](./components.md#namespaces-and-labels).

## NDJSON

Newline-delimited JSON, one object per line, the format of the workload log, the envelopes, and the audit logs. See [envelope](./hostd-api.md#envelope).

## OCI artifact

Content stored in an OCI registry under an image manifest with an `artifactType`. A workspace image uses this storage format. Status: planned. See [workspace image format](./workspaces.md#workspace-image-format).

## OCI referrers

The OCI distribution mechanism that lists the artifacts, such as signatures, SBOMs and build provenance, whose `subject` is a given manifest, which avirtregistryd serves through `Referrers`. Status: planned. See [search API](./workspace-registry.md#search-api).

## OCSF

The Open Cybersecurity Schema Framework, the format in which avirtauditd delivers audit events. Status: planned. See [audit events](./observability.md#audit-events).

## OpenTelemetry

The telemetry standard, with its OTLP protocol, in which avirt emits logs, metrics, and traces to avirtoteld. Status: planned. See [observability](./observability.md#overview).

## Peer

Another avirt host, with its own trust domain, that this host has enrolled. A peering is active once both hosts have enrolled each other. Status: planned. See [peering](./peering.md#enrollment).

## Peer listener

avirtedgeproxyd's TCP listener on the host network, which accepts only enrolled peers' edge proxies over mutual TLS with federated bundles. Status: planned. See [peer listener](./peering.md#peer-listener).

## Peer's host name

`<host>`, a peer's own fully qualified DNS name, taken from its enrollment file at enrollment and fixed for the life of the enrollment, which its avirtedgeproxyd must present as a DNS SAN. It's distinct from the literal label `host` in local avirt hostnames. Status: planned. See [peers' host names](./peering.md#peers-host-names).

## Placeholder

A string that avirtsecretsd issues per workload and binding, a client-acceptable prefix then `avirtph` then 32 random characters, which the sandbox container contains in place of a real credential and which grants nothing by itself. Status: planned. See [placeholders](./egress-credentials.md#placeholders).

## Policy mode

The `audit` or `enforce` setting in every policy snapshot, chosen per namespace. In audit mode the enforcement point allows a decision with no permit and logs it as `would-deny`, except for the checks that apply in both policy modes and the decisions that apply in both modes. Status: planned. See [modes](./egress-policies.md#modes).

## Policy reference

The reference in a workload's configuration to a policy set by name, immutable for the workload's life. Status: planned. See [building snapshots](./egress-policies.md#building).

## Policy set

A named, versioned group of Cedar policies, each addressed by its `@id`, such as the host policy set. Status: planned. See [policy](./admin-cli.md#policy).

## Policy snapshot

A versioned, compiled policy set with the entities its decisions need. It includes its scope, policy mode, and lifetime. Each enforcement point stores and evaluates it locally. The host policy snapshot covers the host, and a workload policy snapshot covers one workload. References to a policy snapshot always include its qualifier to distinguish it from a workspace snapshot. Status: planned. See [policy snapshots](./egress-policies.md#policy-snapshots).

## Port publication

Versioned state in avirthostd that makes one workload port reachable through avirtedgeproxyd, authorized by `publish_port` and ending at the latest with the workload. Status: planned. See [ingress](./workload-topology.md#ingress).

## Prebuild configuration

An operator-registered TOML file that tells avirtbuilderd which refs of one repository to keep built. It specifies triggers, tag format, and retention, for builds before workload launch. Status: planned. See [prebuild configurations](./workspace-build.md#prebuild-configurations).

## `primary` key

The Boolean key that selects which workspace source supplies the image's remote and revision annotations. Without an explicit selection, avirt uses the first `git` or `github` source. Status: planned. See [top-level keys](./workspaces.md#top-level-keys).

## Private-address block

The check, applied in both policy modes, that refuses every resolved address in a non-global range or assigned to one of the host's own interfaces, with 403 `private-dest`, as SSRF protection. It runs in the CONNECT app, and in the planned design at avirtedgeproxyd. See [private-address block](./egress.md#private-address-block-private-dest).

## `private-dest`

A check in both policy modes that refuses a resolved address in a non-global range or assigned to one of the host's own interfaces, with 403 `private-dest`, as SSRF protection. Today avirtmediatord's CONNECT app applies it to every resolved address of a TCP CONNECT. Status: planned. It runs at avirtedgeproxyd for TCP, UDP and ICMP after the mediator translates synthetic addresses to names and the edge resolves the real upstream address. See [`private-dest` check](./egress.md#private-dest).

## Producer

The authenticated origin of a workspace image in avirtregistryd's index, `builder`, `user:<principal>`, or `import`. avirtregistryd never reads it from annotations. Status: planned. See [producers](./workspace-registry.md#producers).

## Prover

The Cedar policy prover in avirtpolicyd. Before avirtpolicyd builds any snapshot, the prover checks every candidate policy set against the schema, the `@id` and `like` rules, and the conditions. It also derives each MITM CA's name constraints. Status: planned. See [the prover](./egress-policies.md#the-prover).

## Provider

A source of secret values that avirtsecretsd reads, such as `env`, `file`, `codex`, `keychain` or `1password`, either built in or a provider plugin. Status: planned. See [providers](./secrets.md#providers).

## Provider plugin

A provider that runs as a separate executable, which avirtsecretsd starts and supervises and talks to over gRPC on a socket pair. Status: planned. See [provider plugin interface](./secrets.md#provider-plugin-interface).

## QUIC

A transport protocol over UDP. avirt relays permitted datagrams after bounded Initial inspection and an SNI check. QUIC interception is outside the planned design. Status: planned. See [QUIC](./networking.md#quic).

## quinn

The Rust QUIC library used with avirt's bounded Initial parser for passthrough inspection. Status: planned. See [QUIC](./networking.md#quic).

## Required host service

A host service whose degraded state makes avirthostd refuse new launches: avirtsecretsd, avirtpolicyd, avirtedgeproxyd, avirtdnsd, avirtregistryd, and avirtmodelgwd. Status: planned. See [all-in-one server image](./components.md#all-in-one-server-image).

## Routing socket

A kernel socket that reports address and interface changes, which avirtedgeproxyd's watcher reads to keep the host address set for `private-dest`. Status: planned. See [networking](./networking.md#address-checks).

## RST

The TCP reset flag. avirtnetd responds with RST to TCP packets that enter its TUN after bypassing nftables REDIRECT. Status: planned. See [guest networking](./networking.md#guest-networking).

## RTP

Real-time Transport Protocol. An RTP datagram whose first byte is `0x80` follows ordinary UDP policy rather than QUIC Initial parsing. Status: planned. See [QUIC](./networking.md#quic).

## Runtime directory

`$TMPDIR/avirt-<uid>/`, avirthostd's per-run directory for workload directories, sockets, scratch homes and caches, which the startup cleanup empties, in the sense of systemd's `RuntimeDirectory=`. See [runtime directory](./lifecycle.md#runtime-directory).

## russh

The Rust SSH library used for mediator interception, including the per-workload host CA and upstream authentication. Status: planned. See [SSH and git](./networking.md#ssh-and-git).

## Sandbox container

The container in which avirtinitd and the agent or another main workload process run. Today it has no network beyond loopback (`--network none` on the Apple container CLI runtime, a VM with no network device on Containerization), and the agent's traffic leaves only through `proxy.sock`. Status: planned. Its TUN and nftables REDIRECT send each flow through avirtnetd and the workload's HTTP/2 connection to its mediator. See [sandbox container](./components.md#sandbox-container) and [connection to the mediator](./workload-topology.md#http2-streams).

## Sandbox image

Any image a sandbox container boots from: in the implemented system `avirt-workload-claude:dev` or `avirt-workload-codex:dev`, and in the planned design the avirt base and every image derived from it. See [sandbox images](./build-and-platforms.md#sandbox-images).

## Sandbox VM

The VM that runs the sandbox container on a runtime with VM isolation. Status: planned. The mediator has its own VM. See [compute runtimes](./compute-runtimes.md#runtime-interface).

## Seatbelt

The macOS sandbox facility that restricts a process according to a profile. avirt uses a profile to restrict host daemons' socket access. Status: planned. See [restrictions on host connections](./networking.md#restrictions-on-host-connections).

## Seccomp

The Linux syscall filter that avirtinitd applies to itself and every process it starts. It denies the socket domains and syscalls in its denylist. See [seccomp](./workload-hardening.md#seccomp).

## Secret record

A named record in avirtsecretsd, `<scope>/<name>`, that says which provider holds a value and how avirtsecretsd treats it, and never holds the value itself. Status: planned. See [secrets and references](./secrets.md#secrets-and-references).

## Secret reference

`secret:<scope>/<name>`, the form in which another record refers to a secret record. Status: planned. See [secrets and references](./secrets.md#secrets-and-references).

## Security context

The UID, capabilities, seccomp filter and Landlock ruleset with which avirtinitd's launcher starts one workload process, applied between fork and exec, in the sense of a Kubernetes `securityContext`. Status: planned. See [security contexts](./workload-hardening.md#security-contexts).

## Sender-constrained

A credential that requires proof from its sender, rather than possession of the credential alone. Bindings prefer this form where the recipient supports it. Status: planned. See [networking](./networking.md#limits-events-and-verification).

## Service

A named service that avirtedgeproxyd publishes from avirthostd's service record, whose backend is a workload's published port or an authorized host-local service, with the Cedar entity `Service`. It's not a host service. Status: planned. See [avirtedgeproxyd](./components.md#avirtedgeproxyd).

## Session control API

The interface through which operators attach to, prompt, cancel, and end a running ACP session. avirtacp serves it, and callers access it through avirtapid, avirthostd, and the mediator. Status: planned. See [session control API](./acp.md#session-control-api).

## SFTP

SSH File Transfer Protocol. A repository-bound credential binding never permits this subsystem. Status: planned. See [SSH and git](./networking.md#ssh-and-git).

## Short ID

The last 8 hexadecimal characters of a workload's canonical ID, unique on the host. It serves only as a DNS label. Authorization never uses it. Status: planned. See [workload IDs](./components.md#workload-ids).

## SNI

The <!-- vale write-good.TooWordy = NO -->Server Name Indication<!-- vale write-good.TooWordy = YES --> in a TLS ClientHello, by which avirthostd's SNI routing chooses a destination and which must equal the CONNECT host under TLS passthrough. Status: planned. See [SNI routing](./workload-topology.md#sni-routing).

## SNI routing

avirthostd reading only the TLS SNI of each connection on a mediator's outbound socket. It forwards the connection's bytes unchanged to the host service or avirtedgeproxyd the SNI names, without decrypting TLS. It never connects one mediator to another. Status: planned. See [SNI routing](./workload-topology.md#sni-routing).

## SPIFFE

The workload identity standard whose IDs, `spiffe://<td>/…`, and X.509-SVIDs authenticate every avirt hop across a container or host boundary. Status: planned. See [trust domain and IDs](./identity.md#trust-domain-and-ids).

## SPIFFE Workload API

The SPIFFE API through which workloads normally fetch SVIDs, which avirt never exposes to a mediator or a sandbox container. Status: planned. See [design invariants](./architecture.md#design-invariants).

## `src`

The `src` label on every planned event, such as `hostd`, `edgeproxy`, `gateway`, `mediator`, `secrets`, or `workload`. avirthostd stamps it from the authenticated connection the event arrived on. It states how far the recipient can trust the event. Status: planned. See [the event's `src` field](./trust-boundaries.md#src).

## SSE

Server-Sent Events, an HTTP response stream. avirt forwards it without buffering or a duration limit, subject to idle timeout. Status: planned. See [HTTP and TLS](./networking.md#http-and-tls).

## SSH interception

avirtmediatord acting as the SSH server for the guest and the SSH client for the upstream. It checks commands against policy and injects credentials through a binding. Status: planned. See [networking](./networking.md#ssh-and-git).

## Startup cleanup

The cleanup avirthostd runs before it listens, which removes every container, VM, socket and workload directory an earlier avirthostd left. See [crash cleanup](./lifecycle.md#crash-cleanup) and [startup cleanup](./lifecycle.md#startup-cleanup).

## State directory

avirthostd's directory for state that persists beyond the runtime directory. Startup cleanup leaves it intact. It stores the installation ID, the SPIFFE root, and the enrollment records, in the sense of systemd's `StateDirectory=`. Status: planned. See [durable state](./lifecycle.md#durable-state).

## SVCB

A DNS service binding record. avirtmediatord handles its `AliasMode`, `TargetName`, port, address hints, and parameter consistency. Status: planned. See [synthetic answers](./dns.md#synthetic-answers).

## SVID

A SPIFFE Verifiable Identity Document. avirt uses X.509-SVIDs with one URI SAN and a 1-hour lifetime, issued only by avirthostd. Status: planned. See [issuer](./identity.md#issuer).

## Synthetic address

An IP address assigned to a name in the mediator's synthetic address table for that workload. The mediator never reassigns it to another name during that workload's lifetime. Status: planned. See [names and DNS](./networking.md#names-and-dns).

## Tap

The JSON-RPC tap: the ACP client's hook that reports every line written to the adapter's stdin and read from its stdout and stderr, unaltered, as workload log envelopes. See [JSON-RPC tap](./acp.md#json-rpc-tap).

## TCP proxy to `proxy.sock`

avirtinitd's implemented listener on a `127.0.0.1` port that forwards each agent connection, unchanged, to the mediator's `proxy.sock`. avirtnetd's forward proxy replaces it in the planned design. See [TCP proxy to `proxy.sock`](./workload-topology.md#tcp-proxy-to-proxysock).

## TLS interception

avirtmediatord accepting the agent's TLS connection with a leaf certificate from the MITM CA. It checks each request and evaluates it under `request`, with the option to inject a credential. It then opens its own TLS connection to the upstream. This happens when `intercept` permits TLS at the destination port. Status: planned. See [how avirtmediatord handles egress](./egress.md#how-avirtmediatord-handles-egress).

## TLS passthrough

avirtmediatord relaying a connection to avirtedgeproxyd byte for byte without decrypting TLS. It neither inspects requests nor injects credentials. Every avirt hostname takes this path, as does an external connection when `passthrough` permits it. Status: planned. See [how avirtmediatord handles egress](./egress.md#how-avirtmediatord-handles-egress).

## Trailers-only

A gRPC response whose initial HEADERS also end the stream. Denials by policy use HTTP 200 and `grpc-status: 7`. Status: planned. See [networking](./networking.md#streaming).

## Transparent TCP listener

An avirtnetd listener that receives TCP redirected by nftables and obtains the original destination with SO_ORIGINAL_DST or IP6T_SO_ORIGINAL_DST. Status: planned. See [networking](./networking.md#guest-networking).

## Trust bundle

The set of root certificates that verifies SVIDs of one trust domain. avirthostd pushes it to every SVID holder, and peers fetch each other's from the bundle endpoint. Status: planned. See [federation](./identity.md#federation).

## Trust domain

The SPIFFE naming and trust scope of one avirt installation, `<install-id>.avirt`, fixed for the life of the installation. Status: planned. See [trust domain and IDs](./identity.md#trust-domain-and-ids).

## TUN

A virtual network device that exchanges IP packets with a process. avirtnetd holds the guest TUN descriptor and handles UDP and ICMP packets. nftables REDIRECT sends TCP to its transparent TCP listeners. Status: planned. See [guest networking](./networking.md#guest-networking).

## UDP association

The authenticated relationship between a UDP stream from the caller and a published port, with caller, workload generation, expiry, revocation, and reply limits. Status: planned. See [networking](./networking.md#ingress-and-peers).

## ULA

Unique Local Address, an IPv6 address from the private local range. Synthetic IPv6 answers use a ULA `/64`. The exact range remains open. Status: planned. See [synthetic answers](./dns.md#synthetic-answers).

## UUIDv7

A UUID whose leading bits are a millisecond timestamp and whose trailing bits are random, the form of a workload's canonical ID. Status: planned. See [workload IDs](./components.md#workload-ids).

## `via`

The Cedar context attribute that specifies the path a request came by: `egress`, `guest-cli`, `gateway`, `service`, `client`, `admin` or `mediator`. Status: planned. See [actions](./egress-policies.md#actions).

## vsock

The virtio socket transport between a VM and its host, which the Containerization runtime uses behind every Unix socket it provides to a sandbox container or mediator, and which seccomp denies to every workload process. Status: planned. See [Unix sockets](./compute-runtimes.md#unix-sockets).

## Workload

The unit avirthostd launches and tears down, consisting of a sandbox container plus its mediator, like a pod with an egress sidecar. It has one canonical ID, one namespace, and one Cedar `Workload` entity. A workload runs one ACP session, or more than one ACP session in a multi-session workload. See [architecture](./architecture.md#overview).

## Workload configuration

The immutable record avirthostd keeps for a workload: its namespace, IDs, alias, labels, image and workspace image digests, configuration layer digest, policy reference, and credential binding references. It stores references and never a secret value. Status: planned. See [host services](./components.md#host-services).

## Workload ID

The 8 random hexadecimal characters that name a workload in the implemented system and specify its log files, `logs/<workload-id>.*`. In the planned design a workload has a canonical ID and a short ID instead. See [workload log](./observability.md#workload-log).

## Workload log

The durable, sequence-numbered NDJSON record of one workload at `logs/<workload-id>.ndjson`. Only avirthostd writes it. It records every JSON-RPC line, meta event, and egress event, ending with `container_exit`. It's distinct from the workload stdio log. See [log files](./hostd-api.md#log-files).

## Workload management

The creation, listing, inspection, stopping and deletion of workloads by another workload through avirtguest, under policy and within the workload management constraints that keep a workload from gaining more authority than it holds. Status: planned. See [workload management](./components.md#workload-management).

## Workload policy snapshot

The policy snapshot scoped to one workload, which that workload's avirtmediatord evaluates. Status: planned. See [building snapshots](./egress-policies.md#building).

## Workload stdio log

The NDJSON file `logs/<workload-id>.stdio.ndjson` of a sandbox container's stdout, stderr and console, written by avirtvmd on the Containerization runtime. Status: implemented. avirtlogd tails the file. Status: planned. See [workload stdio logs](./components.md#workload-stdio-logs).

## Workload SVID

The SVID `spiffe://<td>/ns/<ns>/workload/<uuid>`, stored only by avirtnetd, which authenticates the sandbox container on its HTTP/2 connection to its mediator. Status: planned. See [trust domain and IDs](./identity.md#trust-domain-and-ids).

## `Workloads/KeepAlive`

The `Workloads/KeepAlive` call that a creator's host makes every 60 s to a peer for each workload created there, without which the peer stops that workload after 5 minutes. Status: planned. See [workload management across hosts](./peering.md#workload-management-across-hosts).

## Workspace

The content a sandbox container starts with beyond its harness base. It includes a source tree, dependencies, caches, and individual files. This term is unrelated to the `/workspace` directory itself and to a Cargo workspace. Status: planned. See [workspaces](./workspaces.md#status-planned).

## Workspace image

An OCI artifact that contains a workspace as a stack of layers, which avirthostd composes onto a harness base image when it creates a sandbox container. Status: planned. See [workspace image format](./workspaces.md#workspace-image-format).

## Workspace reference

The grammar that specifies one workspace image, or a rule that selects one: a registry repository, `oci:`, `github:`, `git:` or `dir:`. Status: planned. See [workspace references](./workspaces.md#workspace-references).

## Workspace root

A directory that a workspace image declares in its `roots`, `/workspace` and each directory destination under `/home/agent`, which avirtguestd serves and snapshots. Status: planned. See [workspace image format](./workspaces.md#workspace-image-format).

## Workspace snapshot

A copy of a workspace root that avirtguestd makes while avirtinitd keeps the agent cgroup frozen for the `workspace-snapshot` freeze reason. It's unrelated to a policy snapshot. Status: planned. See [sandbox container](./components.md#sandbox-container).

## Writable layer

Layer 5 of the layer stack, the per-workload writable layer that receives every write made inside the sandbox container. Removing the container also removes this layer. Status: planned. See [layer stack](./workload-lifecycle.md#layer-stack).

## `x-avirt-caller`

The header that contains the originating principal's identity on a stream avirtedgeproxyd originates. Only avirtedgeproxyd, avirtmediatord for `guest-cli`, avirtapid, and the gateways set it. Recipients refuse a missing, repeated, or unparseable header. Status: planned. See [avirtedgeproxyd](./components.md#avirtedgeproxyd).

## zot

The OCI registry that avirtregistryd runs as a supervised child process, reachable only by avirtregistryd over mutual TLS on loopback. Status: planned. See [avirtregistryd](./workspace-registry.md#avirtregistryd).
