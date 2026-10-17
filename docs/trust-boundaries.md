---
type: Security Design
title: Trust boundaries and reachability
description: Every planned avirt listener with its transport, caller identity and allowed operations, the local listeners exempt from mutual TLS, what a compromised agent, guest root, mediator or peer host can reach, and the provenance labels on logged events.
tags: [avirt, security, trust, reachability, threat-model]
implementation: planned
generated: { by: claude-code/claude-opus-5-5, at: 2026-10-17T19:00:00Z }
---

# Status: planned

This document describes the planned design in [components](./components.md). The implemented system's boundaries are in [workload hardening](./workload-hardening.md), [actor topology](./actor-topology.md) and [identity](./identity.md).

Every listener that a caller reaches across a container or host boundary requires mutual TLS with SPIFFE identities from the first byte, and authorizes the caller's exact identity before reading an application byte. Authentication never grants access by itself: each listener then authorizes the operation against the caller's actor, generation and role. Host management and cross-actor access are denied unless a rule below allows them.

## Listeners

| Listener | Where | Transport | Caller identity | Allowed operations |
| --- | --- | --- | --- | --- |
| Loopback egress proxy (`127.0.0.1:PORT`) | Workload, avirtnetd | TCP on loopback | Any workload process | `CONNECT host:port` only, carried as one link stream per connection |
| DNS stub resolver (`127.0.0.53:53`) | Workload, avirtnetd | UDP and TCP on loopback | Any workload process | DNS queries only, carried on the link's DNS stream and subject to the actor's `resolve` policy |
| Agent sockets (`/run/avirt/agent/guest.sock`, `/run/avirt/agent/mcp.sock`) | Workload, avirtnetd | Unix socket, accessible to uid 1000 only, checked with `SO_PEERCRED` | uid 1000 | guest.sock: avirtguest requests, forwarded as `/ns/<ns>/actor/<uuid>/workload/guest-cli`. mcp.sock: MCP sessions to avirtmcpgwd for this actor. |
| Stream sockets (`/run/avirt/streams/<role>.sock`) | Workload, avirtnetd | Unix socket, 0600, one per helper UID, checked with `SO_PEERCRED` | avirtinitd, the session runner, avirtguestd | Submitting uplink envelopes, and the streams that role owns (session control for the runner, ingress for avirtguestd) |
| Launcher sockets (`/run/avirt/init/<role>.sock`) | Workload, avirtinitd | Unix socket, 0600, one per helper UID and owned by it, checked with `SO_PEERCRED` | avirtnetd, the session runner, avirtguestd, each on its own socket | avirtnetd: link state, confirm, freeze holds. Runner: launch the agent. avirtguestd: a snapshot freeze hold. |
| Link listener (`/run/avirt/link.sock`) | Mediator, mediator Envoy | Runtime link, mutual TLS 1.3, h2 | Exactly `/ns/<own ns>/actor/<own uuid>/workload` | The link streams in [actor topology](./actor-topology.md#link-stream) |
| Ingress listener (`/run/avirt/ingress.sock`) | Mediator, mediator Envoy | Ingress channel, mutual TLS 1.3, h2 | Exactly this host's `/service/frontdoor`, carrying the source identity in `x-avirt-caller` | `CONNECT` to a published port, subject to the actor's `ingress` policy for that source |
| Local xDS and SDS (`/run/avirt/xds.sock`) | Mediator, avirtmediatord | Unix socket, 0600, owned by the Envoy UID | The mediator Envoy | Fetching listeners, clusters, its SVID and the trust bundle |
| avirthostd channel | Host, avirthostd | Runtime channel, mutual TLS 1.3, h2 | `/ns/<ns>/actor/<uuid>/mediator/<g>` and `/ns/<ns>/actor/<uuid>/mediator/<g>/envoy`, for the channel's recorded actor and current generation | avirtmediatord: `GET /actor`, `GET /actor/control`, `POST /actor/svid`. Mediator Envoy: `POST /actor/uplink` and `POST /actor/events` only. |
| Service relay | Host, avirthostd | Service channel, TLS not terminated | Any connection on that actor's service channel | Splicing to an avirt host component or to the front-door Envoy, as named by the TLS SNI. Never to another mediator. |
| Host component listeners (`services/<name>.sock`) | Host, each host component | Unix socket, 0660 with a group holding only the UIDs allowed to connect, mutual TLS 1.3 | Mediator Envoys and avirtmediatord of live actors and current generations, and the `/service/<name>` callers each component lists | Actor-scoped operations for a mediator Envoy, and service operations for listed services |
| avirtregistryd (`services/avirtregistryd.sock`) | Host, avirtregistryd | Host component listener | avirtproxyd with `x-avirt-caller`, avirtbuilderd, avirthostd, and build job mediator Envoys of live actors and current generations | `WorkspaceRegistry` calls for avirtproxyd, avirtbuilderd and avirthostd, each authorized by Cedar. For a build job, the OCI distribution API on its own grant only: its inputs, its cache scopes, and writes to `staging/<build-id>` (see [workspaces](./workspaces.md#build-jobs)). |
| zot (`127.0.0.1:<port>`) | Host, zot, a child process of avirtregistryd | Loopback TCP, mutual TLS with a private CA that avirtregistryd holds in memory | avirtregistryd only | The OCI distribution API and zot's search extension |
| avirtbuilderd (`services/avirtbuilderd.sock`) | Host, avirtbuilderd | Host component listener | avirtproxyd with `x-avirt-caller`, avirthostd (job exit reports) | Build, build record, log and watch calls, each authorized by Cedar |
| avirtsecretsd broker API | Host, avirtsecretsd | Host component listener | avirtmodelgwd, avirtmcpgwd, avirtmediatord, mediator Envoys, avirthostd | Gateways: credentials for a binding of the actor they act for. avirtmediatord: its own actor's placeholder set. Mediator Envoy: its own actor's placeholder set and mediator-injected credentials. avirthostd: validating binding references. |
| avirtpolicyd distribution | Host, avirtpolicyd | Host component listener | Mediator Envoys, the front-door Envoy, avirthostd | Reading the snapshot for the caller's actor, or the host snapshot for the front-door Envoy and avirthostd. avirthostd also sends entity revisions. |
| avirthostd issuer (`services/issuer.sock`) | Host, avirthostd | Unix socket, 0600 | Host components holding an enrollment token or a current `/service/<name>` SVID | A CSR for the caller's own `/service/<name>` ID |
| Front-door local client listener | Host, front-door Envoy | Unix socket in the state root, 0600, user-owned | The local user | Routes to avirtproxyd (gRPC from avirt, REST from avirtwebd), avirtwebd and avirthostd |
| avirthostd admin socket (`admin.sock` in the state root) | Host, avirthostd | Local Unix socket, 0600, owned by the UID avirthostd runs as | avirtadmin, authenticated by peer credentials (`LOCAL_PEERCRED` on macOS, `SO_PEERCRED` on Linux): only avirthostd's own UID or root | Host administration, as the principal `Admin::"uid:<uid>"`. Not reachable through avirtproxyd or the front-door Envoy. |
| Ingress relay (`services/ingress-relay.sock`) | Host, avirthostd | Unix socket, checked with `SO_PEERCRED`, TLS not terminated | The front-door Envoy only | Splicing to the ingress channel of the actor named by SNI `<uuid>.ingress.<td>`, at its current generation |
| Front-door egress listener | Host, front-door Envoy | Service channel, mutual TLS 1.3, h2 | Mediator Envoys of live actors and current generations, host components | `CONNECT`, after resolving avirt names to entities and evaluating the host snapshot, to same-host workloads' published ports, virtual services, authorized host-local services, peered-host workloads and external endpoints, and the `guest-cli` stream to avirtproxyd |
| Front-door peer listener | Host, front-door Envoy | TCP on the host network, mutual TLS 1.3 with federated bundles | `spiffe://<peer td>/service/frontdoor` of enrolled peers, with an `x-avirt-caller` in that peer's trust domain | `CONNECT` to local published ports and virtual services that the host snapshot allows for the originating peer identity, and routes to avirtproxyd |
| Workload log files (`logs/<actor-id>.stdio.ndjson`) | Host, written by avirthostd | Files, 0640, group readable only by avirtlogd's UID | avirtlogd, read-only | Tailing with persisted offsets. Identity fields are trusted, because only avirthostd writes the files. Log text is untrusted. |

## Local exceptions

These listeners inside the workload do not use mutual TLS. Each one is reachable only from inside the workload, and each fixes the principal and the operations of every connection it accepts:

- the loopback egress proxy, which accepts only `CONNECT` and forwards every connection to the mediator Envoy for policy;
- the DNS stub resolver, which forwards every query to the mediator Envoy for policy;
- the agent sockets, which carry only avirtguest and avirtmcp traffic, under fixed principals;
- the stream and launcher sockets, which only the helper UIDs can open.

uid 1000 never holds an SVID or its key, the SPIFFE bundle, a CA key or a real credential.

## Compromise scope

| Compromised | Can reach | Cannot reach |
| --- | --- | --- |
| Agent (uid 1000) | The loopback proxy and the DNS stub, subject to the actor's policy in the mediator Envoy and the host snapshot and destination check in the front-door Envoy. DNS as a channel, bounded by `resolve` policy and logged. The agent sockets, as `guest-cli` and as the actor's MCP client. Through `guest-cli`, the workload management operations its policy grants, within the constraints in [workload management](./components.md#workload-management): never more authority than the workload holds, no reserved labels, no host administration. Its own files and the workspace. The placeholders and the MITM CA certificate, which are not secrets. | Any SVID or key, the helper sockets, the memory of other UIDs, any host channel (`AF_VSOCK` is denied, and the workload VM's only host vsock listener is the link's, used by the runtime's relay), any network, any real credential, other workloads except through the front-door Envoy's policy |
| Guest root (the whole workload) | Everything in the workload, including the workload SVID in avirtnetd. Every link stream, as the workload and under every local principal. The workload management operations the workload's policy grants, within the same constraints as for the agent. | The mediator's keys, the actor's MITM CA key, real credentials (they stay at avirtsecretsd and the gateways), host components directly (the link is the workload's only channel and ends at the mediator Envoy), a mediator SVID (avirthostd issues only `/ns/<ns>/actor/<uuid>/workload` to a workload-authenticated request), another namespace's identity, other workloads except through the front-door Envoy's policy |
| Mediator (avirtmediatord or Envoy) | Bypassing its own actor's policy and leak scan. The actor's MITM CA key, with which it can intercept the agent's TLS connections to the hosts within the CA's name constraints, including ones the policy routes on the passthrough path, for this and every later generation of the actor. An actor whose policy permits no interception has no MITM CA. The actor's placeholder set and mediator-injected short-lived credentials. Host components through the service relay, as that actor only. External endpoints, other workloads, virtual services and host-local services that the host snapshot allows for its own identity, all through the front-door Envoy. Forging its own actor's mediator-reported events. | Model and MCP credentials (they never leave the gateways), other actors' credentials (the broker authorizes by actor and generation), private and host addresses (the front-door Envoy checks every resolved address), a workload SVID for its own key (rotation requires the current workload key), any other mediator directly (the service relay never connects two mediators), the SPIFFE CA. A compromised Envoy alone also cannot reach avirtmediatord's key or the avirthostd channel's control routes. |
| Front-door Envoy | Every workload-to-workload, virtual service, host-local service and external connection on the host, as the policy enforcement point for all of them. Asserting any caller identity in `x-avirt-caller` to destination mediators, avirtproxyd and peers. Authorized host-local backends. Its own SVID and the peer bundles it holds. | The SPIFFE CA, model and MCP credentials (they stay at the gateways and avirtsecretsd), any mediator's or workload's keys, the plaintext of re-originated TLS and of workload-to-workload bytes, avirt's host components except through their own authorization |
| Build job (guest root in the build VM) | Its own inputs, its staging repository, its cache scope, and what its Cedar snapshot allows through the front-door Envoy. Writing anything into its staging output. | Any workspace repository, tag or annotation (only avirtbuilderd promotes), another build's inputs or staging, real credentials, avirtproxyd (avirthostd refuses a build job's `guest-cli` stream), avirthostd's control API, any signing key |
| avirtregistryd | Every stored workspace image, and the registry credentials it obtains from avirtsecretsd. Serving a forged index or forged content to avirthostd. | Running content: avirthostd verifies every digest it pulls against the resolved manifest, so forged content fails at launch, while a forged resolution is bounded by `launch_workspace` on the image record it returns |
| avirtbuilderd | Starting build jobs, promoting what it chooses, and writing any annotation | Running commands on the host, any SVID other than its own, avirthostd's control API beyond creating build jobs |
| Peer host | This host's front-door peer listener, as identities in its own trust domain. Local published ports and virtual services that this host's snapshot allows for those identities, and the avirtproxyd routes its enrollment allows. | This host's SVIDs or CA (its bundle only verifies), identities in any other trust domain, any mediator or host component other than through the front-door Envoy, any destination the host snapshot does not allow for its identities |

The rows above describe enforce mode. In audit mode, a policy decision with no permit or with a forbid is logged as `would-deny` and allowed, so a destination the snapshot does not permit is reachable. These hold in both modes: caller authentication, generation fencing, name-to-entity resolution, the destination check, `Host` matching, the leak scan, credential bindings, `intercept` decisions and `resolve` decisions.

The `guest-cli` principal and the actor's MCP access are inside the agent's trust boundary. avirtguest and avirtmcp run as uid 1000, so anything granted to them is granted to the agent.

Runtime isolation bounds a guest-root compromise. On Docker Desktop every actor shares one kernel, so a kernel escape from any workload reaches every actor's workload and mediator (see [compute runtimes](./compute-runtimes.md#docker-desktop-runtime)).

## Event provenance

Every event in an actor log carries `actor`, `gen` and `seq`, and a `src` label naming the component that observed it. avirthostd stamps `src`, `actor` and `gen` from the authenticated channel the event arrived on, and ignores any value the producer supplied. The label states how far the event can be trusted.

| `src` | Written by | Trust |
| --- | --- | --- |
| `hostd` | avirthostd, from its own observation (launch, runtime exits, write-backs, errors) | Host-observed |
| `frontdoor` | libavirtenvoy in the front-door Envoy, for connections it opened | Host-observed |
| `gateway` | avirtmodelgwd and avirtmcpgwd, for requests they served | Host-observed |
| `mediator` | libavirtenvoy in the mediator Envoy and avirtmediatord | Mediator-reported: trustworthy only while the mediator is not compromised |
| `workload` | avirtnetd, avirtinitd and the session runner, through the uplink | Workload-reported and relayed by the mediator: trustworthy only while neither guest root nor the mediator is compromised |

avirtauditd keeps `src` in every OCSF record it produces.
