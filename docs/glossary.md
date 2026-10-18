---
type: Reference
title: Glossary
description: One definition for every avirt term and every standard term the avirt docs rely on, from workload, mediator, avirtedgeproxyd and the control plane to SPIFFE, SDS and OCI referrers, each with the document section that specifies it.
tags: [avirt, glossary, terminology, reference]
implementation: partial
generated: { by: claude-code/claude-opus-5-5, at: 2026-10-18T01:00:00Z }
---

# Overview

Each entry gives one definition, what the term is not where another term is close to it, and the section that specifies it. An entry marked Status: planned names something only the planned design has. A term used unqualified in the docs means what its entry here says, and the other senses of the same word are always qualified.

# Terms

## ACP

The Agent Client Protocol: newline-delimited JSON-RPC 2.0 between an ACP client and an ACP agent, carried over the agent's stdin and stdout. avirt is always the client, and a harness's adapter is always the agent. See [ACP roles](./acp.md#roles).

## ACP session

One conversation that the ACP client opens with `session/new` and drives with `session/prompt` turns. Session alone always means an ACP session: a web UI login session, an MCP session and a TLS session are always named as such. See [session lifecycle](./acp.md#session-lifecycle).

## Adapter

The process that speaks ACP for a harness, claude-agent-acp or codex-acp, which avirt starts as uid 1000 and talks to over stdio. See [adapter process](./acp.md#adapter-process).

## Agent

The adapter and every tool it starts, running as uid 1000 in the workload container, in the agent cgroup in the planned design. A launch's `agent` field and `AVIRT_AGENT` select the harness, and `agent` is also the name of a Codex mode and of the uid 1000 user, which are not the agent process. See [architecture](./architecture.md#overview).

## Agent cgroup

The cgroup v2 group that holds the agent and every tool it starts, which avirtinitd freezes and kills as a unit. Status: planned. See [freeze and reconnect](./workload-topology.md#freeze-and-reconnect).

## Alias

An optional DNS label that a workload or a service carries beside its ID or name. Name resolution maps it to the same entity, and it never grants anything. Status: planned. See [avirt names](./dns.md#avirt-names).

## All-in-one server image

The deployment mode that runs every host service in one container, started in order by avirtinitd in server mode. Status: planned. See [all-in-one server image](./components.md#all-in-one-server-image).

## Apple container CLI runtime

The compute runtime the implemented system uses, which drives Apple's `container` command. It is removed once the Containerization runtime matches its behavior. See [Apple container CLI runtime](./compute-runtimes.md#apple-container-cli-runtime).

## Approval policy

The launch option, `auto`, `operator` or `reject`, that decides how avirtacp answers `session/request_permission`. The implemented system approves every request automatically. Status: planned. See [client capabilities](./acp.md#client-capabilities-1).

## Audit log

An append-only NDJSON file of audit events that one component alone writes, in its own state directory and its own `seq` domain, and that avirtauditd reads by offset. The host audit log is avirthostd's. Status: planned. See [audit logs](./observability.md#audit-logs).

## avirt

The project, always written in lowercase, and its operator CLI, the `avirt` binary. See [operator CLI](./operator-cli.md).

## avirt base image

The image every planned workload image derives from: the OS userland, the `agent` user, the helper users, the `avirt-ws` group and every avirt guest binary. Status: planned. See [layer stack](./workload-lifecycle.md#layer-stack).

## avirt name

A DNS name under `host.avirt.internal`, or ending in an active peer's host name, that names a workload or a service, which only avirtdnsd answers and which resolves to avirtedgeproxyd on the host that serves it. Status: planned. See [avirt names](./dns.md#avirt-names).

## avirtacp

The main workload process that drives an ACP adapter, taps every JSON-RPC line and serves the session control API. In the implemented system avirtinitd holds the ACP client instead. Status: planned. See [workload container](./components.md#workload-container).

## avirtadmin

The local admin CLI, which talks to avirthostd only over its admin socket, authenticated by peer credentials, and carries the `Admin` principal. Status: planned. See [admin CLI](./admin-cli.md).

## avirtagent

The main workload process that drives a harness through its native interface rather than ACP. Status: planned. See [workload container](./components.md#workload-container).

## avirtapid

The API server: the REST and gRPC API in front of the local avirthostd, peers' avirtapid and services, which avirt, avirtwebd and avirtguest reach through avirtedgeproxyd. Status: planned. See [host services](./components.md#host-services).

## avirtauditd

The host service that reads the workload logs and audit logs by offset and delivers them as OCSF. Status: planned. See [audit](./observability.md#audit).

## avirtbuilderd

The host service that schedules workspace builds, runs prebuild policies and promotes build results into avirtregistryd. It runs no build command itself. Status: planned. See [builds](./workspace-build.md#builds).

## avirtdnsd

The host service that answers avirt names with the address of avirtedgeproxyd on the host that serves them. Status: planned. See [DNS](./dns.md).

## avirtedgeproxyd

The edge proxy: the host service that supervises the host's edge Envoy with libavirtenvoy and serves it local xDS and SDS. Through that Envoy it is the single global policy enforcement point for workload-to-workload, service, host-local service and external connections, and the only component that opens connections off the host. It runs in the edge VM. It is part of the data plane. Status: planned. See [avirtedgeproxyd](./components.md#avirtedgeproxyd).

## avirtexec

The main workload process for generic execution, which also runs as the build runner in a build job. Status: planned. See [workload container](./components.md#workload-container).

## avirtguest

The guest CLI, run on demand as uid 1000 inside the workload container, which reaches avirtapid under the `guest-cli` principal. Status: planned. See [guest CLI](./guest-cli.md).

## avirtguestd

The workload helper that serves the file server and the MCP tool surface for workspace roots and workspace snapshots, reachable only through the `guestd` ingress target. Status: planned. See [workload container](./components.md#workload-container).

## avirthostd

The host daemon and the core of the control plane: the control API, workload launch and teardown, the workload logs, and in the planned design the SPIFFE issuer, SNI routing and the content-addressed cache. See [avirthostd API](./hostd-api.md) and [host services](./components.md#host-services).

## avirtinitd

The root process inside the workload container: in the implemented system the ACP client, the TCP proxy to `proxy.sock` and the seccomp launcher, and in the planned design PID 1, the launcher and the freeze authority, with a mediator mode and a server mode. See [ACP roles](./acp.md#roles) and [workload container](./components.md#workload-container).

## avirtlogd

The host service that tails the host-written workload stdio logs and sends them to avirtoteld as OpenTelemetry Logs. Status: planned. See [workload stdout and stderr](./observability.md#workload-stdout-and-stderr).

## avirtmcp

The MCP stdio proxy that the agent launches on demand, which relays MCP to avirtmcpgwd over the `/mcp` stream of the workload's HTTP/2 connection to its mediator. Status: planned. See [workload container](./components.md#workload-container).

## avirtmcpgwd

The MCP gateway: the credential injector for MCP upstream credentials, which scopes tool discovery and invocation per workload. Status: planned. See [avirtmcpgwd](./egress-credentials.md#avirtmcpgwd).

## avirtmediatord

The mediator's control process. In the implemented system it is the whole mediator, a CONNECT proxy on the host that runs Pingora by default and an optional supervised Envoy. In the planned design it keeps the mediator's connection to avirthostd, requests both mediator SVIDs, handles generations and the workload lifecycle, and proxies no traffic. See [mediator process](./egress.md#mediator-process) and [mediator container](./components.md#mediator-container).

## avirtmedproxyd

The mediator proxy: the process in the mediator that supervises the mediator's Envoy with libavirtenvoy and serves it local xDS and SDS. Through that Envoy it carries all of one workload's ingress and egress, ends the workload's HTTP/2 connection to its mediator and evaluates the workload policy snapshot. It is part of the data plane. Status: planned. See [avirtmedproxyd pipeline](./egress.md#avirtmedproxyd-pipeline).

## avirtmodelgwd

The model gateway: the credential injector for model credentials, which also applies per-workload model rules and budgets and serves the Codex refresh endpoint. Status: planned. See [avirtmodelgwd](./egress-credentials.md#avirtmodelgwd).

## avirtnetd

The workload helper at the workload container's end of the workload's HTTP/2 connection to its mediator: it holds the workload SVID and runs the forward proxy for the agent, the DNS stub resolver and the reverse proxy to published ports. Status: planned. See [workload container](./components.md#workload-container).

## avirtoteld

The host's OpenTelemetry Collector: a Rust front end that stamps identity attributes from the caller's SVID, supervising a Collector child process. Status: planned. See [avirtoteld](./observability.md#avirtoteld).

## avirtpolicyd

The host service that stores, validates, proves, compiles and distributes policy snapshots. Status: planned. See [egress policies](./egress-policies.md).

## avirtregistryd

The host service that stores workspace images in a supervised zot, keeps their secondary index, serves the `WorkspaceRegistry` API and makes every off-host registry connection. Status: planned. See [avirtregistryd](./workspace-registry.md#avirtregistryd).

## avirtsecretsd

The secrets broker: the only component that reads a secret provider, refreshes a credential or writes one back, and the issuer of placeholders and leases. Status: planned. See [secrets](./secrets.md).

## avirtwebd

The backend of the avirt web UI, which authenticates web UI users and calls avirtapid over REST. Status: planned. See [web UI](./components.md#web-ui).

## Base image reference

The base that a workspace image's layers depend on, recorded in its config blob as `none` or as a manifest digest and layer `diff_ids`, and in the OCI `base.name` and `base.digest` annotations. Status: planned. See [base image reference](./workspaces.md#base-image-reference).

## Binding

A credential binding: a record in avirtsecretsd that ties one secret to a placeholder, one credential injector, a header template and the destinations it may be injected on. Binding alone always means a credential binding. Status: planned. See [bindings](./egress-credentials.md#bindings).

## Bootstrap SVID

The SVID and key, valid for 10 minutes, that avirthostd mints for each container at creation and hands to the compute runtime, the only issuance that needs no proof of a current key. Status: planned. See [bootstrap](./identity.md#bootstrap).

## Break-glass

The avirtadmin commands that avirthostd authorizes by peer credentials alone, without Cedar, for when policy or the control plane cannot be trusted. Status: planned. See [break-glass](./admin-cli.md#break-glass).

## Broker API

avirtsecretsd's API, also called the credential broker API, through which avirtmediatord, avirtmedproxyd and the gateways obtain placeholders and leases for the workload and generation they serve. Status: planned. See [APIs](./secrets.md#apis).

## Build job

A workload that avirthostd creates for avirtbuilderd, with the build security context, to run one build that processes untrusted content, with no host control or signing authority. Status: planned. See [build jobs](./workspace-build.md#build-jobs).

## Build security context

The security context of a build job's build runner, which runs as root in the build VM and may use the namespaces and mounts BuildKit needs. Status: planned. See [security contexts](./workload-hardening.md#security-contexts).

## Canonical ID

The UUIDv7 that avirthostd assigns to a workload at creation and never reuses, which SPIFFE IDs, Cedar entities and log records carry. Authorization always uses it, never a short ID, alias or generated name. Status: planned. See [workload IDs](./components.md#workload-ids).

## Cedar

The policy language, evaluated with the `cedar-policy` crate, that governs egress, ingress, DNS, TLS interception, workload management, workspaces and secrets in avirt. Status: planned. See [egress policies](./egress-policies.md#schema).

## CEL

The Common Expression Language, which workspace configs and prebuild policies use for filters and tag templates. Status: planned. See [expressions](./workspaces.md#expressions).

## Composed root filesystem

A cached root filesystem, or on Docker Desktop a composed image, made of a harness base and one workspace image's layers, from which each workload container gets its own writable copy. Status: planned. See [assembly per runtime](./workload-lifecycle.md#assembly-per-runtime).

## Compute runtime

The backend that creates and removes a workload's container and mediator and provides their Unix sockets to the host: the Containerization runtime, the Docker Desktop runtime or the Apple container CLI runtime. See [runtime interface](./compute-runtimes.md#runtime-interface).

## Configuration layer

The read-only OCI layer that avirthostd builds for each launch with the harness's managed settings, its MCP configuration and `/etc/avirt/launch.toml`, layer 4 of the layer stack. It holds no secret or placeholder value. Status: planned. See [configuration layer](./workload-lifecycle.md#configuration-layer).

## `confirm`

The control message with which avirtmediatord releases a workload container to start its agent, carrying the MITM CA certificate, the placeholders, the placeholder copies of files, the policy mode and the port publications. Status: planned. See [launch handshake](./workload-topology.md#launch-handshake).

## CONNECT

The HTTP method with which a proxy-aware client asks a forward proxy to open a tunnel to `host:port`, and the only method avirt's egress path accepts. See [CONNECT app](./egress.md#pingora-connect-app).

## CONNECT app

The implemented mediator's Pingora CONNECT proxy on `proxy.sock`, which parses the authority, applies the private-address block and dials upstream. See [CONNECT app](./egress.md#pingora-connect-app).

## Connection header

The first line the implemented mediator writes on each connection to avirthostd's socket, `WORKLOAD <id> <secret>` or `EGRESS <id> <secret>`, naming the connection's kind, the workload and the mediator secret, as in the PROXY protocol. See [sockets](./workload-topology.md#sockets).

## Connection token

The random 128-bit token avirthostd gives only avirtinitd as `AVIRT_CONNECTION_TOKEN`, which the implemented `POST /workloads/<id>/stream` request must carry in `x-avirt-token`. Mutual TLS replaces it in the planned design. See [connection token](./identity.md#connection-token).

## Containerization runtime

The Apple compute runtime of the planned design, which hosts each workload container and each mediator as its own VM inside avirthostd through Apple's `containerization` package. Status: planned. See [Containerization runtime](./compute-runtimes.md#containerization-runtime).

## Content-addressed cache

avirthostd's store of runtime artifacts, keyed by digest: kernels, initfs images, the unpacked avirt base, harness base and mediator images, configuration layers, and the workspace images it pulls. Status: planned. See [operational ownership](./components.md#operational-ownership).

## Control plane

avirthostd plus the host services that configure and decide, such as avirtpolicyd and avirtsecretsd. A control-plane action is one that avirthostd authorizes, such as `create_workload`. avirtedgeproxyd and avirtmedproxyd, which carry traffic, are the data plane. See [components](./components.md#overview).

## Credential injector

The one component at which a binding's real credential may be injected: avirtmodelgwd, avirtmcpgwd or avirtmedproxyd. The binding field and Cedar attribute are both `injector`. Status: planned. See [credential injectors](./egress-credentials.md#credential-injectors).

## Data plane

avirtedgeproxyd and avirtmedproxyd with their Envoys and libavirtenvoy, which carry and inspect traffic, as distinct from the control plane, which configures them. Status: planned. See [components](./components.md#overview).

## Delegation chain

The chain of creators through which a workload was created, recorded as `avirt.internal/creator` and `avirt.internal/root` with a depth that host configuration caps, and carried to peers in `x-avirt-delegation`. Status: planned. See [workload management](./components.md#workload-management).

## Docker Desktop runtime

The compute runtime that runs each workload container and each mediator as a container on Docker Desktop's single shared Linux kernel. Status: planned. See [Docker Desktop runtime](./compute-runtimes.md#docker-desktop-runtime).

## Dynamic forward proxy

The Envoy cluster type with which avirtedgeproxyd's Envoy resolves and dials external endpoints per connection. Status: planned. See [avirtedgeproxyd](./components.md#avirtedgeproxyd).

## Dynamic module

Envoy's interface for loading a shared library that adds filters to Envoy, through which libavirtenvoy runs. Status: planned. See [avirtmedproxyd pipeline](./egress.md#avirtmedproxyd-pipeline).

## Edge VM

The Linux container that runs only avirtedgeproxyd and its Envoy, under avirtinitd, and the only avirt VM with a network interface: a Containerization VM with a vmnet NAT interface on the Containerization runtime, and a container with normal container networking on Docker Desktop. Status: planned. See [edge VM](./components.md#edge-vm).

## Egress event

An event about one connection (`conn`), one intercepted request (`http`) or one DNS query (`dns`), written into the workload log with `dir: "egress"`. The implemented system writes only `conn` events. See [events](./egress.md#events) and [planned events](./egress.md#events-1).

## Enforcement point

A component that evaluates policy and enforces the decision on traffic or a call (PEP). avirtedgeproxyd is the single global PEP for traffic that leaves a workload, and each avirtmedproxyd enforces its own workload policy snapshot. Status: planned. See [enforcement points](./egress-policies.md#enforcement-points).

## Enrollment

The act by which an operator adds a peer to this host from the peer's enrollment file, after comparing its fingerprint out of band, so that its trust bundle and its peered names are trusted. Status: planned. See [enrollment](./peering.md#enrollment).

## Enrollment file

The JSON file, written by `avirtadmin peer export`, that carries a host's trust domain, host names, peer listener addresses and SPIFFE bundle for enrollment on a peer. It holds no key or secret. Status: planned. See [enrollment file](./peering.md#enrollment-file).

## Entity revision

A numbered set of the Cedar entities avirthostd owns, such as namespaces, workloads, services and their labels, that avirtpolicyd builds policy snapshots against, in the way of a Kubernetes `resourceVersion`. Status: planned. See [building snapshots](./egress-policies.md#building).

## Envelope

One line of the workload log, `{ts, dir, msg}`, where `dir` is `client->agent`, `agent->client`, `meta` or `egress`. See [envelope](./hostd-api.md#envelope).

## Envoy Proxy

The proxy that runs the data plane, supervised as avirtmedproxyd's Envoy in each mediator and avirtedgeproxyd's Envoy on the host, extended by libavirtenvoy. Status: planned. See [components](./components.md#overview).

## Event provenance

The `src` label on every planned event, such as `hostd`, `edgeproxy`, `gateway`, `mediator`, `secrets` or `workload`, which avirthostd stamps from the authenticated connection the event arrived on and which states how far the event can be trusted. Status: planned. See [event provenance](./trust-boundaries.md#event-provenance).

## External endpoint

A destination that is neither an avirt name nor a host service, given by a canonical host name or an IP literal and a port, and reached only through avirtedgeproxyd. Status: planned. See [avirtmedproxyd pipeline](./egress.md#avirtmedproxyd-pipeline).

## Federation

SPIFFE trust between two trust domains that exchange trust bundles. avirt federates only enrolled peers, and peers never share a CA key. Status: planned. See [federation](./identity.md#federation).

## Forward proxy

avirtnetd's HTTP CONNECT proxy on a `127.0.0.1` port, which the agent reaches through the proxy variables and which carries each connection as one HTTP/2 stream on the workload's connection to its mediator. It replaces avirtinitd's implemented TCP proxy to `proxy.sock`. Status: planned. See [transport scope](./egress.md#transport-scope).

## Freeze reason

A reason, `connection` or `workspace-snapshot`, for which avirtinitd keeps the agent cgroup frozen. The agent resumes only once no reason remains. Status: planned. See [workload startup and failure semantics](./components.md#workload).

## Generated name

The `<adverb>-<adjective>-<noun>` name, such as `quietly-brave-otter`, that avirthostd gives every workload at creation, unique in its namespace among live workloads. Status: planned. See [avirt names](./dns.md#avirt-names).

## Generation

The number of one mediator instance of a workload, starting at 1 and incremented on each mediator restart, carried in the mediator's SVIDs as `<gen>` and in events as `gen`. Status: planned. See [freeze and reconnect](./workload-topology.md#freeze-and-reconnect).

## Generation fencing

The refusal, at every host listener that accepts mediator SVIDs, of connections from a superseded generation, and the closing of its open connections once its successor is confirmed. Status: planned. See [authorization](./identity.md#authorization).

## `guest-cli` principal

The principal `spiffe://<td>/ns/<ns>/workload/<uuid>/guest-cli` under which avirtnetd forwards avirtguest's calls. It is never issued as an SVID, and anything granted to it is granted to the agent. Status: planned. See [trust domain and IDs](./identity.md#trust-domain-and-ids).

## Guest root

Root inside a workload container's VM or container, taken as a compromise scope: it holds the workload SVID but never a real credential or a mediator key. Status: planned. See [compromise scope](./trust-boundaries.md#compromise-scope).

## Harness

An agent product that avirt runs, Claude Code or Codex, together with its adapter, as one harness base image packages it and as a launch's `agent` selects it. See [`harness.toml`](./acp.md#harnesstoml).

## Harness base image

An image `FROM` the avirt base that adds one harness and its adapter under `/opt/agent` only, and holds no configuration. Status: planned. See [base images](./workload-lifecycle.md#base-images).

## `harness.toml`

The harness config file `/opt/agent/harness.toml` in a harness base image, which names the harness, its adapter command and protocol, its main workload process and its managed configuration paths. Status: planned. See [`harness.toml`](./acp.md#harnesstoml).

## Helper

A workload helper: a process in the workload container other than avirtinitd and the agent, namely avirtnetd, the main workload process and avirtguestd, each under its own UID distinct from 0 and 1000. Status: planned. See [security contexts](./workload-hardening.md#security-contexts).

## Host

The machine that runs avirthostd and its host services. The literal DNS label `host`, a peer host name `<host>`, the `Host` request header and the host name of an external endpoint are distinct and are always written as such. See [architecture](./architecture.md#overview).

## Host-local service

A service running on the host outside avirt, such as one bound to the host's `127.0.0.1`, reachable only as the backend of a service record. Status: planned. See [avirtedgeproxyd](./components.md#avirtedgeproxyd).

## Host policy set

The policy set that applies to every decision on the host, beside the named policy sets that apply only to the workloads that reference them. Status: planned. See [building snapshots](./egress-policies.md#building).

## Host service

An avirt daemon that runs on the host outside every workload, such as avirthostd, avirtsecretsd or avirtedgeproxyd, each with the SPIFFE ID `spiffe://<td>/service/<name>` and the Cedar entity `HostService::"<name>"`. Status: planned. See [host services](./components.md#host-services).

## Image record

avirtregistryd's index entry for one workspace image: its repository, tags, digest, annotations, producer, base image reference, provenance, referrers and the live workloads that reference it. Status: planned. See [search API](./workspace-registry.md#search-api).

## Immutable tag

The tag `build-<build id>` that every promoted workspace image gets, which no other build reuses. Status: planned. See [tags](./workspace-build.md#tags).

## Ingress listener

avirtmedproxyd's listener on `/run/avirt/ingress.sock`, on which avirtedgeproxyd's Envoy, through avirthostd's SNI routing, reaches that workload's published ports. Status: planned. See [runtime-provided sockets](./workload-topology.md#runtime-provided-sockets).

## Issuing CA

The in-memory intermediate SPIFFE CA, valid for 7 days and signed by the root, that signs every SVID. Status: planned. See [issuer](./identity.md#issuer).

## Keepalive

The `Workloads/KeepAlive` call that a creator's host makes every 60 s to a peer for each workload created there, without which the peer stops that workload after 5 minutes. Status: planned. See [workload management across hosts](./peering.md#workload-management-across-hosts).

## Label

A free-form key and value on a workload or a service, used for selection and visible to Cedar as an entity tag. Keys under `avirt.internal/` are set only by avirthostd, and a label is not a trust boundary by itself. Status: planned. See [namespaces and labels](./components.md#namespaces-and-labels).

## Landlock

The Linux security module that confines a process's filesystem access, TCP ports, abstract Unix sockets and signals, which avirtinitd applies per security context. Status: planned. See [sandbox and Landlock](./sandbox.md).

## Launch handshake

The sequence on the workload's HTTP/2 connection to its mediator, from the mutual TLS handshake through `attach`, the first SVID rotation and the accepted `/envelopes` stream to `confirm`, before which no agent process exists. Status: planned. See [launch handshake](./workload-topology.md#launch-handshake).

## Layer stack

The five layers of a workload container's root filesystem: the avirt base, a harness base, the workspace layers, the configuration layer and the per-workload writable layer. Status: planned. See [layer stack](./workload-lifecycle.md#layer-stack).

## Lease

A time-bounded grant of one secret value from avirtsecretsd to one caller, for one workload, binding and generation, in the sense of a Vault lease, that expires no later than the secret's cache lifetime. Status: planned. See [scoping](./secrets.md#scoping).

## libavirtenvoy

The Envoy dynamic module that the `avirtenvoy` crate builds, which evaluates Cedar locally, resolves avirt names to entities, mints MITM leaf certificates, runs the placeholder misuse check, injects mediator-bound credentials and writes events. The implemented module only checks CONNECT destinations and reports connection events. Status: partial. See [mediator container](./components.md#mediator-container).

## Local client listener

avirtedgeproxyd's Unix socket `edgeproxy.sock` in the runtime directory, 0600 and owned by the user, through which avirt and local clients reach avirtapid. Status: planned. See [listeners](./trust-boundaries.md#listeners).

## Local provenance

The mark on a workspace image built from host-local sources, which can never stand in for a ref's image and which policy can forbid. Status: planned. See [local provenance](./workspaces.md#local-provenance).

## Main workload process

The one workload helper that drives the workload's work: avirtacp, avirtagent or avirtexec, named by the `main_process` key of `harness.toml`. Status: planned. See [workload container](./components.md#workload-container).

## Managed configuration paths

The paths, named in `harness.toml`, where a harness reads configuration that overrides project and user configuration, such as `/etc/claude-code`, which only the configuration layer writes. Status: planned. See [base images](./workload-lifecycle.md#base-images).

## MCP

The Model Context Protocol, which the agent speaks only to avirtmcp, and which avirtmcpgwd speaks to upstream MCP servers. Status: planned. See [avirtmcpgwd](./egress-credentials.md#avirtmcpgwd).

## Mediator

The per-workload egress half of a workload, through which every byte the agent sends out passes. In the implemented system it is the avirtmediatord host process. In the planned design it is a container with no network that runs avirtmediatord, avirtmedproxyd and avirtmedproxyd's Envoy. See [architecture](./architecture.md#topology).

## Mediator secret

The random secret that avirthostd gives each implemented mediator and that the mediator presents in its connection headers, replaced by mutual TLS in the planned design. See [mediator secret](./identity.md#mediator-secret).

## Meta event

A workload log envelope with `dir: "meta"`, such as `start`, `auto_approve`, `exit` or `container_exit`. See [meta events](./hostd-api.md#meta-events).

## MITM CA

The per-workload CA whose leaf certificates let avirtmedproxyd's Envoy perform TLS interception of the agent's connections. It is name-constrained to the hosts the workload's policy can intercept, its key never enters the workload container, and it is separate from the SPIFFE CA. Status: planned. See [MITM CA and leaf certificates](./egress.md#mitm-ca-and-leaf-certificates).

## Mutable tag

A movable OCI tag on a promoted workspace image, from `--tag` or a prebuild policy's `tag-format`, which no selector reads. Status: planned. See [tags](./workspace-build.md#tags).

## Mutual TLS

TLS in which both sides present and verify certificates. avirt uses TLS 1.3 with SPIFFE X.509-SVIDs on every hop that crosses a container or host boundary. Status: planned. See [listeners](./trust-boundaries.md#listeners).

## Name constraints

The X.509 extension that limits the names a CA may sign for, which avirt applies to the SPIFFE root and issuing CA and to each MITM CA. Status: planned. See [MITM CA name constraints](./egress-policies.md#mitm-ca-name-constraints).

## Namespace

A grouping of workloads that is a trust and access boundary for policy and a scope for names, local to its host. Every workload is in exactly one namespace, fixed at creation. Status: planned. See [namespaces and labels](./components.md#namespaces-and-labels).

## NDJSON

Newline-delimited JSON, one object per line, the format of the workload log, the envelopes and the audit logs. See [envelope](./hostd-api.md#envelope).

## Non-policy checks

The checks that apply in both policy modes and that no policy can relax: authority parsing, the private-address block, `Host` and SNI matching, the placeholder misuse check, credential bindings, the Codex refresh interception's exact match, caller authentication and generation fencing. Status: planned. See [modes](./egress-policies.md#modes).

## OCI artifact

Content stored in an OCI registry under an image manifest with an `artifactType`, which is how a workspace image is stored. Status: planned. See [workspace image format](./workspaces.md#workspace-image-format).

## OCI referrers

The OCI distribution mechanism that lists the artifacts, such as signatures, SBOMs and build provenance, whose `subject` is a given manifest, which avirtregistryd serves through `Referrers`. Status: planned. See [search API](./workspace-registry.md#search-api).

## OCSF

The Open Cybersecurity Schema Framework, the format in which avirtauditd delivers audit events. Status: planned. See [audit events](./observability.md#audit-events).

## Off-host rule

The rule that avirtedgeproxyd is the only component that opens connections off the host, enforced below the daemons' configuration by a content filter on macOS and an nftables ruleset in the all-in-one server image. Status: planned. See [off-host enforcement](./networking.md#off-host-enforcement).

## OpenTelemetry

The telemetry standard, with its OTLP protocol, in which avirt emits logs, metrics and traces to avirtoteld. Status: planned. See [observability](./observability.md#overview).

## Peer

Another avirt host, with its own trust domain, that this host has enrolled. A peering is active once both hosts have enrolled each other. Status: planned. See [peering](./peering.md).

## Peer host name

`<host>`, a peer's own fully qualified DNS name, taken from its enrollment file at enrollment and fixed for the life of the enrollment, which its avirtedgeproxyd must present as a DNS SAN. It is distinct from the literal label `host` in local avirt names. Status: planned. See [peer host names](./peering.md#peer-host-names).

## Peer listener

avirtedgeproxyd's TCP listener on the host network, which accepts only enrolled peers' edge proxies over mutual TLS with federated bundles. Status: planned. See [peer listener](./peering.md#peer-listener).

## Peered name

An avirt name that ends in `<ns>.workload.<host>` or `<ns>.svc.<host>` for an active peer's host name, which names a workload or service on that peer and resolves locally to the peer's namespace. Status: planned. See [peer host names](./peering.md#peer-host-names).

## Placeholder

A string that avirtsecretsd issues per workload and binding, a client-acceptable prefix then `avirtph` then 32 random characters, which the workload container holds in place of a real credential and which grants nothing by itself. Status: planned. See [placeholders](./egress-credentials.md#placeholders).

## Placeholder misuse check

avirtmedproxyd's rejection, with 403 `credential-misplaced` in both policy modes, of any issued placeholder found anywhere in a request except its bound header. Status: planned. See [placeholders](./egress-credentials.md#placeholders).

## Policy invariant

A condition, written in Cedar, that the prover checks every candidate policy set against before any policy snapshot is built from it: a permissions boundary, which the set may not exceed; a required-allow invariant, which it must permit; or a guardrail, which it must deny. Status: planned. See [policy invariants](./egress-policies.md#policy-invariants).

## Policy mode

`audit` or `enforce`, carried in every policy snapshot and set per namespace. In audit mode a decision with no permit is allowed and logged as `would-deny`, except for the non-policy checks and the decisions that apply in both modes. Status: planned. See [modes](./egress-policies.md#modes).

## Policy reference

The named policy set that a workload's configuration references, immutable for the workload's life. Status: planned. See [building snapshots](./egress-policies.md#building).

## Policy set

A named, versioned group of Cedar policies, each addressed by its `@id`, such as the host policy set. Status: planned. See [policy](./admin-cli.md#policy).

## Policy snapshot

A versioned, compiled policy set with the entities its decisions need, its scope, its policy mode and its lifetime, which each enforcement point holds and evaluates locally. The host policy snapshot covers the host, and a workload policy snapshot covers one workload. A snapshot is always qualified, as distinct from a workspace snapshot. Status: planned. See [policy snapshots](./egress-policies.md#policy-snapshots).

## Port publication

Versioned state in avirthostd that makes one workload port reachable through avirtedgeproxyd, authorized by `publish_port` and ending at the latest with the workload. Status: planned. See [ingress](./workload-topology.md#ingress).

## Prebuild policy

An operator-registered TOML file that tells avirtbuilderd which refs of one repository to keep built, with its triggers, tag format and retention, in the sense of Codespaces and Gitpod prebuilds. Status: planned. See [prebuild policies](./workspace-build.md#prebuild-policies).

## Primary source

The first `git` or `github` source of a workspace config, or the one marked `primary`, whose remote and revision become the image's source annotations. Status: planned. See [top-level keys](./workspaces.md#top-level-keys).

## Private-address block

The non-policy check that refuses every resolved address in a non-global range or assigned to one of the host's own interfaces, with 403 `private-dest`, as SSRF protection. It runs in the CONNECT app, and in the planned design at avirtedgeproxyd. See [private-address block](./egress.md#private-address-block-private-dest).

## Producer

The authenticated origin of a workspace image in avirtregistryd's index, `builder`, `user:<principal>` or `import`, which is never read from annotations. Status: planned. See [producers](./workspace-registry.md#producers).

## Prover

The Cedar policy prover in avirtpolicyd, which checks every candidate policy set against the schema, the `@id` and `like` rules and the policy invariants before any snapshot is built, and derives each MITM CA's name constraints. Status: planned. See [the prover](./egress-policies.md#the-prover).

## Provider

A source of secret values that avirtsecretsd reads, such as `env`, `file`, `codex`, `keychain` or `1password`, either built in or a provider plugin. Status: planned. See [providers](./secrets.md#providers).

## Provider plugin

A provider that runs as a separate executable, which avirtsecretsd starts and supervises and talks to over gRPC on a socket pair. Status: planned. See [provider plugin interface](./secrets.md#provider-plugin-interface).

## Required host service

A host service whose degraded state makes avirthostd refuse new launches: avirtsecretsd, avirtpolicyd, avirtedgeproxyd, avirtdnsd, avirtregistryd and avirtmodelgwd. Status: planned. See [all-in-one server image](./components.md#all-in-one-server-image).

## Runtime directory

`$TMPDIR/avirt-<uid>/`, avirthostd's per-run directory for workload directories, sockets, scratch homes and caches, which the startup cleanup empties, in the sense of systemd's `RuntimeDirectory=`. See [runtime directory](./lifecycle.md#runtime-directory).

## Seccomp

The Linux syscall filter that avirtinitd applies to itself and every process it starts, denying the socket domains and syscalls in its denylist. See [seccomp](./workload-hardening.md#seccomp).

## Secret

A named record in avirtsecretsd, `<scope>/<name>`, that says which provider holds a value and how avirtsecretsd treats it, and never holds the value itself. Status: planned. See [secrets and references](./secrets.md#secrets-and-references).

## Secret discovery service

Envoy's xDS API for TLS certificates, keys, validation contexts and generic secrets (SDS). avirtedgeproxyd and avirtmedproxyd serve it locally to their Envoys, and avirtsecretsd serves binding secrets to avirtmedproxyd's Envoy. Status: planned. See [SDS](./secrets.md#sds).

## Secret reference

`secret:<scope>/<name>`, the form in which every other record names a secret. Status: planned. See [secrets and references](./secrets.md#secrets-and-references).

## Security context

The UID, capabilities, seccomp filter and Landlock ruleset with which avirtinitd's launcher starts one workload process, applied between fork and exec, in the sense of a Kubernetes `securityContext`. Status: planned. See [security contexts](./workload-hardening.md#security-contexts).

## Service

A named service that avirtedgeproxyd publishes from avirthostd's service record, whose backend is a workload's published port or an authorized host-local service, with the Cedar entity `Service`. It is not a host service. Status: planned. See [avirtedgeproxyd](./components.md#avirtedgeproxyd).

## Session control API

The interface through which operators attach to, prompt, cancel and end a running ACP session, served by avirtacp and reached through avirtapid, avirthostd and the mediator. Status: planned. See [session control API](./acp.md#session-control-api).

## Short ID

The last 8 hexadecimal characters of a workload's canonical ID, unique on the host, which is only a DNS label and is never used for authorization. Status: planned. See [workload IDs](./components.md#workload-ids).

## SNI

The Server Name Indication in a TLS ClientHello, by which avirthostd's SNI routing chooses a destination and which must equal the CONNECT host under TLS passthrough. Status: planned. See [SNI routing](./workload-topology.md#sni-routing).

## SNI routing

avirthostd reading only the TLS SNI of each connection on a mediator's outbound socket and forwarding the connection's bytes unchanged, without terminating TLS, to the host service or avirtedgeproxyd it names. It never connects one mediator to another. Status: planned. See [SNI routing](./workload-topology.md#sni-routing).

## SPIFFE

The workload identity standard whose IDs, `spiffe://<td>/…`, and X.509-SVIDs authenticate every avirt hop across a container or host boundary. Status: planned. See [trust domain and IDs](./identity.md#trust-domain-and-ids).

## SPIFFE Workload API

The SPIFFE API through which workloads normally fetch SVIDs, which avirt never exposes to a mediator or a workload container. Status: planned. See [design invariants](./architecture.md#design-invariants).

## Startup cleanup

The cleanup avirthostd runs before it listens, which removes every container, VM, socket and workload directory an earlier avirthostd left. See [crash cleanup](./lifecycle.md#crash-cleanup) and [startup cleanup](./lifecycle.md#startup-cleanup).

## State directory

avirthostd's directory for state that outlives the runtime directory and is never cleaned up at startup: the installation ID, the SPIFFE root and the enrollment records, in the sense of systemd's `StateDirectory=`. Status: planned. See [durable state](./lifecycle.md#durable-state).

## SVID

A SPIFFE Verifiable Identity Document. avirt uses X.509-SVIDs with one URI SAN and a 1-hour lifetime, issued only by avirthostd. Status: planned. See [issuer](./identity.md#issuer).

## Tap

The JSON-RPC tap: the ACP client's hook that reports every line written to the adapter's stdin and read from its stdout and stderr, unaltered, as workload log envelopes. See [JSON-RPC tap](./acp.md#json-rpc-tap).

## TCP proxy to `proxy.sock`

avirtinitd's implemented listener on a `127.0.0.1` port that forwards each agent connection, unchanged, to the mediator's `proxy.sock`. avirtnetd's forward proxy replaces it in the planned design. See [TCP proxy to `proxy.sock`](./workload-topology.md#tcp-proxy-to-proxysock).

## TLS interception

avirtmedproxyd's Envoy terminating the agent's TLS with a leaf certificate from the MITM CA, so that each request is checked, evaluated under `request` and can receive an injected credential, and then opening its own TLS connection to the upstream. It happens only when `intercept` permits it on port 443. Status: planned. See [avirtmedproxyd pipeline](./egress.md#avirtmedproxyd-pipeline).

## TLS passthrough

avirtmedproxyd's Envoy relaying a connection to avirtedgeproxyd byte for byte without terminating its TLS, so that no request is inspected and no credential is injected. Every avirt name takes it, and an external connection takes it when `passthrough` permits. Status: planned. See [avirtmedproxyd pipeline](./egress.md#avirtmedproxyd-pipeline).

## Trust bundle

The set of root certificates that verifies SVIDs of one trust domain, also called the SPIFFE bundle. avirthostd pushes it to every SVID holder, and peers fetch each other's from the bundle endpoint. Status: planned. See [federation](./identity.md#federation).

## Trust domain

The SPIFFE naming and trust scope of one avirt installation, `<install-id>.avirt`, fixed for the life of the installation. Status: planned. See [trust domain and IDs](./identity.md#trust-domain-and-ids).

## UUIDv7

A UUID whose leading bits are a millisecond timestamp and whose trailing bits are random, the form of a workload's canonical ID. Status: planned. See [workload IDs](./components.md#workload-ids).

## `via`

The Cedar context attribute that names the path a request came by: `egress`, `guest-cli`, `gateway`, `service`, `client`, `admin` or `mediator`. Status: planned. See [actions](./egress-policies.md#actions).

## vsock

The virtio socket transport between a VM and its host, which the Containerization runtime uses behind every Unix socket it provides to a workload container or mediator, and which seccomp denies to every workload process. Status: planned. See [Unix sockets](./compute-runtimes.md#unix-sockets).

## Workload

The unit avirthostd launches and tears down: a workload container plus its mediator, like a pod with an egress sidecar, with one canonical ID, one namespace and one Cedar `Workload` entity. A workload runs one ACP session, or several in a multi-session workload. See [architecture](./architecture.md#overview).

## Workload configuration

The immutable record avirthostd keeps for a workload: its namespace, IDs, alias, labels, image and workspace image digests, configuration layer digest, policy reference and credential binding references. It holds references and never a secret value. Status: planned. See [host services](./components.md#host-services).

## Workload container

The container, its own VM on the Containerization runtime, in which avirtinitd and the agent or another main workload process run, with no network interface other than loopback. Its only way out is the workload's HTTP/2 connection to its mediator, which in the implemented system carries only log envelopes, on `mediator.sock`. See [workload container](./components.md#workload-container) and [connection to the mediator](./workload-topology.md#connection-to-the-mediator).

## Workload ID

The 8 random hexadecimal characters that name a workload in the implemented system and name its log files, `logs/<workload-id>.*`. In the planned design a workload has a canonical ID and a short ID instead. See [workload log](./observability.md#workload-log).

## Workload image

Any image a workload container boots from: in the implemented system `avirt-workload-claude:dev` or `avirt-workload-codex:dev`, and in the planned design the avirt base and every image derived from it. See [workload images](./build-and-platforms.md#workload-images).

## Workload log

The durable, sequence-numbered NDJSON record of one workload at `logs/<workload-id>.ndjson`, written only by avirthostd: every JSON-RPC line, every meta event and every egress event, ending with `container_exit`. It is distinct from the workload stdio log. See [log files](./hostd-api.md#log-files).

## Workload management

The creation, listing, inspection, stopping and deletion of workloads by another workload through avirtguest, under policy and within the workload management constraints that keep a workload from gaining more authority than it holds. Status: planned. See [workload management](./components.md#workload-management).

## Workload policy snapshot

The policy snapshot scoped to one workload, which that workload's avirtmedproxyd evaluates. Status: planned. See [building snapshots](./egress-policies.md#building).

## Workload stdio log

The host-written NDJSON file `logs/<workload-id>.stdio.ndjson` of a workload container's stdout, stderr and console, which avirtlogd tails. Status: planned. See [workload stdio logs](./components.md#workload-stdio-logs).

## Workload SVID

The SVID `spiffe://<td>/ns/<ns>/workload/<uuid>`, held only by avirtnetd, which authenticates the workload container on its HTTP/2 connection to its mediator. Status: planned. See [trust domain and IDs](./identity.md#trust-domain-and-ids).

## Workspace

The content a workload container starts with beyond its harness base: a source tree, its dependencies, caches and individual files. It is unrelated to the `/workspace` directory as such and to a Cargo workspace. Status: planned. See [workspaces](./workspaces.md#status-planned).

## Workspace image

An OCI artifact that carries a workspace as a stack of layers, which avirthostd composes onto a harness base image when it creates a workload container. Status: planned. See [workspace image format](./workspaces.md#workspace-image-format).

## Workspace reference

The grammar that names one workspace image, or a rule that selects one: a registry repository, `oci:`, `github:`, `git:` or `dir:`. Status: planned. See [workspace references](./workspaces.md#workspace-references).

## Workspace root

A directory that a workspace image declares in its `roots`, `/workspace` and each directory destination under `/home/agent`, which avirtguestd serves and snapshots. Status: planned. See [workspace image format](./workspaces.md#workspace-image-format).

## Workspace snapshot

A copy of a workspace root that avirtguestd makes while the agent cgroup is frozen for the `workspace-snapshot` freeze reason. It is unrelated to a policy snapshot. Status: planned. See [workload container](./components.md#workload-container).

## Writable layer

Layer 5 of the layer stack, the per-workload writable layer that receives every write made inside the workload container and is deleted with it. Status: planned. See [layer stack](./workload-lifecycle.md#layer-stack).

## `x-avirt-caller`

The header that carries the originating principal's identity on a stream avirtedgeproxyd's Envoy originates, in the role of Envoy's `x-forwarded-client-cert`. Only avirtedgeproxyd, avirtmedproxyd for `guest-cli`, avirtapid and the gateways set it, and a missing, repeated or unparseable one is refused. Status: planned. See [avirtedgeproxyd](./components.md#avirtedgeproxyd).

## xDS

Envoy's family of discovery APIs for listeners, clusters, routes and secrets, which avirtmedproxyd and avirtedgeproxyd serve to their Envoys over local sockets. Status: planned. See [mediator container](./components.md#mediator-container).

## zot

The OCI registry that avirtregistryd runs as a supervised child process, reachable only by avirtregistryd over mutual TLS on loopback. Status: planned. See [avirtregistryd](./workspace-registry.md#avirtregistryd).
