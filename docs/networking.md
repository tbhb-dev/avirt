---
type: Architecture Overview
title: Networking
description: avirt's whole network model in one place, with the topology, internal names and IDs, each connection type end to end with its hops, enforcement points and identities, the transports on each runtime, the supported protocols, the listener inventory, how the restriction that only avirtedgeproxyd connects off the host is enforced on the host, and failure behavior and observability, linking out to the documents that specify each part.
tags: [avirt, networking, topology, egress, ingress, dns, mtls]
implementation: partial
generated: { by: codex/gpt-6, at: 2026-10-18T05:21:55Z }
---

# Overview

This document is the map of avirt's network model. It identifies each path a byte can take and the policy enforcement point (PEP) and identity on each hop, and it links to the document that specifies each part. Where this document and a linked document differ, the linked document is the specification.

The model rests on four rules:

- The workload connects to its mediator through a Unix socket that the compute runtime provides. The mediator has no network interface other than loopback and uses runtime-provided Unix sockets backed by vsock, a bind mount or a Docker named volume (see [workload topology](./workload-topology.md#runtime-provided-sockets)).
- avirtnetd provides the workload's HTTP/2 connection to its mediator. Its forward proxy and DNS stub use that connection.
- avirtedgeproxyd is the only global PEP for workload-to-workload, service, host-local service and external connections, and the only host service that opens connections off the host. It runs on the host (see [where avirtedgeproxyd runs](./components.md#where-avirtedgeproxyd-runs)).
- Each hop that crosses a container or host boundary uses mutual TLS with SPIFFE identities from the first byte, and each intermediate hop where avirthostd routes by SNI or forwards the bytes unchanged transmits ciphertext only (see [identity](./identity.md)).

# Status: implemented

The current code runs on the Apple container CLI runtime (see [compute runtimes](./compute-runtimes.md#apple-container-cli-runtime)). Its topology diagram is in [architecture](./architecture.md#status-implemented).

## Paths

| Path | Hops | Enforcement | Authentication |
| --- | --- | --- | --- |
| Agent to external | Agent, `HTTPS_PROXY` to avirtinitd's TCP proxy to `proxy.sock` on an ephemeral `127.0.0.1` port, `/run/avirt/proxy.sock`, avirtmediatord's CONNECT app on the host, upstream | The CONNECT app: authority parsing and the `private-dest` check (see [CONNECT app](./egress.md#pingora-connect-app)). Rule `allow-all` permits each destination that passes. | None on avirtinitd's TCP proxy. Only root in the sandbox container can open `proxy.sock` (0600). |
| The workload's HTTP/2 connection to its mediator, which today transports only log envelopes | avirtinitd, `/run/avirt/mediator.sock`, avirtmediatord, `hostd/hostd.sock`, avirthostd | avirthostd's `POST /workloads/<id>/stream` route (see [connection to the mediator](./workload-topology.md#connection-to-the-mediator)) | The `WORKLOAD <id> <secret>` connection header with the mediator secret, then `x-avirt-token` on `POST /workloads/<id>/stream` (see [identity](./identity.md#status-implemented)) |
| Egress events | avirtmediatord, `hostd/hostd.sock`, avirthostd | One event connection per workload | The `EGRESS <id> <secret>` connection header |
| Operator CLI to avirthostd | `avirt`, cleartext HTTP/2 on `127.0.0.1:8787` (or `AVIRT_HOSTD_URL`), avirthostd | None | None: any local process can call the control API (see [control API](./hostd-api.md#control-api)) |

- **Workload network.** The sandbox container runs with `--network none`. TCP, UDP, DNS and IPv6 to anything but loopback fail inside it. avirtinitd sets `HTTPS_PROXY`, `https_proxy`, `HTTP_PROXY`, `http_proxy`, `ALL_PROXY`, `npm_config_proxy` and `npm_config_https_proxy` to avirtinitd's TCP proxy, `NO_PROXY` and `no_proxy` to `localhost,127.0.0.1`, and `NODE_USE_ENV_PROXY=1` (see [egress path](./egress.md#path)).
- **Off-host path.** avirtmediatord is a host process, and it resolves names with the host resolver and dials upstreams itself. If any resolved address fails the `private-dest` check, avirtmediatord refuses the whole CONNECT with 403 `private-dest`.
- **Socket domains.** avirtinitd's seccomp filter makes `socket()` with `AF_VSOCK`, `AF_NETLINK` or `AF_PACKET` return `EAFNOSUPPORT` for itself and the agent (see [seccomp](./workload-hardening.md#seccomp)).
- **Half-close constraint.** Nothing on a socket file that the runtime relays may half-close, so avirtinitd's TCP proxy never forwards a client's end-of-stream, and once the client has finished each read on the other direction has a 30 s idle timeout (see [TCP proxy to `proxy.sock`](./workload-topology.md#tcp-proxy-to-proxysock)).
- **Protocols.** The CONNECT app accepts only `CONNECT host:port`, with a hostname, a canonical dotted quad, or a bracketed IPv6 literal. Any other method gets 501. Bytes inside the tunnel are opaque, so TLS, h2, gRPC, and websockets pass through unseen. UDP has no path through this proxy.
- **Listeners.** avirthostd: `127.0.0.1:8787` (TCP) and `hostd/hostd.sock` (0600, in a 0700 directory). avirtmediatord: `workloads/<id>/proxy.sock` and `workloads/<id>/mediator.sock` (0600, in the 0700 workload directory). avirtinitd: its TCP proxy to `proxy.sock`, on an ephemeral `127.0.0.1` port inside the sandbox container. Each host path is under the runtime directory `$TMPDIR/avirt-<uid>/` (see [runtime directory](./lifecycle.md#runtime-directory)).
- **Failure.** avirtinitd doesn't start its TCP proxy or the agent until avirthostd has accepted `POST /workloads/<id>/stream` on the workload's HTTP/2 connection to its mediator, and gives up after 15 s. The mediator's stdin EOF ties its lifetime to avirthostd's. Each proxied connection becomes one `dir: "egress"` event when it closes, and the mediator flushes connections still open at shutdown with `error: "shutdown"` (see [events](./egress.md#events)).

# Status: planned

## Topology

```mermaid
flowchart LR
    subgraph wl["sandbox container (loopback only)"]
        agent["agent (uid 1000)"]
        net["avirtnetd: forward proxy 127.0.0.1:PORT, DNS stub 127.0.0.53:53, workload SVID"]
        agent -->|"HTTPS_PROXY, DNS"| net
    end
    subgraph med["mediator (loopback only)"]
        me["avirtmediatord: workload PEP"]
    end
    subgraph host["host"]
        fwd["avirthostd: forwards the bytes unchanged"]
        hchan["avirthostd: the mediator's connection"]
        relay["avirthostd: SNI routing"]
        comps["host services: avirtsecretsd, avirtpolicyd, avirtdnsd, avirtoteld, avirtmodelgwd, avirtmcpgwd"]
        proxyd["avirtapid"]
        daemons["host services with off-host calls"]
        hl["authorized host-local service"]
        clients["avirt, avirtwebd"]
        webui["web UI users on this machine"]
        admin["avirtadmin"]
        hostd["avirthostd: control plane"]
        csock["edgeproxy.sock"]
        wport["host loopback port"]
        pport["peer port, default 8788"]
        fd["avirtedgeproxyd: global PEP"]
    end
    subgraph med2["destination mediator, same host"]
        me2["avirtmediatord"]
    end
    peer["peer host avirtedgeproxyd"]
    ext(("external endpoints"))
    net <-->|"HTTP/2 over mTLS"| fwd
    fwd <--> me
    me <-->|"mTLS"| hchan
    me -->|"/envelopes stream, events"| hchan
    me -->|"outbound socket, by SNI"| relay
    relay -->|"ciphertext"| comps
    relay -->|"ciphertext"| fd
    fd -->|"SNI routing, ingress listener"| me2
    fd -->|"local Unix socket"| proxyd
    proxyd --> hostd
    fd --> hl
    fd -->|"federated mTLS"| peer
    peer -->|"federated mTLS"| pport
    pport -->|"peer listener"| fd
    fd -->|"host network"| ext
    comps -->|"off-host calls"| fd
    daemons -->|"egress listener"| fd
    clients --> csock
    csock -->|"`edgeproxy.sock`"| fd
    webui --> wport
    wport -->|"local listener"| fd
    admin -->|"admin.sock, peer credentials"| hostd
```

avirtedgeproxyd runs on the host for both runtimes. Each placement is in [components](./components.md). The runtime-provided Unix sockets and SNI routing are in [workload topology](./workload-topology.md#runtime-provided-sockets), and the design invariants are in [architecture](./architecture.md#design-invariants).

## Addressing and naming

### Zones and name forms

avirtdnsd answers the avirt DNS zone and is the only component that does. A local name ends in `<ns>.workload.host.avirt.internal` or `<ns>.svc.host.avirt.internal`, with a short form for the `default` namespace, and an internal name under a peer's host name ends in `<ns>.workload.<host>` or `<ns>.svc.<host>` for an enrolled peer `<host>`. The name forms, the label syntax and the length caps are in [DNS](./dns.md#internal-names).

avirt's host services use SNI names `<name>.svc.<td>` in the trust domain (see [SNI routing](./workload-topology.md#sni-routing)), never internal names, and the mediator refuses an agent CONNECT to one of those names with 403.

### IDs

| ID | Form | Used for |
| --- | --- | --- |
| Canonical ID | The workload's UUIDv7, never reused | SPIFFE IDs (`/ns/<ns>/workload/<uuid>/…`), Cedar entities (`Workload::"<ns>/<uuid>"`), the `workload` field of each log record |
| Short ID | The UUIDv7's last 8 hex characters, unique on the host among live workloads and among ended ones still within their DNS TTL | The `<id>` DNS label only. Authorization never uses it. |
| Alias, generated name | `<alias>-<id>` and `<adverb>-<adjective>-<noun>` | DNS labels only. Both resolve to the canonical entity and grant nothing. |

[Components](./components.md#workload-ids) specifies workload IDs.

### Namespaces

Each workload is in exactly one namespace, fixed at creation, and the namespace scopes service names and aliases. A namespace is local to its host. References to anything on another host must use fully qualified identifiers: its internal name under that peer's host name and an identity in that host's trust domain, along with a Cedar entity qualified by that host (see [namespaces and labels](./components.md#namespaces-and-labels)).

### Name resolution results

| Name | Asked from | DNS answer | Where a connection to it goes |
| --- | --- | --- | --- |
| An internal name on this host | A workload, through avirtnetd's stub, the `/dns` stream and avirtmediatord's `resolve` check, then [avirtdnsd](./dns.md#the-avirtdnsd-zone) | This host's avirtedgeproxyd address | avirtmediatord sends the CONNECT to this host's avirtedgeproxyd, which resolves the name to its canonical entity and routes to the destination workload's avirtmediatord |
| An internal name under a peer's host name | The same | The peer's avirtedgeproxyd address, from its enrollment record | avirtmediatord still sends the CONNECT to this host's avirtedgeproxyd, which forwards it to the peer's avirtedgeproxyd (see [peering](./peering.md#workload-traffic)) |
| A public name | A workload, through the same stub and `resolve` check, then avirtedgeproxyd's DNS proxy | The real answer, with `resolve` evaluated for each name in a CNAME chain | The CONNECT includes the name. avirtedgeproxyd resolves it again when it dials, and the `private-dest` check applies. |
| A denied name | A workload | `NXDOMAIN`, in audit mode too | Nothing |
| Any name, asked of another address | A workload | No resolver on this query path | Nothing |

A proxy-aware client sends the name in its CONNECT, and the proxies route by that name, never by the address DNS returned. A CONNECT to a bare avirtedgeproxyd address lacks a name, and the proxy refuses it. [DNS](./dns.md) specifies avirtdnsd and the stub resolver. [Workload DNS](./egress.md#workload-dns) specifies the query path and its `resolve` check. See [egress policies](./egress-policies.md#name-resolution) for name-to-entity resolution.

## Connection types

Connections between two workloads require checks at three points: the source workload's avirtmediatord and the avirtedgeproxyd of each host it touches, plus the destination workload's avirtmediatord. Each point must permit it (see [enforcement points](./egress-policies.md#enforcement-points)).

| Type | Hops | PEPs | Identity on each leg |
| --- | --- | --- | --- |
| Workload to external | Agent, avirtnetd's forward proxy, the workload's HTTP/2 connection to its mediator, avirtmediatord, SNI routing, avirtedgeproxyd's egress listener, upstream | avirtmediatord: workload policy snapshot `resolve`, `connect`, `intercept` or `passthrough`, `request`. avirtedgeproxyd checks `connect` against the host policy snapshot and applies the `private-dest` check. | Agent to avirtnetd: none, loopback. avirtnetd to avirtmediatord: workload SVID and avirtmediatord's SVID. SNI routing: ciphertext. avirtmediatord to avirtedgeproxyd: avirtmediatord's SVID and `/service/avirtedgeproxyd`. Upstream uses webpki TLS. Under TLS interception, avirtmediatord opens its own TLS connection to the upstream inside the tunnel. TLS passthrough uses the agent's own TLS. |
| Workload to model API | Agent, avirtnetd's forward proxy, the workload's HTTP/2 connection to its mediator, avirtmediatord, SNI routing, avirtmodelgwd, avirtedgeproxyd's egress listener, upstream | avirtmediatord applies the checks in the preceding row. avirtmodelgwd: per-workload model rules and budgets. avirtedgeproxyd: host policy snapshot for avirtmodelgwd and the originating workload. | avirtmediatord to avirtmodelgwd: avirtmediatord's SVID and `/service/avirtmodelgwd`. avirtmodelgwd to avirtedgeproxyd: `/service/avirtmodelgwd` with the workload in `x-avirt-caller`. Upstream: avirtmodelgwd's TLS, with the injected credential. |
| Workload to same-host workload | Source workload's avirtmediatord, SNI routing, avirtedgeproxyd's egress listener, SNI routing through `services/ingress-relay.sock`, the destination workload's avirtmediatord's ingress listener, the `/inbound-notices` and `/ingress` HTTP/2 streams, avirtnetd, `127.0.0.1:<port>` | Source workload's avirtmediatord: `resolve` and `connect`. avirtedgeproxyd: host policy snapshot `connect`. Destination workload's avirtmediatord: `ingress` for the caller and port, and the publication's expiry. | The source uses the identities in the preceding row. avirtedgeproxyd to destination: a new mTLS connection as `/service/avirtedgeproxyd`, with the source identity in `x-avirt-caller`. avirtedgeproxyd treats the bytes as opaque. |
| Workload to peer workload | Source workload's avirtmediatord, this host's avirtedgeproxyd, the peer's listener on its host, the peer's avirtedgeproxyd, then as same-host on the peer | Source workload's avirtmediatord. This host's avirtedgeproxyd, against this host's host policy snapshot. The peer's avirtedgeproxyd, against its own host policy snapshot. The destination workload's avirtmediatord on the peer: `ingress`. | Between the two avirtedgeproxyd instances: `/service/avirtedgeproxyd` on both sides with federated bundles, and `x-avirt-caller` set to the originating workload in this host's trust domain, which the peer accepts only in the trust domain of the SVID that supplied it (see [peering](./peering.md#workload-traffic)) |
| Workload to host service | avirtmediatord, SNI routing by `<name>.svc.<td>`, `services/<name>.sock` | The component's own authorization of the caller's SVID, workload, and generation. The path bypasses avirtedgeproxyd. | avirtmediatord's SVID and `/service/<name>`. The agent connects to a host service only through a named HTTP/2 stream: `/dns` to avirtdnsd, `/mcp` to avirtmcpgwd, `/otlp` to avirtoteld, and the registry socket to avirtregistryd for a build job. |
| Workload to host-local service | Source workload's avirtmediatord, avirtedgeproxyd's egress listener, the backend address in a service record | avirtmediatord: `connect` to the `Service`. avirtedgeproxyd checks `connect` against the host policy snapshot. It dials only that record's backend, the one host address the `private-dest` check allows. | The source leg uses the preceding identities. avirtedgeproxyd to the backend: whatever the backend speaks from the host. |
| Inbound to a published port or service | A workload or avirtapid on this host, or a peer's avirtedgeproxyd on the peer listener, then avirtedgeproxyd, SNI routing through `services/ingress-relay.sock`, the destination workload's avirtmediatord's ingress listener, avirtnetd, `127.0.0.1:<port>` or `guestd` | avirtedgeproxyd: host policy snapshot `connect`, publication expiry. Destination workload's avirtmediatord: `ingress`. avirtnetd: only unexpired ports in its publication set. avirtguestd authorizes each operation for the `guestd` target. | `/service/avirtedgeproxyd` on the ingress listener, with the source in `x-avirt-caller` |
| avirt, avirtwebd, avirtguest to avirtapid | avirt over gRPC and avirtwebd over REST, on `edgeproxy.sock` in the runtime directory, avirtedgeproxyd's local Unix socket, then avirtedgeproxyd, avirtapid's local Unix socket, avirtapid, avirthostd. avirtguest: `guest.sock`, the `/guest-cli` HTTP/2 stream, avirtmediatord, SNI routing, avirtedgeproxyd, avirtapid, avirthostd. With `--host`, avirtapid forwards through avirtedgeproxyd to the peer's avirtapid. | avirtedgeproxyd requires the host policy snapshot to permit `connect` to `HostService::"avirtapid"`. avirthostd: the control-plane action, and for `guest-cli` the workload management constraints. A peer's avirtapid checks the peer's host policy snapshot and the enrollment's routes. | `edgeproxy.sock`: the socket's mode, as the local user, `via: "client"`. avirtguest: `SO_PEERCRED` on `guest.sock`, then the `…/workload/guest-cli` principal that avirtmediatord sets in `x-avirt-caller`. avirtedgeproxyd to avirtapid: `/service/avirtedgeproxyd` with `x-avirt-caller`. avirtapid to avirthostd: `/service/avirtapid`. |
| Web UI users to avirtwebd | A browser on this machine, avirtedgeproxyd's loopback listener on the host, avirtedgeproxyd, avirtwebd's local Unix socket, avirtwebd | avirtwebd: web UI user authentication (see [web UI](./components.md#web-ui)) | Browser to the host loopback port: none. avirtedgeproxyd to avirtwebd: `/service/avirtedgeproxyd`. |
| avirtadmin to avirthostd | `admin.sock` in the runtime directory, directly | avirthostd: host policy snapshot, as `Admin::"uid:<uid>"`, `via: "admin"` | Peer credentials (`LOCAL_PEERCRED` on macOS, `SO_PEERCRED` on Linux), avirthostd's own UID or root only. TLS and avirtedgeproxyd are absent from this path. |
| Host services off-host | The host service, an h2 CONNECT stream to avirtedgeproxyd's egress listener, upstream from the host network | avirtedgeproxyd checks `connect` against the host policy snapshot for the host service. For a call made on a caller's behalf, it also checks the originating principal. It applies the `private-dest` check | `/service/<name>`, with `x-avirt-caller` from avirtapid, avirtmodelgwd, and avirtmcpgwd only. Child processes connect to the network only through their parent daemon. Secret provider plugins use avirtsecretsd's local egress socket, and the Collector uses avirtoteld. zot never opens an off-host connection. |
| Build jobs | The build runner's registry socket, avirtmediatord, SNI routing by `avirtregistryd.svc.<td>`, avirtregistryd. Each other connection: avirtnetd's forward proxy, avirtmediatord, avirtedgeproxyd. | avirtregistryd: the job's grant (inputs, cache scopes, `staging/<build-id>`). avirtmediatord and avirtedgeproxyd: the job's workload policy snapshot and the host policy snapshot. avirthostd refuses a build job's `/guest-cli` stream. | The job's workload SVID only. BuildKit's containers use the build VM's loopback, so they reach only avirtnetd's forward proxy. |

See the specifications for each path. [how avirtmediatord handles egress](./egress.md#how-avirtmediatord-handles-egress) and [edge proxy egress](./egress.md#edge-proxy-egress) for egress, [ingress](./workload-topology.md#ingress) for inbound connections, [avirtedgeproxyd](./components.md#avirtedgeproxyd) for services and host-local services, [peering](./peering.md) and [federation](./identity.md#federation) for peers, [guest CLI path](./guest-cli.md#path), [operator CLI contexts](./operator-cli.md#contexts-and-authentication), [admin CLI connection](./admin-cli.md#connection-and-authentication), [provider plugins](./secrets.md#provider-plugin-interface), [avirtoteld](./observability.md#avirtoteld), and [build jobs](./workspace-build.md#build-jobs).

### Workload to external

```mermaid
sequenceDiagram
    participant A as agent
    participant N as avirtnetd
    participant E as avirtmediatord
    participant R as avirthostd SNI routing
    participant F as avirtedgeproxyd
    participant U as upstream
    A->>N: CONNECT host:443 on 127.0.0.1:PORT
    N->>E: h2 CONNECT stream on the HTTP/2 connection (mTLS)
    E->>E: authority, classification, resolve, connect, intercept or passthrough
    E-->>N: 200
    A->>E: TLS to a MITM CA leaf (intercept) or opaque bytes (passthrough)
    E->>R: outbound socket, SNI edgeproxy.svc.TD
    R->>F: ciphertext, forwarded unchanged
    E->>F: mTLS, h2 CONNECT host:443
    F->>F: caller SVID, host policy snapshot connect, resolve, `private-dest` check
    F->>U: dial checked addresses only
    E->>U: its own TLS connection inside the tunnel (intercept), or the agent's TLS (passthrough)
```

### Workload to workload

```mermaid
sequenceDiagram
    participant S as source workload's avirtmediatord
    participant F as avirtedgeproxyd
    participant P as peer avirtedgeproxyd
    participant D as destination workload's avirtmediatord
    participant N as destination avirtnetd
    S->>S: resolve name to entity, connect (workload policy snapshot)
    S->>F: h2 CONNECT name:port (SNI routing, mTLS)
    F->>F: resolve name to entity, connect (host policy snapshot)
    alt same host
        F->>D: SNI routing, mTLS as /service/avirtedgeproxyd, x-avirt-caller
    else peered host
        F->>P: peer listener, federated mTLS, x-avirt-caller in this trust domain
        P->>P: connect (peer's host policy snapshot)
        P->>D: ingress listener on the peer, x-avirt-caller
    end
    D->>D: ingress (workload policy snapshot), publication unexpired
    D->>N: inbound connection notice, then extended CONNECT /ingress
    N->>N: 127.0.0.1:PORT
```

## Transports by runtime

Each runtime presents the same Unix sockets inside the containers under `/run/avirt/`. The dialing side never changes TLS roles. Each runtime relay and each hop where avirthostd forwards bytes unchanged transmits ciphertext only, as does each shared volume.

| Socket | Today, Apple container CLI | Containerization | Docker Desktop |
| --- | --- | --- | --- |
| Agent egress | `proxy.sock`, a host socket file relayed by the runtime, plaintext CONNECT | An h2 CONNECT stream on the workload's HTTP/2 connection to its mediator | The same as Containerization |
| Log envelopes | `mediator.sock`, a host socket file, whose bytes avirtmediatord forwards unchanged to `hostd.sock` | The `/envelopes` HTTP/2 stream, which avirtmediatord forwards on the mediator's connection to avirthostd | The same as Containerization |
| Workload's HTTP/2 connection to its mediator | None | A vsock connection from the sandbox VM to avirthostd's one listener on that VM's device, whose bytes avirthostd forwards unchanged to the current mediator generation's vsock connection | A per-workload Docker named volume `avirt-<id8>-sock`, with avirtmediatord listening on `/run/avirt/workload.sock` and avirtnetd dialing it as `/run/avirt/mediator.sock`, inside the Docker Desktop VM |
| Mediator's connection to avirthostd | `hostd/hostd.sock` with the connection headers | vsock from the mediator VM | A single-file bind mount of `workloads/<id>/c.sock` |
| Mediator's outbound socket to avirthostd | None | vsock from the mediator VM to avirthostd's SNI routing | A single-file bind mount of `workloads/<id>/s.sock` |
| avirtmediatord's ingress listener | None | vsock from avirthostd into the mediator VM | Open (see [components](./components.md#open-questions)) |
| avirthostd's SNI routing to avirtedgeproxyd's egress listener | None | Host Unix socket | Host Unix socket |
| avirtedgeproxyd to host services and `services/ingress-relay.sock` | None | Host Unix sockets | Host Unix sockets |
| `edgeproxy.sock`, `edgeproxy.sock` | None | Host Unix socket | Host Unix socket |
| Web UI users | None | TCP listener on the host's loopback interface | TCP listener on the host's loopback interface |
| Peer listener (`peering.listen`, default 8788) | None | TCP listener on the host | TCP listener on the host |
| Host socket paths under the 104-byte limit | `proxy.sock`, `mediator.sock`, `hostd.sock` | None | `c.sock`, `s.sock` |

avirtedgeproxyd reaches host services over local Unix sockets. Each service accepts mutual TLS connections. Peer authentication checks the exact SVID against the enrolled trust domain's bundle and never authorizes by source address.

On every runtime, the `AF_VSOCK` denial applies to every workload process. Only the runtime's own relay uses the sandbox VM's vsock device. For the workload's HTTP/2 connection to its mediator, the protocol (TLS 1.3, then h2, with avirtnetd as client) is the same on each runtime. [Workload topology](./workload-topology.md#runtime-provided-sockets) specifies the sockets. [Compute runtimes](./compute-runtimes.md#unix-sockets) and [Docker Desktop](./compute-runtimes.md#docker-desktop-runtime) specify them per runtime. [Lifecycle](./lifecycle.md#path-length) specifies the path-length rule.

## Protocols

| Protocol | Behavior |
| --- | --- |
| CONNECT over h1 | The forward proxy's tunnel protocol. avirtnetd's forward proxy accepts `CONNECT host:port` and answers any other method with 501 (`plain-http` for an absolute `http://` URI, `unsupported-method` otherwise). |
| Extended CONNECT | `/ingress` uses RFC 8441 with `:protocol: connect-tcp` and a single-use ingress ID (see [ingress](./workload-topology.md#ingress)). |
| CONNECT over h2 | Each hop after avirtnetd uses one HTTP/2 stream per agent connection. Each connection from avirtmediatord, a gateway, or a host service to avirtedgeproxyd also uses one stream. `x-avirt-caller` is a header on the outer stream, never in the tunnelled bytes. |
| TLS on port 443 | Under TLS interception, when policy permits `intercept`, avirtmediatord is the agent's TLS endpoint using a MITM CA leaf, with agent-facing ALPN `http/1.1`, and opens its own TLS connection to the upstream with ALPN `h2` and `http/1.1`. Otherwise TLS passthrough, when policy permits `passthrough` (see [how avirtmediatord handles egress](./egress.md#how-avirtmediatord-handles-egress)). |
| TLS passthrough | avirtmediatord reads the first bytes before forwarding any. They must be a ClientHello whose SNI equals the canonical CONNECT host. An allowed host on a shared address can't supply another name. The proxy closes a connection that doesn't start with TLS unless the `passthrough` permit has `context.plaintext` true. Credential injection remains off for these connections. Each internal name takes TLS passthrough, authorized at avirtedgeproxyd. |
| gRPC | Under TLS passthrough, as bytes. Under TLS interception, the proxy offers agent-facing h2 only for a destination that one of the workload's gRPC bindings specifies (see [gRPC credentials](./egress-credentials.md#grpc)). |
| Websockets | An `Upgrade` request forces h1 upstream. The upgrade request passes the request checks and the `credential-misplaced` check, and the proxy relays frames without scanning after `101` (see [websockets](./egress.md#websockets)). |
| DNS | UDP and TCP on `127.0.0.53:53` inside the sandbox container, sent as one `POST /dns` HTTP/2 stream per query with `application/dns-message` bodies and their 16-stream limit (see [workload DNS](./egress.md#workload-dns)) |
| UDP, QUIC and HTTP/3 | Unsupported. UDP may leave the sandbox container only for DNS to the stub. A client that tries HTTP/3 falls back to TCP through the proxy. Transparent TCP and UDP are separate planned features with their own security design (see [transport scope](./egress.md#transport-scope)). |
| `git://`, `ssh://` and other clients that ignore the proxy variables | These clients need an explicit proxy adapter to use the CONNECT path. |
| IPv6 | The workload has no IPv6 route. A CONNECT may specify a bracketed IPv6 literal, which only a policy naming that `resource.ip` can permit. The `private-dest` check covers the IPv6 non-global ranges, and the proxy refuses IPv4-mapped and IPv4-compatible literals (see [external host canonicalization](./egress-policies.md#external-host-canonicalization)). avirtedgeproxyd dials IPv6 upstreams where the host has an IPv6 route. |

## Ports and listeners

Each listener's caller identity and allowed operations are in [trust boundaries](./trust-boundaries.md#listeners), and the sandbox container's listeners exempt from mutual TLS in [local exceptions](./trust-boundaries.md#local-exceptions). This inventory names where each one is.

| Placement | Listener | Transport |
| --- | --- | --- |
| Sandbox container | avirtnetd's forward proxy `127.0.0.1:PORT`, an ephemeral port | TCP, loopback |
| Sandbox container | DNS stub `127.0.0.53:53` | UDP and TCP, loopback |
| Sandbox container | Agent sockets `/run/avirt/agent/guest.sock` and `/run/avirt/agent/mcp.sock` | Unix, uid 1000 only |
| Sandbox container | Local service sockets `/run/avirt/streams/<role>.sock`, launcher sockets `/run/avirt/init/<role>.sock` | Unix, one per-process UID each |
| Sandbox container | Published ports, `127.0.0.1:<port>`, served by workload processes | TCP, loopback, reached only through avirtnetd's reverse proxy |
| Mediator | avirtmediatord: the workload's connection `/run/avirt/workload.sock`, the ingress listener `/run/avirt/ingress.sock` | Runtime-provided Unix socket, mTLS 1.3, h2 |
| Host | avirtedgeproxyd's egress listener | Unix, mTLS 1.3, h2. SNI routing for mediators and direct calls from host services |
| Host | avirthostd's end of the mediator's connection, one per workload | Runtime-provided Unix socket, mTLS 1.3, h2 |
| Host | avirthostd's SNI routing: each mediator's outbound socket, and `services/ingress-relay.sock` for avirtedgeproxyd | Runtime-provided Unix socket and Unix, TLS forwarded unchanged |
| Host | Host service listeners `services/<name>.sock` (0660 with a group), the issuer `services/issuer.sock` | Unix, mTLS 1.3 (the issuer accepts an enrollment token or a current SVID) |
| Host | avirtedgeproxyd's `edgeproxy.sock` `edgeproxy.sock`, and avirthostd's `admin.sock`, in the runtime directory | Unix, 0600, user-owned |
| Host | avirtedgeproxyd's loopback listener for web UI users | TCP, loopback |
| Host | The peer port, on the `peering.listen` addresses, default port 8788, and absent while `peering.listen` is empty | TCP on the host network |
| Host | zot, `127.0.0.1:<port>` | Loopback TCP, mTLS from avirtregistryd's private CA |
| Host | Workload stdio logs `logs/<workload-id>.stdio.ndjson` | Files, not a listener, read by avirtlogd |

The peer listener is the only avirt listener on a non-loopback host address.

## Restrictions on host connections

avirtedgeproxyd is the only host service that opens connections off the host. The host enforces this below the daemons' configuration, so a compromised or misconfigured daemon still can't reach the network directly. Workload traffic enters the host through the mediator's sockets.

### External resolution

avirtedgeproxyd resolves external names with its own resolver against the configured upstream servers. Its DNS cache honors answer time-to-live values. It resolves again when dialing and applies the private-address check to every resolved address, including on retries. It reads the host's interfaces with `getifaddrs` on each connection, refusing the connection if that read fails (see [edge proxy egress](./egress.md#edge-proxy-egress)). Other host services send names to avirtedgeproxyd rather than resolving them externally themselves.

### macOS host

A Network Extension content filter, distributed as a system extension in the avirt app bundle, enforces the rule on a direct macOS install.

- **Scope.** The filter evaluates each new non-loopback flow. It identifies the process that opened the flow from the flow's `sourceProcessAuditToken` and checks the process's code signature against the avirt <!-- Apple code signing term. --><!-- vale write-good.TooWordy = NO -->designated requirement<!-- vale write-good.TooWordy = YES -->. The filter allows flows from processes outside avirt's signing identity unchanged.
- **Rule.** The filter permits non-loopback flows from the signed avirtedgeproxyd binary and drops them from every other avirt-signed host process, including avirthostd, avirtapid, avirtsecretsd, gateways, and their children. The signature check must also match avirtedgeproxyd's signing identifier. Matching the shared signing identity alone never grants access. Loopback and Unix-socket connections remain available.
- **Identity.** The filter keys on the code signature, not the UID, so it holds when each host service runs as the user's own UID.
- **Fail closed.** avirthostd reads the filter's state from `NEFilterManager` at start and while it runs. While the filter isn't enabled and running, avirthostd refuses new launches with 503, and it records the state in the `start` meta event of each workload.
- **Install.** The system extension requires the content filter entitlement, a Developer ID signature, the app in `/Applications`, and the user's approval in System Settings or an MDM profile that pre-approves it.

### Linux and the all-in-one server image

In the all-in-one server image, avirtinitd in server mode installs an nftables ruleset in the container's network namespace before it starts any host service, and then drops `CAP_NET_ADMIN`.

- **Rule.** The ruleset uses an `inet` family output chain with policy `drop`. The chain accepts loopback traffic and established or related traffic. New flows require `meta skuid` equal to avirtedgeproxyd's UID. The chain drops all other traffic. The same chain covers TCP, UDP (including DNS) and IPv6.
- **Identity.** Each host service already runs under its own UID in the image (see [all-in-one server image](./components.md#all-in-one-server-image)), and a child process such as zot, the Collector or a provider plugin runs under its parent's UID or its own, never that of avirtedgeproxyd.
- **Fail closed.** If installing the ruleset fails, avirtinitd doesn't start any host service and the image exits.

## Failure behavior

| Event | Effect | Specified in |
| --- | --- | --- |
| The workload's HTTP/2 connection to its mediator fails with no replacement | avirtnetd reconnects. avirtinitd freezes the agent cgroup while it lacks an attached connection of the current or a higher generation, and ends every process in that cgroup after 30 s. Open egress streams end and clients retry. | [freeze and reconnect](./workload-topology.md#freeze-and-reconnect) |
| The mediator exits | avirthostd starts a new generation, at most twice per workload. The new generation's SVID supersedes the old one at each listener. | [mediator container](./components.md#mediator-container) |
| The mediator loses its connection to avirthostd | avirtmediatord exits, with the preceding effect | [connection to avirthostd](./workload-topology.md#connection-to-avirthostd) |
| avirthostd's SNI routing receives an unknown SNI or a destination outside the workload's set, or doesn't receive SNI | avirthostd closes the connection before forwarding a byte | [SNI routing](./workload-topology.md#sni-routing) |
| The proxy denies a destination | 400 `bad-request`, 403 (policy, `private-dest`, a host service authority, a name with no entity), 421 (`Host` mismatch), 501 (not CONNECT). A denied DNS name gets `NXDOMAIN`. | [how avirtmediatord handles egress](./egress.md#how-avirtmediatord-handles-egress), [edge proxy egress](./egress.md#edge-proxy-egress) |
| Upstream DNS or dial fails | 502, or 504 after the 10 s connect timeout | [edge proxy egress](./egress.md#edge-proxy-egress) |
| A proxy process has no valid policy snapshot | avirtmediatord isn't ready and no agent starts. avirtedgeproxyd refuses each stream with 503. An expired policy snapshot denies each new connection, request, and DNS query. | [policy failure behavior](./egress-policies.md#policy-failure-behavior) |
| A policy snapshot change or expiry | Each proxy process re-evaluates its open flows | [revocation of open flows](./egress-policies.md#revocation-of-open-flows) |
| A workload's event ring is full | Its proxy process refuses that workload's new connections, requests and DNS queries with 503 until the ring drains | [events](./egress.md#events-1) |
| Publication revocation or expiry | Its open ingress streams close | [ingress](./workload-topology.md#ingress) |
| The off-host filter isn't active | avirthostd refuses new launches with 503 | [restrictions on host connections](#restrictions-on-host-connections) |

## Observability

| Signal | Producer | Specified in |
| --- | --- | --- |
| `conn`, `http` and `dns` events in the workload log, with `src` `mediator`, `edgeproxy` or `gateway` | avirtmediatord, avirtedgeproxyd, the gateways | [events](./egress.md#events-1), [the event's `src` field](./trust-boundaries.md#src) |
| `CONNECT` and per-request spans | avirtmediatord, avirtedgeproxyd, the gateways | [egress spans](./observability.md#egress-spans) |
| Trace context across hops, never to external endpoints and only `traceparent` to peers | Each hop | [propagation](./observability.md#propagation), [external endpoints](./observability.md#external-endpoints) |
| Connection and egress metrics | avirtnetd, the mediator, the proxies | [workload connection](./performance.md#workload-connection-to-the-mediator), [egress](./performance.md#egress), and [metrics from avirtmediatord and avirtedgeproxyd](./observability.md#metrics-from-avirtmediatord-and-avirtedgeproxyd) |
| Alerts, such as `EventRingRefusing` and `PolicySnapshotExpiring` | Grafana rules | [alerts](./observability.md#alerts) |

The bypass checks that prove the workload has no other way out are in the [regression suite](./roadmap.md#regression-suite).

# Open questions

1. **Development builds on macOS.** An ad-hoc signed development build can't have the content filter entitlement. How development enforces the restriction that only avirtedgeproxyd connects off the host remains open.
2. **Host-local backends and avirt's own listeners.** Service records need a rule for ports that avirt components listen on, such as zot's loopback port.
3. **Content filter prototype.** Verify that the rule based on the signature and signing identifier of the executable allows avirtedgeproxyd and drops non-loopback flows from every other avirt-signed process, including one running under the same UID. Its signing identifier must remain stable across updates.
