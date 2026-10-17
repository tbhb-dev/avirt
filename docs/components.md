---
type: Architecture Overview
title: Components
description: The avirt component inventory across the host, the mediator container, the workload container and the all-in-one server image, with where each component runs, what it owns, what it talks to, its startup and failure semantics, its status, and how the current binaries map onto it.
tags: [avirt, architecture, components, inventory]
implementation: partial
generated: { by: claude-code/claude-opus-5-5, at: 2026-10-17T19:00:00Z }
---

# Overview

`avirt`'s components fall into four placements. avirthostd and the host services that configure and decide make up the control plane, and avirtedgeproxyd and avirtmedproxyd with their Envoys are the data plane. Each workload has a workload container and a mediator container. The all-in-one server image runs the host services in one container as a deployment mode. The Container column names the container a component lives in. Host-side components run directly on the host or together in the all-in-one server image.

A **workload** is the lifecycle unit, like a pod with a sidecar: the **workload container**, which runs avirtinitd and the agent or another main workload process, plus its **mediator**, the per-workload mediator container that runs avirtmediatord, avirtmedproxyd and avirtmedproxyd's Envoy. Each runs in its own container, which is its own VM on the Containerization runtime and a container on a shared kernel on Docker Desktop (see [compute runtimes](./compute-runtimes.md)).

| Component | Description | Container | Status |
| --- | --- | --- | --- |
| avirt | The CLI and TUI client (see [operator CLI](./operator-cli.md)) | host | Partial |
| avirtadmin | The local admin CLI, which talks directly to avirthostd over a local Unix socket (see [admin CLI](./admin-cli.md)) | host | Planned |
| avirtwebd | The backend for the avirt web UI | host | Planned |
| avirtedgeproxyd | The edge proxy: it supervises the host's edge Envoy Proxy with libavirtenvoy and serves it local xDS and SDS. Through that Envoy it is the global policy enforcement point for workload traffic: it publishes services, carries every workload-to-workload connection on this host and between hosts, fronts the host services for local clients and peers, and is the only host service that opens connections off the host | host | Planned |
| avirthostd | Owns workload lifecycle, namespaces, workspaces and immutable workload configuration for a single host, and issues SPIFFE identities | host | Partial |
| avirtapid | The API server: a REST and gRPC API in front of the local avirthostd, peers' avirtapid and services, reached through avirtedgeproxyd | host | Planned |
| [avirtdnsd](./dns.md) | Makes local and peered workloads and services addressable by name, resolving each name to the edge proxy of the host that serves it | host | Planned |
| avirtmcpgwd | The MCP gateway, where MCP upstream credentials are injected | host | Planned |
| avirtmodelgwd | The model gateway, where model credentials are injected | host | Planned |
| avirtregistryd | The host's OCI registry for workspace images: it runs zot as a supervised child process and adds avirt's own secondary index and search API (see [workspaces](./workspace-registry.md#avirtregistryd)) | host | Planned |
| avirtbuilderd | Builds repository OCI artifacts from version control in disposable sandboxed jobs, so that workloads start quickly | host | Planned |
| avirtpolicyd | Stores, validates, proves, compiles and distributes versioned Cedar policy snapshots for the host | host | Planned |
| [avirtsecretsd](./secrets.md) | Holds secrets from providers, owns credential refresh, and serves the workload-scoped credential broker API and an Envoy secret discovery service (SDS) for mediator-injected bindings | host | Planned |
| avirtlogd | Acquires workload container logs, transforms them, and sends them to avirtoteld for delivery as OpenTelemetry Logs (see [observability](./observability.md#workload-stdout-and-stderr)) | host | Planned |
| avirtauditd | Audit logging: it reads the workload logs and every component's audit log and delivers them as OCSF (see [observability](./observability.md#audit)) | host | Planned |
| avirtoteld | The OpenTelemetry Collector for the host: a Rust front end supervising a pinned Collector built with the OpenTelemetry Collector Builder (see [observability](./observability.md#avirtoteld)) | host | Planned |
| avirtinitd | One minimal supervisor with three role modes: PID 1 and launcher in the workload container, and supervisor in the mediator container and in the all-in-one server image | workload, mediator, all-in-one | Partial |
| avirtmediatord | The mediator's control process: the mediator's connection to avirthostd, the `/control` stream, readiness, generations, lifecycle and identity rotation for itself and avirtmedproxyd's Envoy | mediator | Partial |
| avirtmedproxyd | The mediator proxy: it supervises the mediator's Envoy Proxy with libavirtenvoy and serves it local xDS and SDS | mediator | Planned |
| Envoy Proxy | The data plane: avirtmedproxyd's Envoy carries all of the workload's ingress and egress, and avirtedgeproxyd's Envoy carries the host's | mediator, host | Planned |
| libavirtenvoy | An Envoy dynamic module that evaluates Cedar policy (embedding the `cedar-policy` crate) and injects HTTP and gRPC credentials | mediator, host | Planned |
| avirtacp | Runs an agent workload through the Agent Client Protocol (see [ACP](./acp.md)) | workload | Partial |
| avirtagent | Runs an agent workload through its native harness | workload | Planned |
| avirtexec | Generic execution | workload | Planned |
| avirtnetd | Guest networking: the client end of the workload's HTTP/2 connection to its mediator and holder of the workload SVID, the forward proxy, the DNS stub resolver, the reverse proxy and the guest control gRPC API | workload | Partial |
| avirtguestd | Workload services: a file server and an MCP server | workload | Planned |
| avirtmcp | A stdio MCP server, launched by the agent, that relays to avirtmcpgwd | workload | Planned |
| avirtguest | The guest CLI in workload images, which reaches avirtapid through avirtnetd, avirtmedproxyd and avirtedgeproxyd (see [guest CLI](./guest-cli.md)) | workload | Planned |

Partial means the component's function exists in the current code but not yet as the component described here. The next section says where each partial function lives.

# Status: implemented

The current code has four binaries and the shared `avirt-core` library, and they carry the implemented parts of the inventory as follows.

| Binary | Container | Carries |
| --- | --- | --- |
| `avirt` | host | The CLI. It talks to avirthostd directly over HTTP on `127.0.0.1:8787` (`POST /launch`), or the URL in `AVIRT_HOSTD_URL`. |
| `avirthostd` | host | The host daemon: the control API, workload launch and teardown, the Codex `auth.json` seed and write-back, and the workload logs in `logs/<workload-id>.ndjson`. |
| `avirtmediatord` | host, one process per workload | The mediator as a host process: one Pingora server with the CONNECT proxy on `proxy.sock`, and on `uplink.sock` a TCP proxy that forwards avirtinitd's HTTP/2 connection to avirthostd with the bytes unchanged. |
| `avirtinitd` | workload, as root | The workload container's root process and image entrypoint. Together with the harness in `avirt-core`, it contains the ACP client that avirtacp carries in the planned design (it spawns the adapter as uid 1000 under seccomp and taps every JSON-RPC line, see [ACP](./acp.md)), and the TCP proxy to `proxy.sock` that avirtnetd's forward proxy replaces in the planned design. |

`avirtinitd` is not PID 1 in the current code. The workload container runs with `--init`, so the runtime's init is PID 1 and avirtinitd is its direct child (see [workload hardening](./workload-hardening.md)). There is no mediator container: the mediator is the host process above. Landlock is not applied by the current code. None of avirtedgeproxyd, avirtmedproxyd, the Envoys, libavirtenvoy or the other host services exist.

# Status: planned

## Host services

Each host service owns the state in its Owns column, and no other component writes it. avirthostd passes references to secrets and policies (binding ids and policy ids) in workload configuration, never secret values.

| Component | Owns | Talks to |
| --- | --- | --- |
| avirt | Nothing. It is the CLI and TUI client (see [operator CLI](./operator-cli.md)). | avirtapid over gRPC, through avirtedgeproxyd's local client listener |
| avirtadmin | Nothing. It is the local admin CLI (see [admin CLI](./admin-cli.md)). | avirthostd directly, over avirthostd's local admin Unix socket, where avirthostd authenticates it by peer credentials (see [trust boundaries](./trust-boundaries.md#listeners)). It does not go through avirtapid or avirtedgeproxyd. |
| avirtwebd | Web UI user authentication and sessions, and the web UI's static assets, bundled into its binary (see [web UI](#web-ui)). It acts for web UI users the way avirt acts for its user. | avirtapid over REST, through avirtedgeproxyd. Web UI users reach it, and its static assets, through avirtedgeproxyd. |
| avirtedgeproxyd | Its Envoy's configuration, served to the Envoy over local xDS and SDS: listeners, clusters, its SVID and key, the SPIFFE bundle and the active peers' bundles (see [peering](./peering.md#trust-bundles)). Through its Envoy, policy enforcement for every workload-to-workload connection on this host and between hosts, the published services, connections to authorized host-local services, and the host's off-host connections. It also fronts avirthostd, avirtapid, avirtmcpgwd, avirtmodelgwd and avirtwebd for local clients and peers, and carries every off-host connection the host services make. | Its Envoy (local xDS and SDS), avirthostd (its SVID from the issuer, service records and enrollment records), avirt, web UI users, avirtmedproxyd's Envoys (through avirthostd's SNI routing, in both directions), the host services it fronts, authorized host-local services, peers' avirtedgeproxyd, external endpoints, avirtpolicyd, avirtdnsd |
| avirthostd | Workload lifecycle (launch, generations, teardown, the startup cleanup), namespaces, immutable workload configuration (namespace, workload short ID and alias, labels, image digest, resolved workspace image digest, configuration layer digest, policy reference, credential binding references), port publications and service records as versioned state, peer enrollment records (see [peering](./peering.md#enrollment-record)), the entity revisions it sends to avirtpolicyd, the SPIFFE issuer, each workload's MITM CA, the durable workload log, the workload stdio log files on the Containerization runtime, SNI routing, resource admission, the content-addressed cache, and each launch's configuration layer (see [workload lifecycle](./workload-lifecycle.md#configuration-layer)) | The compute runtime, each mediator's connection to avirthostd, avirtpolicyd (entity revisions and policy writes), avirtsecretsd (to validate the references it stores, and its Admin API), avirtauditd (delivery status), avirtedgeproxyd (its SVID, service records and enrollment records), avirtapid (inbound, through avirtedgeproxyd), avirtadmin (inbound, on its local admin Unix socket), avirtbuilderd |
| avirtapid | Nothing durable. It forwards REST and gRPC calls to the local avirthostd, to peers' avirtapid through avirtedgeproxyd, and to services through avirtedgeproxyd. It forwards the caller's authenticated identity and rejects caller-supplied identity headers. Workloads use it to manage other workloads (see [workload management](#workload-management)). | avirt and avirtguest over gRPC and avirtwebd over REST, all through avirtedgeproxyd, avirthostd, avirtbuilderd, avirtregistryd, peers' avirtapid |
| avirtdnsd | The avirt DNS zone, which resolves each avirt name to the edge proxy of the host that serves it (see [DNS](./dns.md#the-avirtdnsd-zone)). | avirtmedproxyd's Envoys (through SNI routing), avirtedgeproxyd, avirthostd (namespaces, workloads, services and active peers) |
| avirtmcpgwd | MCP sessions. It injects MCP upstream credentials that it obtains from avirtsecretsd, scopes tool discovery and invocation by workload, and never retries a mutating tool call automatically. | avirtmedproxyd's Envoys (through SNI routing), avirtsecretsd, MCP upstreams through avirtedgeproxyd |
| avirtmodelgwd | Model routing, per-workload model budgets and the Codex refresh endpoint. It injects model credentials that it obtains from avirtsecretsd. | avirtmedproxyd's Envoys (through SNI routing), avirtsecretsd, upstream model APIs through avirtedgeproxyd |
| avirtregistryd | Workspace images and their registry storage, in zot under avirtregistryd's own state directory, with zot's `sync`, `ui`, `userprefs`, `events` and `metrics` extensions off and its listener on loopback, accepting only mTLS from a private CA avirtregistryd holds in memory. Its secondary index is its own SQLite database, updated with every write and reconciled against zot's catalog. It makes every off-host registry connection. | avirtbuilderd, avirthostd, avirtapid and build jobs' avirtmedproxyd Envoys (its `WorkspaceRegistry` API), zot, avirtsecretsd (registry credentials), off-host registries and peers' registries through avirtedgeproxyd |
| avirtbuilderd | Build scheduling. It watches version control such as git and runs each build in a disposable sandboxed job with no host control or signing authority. | Repositories through avirtedgeproxyd, avirtregistryd |
| avirtpolicyd | Cedar policy storage, schema validation, the Cedar policy prover, compilation, and versioned snapshot distribution (see [egress policies](./egress-policies.md)). The prover statically analyses every policy set before a snapshot built from it is distributed, and reasons precisely about entity-hierarchy membership such as `in Namespace::"team-a"`. It rejects any policy that applies `like` to anything other than a canonicalized external host or a canonicalized request path. Each snapshot pairs a policy version with an entity revision from avirthostd. | libavirtenvoy in every avirtmedproxyd's Envoy and in avirtedgeproxyd's Envoy, avirthostd (entity revisions and policy writes in, host policy snapshot out) |
| avirtsecretsd | Provider access, every real credential, the Codex refresh token, credential refresh with one owner per credential, placeholder issuance, the workload-scoped credential broker API, and SDS for the `binding/<id>` secrets of mediator-injected bindings (see [credentials](./egress-credentials.md) and [secrets](./secrets.md#sds)) | Secret providers through their provider plugins (off-host ones through avirtedgeproxyd), avirtmodelgwd, avirtmcpgwd, avirtmediatord and avirtmedproxyd's Envoys (broker API), avirtmedproxyd's Envoys (SDS), avirthostd (workload and binding registrations, and the Admin API) |
| avirtlogd | Workload stdio log delivery. It acquires the workload containers' stdout, stderr and console logs with a runtime-adapted method, transforms them, and sends them to avirtoteld for delivery as OpenTelemetry Logs (see [observability](./observability.md#workload-stdout-and-stderr)). | The host-written log files it tails, avirtoteld |
| avirtauditd | Audit delivery as OCSF. It reads avirthostd's committed workload logs and every component's audit log by offset, and keeps the raw security events beside the OCSF form (see [observability](./observability.md#audit-logs)). | The workload logs and audit logs, avirthostd (delivery status), an OCSF sink through avirtedgeproxyd |
| avirtoteld | Telemetry delivery: a Rust front end supervising a pinned OpenTelemetry Collector built with the OpenTelemetry Collector Builder (see [observability](./observability.md#avirtoteld)) | avirtmedproxyd's Envoys (through SNI routing), avirtmediatord, avirtlogd, the host services, avirtedgeproxyd, downstream telemetry backends through avirtedgeproxyd |

avirthostd's sequence-numbered workload log (see [workload topology](./workload-topology.md#resuming-after-reconnect)) is the durable record. avirtauditd consumes it and the components' audit logs asynchronously, and avirtlogd consumes the workload stdio log files asynchronously, so telemetry or OCSF backpressure delays only them and never stalls a workload (see [observability](./observability.md)). Every event carries the provenance labels defined in [trust boundaries](./trust-boundaries.md#event-provenance).

## Namespaces and labels

A **namespace** is a grouping of workloads and a trust and access boundary for policy, and nothing more.

- Every workload is in exactly one namespace, fixed when the workload is created. The default is `default`.
- Creating a workload in a namespace requires authorization for that namespace. avirthostd evaluates it against the host's Cedar policy snapshot.
- A namespace scopes names: service names and their aliases are unique within their namespace. A workload is named by its short ID, which is unique on the host.
- Workloads can manage other workloads, under policy (see [workload management](#workload-management)).
- A namespace name is a lowercase DNS label of at most 63 characters (see [avirt names](#avirt-names)).
- A namespace is local to its host. A namespace with the same name on another host is a different namespace, and anything on another host is addressed fully qualified: by its peered name, by an identity in that host's trust domain, and by a Cedar entity qualified by that host.

A **label** is a free-form key and value on a workload or a service, used for selection and grouping, for example `project`, `repo` or `clone-of`. Policy can use labels as attributes, but a label is not a trust boundary by itself. Setting a label key is itself authorized: keys under the reserved prefix `avirt.internal/` are set only by avirthostd, and the host policy can reserve further prefixes for the principals it names.

In Cedar, a `Workload` is `in` its `Namespace`, so policies can say `principal in Namespace::"team-a"`, and labels are entity tags (see [egress policies](./egress-policies.md#tags)). In SPIFFE, the namespace is part of every workload identity (see [identity](./identity.md#trust-domain-and-ids)).

## Workload IDs

Every workload has a canonical ID, a UUIDv7 that avirthostd generates when it creates the workload. It is never reused. SPIFFE IDs carry it (`/ns/<ns>/workload/<uuid>/…`, see [identity](./identity.md#trust-domain-and-ids)), and so do Cedar entities (`Workload::"<ns>/<uuid>"`) and the `workload` field of every log record.

The workload's short ID is the last 8 hexadecimal characters of the UUIDv7, for example `7e759259` for `0192f3a4-5b6c-7d8e-9f01-23457e759259`. Constraint: the leading bits of a UUIDv7 are a millisecond timestamp, so the short ID is always taken from its trailing random bits, never from its prefix. The short ID is only a DNS handle: authorization never uses it.

The short ID is unique on the host among live workloads and among ended workloads whose names are still within their DNS TTL. When a new UUIDv7's short ID clashes with one of them, avirthostd discards that UUIDv7 and generates another, up to 8 times. If every attempt clashes, it refuses the launch with 503 and records the clash.

## avirt names

Every workload and service has local and peered avirt names under `host.avirt.internal` or an active peer's host name, which is the peer's FQDN (see [peering](./peering.md#peer-host-names)), and avirtdnsd is the only component that answers them. The name forms, label syntax, length caps and the rejection rule are in [DNS](./dns.md#avirt-names).

## Workload management

A workload can manage other workloads: create, list, inspect, stop and delete them. The agent runs avirtguest (see [guest CLI](./guest-cli.md)), which reaches avirtapid through avirtnetd, avirtmedproxyd and avirtedgeproxyd. The calling workload is the principal, as `Workload::"<ns>/<uuid>"` with `via: "guest-cli"` in the context, and avirthostd authorizes every call per action and per resource (namespace, labels, credential bindings, policy reference) against the host policy snapshot. Nothing is permitted by default.

These constraints bound what a workload can do through this path, whatever the policy permits:

- **No greater authority.** A workload can create a workload only in a namespace where it may itself create workloads, only with credential bindings it holds itself, and only from a workspace image it may itself launch (`launch_workspace`, see [workspaces](./workspaces.md#policy)). The child's policy reference is the creator's own, or one the host policy names as assignable by that creator. The child's resource requests are charged to the creator's namespace budget.
- **Reserved labels.** A workload never sets a label under `avirt.internal/`. avirthostd sets them on every workload it creates.
- **Delegation chain.** avirthostd records each workload's creator in `avirt.internal/creator` (the creator's UUIDv7, or the local or web UI user) and its root in `avirt.internal/root`, the user at the top of the chain. The delegation chain depth is capped by host configuration, and a create past the cap is refused.
- **Lifetime.** A child ends when its creator ends, unless the policy permits a detached child.
- **No self-escalation.** A workload cannot change its own namespace, labels, bindings or policy reference, which are immutable, and cannot publish ports on a workload it did not create unless the policy permits it.
- **Host scope.** A workload manages workloads on its own host. Managing a peer's workloads goes through that peer's avirtapid, under the peer's policy for the caller's fully qualified identity (see [peering](./peering.md#workload-management-across-hosts)).
- **No host administration.** Namespace creation, peer enrollment, policy writes and other host administration are not available to workload principals.
- **Audit.** Every management call is an event with the caller, the action, the resource and the delegation chain.

## avirtedgeproxyd

avirtedgeproxyd is the host's edge proxy. It supervises an Envoy Proxy with libavirtenvoy loaded, the way avirtregistryd supervises zot, and serves that Envoy local xDS and SDS: its listeners and clusters, its `/service/avirtedgeproxyd` SVID and key from avirthostd's issuer, the SPIFFE bundle and the active peers' bundles. Through that Envoy it is the single global policy enforcement point for workload traffic, and the only host service that opens connections off the host.

- **Workload to workload.** Every connection from one workload to another passes through avirtedgeproxyd, on the same host and between hosts: avirtmedproxyd, avirtedgeproxyd, the destination avirtmedproxyd, the destination workload container. Between hosts it passes both hosts' avirtedgeproxyd, and each evaluates its own host policy snapshot (see [peering](./peering.md#workload-traffic)).
- **Services.** Services that workloads expose are published at avirtedgeproxyd, from avirthostd's service records. A record has a name, a namespace, labels, a port, and a backend: a port that a workload publishes, or an authorized host-local service.
- **Host-local services.** A service running on the host itself outside avirt, such as one bound to the host's `127.0.0.1`, is reachable only as the backend of a service record. The private-address block refuses every other host address.
- **External endpoints.** avirtmedproxyd's Envoys reach the egress listener through avirthostd's SNI routing (see [workload topology](./workload-topology.md#sni-routing)) for connections to external endpoints, which avirtedgeproxyd's Envoy serves as a dynamic forward proxy.
- **Clients.** On the local client listener it fronts avirthostd, avirtapid, avirtmcpgwd, avirtmodelgwd and avirtwebd: avirt reaches avirtapid over gRPC, avirtwebd reaches it over REST, and web UI users reach avirtwebd. On the peer listener, which binds the addresses in `peering.listen` (default port 8788) and does not exist while that list is empty (see [peering](./peering.md#peer-listener)), it serves active peers only local published ports, services, the SPIFFE bundle endpoint and avirtapid, never avirthostd, avirtmcpgwd or avirtmodelgwd.
- **avirtguest.** avirtguest reaches avirtapid over gRPC from inside the workload container: avirtnetd, avirtmedproxyd, avirtedgeproxyd, avirtapid. avirtedgeproxyd authorizes its `guest-cli` principal against the host policy snapshot as it does other workload traffic.
- **Host services.** Every off-host connection a host service makes goes through it: avirtmodelgwd and avirtmcpgwd calling upstreams, avirtsecretsd calling remote secret providers, avirtoteld and avirtauditd exporting, avirtbuilderd fetching sources, avirtregistryd importing from peers, and avirtapid calling peers.

Traffic from avirtmedproxyd's Envoy to avirt's own host services (avirtsecretsd, avirtoteld, avirtmcpgwd, avirtmodelgwd, avirtdnsd and avirtpolicyd, and avirtregistryd for build jobs only) does not pass through avirtedgeproxyd. It goes directly to each component through avirthostd's SNI routing, with mutual TLS and X.509-SVIDs on both sides.

libavirtenvoy in avirtedgeproxyd's Envoy authorizes each caller by its SVID, resolves avirt names to canonical entities and denies names that resolve to none, evaluates the host's Cedar policy snapshot, resolves external names, applies the private-address block to every resolved address (see [egress](./egress.md#edge-proxy-egress)), and records one event per connection. To reach a destination mediator it opens its own mutual TLS connection as `/service/avirtedgeproxyd` to that mediator's ingress listener, and carries the authenticated source identity in `x-avirt-caller`, which it sets after removing any the request carried. It injects no credentials, and the bytes it carries between workloads and inside the TLS connections that avirtmedproxyd's Envoy opens to the upstream stay opaque to it. `x-avirt-caller` is a header on the outer h2 CONNECT stream avirtedgeproxyd's Envoy originates, never part of the tunnelled bytes. It appears exactly once, and a missing, repeated or unparseable one is refused. When avirtapid, avirtregistryd, avirtmodelgwd or avirtmcpgwd call through avirtedgeproxyd on behalf of a caller, they carry that caller's identity in `x-avirt-caller`. avirtedgeproxyd accepts it only from those four services and only for a caller the service is serving (for a gateway, the workload and generation of the `/mediator/<gen>/medproxy` SVID on its inbound connection; for avirtregistryd, a peer import), and it authorizes both the service and the originating principal.

```mermaid
flowchart LR
    wl["workload container (avirtnetd)"] -->|"HTTP/2 connection to its mediator"| me["avirtmedproxyd's Envoy"]
    me -->|"SNI routing, direct"| hs["avirtsecretsd, avirtpolicyd, avirtdnsd, avirtoteld, avirtmcpgwd, avirtmodelgwd"]
    me -->|"SNI routing"| fd["avirtedgeproxyd's Envoy (PEP)"]
    fd -->|"gRPC, guest-cli"| apid["avirtapid"]
    fd -->|"ingress listener"| me2["destination avirtmedproxyd's Envoy (same host)"]
    me2 --> wl2["destination workload container"]
    fd -->|"service backend"| hl["authorized host-local service"]
    fd -->|"dynamic forward proxy"| net(("external endpoints"))
    fd -->|"mTLS, federated"| pfd["peer avirtedgeproxyd's Envoy (PEP)"]
    pfd --> pme["peer avirtmedproxyd's Envoy"]
    hs -->|"off-host calls"| fd
```

## Policy and credential placement

| Point | Policy | Credentials |
| --- | --- | --- |
| avirtmedproxyd's Envoy | The workload's Cedar policy snapshot (`resolve`, `connect`, `intercept`, `passthrough`, `request` and `ingress`), after resolving avirt names to entities, plus the non-policy checks: authority normalization, `Host` matching, the placeholder misuse check and the Codex refresh intercept | Only narrow, short-lived, workload-scoped credentials for bindings that must be injected at the mediator |
| avirtedgeproxyd's Envoy | The host's Cedar policy snapshot, after resolving avirt names to entities: caller authorization, workload-to-workload connections on this host and between hosts, services, host-local services, external endpoints, the private-address block and peer routing | None |
| avirtmodelgwd | Its per-workload model rules and budgets | Model credentials, obtained per request from avirtsecretsd |
| avirtmcpgwd | Its per-workload tool scope | MCP upstream credentials, obtained from avirtsecretsd |
| avirthostd | The host policy snapshot, for control-plane actions: `create_workload`, `list_workloads`, `inspect_workload`, `stop_workload`, `delete_workload`, `set_label`, `attach_binding`, `publish_port`, `publish_service` and `launch_workspace` | None |

## Mediator container

Each workload has one mediator container, a sidecar to its workload container. The mediator has no network interface other than loopback. Its only connections are the four Unix sockets the compute runtime provides, described in [workload topology](./workload-topology.md#runtime-provided-sockets): the workload's HTTP/2 connection to its mediator on `/run/avirt/workload.sock`, the mediator's connection to avirthostd, the mediator's outbound socket to avirthostd, and avirtmedproxyd's ingress listener.

| Component | What it does | Talks to |
| --- | --- | --- |
| avirtinitd (mediator mode) | Starts avirtmediatord and then avirtmedproxyd, each under its own UID and with its own Landlock ruleset (see [sandbox](./sandbox.md#mediator)). Exits as soon as either child exits. | Its children |
| avirtmediatord | The mediator's control process. It holds the mediator's connection to avirthostd, fetches the workload's configuration, generates its own key and the key of avirtmedproxyd's Envoy, rotates both SVIDs, hands avirtmedproxyd the Envoy's SVID and key and the configuration for its listeners, receives libavirtenvoy's readiness reports on a local status socket, runs the `/control` stream and launch handshake, and reports readiness. It proxies no traffic. | avirthostd (the mediator's connection to avirthostd), avirtsecretsd (placeholders, through SNI routing), avirtmedproxyd (the Envoy's SVID and configuration), libavirtenvoy (status socket), avirtnetd (the `/control` stream, routed to it by avirtmedproxyd's Envoy) |
| avirtmedproxyd | The mediator proxy. It starts its Envoy under its own UID, applying the Envoy's Landlock ruleset as a nested domain, serves it listeners, clusters, its SVID and key and the trust bundle over local xDS and SDS on `/run/avirt/xds.sock`, and exits when its Envoy exits. | avirtmediatord, its Envoy (local xDS and SDS) |
| Envoy Proxy | avirtmedproxyd's Envoy, the data plane. It terminates the workload's HTTP/2 connection to its mediator, serves the agent's CONNECT streams and DNS queries, and sends each connection to a host service or to avirtedgeproxyd. It accepts avirtedgeproxyd's connections to published ports on its ingress listener. | avirtnetd, avirthostd (the `/envelopes` stream and events), the host services, avirtedgeproxyd |
| libavirtenvoy | Evaluates the workload's Cedar policy snapshot locally, normalizes authorities, resolves avirt names to entities, applies DNS policy, mints leaf certificates from the workload's MITM CA, runs the placeholder misuse check, injects mediator-bound credentials, routes the Codex refresh intercept, and writes events | avirtpolicyd, avirtsecretsd (broker API), avirtdnsd |

```mermaid
flowchart TD
    init["avirtinitd (mediator mode)"]
    med["avirtmediatord (uid avirt-med)"]
    mp["avirtmedproxyd (uid avirt-envoy)"]
    envoy["Envoy Proxy (uid avirt-envoy, nested ruleset)"]
    lib["libavirtenvoy (dynamic module)"]
    init -->|"1. start, wait for ready"| med
    init -->|"2. start"| mp
    mp -->|"start"| envoy
    med -.->|"Envoy SVID and configuration"| mp
    mp -.->|"local xDS and SDS"| envoy
    envoy --- lib
```

## Workload container

The workload container runs the agent or another main workload process. `avirtinitd` is PID 1 and the launcher: it starts every other process with that process's own UID, capabilities, seccomp filter and Landlock ruleset, and none of them inherits avirtinitd's (see [workload hardening](./workload-hardening.md#security-contexts) and [sandbox](./sandbox.md#per-process-rulesets)). It runs exactly one main workload process (avirtacp, avirtagent or avirtexec), alongside avirtnetd and avirtguestd.

Every workload container's root filesystem is a harness base image derived from the avirt base image, then the workspace layers, the configuration layer and the per-workload writable layer (see [workload lifecycle](./workload-lifecycle.md)). The avirt base carries every workload binary in the table below, and the harness base adds only the harness and its adapter under `/opt/agent`.

| Process | UID | What it does | Talks to |
| --- | --- | --- | --- |
| avirtinitd | 0 | PID 1, the launcher, lifecycle and freeze authority. It holds no SVID. Root is required to start children under other UIDs, create the agent cgroup, write the system CA bundle and `/etc/resolv.conf`, bind the DNS stub resolver's port 53 sockets and the forward proxy's `127.0.0.1` listener for avirtnetd, and freeze and kill the agent cgroup. | Its children, over one launcher socket per role, `/run/avirt/init/<role>.sock` |
| avirtnetd | `avirt-net` | The client end of the workload's HTTP/2 connection to its mediator: TLS client and h2 client on `/run/avirt/mediator.sock`, holding the workload SVID and key in memory. It owns the `/envelopes` stream (sequence numbers and the replay ring), the forward proxy on the listener avirtinitd hands it, the DNS stub resolver (see [egress](./egress.md#workload-dns)), the reverse proxy and the guest control gRPC API, and serves the local service sockets the other workload processes use. | avirtmedproxyd's Envoy, workload processes over loopback and Unix sockets |
| avirtacp | `avirt-run` | Runs an agent workload through the Agent Client Protocol and exposes a session control API through the mediator (see [ACP](./acp.md#session-control-api)). avirtinitd launches the adapter at its request and hands it the adapter's stdio pipes. It taps every JSON-RPC line and submits the envelopes to avirtnetd. It carries the ACP client that is part of avirtinitd in the current code. | The ACP adapter (stdio), avirtnetd, avirtinitd |
| avirtagent | `avirt-run` | Runs an agent workload through its native harness | The agent's harness, avirtnetd, avirtinitd |
| avirtexec | `avirt-run` | Generic execution | avirtnetd, avirtinitd |
| avirtguestd | `avirt-guest`, in the workspace group `avirt-ws` it shares with the agent | A file server for remote file management and workspace snapshots, and an MCP server with workspace and workspace snapshot tools, on `/run/avirt/streams/guestd.sock` only. It reaches the agent's files through the workspace group and its default ACLs (see [workload hardening](./workload-hardening.md#security-contexts)). | avirtnetd (ingress), avirtinitd (quiesce) |
| Agent | 1000 | The adapter and every tool it runs, in the agent cgroup | avirtnetd's forward proxy and agent sockets |
| avirtmcp | 1000 | A stdio MCP server that the agent launches on demand. It relays MCP to avirtmcpgwd through avirtnetd's MCP socket and the mediator. | The agent (stdio), avirtnetd |
| avirtguest | 1000 | The guest CLI, run on demand (see [guest CLI](./guest-cli.md)). It reaches avirtapid over gRPC through avirtnetd's guest CLI socket, avirtmedproxyd and avirtedgeproxyd, and avirtnetd forwards its requests under the principal `/ns/<ns>/workload/<uuid>/guest-cli` (see [identity](./identity.md#trust-domain-and-ids)). It manages other workloads under policy (see [workload management](#workload-management)). | avirtnetd |

avirtguestd's MCP tool surface:

- **Workspace tools.** List, stat, read, write and remove, on paths under the workload's declared workspace roots only. Paths are resolved beneath the root without following symlinks or magic links out of it, and writes are bounded by the workspace quota.
- **Workspace snapshot tools.** Create a workspace snapshot of a workspace root, and list workspace snapshots. A workspace snapshot quiesces the workspace by asking avirtinitd to freeze the agent cgroup for the duration of the copy.

The surface has no command execution and no path outside the workspace roots. avirtguestd listens only on `/run/avirt/streams/guestd.sock`, never on a loopback port, so the agent cannot reach it. Its file server and MCP server are reachable only through the `guestd` ingress target, which avirtnetd forwards to that socket with the caller's identity (see [workload topology](./workload-topology.md#ingress)). The agent's own MCP configuration names only avirtmcp.

```mermaid
flowchart TD
    init["avirtinitd (PID 1, root, launcher)"]
    net["avirtnetd (avirt-net)"]
    subgraph one["exactly one main workload process (avirt-run)"]
        acp["avirtacp"]
        agentr["avirtagent"]
        exec["avirtexec"]
    end
    guestd["avirtguestd (avirt-guest)"]
    subgraph cg["agent cgroup (uid 1000)"]
        adapter["agent adapter and tools"]
        mcp["avirtmcp (stdio, on demand)"]
        guest["avirtguest (CLI, on demand)"]
    end
    init -->|"1. start"| net
    init -->|"2. start after confirm"| one
    init -->|"2. start after confirm"| guestd
    init -->|"3. launch at the runner's request"| cg
    adapter -.-> mcp
    adapter -.-> guest
```

## Startup and failure semantics

### Workload

1. avirtinitd creates `/run/avirt` and the agent cgroup, reads the bootstrap SVID, unlinks it, binds the forward proxy's listener on `127.0.0.1`, and starts avirtnetd with the bootstrap SVID and key and that listener on inherited file descriptors.
2. avirtnetd opens the workload's HTTP/2 connection to its mediator, completes mutual TLS, attaches, takes part in the first rotation, and has its `/envelopes` stream accepted by avirthostd (see [workload topology](./workload-topology.md#launch-handshake)).
3. On `confirm`, avirtnetd passes the `confirm` contents to avirtinitd. avirtinitd installs the MITM CA, writes the placeholder files, and starts the main workload process and avirtguestd.
4. The main workload process asks avirtinitd to launch the agent. avirtinitd launches it as uid 1000 in the agent cgroup, with the placeholder environment and its security context, and avirtnetd reports `started`.

avirtinitd keeps a set of freeze reasons (`connection` and `workspace-snapshot`) and freezes the agent cgroup while the set is non-empty. It tells the main workload process when the cgroup freezes and thaws, and the main workload process suspends its adapter deadlines while it is frozen.

No agent process exists before the workload's connection to its mediator, the workload identity, the MITM CA, the placeholders, the workload's policy snapshot in avirtmedproxyd's Envoy and the accepted `/envelopes` stream are all in place.

| Event | Effect |
| --- | --- |
| The workload's connection to its mediator fails with no replacement connection | avirtinitd adds a `connection` freeze reason. If a connection to the same or a higher mediator generation is attached and confirmed within 30 s it removes the reason, otherwise it kills the cgroup and exits 2 with `mediator lost` (see [workload topology](./workload-topology.md#freeze-and-reconnect)). |
| avirtnetd exits | avirtinitd freezes the agent cgroup, then kills it and exits 2. The workload identity lives only in avirtnetd, so the workload ends. |
| The main workload process exits | avirtinitd lets avirtnetd drain the `/envelopes` stream, kills the agent cgroup and exits. |
| The agent exits | The main workload process ends the session. The agent is never restarted. |
| avirtguestd exits | avirtinitd restarts it at most twice with backoff. Its file and MCP requests fail meanwhile. |
| A security context cannot be applied | The process it was for does not start. For the agent, the workload ends before any agent process exists. |

### Mediator

1. avirtinitd starts avirtmediatord. It reads the bootstrap SVID, fetches `GET /workload`, generates its own key and the key of avirtmedproxyd's Envoy, completes the first rotation of both, and prepares the Envoy's configuration.
2. Once avirtmediatord reports ready, avirtinitd starts avirtmedproxyd, which takes the Envoy's SVID and configuration from avirtmediatord and starts its Envoy. The Envoy's bootstrap names only avirtmedproxyd's local xDS and SDS socket, so it serves nothing until avirtmedproxyd delivers its listeners and its SVID.
3. libavirtenvoy loads the workload's policy snapshot from avirtpolicyd and the workload's placeholders from avirtsecretsd, and reports each on the local status socket. avirthostd reports the accepted `/envelopes` stream to avirtmediatord with `{"t":"envelopes_accepted"}` on `GET /workload/control`. avirtmediatord sends `confirm` only once it has all three.

| Event | Effect |
| --- | --- |
| avirtmediatord or avirtmedproxyd exits (avirtmedproxyd exits when its Envoy exits) | avirtinitd stops the other, and the mediator container exits. avirthostd starts a new generation, at most twice per workload. |
| The mediator's connection to avirthostd is lost | avirtmediatord exits, with the effect above. |
| avirtpolicyd is unreachable | libavirtenvoy keeps evaluating its current policy snapshot until it expires, then denies (see [egress](./egress-policies.md#policy-failure-behavior)). |
| avirtsecretsd is unreachable | Requests that need a mediator-injected credential are refused. Model and MCP traffic depends only on the gateways. |

### All-in-one server image

avirtinitd in server mode starts the host services in this order, each under its own UID and each able to read only its own secrets. Each step waits for the previous step's readiness. avirtedgeproxyd, avirtregistryd and avirtoteld each supervise one child process of their own: avirtedgeproxyd its Envoy, avirtregistryd zot, and avirtoteld its OpenTelemetry Collector.

1. avirthostd
2. avirtsecretsd and avirtpolicyd
3. avirtedgeproxyd
4. avirtlogd, avirtauditd, avirtoteld and avirtdnsd
5. avirtregistryd, then avirtmcpgwd, avirtmodelgwd and avirtbuilderd
6. avirtapid
7. avirtwebd

Readiness is local: a daemon is ready once it holds its SVID and serves its listener, and no daemon's readiness waits on an off-host connection. avirtsecretsd is ready before it has fetched any remote secret, and avirtedgeproxyd's identity (from avirthostd's issuer) and host policy snapshot (from avirtpolicyd) are available before anything needs to leave the host. avirtedgeproxyd is ready once its Envoy serves its listeners, and its Envoy answers 503 on a route whose backend has not yet started.

If avirthostd exits, the deployment exits, every workload ends, and the next start sweeps them (see [lifecycle](./lifecycle.md)). Every other daemon is restarted with exponential backoff, at most 5 times in 10 minutes. A daemon that exhausts that budget is marked degraded and restarted with a capped slow backoff, and running workloads keep running. While an required host service (avirtsecretsd, avirtpolicyd, avirtedgeproxyd, avirtdnsd, avirtregistryd or avirtmodelgwd) is degraded, avirthostd refuses new launches with 503. avirtlogd, avirtauditd, avirtoteld, avirtbuilderd, avirtmcpgwd, avirtapid and avirtwebd are not essential: their callers see errors while they are down, and avirthostd's durable workload log keeps every record avirtauditd has not yet delivered. Shutdown runs in reverse order, and avirtedgeproxyd stops accepting first.

```mermaid
flowchart TD
    init["avirtinitd (server mode)"]
    hostd["1. avirthostd"]
    s2["2. avirtsecretsd, avirtpolicyd"]
    fd["3. avirtedgeproxyd"]
    fdenvoy["Envoy Proxy"]
    s4["4. avirtlogd, avirtauditd, avirtoteld, avirtdnsd"]
    col["OpenTelemetry Collector"]
    s5["5. avirtregistryd, avirtmcpgwd, avirtmodelgwd, avirtbuilderd"]
    zot["zot"]
    apid["6. avirtapid"]
    webd["7. avirtwebd"]
    init --> hostd
    hostd -->|"ready"| s2
    s2 -->|"ready"| fd
    fd -.->|"supervises"| fdenvoy
    fd -->|"ready"| s4
    s4 -.->|"avirtoteld supervises"| col
    s4 -->|"ready"| s5
    s5 -.->|"avirtregistryd supervises"| zot
    s5 -->|"ready"| apid
    apid -->|"ready"| webd
```

## Host request paths

avirtedgeproxyd fronts avirthostd, avirtapid, avirtmcpgwd, avirtmodelgwd and avirtwebd for clients and peers, and carries every workload-to-workload, service, host-local service and external connection. avirt, avirtwebd and avirtguest reach avirtapid through avirtedgeproxyd, and avirtadmin reaches avirthostd directly over its local admin socket. avirtmedproxyd's Envoy reaches avirt's own host services directly through avirthostd's SNI routing. Every connection off the host leaves from an avirtedgeproxyd. Each path, hop by hop, is in [networking](./networking.md#connection-types).

## Web UI

The web UI is a separate frontend app in the repository, a single-page app likely built with Vite, React and React Router. The same UI may also ship in a Tauri desktop app. avirtwebd bundles the built static assets into its binary and serves them through avirtedgeproxyd, and it is the web UI's only backend: it reaches avirtapid over REST through avirtedgeproxyd.

avirtwebd authenticates web UI users.

| Kind | Methods |
| --- | --- |
| Authentication | Username and password, magic code or link by email, WebAuthn, generic OAuth2, and generic OIDC |
| Multi-factor authentication | TOTP, magic code or link by email, and WebAuthn |

## Workload stdio logs

avirtlogd acquires the workload containers' stdout, stderr and console logs by tailing log files that the host side writes, and sends each record to avirtoteld, which delivers it as OpenTelemetry Logs. The method on each runtime, avirthostd's writer on the Containerization runtime and the identity attributes on each record are in [observability](./observability.md#workload-stdout-and-stderr).

## Operational ownership

Status: planned. These items have no phase yet (see [roadmap](./roadmap.md#unscheduled)).

- **Resource admission and accounting.** avirthostd reserves CPU, memory, disk, PIDs and egress bandwidth budgets for both containers of a workload before it creates either, accounts the shared services' cost against the host, and refuses a launch that does not fit with a 503.
- **Content-addressed cache.** avirthostd owns a content-addressed store for the runtime artifacts: kernels, initfs images, the unpacked avirt base, harness base and mediator images, and configuration layers. Workspace images live in avirtregistryd, and avirthostd pulls them by digest. It verifies every artifact's digest when the artifact is written and again when a workload is created from it, and garbage-collects artifacts that no workload configuration or retention rule references.
- **Builds.** avirtbuilderd schedules builds. Each build that processes untrusted content runs in a disposable build job, a workload that avirthostd creates with the build security context, with no access to avirtapid or avirthostd's control API and no signing keys. A job pushes only to its staging repository, and avirtbuilderd checks the output and promotes it (see [workspaces](./workspace-build.md#builds)).
- **Rollout and version compatibility.** Every protocol (the control messages on the workload's HTTP/2 connection to its mediator, the mediator's connection to avirthostd, the broker API, the policy snapshot format) carries a version, and each side refuses a version it does not support with a named error. An upgrade drains the host first: avirthostd refuses new launches, waits up to a deadline for running workloads to end, and then ends the rest.

# Open questions

1. **Per-service Envoys.** A single avirtedgeproxyd fronts avirthostd, avirtapid, avirtmcpgwd, avirtmodelgwd and avirtwebd, with separate listeners. Whether any of those services also has its own Envoy Proxy behind avirtedgeproxyd's Envoy is open.
2. **The session control API.** Its framing, and whether avirtagent and avirtexec expose the same one, are open in [ACP](./acp.md#session-control-api).
3. **Namespace creation.** Who may create a namespace, and what creating one grants its creator by default, is open.
4. **Namespace-scoped resources.** Workloads, workload and service names and workspace repositories are namespace-scoped, and secrets are host-scoped or namespace-scoped (see [secrets](./secrets.md#scoping)). Which other resources are (quotas, credential bindings, the right to publish services, policies) is open.
5. **Docker Desktop ingress listener.** On Docker Desktop, container-side listeners do not cross to the host, so avirthostd cannot open a connection to avirtmedproxyd's ingress listener in a mediator container. How inbound connections reach a mediator there is open.
6. **Docker Desktop socket ownership.** A file-mounted host socket appears in the container as `root:root 0660`, while avirtmediatord and avirtmedproxyd run under their own UIDs. How those UIDs are given access without root is open.
7. **Docker Desktop log files.** Docker's container log files live inside the Docker Desktop VM. How avirtlogd on the macOS host reaches them, by a file path the VM shares or through Docker's logs API, is open.
8. **Host service supervision outside the all-in-one image.** On a direct host install, which process starts and restarts the host services and hands each its one-time enrollment token is open.
9. **Web UI user accounts.** Where web UI user accounts and their credentials are stored, in avirtwebd, in avirthostd, or in a separate store, is open.
10. **Web UI user identity.** How a web UI user's identity is carried from avirtwebd to avirtapid, and how it maps to a Cedar principal, is open.
11. **Web UI sessions.** Session handling for web UI users, including lifetime, renewal and revocation, is open.
12. **Web UI listener.** Which avirtedgeproxyd listener web UI users reach avirtwebd through is open.
13. **Frontend repository layout.** Where the web UI's frontend app lives in the repository is open.
14. **Local user principal.** How a local caller on avirtedgeproxyd's local client listener is represented as a Cedar principal and carried in `x-avirt-caller` is open.
15. **Enforcing the off-host rule on the host.** avirtedgeproxyd is the only host service that opens connections off the host. Which host mechanism enforces that for the other daemons, beyond their configuration, is open (see [networking](./networking.md#off-host-enforcement)).
