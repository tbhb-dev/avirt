---
type: Security Design
title: Trust boundaries and reachability
description: The implemented avirtvmd control socket, each planned avirt listener with its transport, caller identity and allowed operations, the local listeners exempt from mutual TLS, what a compromised agent, root in the sandbox container, mediator or peer can reach, and the `src` field of each logged event.
tags: [avirt, security, trust, reachability, threat-model]
implementation: partial
generated: { by: codex/gpt-6, at: 2026-10-18T05:14:22Z }
---

# Status: implemented

## avirtvmd control socket

avirtvmd listens without mutual TLS on the Unix socket `workloads/<id>/v.sock`, which has mode 0600 and is in a workload directory with mode 0700. At startup, before it binds the socket, avirtvmd reads its own code signature and selects one control-peer policy for the process lifetime. If its signature meets Apple's Developer ID rule for apps but has no team identifier, or if avirtvmd can't read its signing information, it exits with status 70 instead of selecting the development policy. avirtvmd runs unconfined as the host user and can reach anything that user can reach.

For each accepted connection, avirtvmd first reads the peer's user ID with `LOCAL_PEERCRED`. A mismatch closes the connection without a message. When avirtvmd's own code meets Apple's Developer ID rule for apps and includes a team identifier, avirtvmd next reads the peer's audit token from the accepted connection with `LOCAL_PEERTOKEN` and checks the peer's code. The peer's code must meet Apple's Developer ID rule for apps. Its leaf-certificate team organizational unit must equal avirtvmd's team identifier, and its signing identifier must equal `dev.avirt.hostd`. The check requires the signing identifier `dev.avirt.hostd` and rejects another binary from the same team, including avirtsecretsd. Acceptance of a Developer ID-signed avirthostd remains unverified. Failure to read the audit token or the peer's code fails the check.

A signature check failure returns a `Refusal` with `REFUSAL_CODE_WRONG_PEER` and closes the connection before the hello. The peer receives neither the workload ID nor the incarnation. The audit token establishes the peer's identity when avirtvmd accepts the connection. avirtvmd doesn't check again after an `exec`, and the connection's authority ends when the connection closes. After these checks, avirtvmd sends the hello and continues the handshake.

An avirtvmd signature that doesn't meet Apple's Developer ID rule for apps, including the ad-hoc signature from `build-vmd` or a signature from any other certificate, selects the development policy. That policy checks the user ID only. The workload directory has mode 0700, and avirthostd checks the helper's incarnation in the hello. avirtvmd writes `avirtvmd: development build: control peers are checked by user ID only` to `logs/<workload-id>.avirtvmd.log`.

avirthostd doesn't check avirtvmd's signature. If avirtvmd refuses avirthostd instead of sending the hello, launch fails with `avirtvmd refused the connection (wrong peer): ...`. During restart, avirthostd lists a live helper that refuses its connection as unknown, waits through the grace period, and then sends `SIGKILL`.

# Status: planned

This document describes the planned design in [components](./components.md). [Networking](./networking.md) specifies the connections and their transports. Process restrictions are in [workload hardening](./workload-hardening.md) and [Landlock](./landlock.md).

Each listener that a caller reaches across a container or host boundary requires mutual TLS with SPIFFE identities from the first byte, and authorizes the caller's exact identity before reading any application data. Authentication never grants access by itself: each listener then authorizes the operation against the caller's workload, generation, and role. Listeners deny host management and cross-workload access unless a rule below allows them.

## Listeners

| Listener | Where | Transport | Caller identity | Allowed operations |
| --- | --- | --- | --- | --- |
| avirtnetd's transparent TCP listeners | Sandbox container, avirtnetd | nftables REDIRECT to separate IPv4 and IPv6 listeners | Any workload process | Original destination lookup and one CONNECT stream per flow |
| avirtnetd's TUN | Sandbox container, avirtnetd alone holds its descriptor | IPv4 and IPv6 packets | Guest kernel | UDP to CONNECT-UDP, echo to `icmp`, port 53 to `/dns`. TCP receives RST |
| avirtnetd's optional forward proxy (`127.0.0.1:PORT`) | Sandbox container, avirtnetd, on a listener that avirtinitd binds and hands to it | TCP on loopback | Any workload process | `CONNECT host:port` only, forwarded as one HTTP/2 stream per connection on the workload's HTTP/2 connection to its mediator |
| DNS stub resolver (`127.0.0.53:53`) | Sandbox container, avirtnetd | UDP and TCP port 53 at any address, redirected to the resolver | Any workload process | DNS queries only, one `/dns` stream per query and subject to the workload's `resolve` policy |
| Agent sockets (`/run/avirt/agent/guest.sock`, `/run/avirt/agent/mcp.sock`) | Sandbox container, avirtnetd | Unix socket, accessible to uid 1000 only, checked with `SO_PEERCRED` | uid 1000 | guest.sock: avirtguest requests, forwarded as `/ns/<ns>/workload/<uuid>/guest-cli`. mcp.sock: MCP sessions to avirtmcpgwd for this workload. |
| Local service sockets (`/run/avirt/streams/<role>.sock`) | Sandbox container, avirtnetd | Unix socket, 0600, one per process UID, checked with `SO_PEERCRED` | avirtinitd, the main workload process, avirtguestd | Submitting envelopes for the `/envelopes` stream, and the streams that belong to that role (session control for the main workload process, ingress for avirtguestd) |
| Launcher sockets (`/run/avirt/init/<role>.sock`) | Sandbox container, avirtinitd | Unix socket, 0600, one per process UID and owned by it, checked with `SO_PEERCRED` | avirtnetd, the main workload process, avirtguestd, each on its own socket | avirtnetd: the state of its connection to the mediator, confirm, freeze reasons. Main workload process: launch the agent. avirtguestd: a `workspace-snapshot` freeze reason. |
| Workload listener (`/run/avirt/workload.sock`) | Mediator, avirtmediatord | Runtime-provided Unix socket, mutual TLS 1.3, h2 | Exactly `/ns/<own ns>/workload/<own uuid>` | The HTTP/2 streams in [workload topology](./workload-topology.md#http2-streams) |
| Ingress listener (`/run/avirt/ingress.sock`) | Mediator, avirtmediatord | Runtime-provided Unix socket, mutual TLS 1.3, h2 | Exactly this host's `/service/avirtedgeproxyd`, including the source identity in `x-avirt-caller` | `CONNECT` to a published port, subject to the workload's `ingress` policy for that source |
| The mediator's connection to avirthostd (`/run/avirt/hostd.sock`) | Host, avirthostd | Runtime-provided Unix socket, mutual TLS 1.3, h2 | `/ns/<ns>/workload/<uuid>/mediator/<g>`, for the socket's recorded workload and current generation | `GET /workload`, `GET /workload/control`, `POST /workload/svid`, `POST /workload/envelopes`, and `POST /workload/events`, for its own workload. |
| SNI routing on the mediator's outbound socket (`/run/avirt/svc.sock`) | Host, avirthostd | Runtime-provided Unix socket, TLS forwarded unchanged | Any connection on that workload's outbound socket | Forwarding the bytes unchanged to an avirt host service or to avirtedgeproxyd, as named by the TLS SNI. Never to another mediator. |
| Host service listeners (`services/<name>.sock`) | Host, each host service | Unix socket, 0660 with a group containing only the UIDs allowed to connect, mutual TLS 1.3 | avirtmediatord processes of live workloads and current generations, and the `/service/<name>` callers each component lists | Workload-scoped operations for avirtmediatord, and service operations for listed services |
| avirtregistryd (`services/avirtregistryd.sock`) | Host, avirtregistryd | Host service listener | avirtapid with `x-avirt-caller`, avirtbuilderd, avirthostd, and the avirtmediatord processes of build jobs (live workloads and current generations) | `WorkspaceRegistry` calls for avirtapid, avirtbuilderd and avirthostd, each authorized by Cedar. For a build job, the OCI distribution API on that job's own grant only, restricted to its inputs and cache scopes, with writes only to `staging/<build-id>` (see [workspaces](./workspace-build.md#build-jobs)). |
| zot (`127.0.0.1:<port>`) | Host, zot, a child process of avirtregistryd | Loopback TCP, mutual TLS with a private CA that avirtregistryd stores in memory | avirtregistryd only | The OCI distribution API and zot's search extension |
| avirtbuilderd (`services/avirtbuilderd.sock`) | Host, avirtbuilderd | Host service listener | avirtapid with `x-avirt-caller`, avirthostd (job exit reports) | Build, build record, log and watch calls, each authorized by Cedar |
| avirtsecretsd (`services/avirtsecretsd.sock`) | Host, avirtsecretsd | Host service listener | Broker API: avirtmodelgwd, avirtmcpgwd, avirtmediatord. The broker's `service_credential` operation: avirtregistryd and avirtbuilderd. Registration and Admin: avirthostd. | Gateways: credentials for a binding of the workload they act for. avirtmediatord: its own workload's placeholders, and leases for mediator-injected bindings of its own workload. avirtregistryd and avirtbuilderd: the service credentials they're bound to. avirthostd: registering and validating binding references, and the Admin API (see [secrets](./secrets.md#apis)). |
| avirtpolicyd distribution | Host, avirtpolicyd | Host service listener | avirtmediatord processes, avirtedgeproxyd, avirthostd, avirtbuilderd, avirtregistryd, avirtsecretsd | Reading the workload policy snapshot for the caller's workload, or the host policy snapshot for avirtedgeproxyd, avirthostd, avirtbuilderd, avirtregistryd, and avirtsecretsd. avirthostd also sends entity revisions. |
| avirtoteld (`services/avirtoteld.sock`) | Host, avirtoteld | Host service listener, OTLP over gRPC | avirtmediatord processes of live workloads and current generations, the host services, and avirtedgeproxyd | OTLP export. avirtoteld stamps the caller's identity attributes from its SVID (see [observability](./observability.md#avirtoteld)). |
| avirthostd control API (`services/avirthostd.sock`) | Host, avirthostd | Host service listener | avirtapid with `x-avirt-caller`, avirtedgeproxyd for its local client routes, and avirtbuilderd | Workload management and the other control API calls, each authorized against the host policy snapshot. avirtbuilderd: creating build jobs and reporting their exits. |
| avirthostd issuer (`services/issuer.sock`) | Host, avirthostd | Unix socket, 0600 | Host services holding an enrollment token or a current `/service/<name>` SVID | A CSR for the caller's own `/service/<name>` ID |
| avirtedgeproxyd's `edgeproxy.sock` in the runtime directory | Host, avirtedgeproxyd | Unix socket, 0600, user-owned | The local user | Routes to avirtapid (gRPC from avirt, REST from avirtwebd), avirtwebd, and avirthostd |
| Web UI port (a host loopback port) | Host, avirtedgeproxyd | TCP on the host's loopback | Any local process | Routes to avirtwebd, which authenticates web UI users (see [web UI](./components.md#web-ui)) |
| avirthostd admin socket (`admin.sock` in the runtime directory) | Host, avirthostd | Local Unix socket, 0600, owned by the UID avirthostd runs as | avirtadmin, authenticated by peer credentials (`LOCAL_PEERCRED` on macOS, `SO_PEERCRED` on Linux): only avirthostd's own UID or root | Host administration, as the principal `Admin::"uid:<uid>"`. Not reachable through avirtapid or avirtedgeproxyd. See [admin CLI](./admin-cli.md). |
| SNI routing for inbound connections (`services/ingress-relay.sock`) | Host, avirthostd | Unix socket, checked with `LOCAL_PEERCRED` on macOS and `SO_PEERCRED` on Linux, TLS forwarded unchanged | avirtedgeproxyd only | Forwarding the bytes unchanged to avirtmediatord's ingress listener of the workload named by SNI `<uuid>.ingress.<td>`, at its current generation |
| avirtedgeproxyd egress listener | Host, avirtedgeproxyd | Reached through SNI routing on a mediator's outbound socket, or directly by host services, mutual TLS 1.3, h2 | avirtmediatord processes of live workloads and current generations, host services | CONNECT, CONNECT-UDP and `icmp`, after resolving avirt hostnames to entities and evaluating the host policy snapshot, to same-host workloads' published ports, services, authorized host-local services, peered-host workloads and external endpoints, and the `guest-cli` stream to avirtapid |
| Peer listener | Host, avirtedgeproxyd | TCP on the host network, on the addresses in `peering.listen` (default port 8788, no listener while it's empty), mutual TLS 1.3 with federated bundles. Peers authenticate by SVID, never by source address. | `spiffe://<peer td>/service/avirtedgeproxyd` of active enrolled peers, with an `x-avirt-caller` in that peer's trust domain | CONNECT, CONNECT-UDP or `icmp` to local published ports and services that the host policy snapshot allows for the originating peer identity, and routes to avirtapid (see [peering](./peering.md#peer-listener)) |
| Workload stdio logs (`logs/<workload-id>.stdio.ndjson`) | Host, written by avirtvmd on the Containerization runtime | Files, mode 0600. Status: planned. Mode 0640, group readable only by avirtlogd's and avirtauditd's UIDs | avirtlogd and avirtauditd, read-only | Tailing with persisted offsets. Only avirtvmd writes the files, making their identity fields trustworthy. Log text is untrusted. |

## Local exceptions

These listeners inside the sandbox container don't use mutual TLS. Each one is reachable only from inside the sandbox container, and each fixes the principal and the operations of each connection it accepts:

- avirtnetd's transparent TCP listeners and TUN, which fix the workload principal and send each flow to the mediator.
- avirtnetd's optional forward proxy, which accepts only `CONNECT` and forwards each connection to avirtmediatord for policy.
- the DNS stub resolver, which forwards each query to avirtmediatord for policy.
- the agent sockets, which transport only avirtguest and avirtmcp traffic, under fixed principals.
- the local service sockets and the launcher sockets, which only the per-process UIDs can open.

On the host, these local listeners don't use mutual TLS either, and authenticate the local caller instead:

- avirtedgeproxyd's `edgeproxy.sock`, a user-owned 0600 socket in the runtime directory.
- the web UI port, a host loopback port, where avirtwebd authenticates web UI users.
- avirtvmd's `v.sock`, a 0600 socket in the 0700 workload directory, which checks avirthostd's user ID and, when avirtvmd selects its Developer ID policy, avirthostd's Developer ID team and signing identifier.
- avirthostd's `admin.sock`, which admits only avirthostd's own UID or root by peer credentials (see [admin CLI](./admin-cli.md)).

uid 1000 never holds an SVID or its key, the trust bundle, a CA key, or a credential from avirt. A trusted upstream can return an injected credential in its response.

Destination checks occur outside the VM. Only a workload's own mediator accepts its SVID. Bypass tests include unrestricted root inside the guest, runtime processes, and inherited descriptors.

## Compromise scope

| Compromised | Can reach | Can't reach |
| --- | --- | --- |
| Agent (uid 1000) | TCP, UDP, ICMP and DNS through avirtnetd, subject to the workload's policy in avirtmediatord and the host policy snapshot and `private-dest` check in avirtedgeproxyd. DNS as an exfiltration path, bounded by `resolve` policy and logged. The agent sockets, as `guest-cli` and as the workload's MCP client. Through `guest-cli`, the workload management operations its policy grants, within the constraints in [workload management](./components.md#workload-management). Authority remains limited to the workload's own. Reserved labels and host administration remain forbidden. Its own files and the workspace. The placeholders and the MITM CA certificate, which aren't secrets. | Any SVID or key, the sockets owned by avirtnetd, avirtacp, avirtagent, avirtexec, and avirtguestd, the memory of other UIDs, any host socket (seccomp denies `AF_VSOCK`, and the sandbox VM's only host vsock listener is the one behind `/run/avirt/mediator.sock`, used by the runtime's in-guest socket forwarder), any network path outside mediation, any real credential except one returned by a trusted recipient, other workloads except through avirtedgeproxyd's policy. The filesystem limits are in [Landlock](./landlock.md#guarantees-by-layer). |
| Root in the sandbox container | Everything in the sandbox container, including the workload SVID in avirtnetd. Each HTTP/2 stream on the workload's connection to its mediator, as the workload and as each process in the container. The workload management operations the workload's policy grants, within the same constraints as for the agent. | The mediator's keys, the workload's MITM CA key, real credentials except those returned by trusted recipients. Direct host service access: the workload's HTTP/2 connection to its mediator is its only connection out and ends at avirtmediatord. A mediator SVID: avirthostd issues only `/ns/<ns>/workload/<uuid>` to a workload-authenticated request. Another namespace's identity. Other workloads except through avirtedgeproxyd's policy |
| Mediator (avirtmediatord) | A parser bug exposes the private key of avirtmediatord's SVID and mediator-injected credentials inside its mediator VM with no network device. Bypassing its own workload's policy and `credential-misplaced` check. The workload's MITM CA key permits interception of the agent's TLS connections to hosts within the CA's name constraints. This includes hosts the policy routes through TLS passthrough and applies to this and each later workload generation. A workload lacks a MITM CA unless its policy permits interception. The workload's placeholders and mediator-injected short-lived credentials. Host services through avirthostd's SNI routing, as that workload only. External endpoints, other workloads, services and host-local services that the host policy snapshot allows for its own identity, all through avirtedgeproxyd. Forging its own workload's mediator-reported events. | Model and MCP credentials: they never leave the gateways. Other workloads' credentials: the broker authorizes by workload and generation. Private and host addresses: avirtedgeproxyd checks each resolved address. Also inaccessible are a workload SVID for its own key (rotation requires the current workload key) and the SPIFFE CA. Direct access to another mediator: SNI routing never connects two mediators.  |
| avirtvmd (same host user as avirthostd) | Its workload's VM, including guest memory, the relayed sockets, and the identity fields of the stdio log. Everything that user can read or open (including sibling workload directories and host sockets). | Anything that user can't reach (see [open questions](#open-questions)). |
| avirtedgeproxyd | Each workload-to-workload, service, host-local service and external connection on the host, as their policy enforcement point. The host's network and its own addresses, bypassing avirtedgeproxyd's private-address check. Asserting any caller identity in `x-avirt-caller` to destination mediators, avirtapid, and peers. Authorized host-local backends. Its own SVID and its stored peer bundles. | The SPIFFE CA. Model and MCP credentials (which stay at the gateways and avirtsecretsd). Any mediator's or workload's keys. Plaintext from a mediator's upstream TLS connections or from workload-to-workload bytes. avirt's host services except through their own authorization |
| Build job (root in the sandbox container in the build VM) | Its own inputs and staging repository. Its cache scope and anything its workload policy snapshot allows through avirtedgeproxyd. Writing anything into its staging output. | Any workspace repository, tag or annotation (only avirtbuilderd promotes), another build's inputs or staging, real credentials, avirtapid (avirthostd refuses a build job's `guest-cli` stream), avirthostd's control API, any signing key |
| avirtregistryd | Each stored workspace image, and the registry credentials it obtains from avirtsecretsd. Serving a forged index or forged content to avirthostd. | Running content. avirthostd verifies each digest it pulls against the resolved manifest, so forged content fails at launch. `launch_workspace` on the returned image record limits a forged resolution |
| avirtbuilderd | Starting build jobs and promoting chosen outputs. Writing any annotation | Running commands on the host, any SVID other than its own, avirthostd's control API beyond creating build jobs |
| Peer | This host's peer listener, as identities in its own trust domain. Local published ports and services that this host's host policy snapshot allows for those identities, and the avirtapid routes its enrollment allows (see [peering](./peering.md#what-crosses-a-peering)). | This host's SVIDs or CA (its bundle only verifies), identities in any other trust domain, any mediator or host service other than through avirtedgeproxyd, any destination the host policy snapshot doesn't allow for its identities |

Permitted passthrough endpoints can tunnel arbitrary traffic. DNS names, encrypted DNS, UDP and ICMP payloads and timing are residual channels under their endpoint policies. A credential binding requires trust in its recipient, which can return an injected credential to the workload. Optional response scanning can't guarantee detection.

The table describes enforce mode. In audit mode, avirt logs a policy decision with no permit or with a forbid as `would-deny` and allows the operation. This permits access to destinations without a policy permit. These hold in both modes: caller authentication, generation fencing, name-to-entity resolution, the `private-dest` check, `Host` matching, the `credential-misplaced` check, credential bindings, `intercept` decisions and `resolve` decisions.

The `guest-cli` principal and the workload's MCP access are inside the agent's trust boundary. avirtguest and avirtmcp run as uid 1000, so their grants also apply to the agent.

Runtime isolation bounds a compromise of root in the sandbox container. On Docker Desktop each workload shares one kernel, so a kernel escape from any sandbox container permits access to each sandbox container and mediator (see [compute runtimes](./compute-runtimes.md#docker-desktop-runtime)).

## `src`

Each event in a workload log includes `workload`, `gen` and `seq`, and a `src` label naming the component that observed it. avirthostd stamps `src`, `workload` and `gen` from the authenticated connection the event arrived on, and ignores any value the producer supplied. The label states the extent of trust in the event.

| `src` | Written by | Trust |
| --- | --- | --- |
| `hostd` | avirthostd, from its own observation (launch, runtime exits, write-backs, errors) | Host-observed |
| `edgeproxy` | avirtedgeproxyd, for connections it opened | Host-observed |
| `gateway` | avirtmodelgwd and avirtmcpgwd, for requests they served | Host-observed |
| `secrets` | avirtsecretsd, for credential fetches, refreshes, and provider errors it observed | Host-observed |
| `mediator` | avirtmediatord | Mediator-reported: trustworthy only while the mediator isn't compromised |
| `workload` | avirtnetd, avirtinitd and the main workload process, on the `/envelopes` stream | Workload-reported and relayed by the mediator: trustworthy only while both root in the sandbox container and the mediator remain free of compromise |

avirtauditd keeps `src` in each OCSF record it produces.

# Open questions

1. **Confining avirtvmd.** How to confine avirtvmd below its host user's access remains unspecified.
