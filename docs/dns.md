---
type: Component Design
title: DNS
description: The avirt DNS zone that avirtdnsd serves, the local and peered name forms for workloads and services, the label syntax and length caps, how names reach avirtedgeproxyd, and how the workload resolver and the DNS stream use them.
tags: [avirt, dns, avirtdnsd, names, hickory]
implementation: planned
generated: { by: claude-code/claude-opus-5-5, at: 2026-10-17T19:00:00Z }
---

# Overview

avirtdnsd makes local and peered workloads and services addressable by name. It runs on the host, and it is a host service (see [components](./components.md#host-services)). The workload's resolver is a stub in avirtnetd that sends every query over the workload's HTTP/2 connection to its mediator (see [workload DNS](./egress.md#workload-dns)). Where DNS sits in the whole network model is in [networking](./networking.md#addressing-and-naming).

# Status: planned

## Implementation

- avirtdnsd is built on hickory 0.26.3: hickory-proto for messages, hickory-net for transports, and hickory-server with a custom `RequestHandler`.
- libavirtenvoy parses `/dns` queries with hickory-proto, using `Request::from_bytes`.
- Envoy's dns_filter, the trust-dns-* crates and `domain` older than 0.12.3 are not used.
- `cargo audit` must stay clean for RUSTSEC-2026-0119 (O(n²) name-compression encoding).

## The avirtdnsd zone

The avirt DNS zone. It answers every name under `host.avirt.internal` with this host's avirtedgeproxyd address, and the peered name forms under the peer host name of an `active` peer with that peer's avirtedgeproxyd address (see [avirt names](#avirt-names)). Every other name resolves normally, including the peer host name itself and any other name under it (see [peer host names](./peering.md#peer-host-names)). avirtdnsd talks to avirtmedproxyd (through avirthostd's SNI routing), avirtedgeproxyd, and avirthostd (namespaces, workloads, services and enrolled peers).

## avirt names

| Form | Names | Resolves to |
| --- | --- | --- |
| `<id>.<ns>.workload.host.avirt.internal`, `<alias>-<id>.<ns>.workload.host.avirt.internal`, `<generated>.<ns>.workload.host.avirt.internal` | A workload on this host | This host's avirtedgeproxyd |
| `<name>.<ns>.svc.host.avirt.internal`, `<alias>.<ns>.svc.host.avirt.internal` | A service on this host | This host's avirtedgeproxyd |
| `<id>.workload.host.avirt.internal`, `<alias>-<id>.workload.host.avirt.internal`, `<generated>.workload.host.avirt.internal`, `<name>.svc.host.avirt.internal`, `<alias>.svc.host.avirt.internal` | The same entity as the long form in the `default` namespace | This host's avirtedgeproxyd |
| `<id>.<ns>.workload.<host>`, `<alias>-<id>.<ns>.workload.<host>`, `<generated>.<ns>.workload.<host>`, `<name>.<ns>.svc.<host>`, `<alias>.<ns>.svc.<host>` | A workload or service on an `active` peer, where `<host>` is that peer's fully qualified host name | That peer's avirtedgeproxyd |

- **Workload name forms.** A workload's first DNS label is its `<id>`, `<alias>-<id>` if it has an alias, or its `<generated>` name. All three name the same workload.
  - `<id>` is the workload's short ID, exactly 8 lowercase hexadecimal characters (`[0-9a-f]{8}`). It is a DNS-only name. The workload's canonical ID is its UUIDv7, which SPIFFE IDs and Cedar entities carry (see [workload IDs](./components.md#workload-ids)).
  - `<alias>-<id>` parses unambiguously, because `<id>` is always the last 8 characters.
  - `<generated>` is a hyphenated `<adverb>-<adjective>-<noun>` name, such as `quietly-brave-otter`, which avirthostd always generates at creation with the `petname` crate. It is lowercase `a`-`z` and hyphens only, and unique per host and namespace among live workloads: on a collision avirthostd generates another. The word lists are chosen so that the longest combination fits in 63 characters, and the noun list holds no 8-character word made only of hex digits, so a generated name never parses as `<alias>-<id>`.
- **Service name forms.** A service's first DNS label is its `<name>` or one of its aliases. All of them name the same service.
- **DNS label syntax.** Every DNS label in an avirt name is lowercase: `a`-`z`, `0`-`9` and `-`, at most 63 characters. Aliases, service names and namespace names are validated to that form. `<host>` is a fully qualified name of at least two such labels, none starting or ending with `-`, with an internationalized name in its A-label (`xn--`) form and no trailing dot (see [peer host names](./peering.md#peer-host-names)).
- **Aliases grant nothing.** Name resolution maps every form of a name to the same entity (see [egress](./egress-policies.md#name-resolution)), and authorization uses the entity, keyed by the workload's ID or the service's name. Neither a workload's alias nor its generated name is ever used for authorization: the ID is.

DNS limits each label to 63 octets and a whole name to 253 characters in text form, without the trailing dot. These caps follow from them:

| Item | Cap | Derivation |
| --- | --- | --- |
| Workload alias | 54 characters | 63, less the hyphen and the 8-character ID |
| Generated workload name | 63 characters | One label, guaranteed by the word lists |
| Service name, service alias | 63 characters | One label |
| Namespace name | 63 characters | One label |
| Local name | 156 characters at most | 63 + 1 + 63 + 1 + 28 for `workload.host.avirt.internal`, within 253 |
| Peer host name (`<host>`) | 116 characters | `<alias>-<id>.<ns>.workload.` takes at most 63 + 1 + 63 + 1 + 8 + 1 = 137 characters, and 253 − 137 = 116. The `svc` form would allow 121, so 116 is the binding cap. |

avirthostd validates every alias, service name and namespace name when it is created, and enrollment validates a peer's host name (see [enrolling](./peering.md#enrolling)). A name that exceeds its cap or breaks the label syntax is rejected, never truncated.

`host` in the local forms is a literal label, in the way `host.docker.internal` is. avirtdnsd answers these names, and it is the only component that does.

avirtmedproxyd sends every connection to an avirt name, local or peered, to this host's avirtedgeproxyd through avirthostd's SNI routing, whatever address the name resolved to, so it never needs a peer's address. avirtedgeproxyd resolves the name to its canonical entity (see [egress](./egress-policies.md#name-resolution)), routes a local one to the destination's avirtmedproxyd, and routes a peered one to the peer's avirtedgeproxyd at the address avirtdnsd returns for it. A workload name reaches only the ports that workload publishes (see [workload topology](./workload-topology.md#ingress)).

## Workload DNS policy and the DNS stream

The workload's resolver, the `resolve` check on every query and the public-name path are in [egress](./egress.md#workload-dns). The Cedar `resolve` action and the resolution of an avirt name to its canonical entity are in [egress policies](./egress-policies.md#name-resolution). The `/dns` HTTP/2 stream that carries the queries is in [workload topology](./workload-topology.md#http2-streams). Workload IDs, including the short ID that `<id>` carries, are in [components](./components.md#workload-ids).

# Open questions

1. **Reuse of generated names.** Whether a generated name can be given to a new workload once its workload is gone, which risks confusion through DNS caches and old references, or is never reused, which slowly exhausts a namespace's name space, is open.
2. **DNS answers under transparent TCP.** When transparent TCP is designed, whether DNS answers inside the workload container become per-name synthetic addresses that avirtnetd maps back to names, which raw TCP without SNI or `Host` needs in order to be routed by name, is open.
