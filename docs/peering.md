---
type: Security Design
title: Peering
description: How two avirt hosts peer, with peer host names as fully qualified DNS names, out-of-band enrollment with enrollment files and fingerprints, trust bundle exchange, root rotation, revocation and removal, discovery and transport over avirtedgeproxyd's peer listener, what crosses a peering and what never does, policy on both sides with cross-host delegation chain and the keepalive that bounds a workload created from a peer, pairwise topology, failure modes, observability and audit, and the avirtadmin and avirt commands.
tags: [avirt, peering, federation, spiffe, edge-proxy, policy, security]
implementation: planned
generated: { by: claude-code/claude-opus-5-5, at: 2026-10-17T23:30:00Z }
---

# Status: planned

The current code has no peering. Nothing in this document is implemented.

A **peering** joins two avirt hosts so that workloads on one reach workloads and services on the other, and so that callers on one manage workloads on the other. Each host is its own trust domain, `<install-id>.avirt` (see [identity](./identity.md#trust-domain-and-ids)), and the two never share a CA key. avirtedgeproxyd is the only component that opens a connection off the host, and it is the policy enforcement point for every connection between hosts: each side's avirtedgeproxyd evaluates its own host policy snapshot, and neither side trusts the other's decision. Federated identity, the peer listener's caller rules and the trust boundaries are in [identity](./identity.md#federation) and [trust boundaries](./trust-boundaries.md#listeners). This document specifies the rest.

## Peer host names

A peer is named by its **peer host name**, `<host>`: a fully qualified DNS name of the peer, such as `mac-studio.example.com` or the tailnet name `studio.tail1234.ts.net`. It is the peer's real host name, not a name avirt assigns, and it is recorded at enrollment from the peer's enrollment file.

- **Own host names.** Each host lists its own fully qualified names in the host setting `peering.hosts`, which defaults to the machine's fully qualified host name when it has at least two labels. Its enrollment file carries them, and avirtedgeproxyd's SVID carries each of them as a DNS SAN beside its SPIFFE ID (see [peer listener](#peer-listener)).
- **Syntax.** `<host>` is lowercase ASCII, at least two labels, each label 1 to 63 characters of `a`-`z`, `0`-`9` and `-`, not starting or ending with `-`, with no trailing dot. An internationalized name is given in its A-label (`xn--`) form. `<host>` is at most 116 characters, the cap in [DNS](./dns.md#avirt-names), so the longest peered name, `<alias>-<id>.<ns>.workload.<host>`, is at most 137 + 116 = 253 characters.
- **Name forms.** A peer workload is `<id>.<ns>.workload.<host>`, `<alias>-<id>.<ns>.workload.<host>` or `<generated>.<ns>.workload.<host>`, and a peer service is `<name>.<ns>.svc.<host>` or `<alias>.<ns>.svc.<host>`. Peered names have no short form without `<ns>`.
- **Parsing.** In a name with at least four labels, `<host>` is everything after the third label. The name is a peered name only if its third label is `workload` or `svc`, its first two labels are valid for that form, and `<host>` is the peer host name of an `active` peer (see [enrollment record](#enrollment-record)). Any other name, including the peer host name itself, any other name under it, a name under a peer that is not `active`, and a name under a host that is not enrolled, is not an avirt name: it resolves normally and is classified as an external endpoint.
- **Matching.** The peer's avirtedgeproxyd must present `<host>` as a DNS SAN on every connection, in addition to `spiffe://<peer td>/service/avirtedgeproxyd`. A connection whose certificate lacks it is refused.
- **Qualification.** Cedar entities, `avirt` contexts and `--host` use the full `<host>`, with no alias: `Peer::"mac-studio.example.com"`, `Namespace::"mac-studio.example.com/team-a"`, `Workload::"mac-studio.example.com/team-a/<uuid>"`.
- **One name per enrollment.** A peer whose enrollment file lists several names is enrolled under one of them, the first unless `--host` chooses another from the file's list. A name not in the file is refused.
- **Uniqueness.** One enrollment exists per peer trust domain and per peer host name. Enrolling a trust domain that is already enrolled, under any name, is refused, and so is enrolling a name already in use, a name in this host's own `peering.hosts`, a name under `avirt.internal`, and this host's own trust domain.
- **Name changes.** The peer host name is fixed for the life of the enrollment. A peer whose name changes is enrolled again: `avirtadmin peer revoke` and then `peer enroll` from a new enrollment file. `peer set` never changes it.
- **Policy references.** Enrolling a name that the current policy sets already reference requires `--accept-policy-references`, and avirtadmin lists the referencing policy `@id`s before it asks.

## Enrollment

Enrollment is mutual and out of band. Each host's operator exports an enrollment file, the operators exchange files, and each operator enrolls the other's file on their own host after comparing its fingerprint by a means they already trust, such as in person or on a call. A peering is active only once both hosts have enrolled each other.

### Enrollment file

`avirtadmin peer export > host.json` writes this host's enrollment file, a JSON object with `version` set to 1 and these fields:

| Field | Value |
| --- | --- |
| `trust_domain` | This host's trust domain |
| `hosts` | This host's fully qualified names, from `peering.hosts` |
| `addresses` | The peer listener's addresses, in preference order, from the host configuration and `--address` (see [addresses](#addresses)) |
| `bundle` | This host's SPIFFE bundle in the JWKS format of [bundle endpoint](#bundle-endpoint), holding every root currently in the bundle |
| `created` | The time the file was written |

The enrollment file holds no private key and no secret. The fingerprint covers `trust_domain` and `bundle`. The peer host name is not covered by the fingerprint: it is checked on every connection against the DNS SANs of a certificate that chains to the fingerprinted bundle. `addresses` are hints the enrolling operator can override.

### Fingerprint

The fingerprint is SHA-256 over the trust domain's bytes, a NUL byte, and the DER of every root certificate in `bundle`, sorted bytewise and concatenated. avirtadmin shows it as the first 20 characters of its lowercase base32 encoding in five groups of four, such as `k3qz-7f2m-a9xe-p4wd-n6tc`, and the full digest with `--long`. `avirtadmin peer fingerprint` shows this host's own fingerprint. A root rotation changes the fingerprint, and a peer that is already enrolled follows it through bundle refresh, never through a new comparison.

### Enrolling

`avirtadmin peer enroll host.json --fingerprint FP` enrolls a peer from its enrollment file, given as a path or as `-` for standard input:

1. It parses the file and refuses one whose `version` it does not support, whose `created` is more than 7 days old, whose bundle holds no unexpired root, or whose roots are not name-constrained to the file's trust domain.
2. It shows the trust domain, the file's host names, the peer host name it will record, and the fingerprint. With `--fingerprint FP` it compares them and refuses on a mismatch. Without it, it asks the operator to confirm on a terminal, and without a terminal it refuses.
3. avirthostd validates the name and the trust domain against [peer host names](#peer-host-names), evaluates `enroll_peer`, writes the enrollment record to `peers/` in its state directory (see [lifecycle](./lifecycle.md#durable-state)), and adds the `Peer` entity to its next entity revision.
4. avirtedgeproxyd serves the peer's bundle to its Envoy over local SDS, and its peer host name reaches avirtdnsd, which answers its peered names once the record is `active`. The record's state is `pending`.
5. avirthostd fetches the peer's bundle through avirtedgeproxyd at once, and then on the refresh schedule. The first fetch that completes mutual TLS and returns a valid bundle moves the record to `active`. Until the peer has enrolled this host too, the peer refuses the handshake and the record stays `pending`.

The enrollment file's bundle is the bootstrap bundle: the first connection to the peer authenticates it with that bundle, and every later connection with the most recently fetched one.

```mermaid
sequenceDiagram
    participant OA as operator A
    participant A as host A (avirtadmin, avirthostd, avirtedgeproxyd)
    participant B as host B (avirtadmin, avirthostd, avirtedgeproxyd)
    participant OB as operator B
    OA->>A: avirtadmin peer export > a.json
    A-->>OA: a.json (trust domain, host names, addresses, bundle)
    OB->>B: avirtadmin peer export > b.json
    B-->>OB: b.json
    OA-)OB: exchange the files out of band
    OA->>OB: compare both fingerprints by a trusted means
    OA->>A: avirtadmin peer enroll b.json --fingerprint FP-B
    A->>A: enroll_peer, record pending, bundle B to SDS, host name to avirtdnsd
    A->>B: mTLS to the peer listener as A's avirtedgeproxyd, GET bundle
    B-->>A: handshake refused (A not enrolled at B)
    OB->>B: avirtadmin peer enroll a.json --fingerprint FP-A
    B->>B: enroll_peer, record pending, bundle A to SDS
    B->>A: mTLS as B's avirtedgeproxyd, GET bundle
    A-->>B: bundle A (sequence, refresh hint)
    B->>B: record active
    A->>B: next refresh: mTLS, GET bundle
    B-->>A: bundle B
    A->>A: record active
```

### Enrollment record

| Field | Value |
| --- | --- |
| `host` | The peer host name, a fully qualified DNS name |
| `trust_domain` | The peer's trust domain |
| `addresses` | The configured addresses, and those learned by discovery (see [addresses](#addresses)) |
| `bundle`, `sequence` | The peer's current bundle and its `spiffe_sequence` |
| `distrusted` | Fingerprints of the root certificates that this host never accepts for the peer again (see [revocation and removal](#revocation-and-removal)) |
| `routes` | The avirtapid services and RPCs that the peer's callers may use here (see [control-plane calls](#control-plane-calls)) |
| `state` | `pending`, `active` or `suspended` |
| `reachable` | False after three consecutive failed dials or refreshes, true again after the next success |
| `enrolled`, `last_handshake`, `last_refresh`, `last_error` | Times and the last failure reason |

| State | Meaning | Peer listener | Peered names |
| --- | --- | --- | --- |
| `pending` | Enrolled here, no mutual handshake yet | Accepts the peer | Not avirt names: they resolve normally. Bundle fetches are attempted, and refused by the peer until it enrolls this host. |
| `active` | A mutual handshake and a bundle fetch have succeeded | Accepts the peer | avirt names, routed to the peer, whether `reachable` is true or not |
| `suspended` | `avirtadmin peer suspend` | Refuses the peer, and its open connections are closed | Not avirt names: they resolve normally |

## Trust bundles

### Bundle endpoint

avirtedgeproxyd serves this host's bundle on its peer listener at `GET /avirt/peer/v1/bundle`, to enrolled peers only, over the same mutual TLS as every other peer route. The body is a SPIFFE trust bundle in the JWKS format of the SPIFFE federation standard: one key per root certificate with `use` set to `x509-svid` and the certificate in `x5c`, plus `spiffe_sequence` and `spiffe_refresh_hint`. `spiffe_sequence` increases with every change to the bundle and is never reused. `spiffe_refresh_hint` is 300 seconds.

Constraint: the endpoint requires client authentication, so it is not a conformant SPIFFE `https_spiffe` bundle endpoint, and a SPIFFE implementation other than avirt cannot fetch it.

### Refresh

avirthostd refreshes each enrolled peer's bundle every `spiffe_refresh_hint` seconds of the last bundle it accepted, and immediately after enrollment. The request goes through avirtedgeproxyd's egress listener as avirthostd's own connection, and avirtedgeproxyd opens it to the peer as `/service/avirtedgeproxyd`. avirthostd accepts the fetched bundle only if:

- the server presented `spiffe://<peer td>/service/avirtedgeproxyd`, chaining to the peer's current bundle;
- its `spiffe_sequence` is higher than the current one;
- it holds at least one unexpired root, every root is name-constrained to the peer's trust domain, and no root is in the record's `distrusted` set.

A bundle that fails any check is rejected whole, recorded in `last_error`, and the current bundle stays. An accepted bundle replaces the current one atomically, and avirtedgeproxyd serves it to its Envoy over local SDS for every new handshake. A refresh failure leaves the current bundle in force and the next attempt waits for the next interval, with no faster retry.

### Root and issuing CA rotation

- **Issuing CA.** An issuing CA rotation changes nothing a peer holds. Every SVID carries its full chain, and peers trust only roots.
- **Root.** avirthostd adds a new root to its bundle 30 days before the current root expires (see [issuer](./identity.md#issuer)), and the bundle's `spiffe_sequence` increases. avirthostd signs no issuing CA with the new root until every `active` peer has fetched a bundle that holds it, as avirtedgeproxyd observes from the authenticated fetches on its peer listener, or until 7 days have passed. The old root leaves the bundle when it expires.
- **Unplanned root rotation.** `avirtadmin ca rotate --root` follows the same rules.

A peer that fetches no bundle during the whole 30-day overlap holds only the old root once it expires, and the peering then fails authentication in both directions until it is enrolled again (see [failure modes](#failure-modes)).

### Revocation and removal

| Command | Effect |
| --- | --- |
| `avirtadmin peer suspend HOST` | avirtedgeproxyd stops accepting the peer and closes every open connection authenticated by its trust domain, at once, ending every stream on them. The peer's peered names are no longer avirt names, so nothing is dialed to the peer. The record, its bundle and its policy references stay. |
| `avirtadmin peer resume HOST` | Returns the record to `pending` and refreshes at once. |
| `avirtadmin peer distrust-root HOST FINGERPRINT` | Removes the root with that fingerprint (the SHA-256 of its DER, in the same encoding as the peer fingerprint) from the peer's bundle and adds it to `distrusted`, so no later refresh can return it. Connections whose peer chain uses that root are closed. A bundle left with no root suspends the peer. |
| `avirtadmin peer revoke HOST` | Removes the enrollment: the bundle leaves SDS, every connection with the peer is closed, the `Peer` entity leaves the next entity revision, and the peer's peered names are no longer avirt names and resolve normally. Every workload created here from that peer is stopped, as when its creator stops sending keepalives (see [workload management across hosts](#workload-management-across-hosts)). |

Closing connections is libavirtenvoy's job: a validation context removed through SDS stops new handshakes only, so libavirtenvoy in avirtedgeproxyd's Envoy closes each existing connection whose peer trust domain is no longer trusted, or whose chain uses a distrusted root. Revocation is local and immediate on the host that runs it. The other host sees refused handshakes from then on, and its record's `reachable` becomes false.

A compromised peer root is handled by the peer's operator rotating the root and by this host's operator running `distrust-root` for the compromised one, because a refresh is authenticated by the bundle it replaces.

## Discovery and transport

### Addresses

An enrollment record holds one or more addresses for the peer listener, each a `host:port` with a DNS name or an IP literal. The first is always `<host>:8788`, the peer host name at the default port, unless `--address` at enrollment or `peer set` replaces it. avirtedgeproxyd dials them in order and uses the first that completes the handshake with the peer's exact SPIFFE ID. Discovery only supplies addresses: no address source authenticates a peer, and no address is trusted for anything but dialing.

| Source | How |
| --- | --- |
| Static | The peer host name, the enrollment file's `addresses`, and `avirtadmin peer set HOST --address ADDR`, repeatable. A DNS name is resolved by avirtedgeproxyd at each dial, through the host's resolver and never through avirtdnsd. |
| mDNS and DNS-SD | Off by default. With `peering.mdns = true` in the host configuration, avirthostd announces `_avirt-peer._tcp.local` with the peer listener's port and a TXT record `tdh=` holding the first 16 bytes of SHA-256 over the trust domain, base32. It browses for the same type and adds a discovered address to the record whose trust domain hashes to that value, after the addresses configured statically. A discovered address expires with its mDNS TTL. A peer host name under `local`, such as `mac-studio.local`, is itself resolved by mDNS. `avirtadmin peer discover` lists announcements on the LAN, for enrollment. |
| Tailnet | A tailnet such as Tailscale is a network like any other. A peer's MagicDNS name, such as `studio.tail1234.ts.net`, can be its peer host name, a tailnet address is a static address, and the peer listener binds to the tailnet interface through `peering.listen`. The tailnet's own node identity and ACLs are not avirt identities: the mutual TLS handshake still authenticates the peer. |

Constraint: avirtedgeproxyd's private-address block refuses non-global and host addresses for workload egress (see [edge proxy egress](./egress.md#edge-proxy-egress)). A connection to the peer listener of an enrolled peer is not workload egress: it may dial a private, CGNAT or link-local address, but only an address in that peer's record or one that a DNS name in the record resolves to, and never an address assigned to one of this host's own interfaces.

avirt does no NAT traversal. Each direction of a peering needs the dialing host to reach the other host's peer listener: through a public address, a port forward, a VPN or a tailnet. A host that can reach its peer but cannot be reached by it can send streams but not receive them, and its peer cannot refresh its bundle.

### Peer listener

| Item | Value |
| --- | --- |
| Bind | `peering.listen`, a list of `address:port`, empty by default. With it empty, the host has no peer listener and can only dial out. The default port is 8788. In the all-in-one server image, this is the one port the container publishes for peering. |
| Transport | TCP, TLS 1.3 only, mutual, ALPN `h2` only |
| Server identity | `spiffe://<td>/service/avirtedgeproxyd`, with every name in `peering.hosts` as a DNS SAN. A dialing peer requires the URI SAN and the DNS SAN of the peer host name it recorded. |
| Client identity | Exactly `spiffe://<peer td>/service/avirtedgeproxyd` of a peer whose state is not `suspended`, verified against that trust domain's own bundle only, never against another peer's bundle or this host's |
| Validity | A peer certificate's `not_before` may be up to 5 minutes after this host's clock. `not_after` has no tolerance. |
| Routes | `CONNECT` to local published ports and services, the gRPC services in the peer's `routes`, and `GET /avirt/peer/v1/bundle` |

Every stream on the peer listener carries exactly one `x-avirt-caller`, the originating principal's SPIFFE ID, which must be in the trust domain of the connection's peer. A missing, repeated, unparseable or foreign one is refused with 403 (see [workload topology](./workload-topology.md#ingress)). avirtedgeproxyd maps it to a Cedar principal and to `via`:

| `x-avirt-caller` | Principal | `via` | Allowed on |
| --- | --- | --- | --- |
| `spiffe://<peer td>/ns/<ns>/workload/<uuid>` | `Workload::"<host>/<ns>/<uuid>"` | `egress` | `CONNECT` |
| `spiffe://<peer td>/ns/<ns>/workload/<uuid>/guest-cli` | `Workload::"<host>/<ns>/<uuid>"` | `guest-cli` | Routes |
| `spiffe://<peer td>/service/avirthostd` | `HostService::"<host>/avirthostd"` | `service` | `Workloads/KeepAlive` only |
| A peer's local or web UI user | Open (see [open questions](#open-questions)) | `client` | Routes |

Any other identity is refused with 403. Generation fencing does not apply to peer identities, because this host has no record of a peer's generations: the source avirtedgeproxyd fences its own callers before it forwards.

### Peer connections

Each host's avirtedgeproxyd keeps a pool of h2 connections to each peer it dials, and multiplexes every stream bound for that peer onto them, whoever the caller is. A connection is authenticated as the two hosts' avirtedgeproxyd only, and each stream carries its own caller.

- **Workload streams.** h2 `CONNECT`, carrying opaque bytes. The source avirtedgeproxyd rewrites `:authority` from the peered form to the peer's own local form, so `7e759259.team-a.workload.mac-studio.example.com:8080` reaches the peer as `7e759259.team-a.workload.host.avirt.internal:8080`.
- **Control-plane calls.** gRPC over h2, for the routes in [control-plane calls](#control-plane-calls).
- **Bundle fetches.** `GET /avirt/peer/v1/bundle`.
- **Headers.** Each stream carries `x-avirt-caller`, `x-request-id` (a UUIDv7 that the source avirtedgeproxyd draws per stream, used only to correlate both hosts' events), `traceparent` unless the host configuration turns it off (see [propagation](./observability.md#propagation)), and for a workload management call `x-avirt-delegation` (see [workload management across hosts](#workload-management-across-hosts)). The source avirtedgeproxyd removes every other `x-avirt-*` header.
- **Liveness.** h2 `PING` every 30 s. A connection with two unanswered pings, or a transport error, is dead, and every stream on it ends.
- **Limits.** Host settings bound the connections per peer, the concurrent streams per peer, and the concurrent streams per caller identity from a peer. A stream past a limit is refused with 503.
- **Protocols.** HTTP/3 and QUIC are not used on the peer listener.

## What crosses a peering

| Crosses | How | Authorized by |
| --- | --- | --- |
| Workload-to-workload connections to published ports | h2 `CONNECT`, opaque bytes | Source avirtmedproxyd, source avirtedgeproxyd, destination avirtedgeproxyd, destination avirtmedproxyd `ingress` |
| Connections to services | h2 `CONNECT`, opaque bytes | The same, without `ingress` for a service with a host-local backend |
| Control-plane calls | gRPC from avirtapid to the peer's avirtapid | The source avirtedgeproxyd's `connect`, the peer's enrollment routes, and the peer's own action |
| Workspace images | Explicit imports, through the peer's avirtapid | `import_workspace` here, `pull_workspace` at the peer |
| Trust bundles | `GET /avirt/peer/v1/bundle` | Enrollment |
| Trace context | `traceparent` only | Host configuration |

These never cross a peering:

- **Secrets.** Credential values, placeholders, bindings and binding references, provider configuration and avirtsecretsd records. A workload created on a peer gets only the peer's own bindings, under the peer's policy.
- **Keys.** Any private key, the SPIFFE CA, the MITM CAs, and every SVID other than the two hosts' avirtedgeproxyd SVIDs.
- **Policy.** Policy sets, policy snapshots, policy invariants and decisions. Neither host learns the other's policy, and neither host's decision binds the other.
- **Audit and logs.** Audit logs, workload logs and audit records stay on the host that wrote them. A peer reads a workload's log only through `Logs/ReadLog` on its routes, under the peer's own `read_log` policy, which is a control-plane call and not audit delivery.
- **DNS queries.** Every peered name is answered locally (see [DNS](#dns)).
- **Topology.** A peer never learns this host's other peers: `Peers/*` is never a peer route, and no stream is forwarded to a third host.

### Workload traffic

A workload reaches a peer's workload or service by its peered name:

1. The source avirtmedproxyd resolves the peered name to `Namespace::"<host>/<ns>"` (see [name resolution](./egress-policies.md#name-resolution)), evaluates its workload policy snapshot's `resolve` and `connect`, and sends the stream to its own avirtedgeproxyd through avirthostd's SNI routing, as it does for every avirt name.
2. The source avirtedgeproxyd authorizes the SVID of avirtmedproxyd's Envoy with generation fencing, evaluates the host policy snapshot's `connect` for the source `Workload` and `Namespace::"<host>/<ns>"`, rewrites `:authority`, and opens the stream on a connection to the peer, with `x-avirt-caller` set to the source workload's SPIFFE ID.
3. The destination avirtedgeproxyd checks the caller's trust domain, resolves the local name to its own entity, evaluates its host policy snapshot's `connect` for `Workload::"<source host>/<ns>/<uuid>"`, and connects to the destination avirtmedproxyd's ingress listener, or to a host-local backend, as for a local caller (see [edge proxy egress](./egress.md#edge-proxy-egress)).
4. The destination avirtmedproxyd evaluates `ingress` for the peer `Workload` in `x-avirt-caller`. The workload policy snapshot carries the enrolled peers' trust domains and names, so it maps the caller as avirtedgeproxyd does.

A refusal from the peer reaches the source avirtmedproxyd with the peer's status, and the source avirtedgeproxyd's event records it as `peer_status`.

```mermaid
flowchart LR
    subgraph A["host A (trust domain A)"]
        wa["workload (avirtnetd)"] -->|"HTTP/2 connection"| mea["avirtmedproxyd: workload policy snapshot resolve, connect"]
        mea -->|"SNI routing"| fda["avirtedgeproxyd: host policy snapshot connect to Namespace B/ns"]
        pxa["avirtapid"] -->|"gRPC, x-avirt-caller, x-avirt-delegation"| fda
        hda["avirthostd"] -->|"bundle refresh, keepalives"| fda
    end
    subgraph B["host B (trust domain B)"]
        fdb["avirtedgeproxyd peer listener: caller in trust domain A, host policy snapshot connect"]
        meb["avirtmedproxyd: ingress for Workload A/ns/uuid"]
        wb["destination workload container"]
        pxb["avirtapid"]
        hdb["avirthostd: actions for the caller, qualified by its peer host name"]
        fdb -->|"ingress listener"| meb --> wb
        fdb -->|"routes in the enrollment"| pxb --> hdb
    end
    fda ==>|"mTLS 1.3, h2, A edge proxy to B edge proxy"| fdb
```

### Control-plane calls

`avirt --host <host>`, a context with `host` set (see [operator CLI](./operator-cli.md#contexts-and-authentication)), and avirtguest with `--host` all send their call to the local avirtapid, which forwards it through the local avirtedgeproxyd to the peer's avirtapid.

1. The local avirtedgeproxyd evaluates the host policy snapshot's `connect` for avirtapid and for the originating principal, with `Peer::"<host>"` as the resource, `port` 0 and the originating `via`.
2. The peer's avirtedgeproxyd accepts the call only if its gRPC service or RPC is in the enrollment's `routes`, and refuses any other with 403 before it reaches avirtapid.
3. The peer's avirtapid forwards it to the serving component, which evaluates the action against the peer's host policy snapshot, with the caller as principal, qualified by its peer host name (see [entities](#entities)).

`routes` lists avirtapid services (`Workloads`) or single RPCs (`Workloads/GetWorkload`), and is empty at enrollment. `Peers/*` cannot be listed. The peer's avirtapid never forwards a peer caller's call to another peer.

### Workspace images

A workspace reference resolves only in the local avirtregistryd. Resolution never contacts a peer, and no selector (`github:`, `git:` or a repository tag) matches a peer's image. A peer's image enters this host only by an explicit import:

```text
avirt workspace import peer:<host>/<ns>/<name>[:<tag>|@<digest>] [REPOSITORY[:TAG]]
```

1. avirtregistryd evaluates `import_workspace` for the caller, with `registry` set to `peer:<host>`.
2. It calls the peer's `WorkspaceRegistry` through avirtedgeproxyd, carrying the caller in `x-avirt-caller`. The peer evaluates `pull_workspace` with `purpose` set to `peer-import`, for the caller, qualified by its peer host name.
3. It pulls the manifest, the config and every layer, verifying every digest against the manifest, and stores the image with producer `import`, its source recorded as `peer:<host>/<repository>@<digest>`, and the peer's own producer recorded as `peer_producer`.

`peer_producer` carries only the peer's assertion, and `launch_workspace` policies can read it. A launch on a peer with `--host` resolves the reference at the peer, in the peer's registry, under the peer's policy.

### DNS

avirtdnsd answers a name only when it is a peered name of an `active` peer (see [parsing](#peer-host-names)): `*.<ns>.workload.<host>` or `*.<ns>.svc.<host>`, with a first label valid for the form. The answer is the address the peer's first address resolves to. avirtdnsd answers nothing else under `<host>`: the peer host name itself, every other name under it, and every peered form of a peer that is `pending` or `suspended` resolve normally, as public names through avirtedgeproxyd's DNS proxy (see [workload DNS](./egress.md#workload-dns)), and a connection to one is an external endpoint under external policy. No DNS query for a peered name crosses a peering: the peer's avirtedgeproxyd resolves its own local names, and avirtmedproxyd routes by name, never by the address in the answer (see [avirt names](./dns.md#avirt-names)).

## Policy

### Entities

A `Peer` entity represents an enrolled peer, and a peer's namespaces are in it, so `principal in Peer::"<host>"` matches every identity from that peer and `resource in Peer::"<host>"` every peered name under that peer host name.

| Entity | Id | Parents | Attributes | Source |
| --- | --- | --- | --- | --- |
| `Peer` | `<host>` | None | `trust_domain` | avirthostd's entity revision, from the enrollment records |
| `Namespace` (peer) | `<host>/<ns>` | Its `Peer` | None | Built at evaluation from the peered name or the asserted caller |
| `Workload` (peer) | `<host>/<ns>/<uuid>` | Its peer `Namespace` | None, and no tags | Built at evaluation from the asserted caller |
| `HostService` (peer) | `<host>/<name>` | None | None | Built at evaluation from the asserted caller |

The `Peer` entity, `Namespace in [Peer]`, `Peer` as a `connect` resource, the actions `enroll_peer`, `update_peer`, `revoke_peer`, `list_peers` and `inspect_peer`, and the `pull_workspace` purpose `peer-import` are in the [schema](./egress-policies.md#schema-text).

### Evaluation on each side

| Point | Host | Principal | Resource | Sees |
| --- | --- | --- | --- | --- |
| Source avirtmedproxyd | Source | The source `Workload`, with its attributes and tags | `Namespace::"<host>/<ns>"` | The peer and namespace in the name, the port. Nothing below the namespace. |
| Source avirtedgeproxyd | Source | The source `Workload`, or avirtapid and the originating principal | `Namespace::"<host>/<ns>"`, or `Peer::"<host>"` for a control-plane call | The same, plus `via` |
| Destination avirtedgeproxyd | Destination | `Workload::"<source host>/<ns>/<uuid>"`, with no attributes and no tags | The local `Workload` or `Service`, with attributes and tags | The source's peer, namespace and UUIDv7, and nothing else about it |
| Destination avirtmedproxyd | Destination | The same peer `Workload` | The local `Workload` | The same, plus `port` and `protocol` |
| Destination avirthostd and the other serving components | Destination | The caller, qualified by its peer host name | The local resource | The caller, the asserted delegation chain (see below), `via` |

Each host evaluates in its own mode: the source applies its namespace modes to its own workloads, and the destination applies its host default to every peer caller (see [modes](./egress-policies.md#modes)).

A peer asserts identities in its own trust domain only, and this host cannot verify anything about them beyond that. A policy that grants something to `Peer::"<host>"` or to a peer namespace grants it to whatever that peer chooses to assert, including after a compromise of that peer.

### Workload management across hosts

A caller on host A manages workloads on host B only through B's avirtapid, under B's policy. The constraints in [workload management](./components.md#workload-management) hold on B as follows:

- **No greater authority.** B never learns what the creator holds on A. A peer creator can create a workload only in a namespace where B's policy permits it to, can attach only the bindings B's policy permits that peer principal to attach (`attach_binding`), and can assign only a policy reference that B's policy names as assignable by it. A peer principal holds no binding of its own on B.
- **Reserved labels.** A peer creator never sets a label under `avirt.internal/`.
- **Delegation chain.** A's avirtapid sends `x-avirt-delegation: root=<principal>; depth=<n>` with every workload management call from a workload, with the root and depth that A's avirthostd recorded for the caller. B records the child's `avirt.internal/creator` as the fully qualified creator, `<host>/<ns>/<uuid>`, and `avirt.internal/root` as the asserted root qualified by the peer, `<host>/<root>`. The child's depth is the asserted depth plus one, `context.depth` in `create_workload` is that value, and B's own depth cap applies to it. The delegation chain is A's assertion and carries A's trust only.
- **Lifetime.** A workload created on B by a workload on A stops unless A keeps confirming that its creator is alive. A's avirthostd records the child from the create response and, while the creator lives, calls `Workloads/KeepAlive` on B every 60 s for it, as `/service/avirthostd`. B accepts the call only from the creator's trust domain and only for workloads that peer created. B stops the child when A stops it, when no keepalive has arrived for 5 minutes, or when the peer is revoked. A child that B's policy permits to be detached needs no keepalive.
- **No self-escalation.** As on a single host.
- **Host scope.** B's avirtapid never forwards a peer caller's call to another host, so a chain of hosts is always a chain of separate calls, each authorized on the host it reaches.
- **No host administration.** Peer principals never reach avirtadmin's actions, and `enroll_peer`, `update_peer` and `revoke_peer` take only `Admin`.
- **Audit.** Each host records the call in its own host audit log: A the forwarded call with the caller and the peer, B the call with the caller qualified by its peer host name, the action, the resource and the asserted delegation chain.

### Examples

On host A, `laptop.tail1234.ts.net`, workloads in `team-a` may reach `team-a` on the peer `mac-studio.example.com`, and its `ci` workloads may manage workloads there:

```cedar
@id("team-a-to-mac-studio")
permit (principal in Namespace::"team-a", action in [Action::"resolve", Action::"connect"], resource in Namespace::"mac-studio.example.com/team-a");

@id("team-a-ci-to-mac-studio-control")
permit (principal in Namespace::"team-a", action == Action::"connect", resource == Peer::"mac-studio.example.com")
when { context.via == "guest-cli" && principal.hasTag("role") && principal.getTag("role") == "ci" };
```

On `mac-studio.example.com`, which enrolled host A as `laptop.tail1234.ts.net`, `team-a` on host A may reach the `db` service and create workloads in `remote-ci` at most three levels deep, and no peer may attach any binding:

```cedar
@id("laptop-team-a-db")
permit (principal in Namespace::"laptop.tail1234.ts.net/team-a", action == Action::"connect", resource == Service::"team-a/db");

@id("laptop-ci-creates")
permit (principal in Namespace::"laptop.tail1234.ts.net/team-a", action == Action::"create_workload", resource == Namespace::"remote-ci")
when { context.via == "guest-cli" && context.depth <= 3 };

@id("no-peer-bindings")
forbid (principal in Peer::"laptop.tail1234.ts.net", action == Action::"attach_binding", resource);
```

## Topology

Peering is pairwise and never transitive:

- A stream that arrives on the peer listener goes only to a local destination: avirtedgeproxyd never forwards it to another peer.
- avirtapid never forwards a peer caller's call to another peer.
- A peered form under a host that is not an `active` peer of this host is not an avirt name, whatever another peer knows: it resolves normally and is an external endpoint.

A host may enroll any number of peers, each with its own record, bundle, connection pool and limits. A mesh of n hosts is n(n − 1)/2 mutual enrollments.

## Failure modes

| Failure | Effect on new flows | Effect on open flows | Recovery |
| --- | --- | --- | --- |
| Peer unreachable | New streams fail: 502 for a dial failure, 504 after the 10 s connect timeout. Control-plane calls fail with `UNAVAILABLE`, exit 7 in `avirt`. The peer stays `active`, so avirtdnsd still answers its peered names. | Streams on a connection that dies end with `error: "peer-lost"`. No stream is resumed. | Automatic once a dial succeeds. After three failures the record's `reachable` is false. |
| Partition | As above, in each direction that cannot reach the other | Streams end when liveness fails, within about 60 s. Workloads created from the other side get no keepalives and are stopped after 5 minutes, unless detached. | Automatic. A workload stopped for missing keepalives stays stopped. |
| Bundle refresh fails | None while the current bundle verifies the peer | None | The next interval. The record keeps `last_error`. |
| Bundle or name mismatch: the peer was reinstalled, its roots no longer chain, or its certificate no longer carries the recorded peer host name | Every handshake with the peer fails, recorded as `peer-auth-failed`, in both directions | Connections close at their next handshake, and at the latest when the peer certificate expires | `avirtadmin peer revoke`, then a new enrollment from a new enrollment file |
| Bundle expired: every root the record holds has expired | As for a mismatch | As for a mismatch | As for a mismatch |
| Clock skew beyond 5 minutes | Handshakes fail on validity, recorded as `peer-clock-skew` | Open connections close at the peer certificate's `not_after` as seen locally | Synchronize the clocks |
| Local host policy snapshot expired | Every stream to and from peers is denied, with rule `snapshot-expired` | Re-evaluated and closed (see [revocation of open flows](./egress-policies.md#revocation-of-open-flows)) | A valid host policy snapshot applies |
| Peer's host policy snapshot expired | The peer refuses, and the status is relayed | The peer closes them | At the peer |
| Peer suspended or revoked here | Its peered names resolve normally and are external endpoints, so nothing is sent to the peer | Closed at once | `peer resume`, or a new enrollment |
| Policy change on either side | Evaluated against the new policy snapshot | Each host's avirtedgeproxyd and each avirtmedproxyd re-evaluates its own open flows | None needed |

## Observability and audit

Each host records its own side of every peered flow. Neither host sends events to the other.

- **Source avirtedgeproxyd.** One `conn` event per peer stream, attributed to the source workload, with `src: "edgeproxy"`, `peer`, `request_id`, the destination name, `peer_status` and the byte counts (see [events](./egress.md#events-1)).
- **Destination avirtedgeproxyd.** One `conn` event per peer stream, attributed to the destination workload, with `src: "edgeproxy"`, `peer`, `request_id` and the caller in `x-avirt-caller`.
- **Destination avirtmedproxyd.** Its `ingress` decision event, with the peer caller.
- **Host audit log.** avirthostd records enrollment, every state change, suspension, distrust and revocation, every bundle accepted or rejected with its sequence and reason, every peer connection opened and closed with the handshake outcome (`peer-auth-failed`, `peer-clock-skew`), keepalives and the workloads stopped for missing them, and every forwarded and received control-plane call (see [audit logs](./observability.md#audit-logs)). avirtauditd delivers enrollment changes and handshake failures as Authentication (3002), and control-plane calls as API Activity (6003) (see [audit events](./observability.md#audit-events)).
- **Correlation.** `request_id` is the same on both hosts' events for one stream, so two operators can join their records without sharing them.
- **Metrics.** `avirt.peer.connections`, `avirt.peer.streams`, `avirt.peer.handshake_failures` (with `avirt.reason`), `avirt.peer.bundle_age` and `avirt.peer.rtt`, from avirtedgeproxyd and avirthostd, each with `avirt.peer.host`, an attribute bounded by the number of enrolled peers.

## CLI

### avirtadmin

Every `avirtadmin peer` command is host administration on avirthostd's admin socket, as `Admin::"uid:<uid>"` (see [admin CLI](./admin-cli.md#peers-and-enrollment)).

| Command | Does | Cedar action |
| --- | --- | --- |
| `avirtadmin peer export [--address ADDR]...` | Writes this host's enrollment file to standard output | None |
| `avirtadmin peer fingerprint [--long]` | Prints this host's fingerprint | None |
| `avirtadmin peer enroll FILE\|- [--host FQDN] [--fingerprint FP] [--address ADDR]... [--route ROUTE]... [--accept-policy-references]` | Enrolls a peer from its enrollment file (see [enrolling](#enrolling)) | `enroll_peer` |
| `avirtadmin peer list`, `peer inspect HOST` | Lists peers with their state, trust domain, addresses, bundle sequence and root expiry, routes and last refresh, or shows one with its roots' fingerprints | `list_peers`, `inspect_peer` |
| `avirtadmin peer set HOST [--address ADDR]... [--route ROUTE]... [--remove-route ROUTE]...` | Changes a peer's addresses and routes, never its peer host name | `update_peer` |
| `avirtadmin peer refresh HOST` | Refreshes the peer's bundle now | `update_peer` |
| `avirtadmin peer suspend HOST`, `peer resume HOST` | Suspends or resumes a peer | `update_peer` |
| `avirtadmin peer distrust-root HOST FINGERPRINT` | Distrusts one of the peer's roots | `update_peer` |
| `avirtadmin peer revoke HOST` | Removes the enrollment | `revoke_peer` |
| `avirtadmin peer discover [--timeout DURATION]` | Lists `_avirt-peer._tcp` announcements on the LAN, with their addresses and trust domain hashes, marking enrolled ones | None |

### avirt

`avirt peer list` and `avirt peer inspect HOST` show the peers for operators who are not administrators, under `list_peers` and `inspect_peer` (see [peers](./operator-cli.md#peers)). Every other command runs on a peer with `--host HOST` or a context's `host`. `avirt whoami --host HOST` prints the principal the peer sees for this caller, qualified by the peer host name the peer recorded for this host.

# Open questions

1. **Peer user principals.** The local and web UI user principals are open (see [components](./components.md#open-questions)), and so is how a peer represents another host's user, as `<host>/<user>` or otherwise.
2. **Peer entities below the namespace.** On the source host a peered name resolves to the peer's namespace (see [egress policies](./egress-policies.md#open-questions)). Whether a peer exports a catalog of the services it lets a peer see, so the source can resolve service names and their aliases to `Service::"<host>/<ns>/<name>"` entities, is open.
3. **One-way reachability.** A host that cannot be dialed cannot receive streams, and its peer cannot refresh its bundle. Whether a peer may open reverse streams on a connection the other host dialed, and whether bundles can also be pushed, is open.
4. **A conformant bundle endpoint.** Whether a host also serves its bundle without client authentication, as a SPIFFE `https_spiffe` bundle endpoint, for federation with non-avirt SPIFFE deployments, is open.
5. **Short-code enrollment.** Whether enrollment also offers a short one-time code that both hosts exchange online under a password-authenticated key exchange, instead of exchanging enrollment files, is open.
6. **HTTP/3.** Whether the peer listener also serves HTTP/3 over QUIC is open.
7. **Keepalive timing.** Whether the 60 s keepalive interval and the 5-minute limit are host settings, or set per child by the creator's host, is open.
