---
type: Component Design
title: DNS
description: The avirt DNS zone that avirtdnsd serves, the internal name forms on this host and under a peer's host name for workloads and services, the label syntax and length caps, how avirtedgeproxyd receives connections to those names, and how the workload resolver and the DNS stream use them.
tags: [avirt, dns, avirtdnsd, names, hickory]
implementation: planned
generated: { by: codex/gpt-6, at: 2026-10-18T05:20:59Z }
---

# Overview

avirtdnsd makes local and peered workloads and services addressable by name. It runs as a host service (see [components](./components.md#host-services)). The workload's resolver is a stub in avirtnetd that sends every query over the workload's HTTP/2 connection to its mediator (see [workload DNS](./egress.md#workload-dns)). Where DNS sits in the whole network model is in [networking](./networking.md#names-and-dns).

# Status: planned

## Libraries

- avirtdnsd uses hickory 0.26.3: hickory-proto for messages, hickory-net for transports, and hickory-server with a custom `RequestHandler`.
- avirtmediatord parses `/dns` queries with hickory-proto, using `Request::from_bytes`.
- The DNS components exclude the trust-dns-* crates, and `domain` older than 0.12.3.
- `cargo audit` must pass its check for RUSTSEC-2026-0119 (O(n²) name-compression encoding).

## The avirtdnsd zone

The avirt DNS zone maps names under `host.avirt.internal` to this host's avirtedgeproxyd and internal names under an `active` peer's host name to that peer's avirtedgeproxyd (see [internal names](#internal-names)). avirtmediatord returns a per-workload synthetic address to the sandbox container for each name, so guest answers expose neither edge address. Every other name resolves normally, including the peer's host name itself and any other name under it (see [peers' host names](./peering.md#peers-host-names)). avirtdnsd talks to avirtmediatord (through avirthostd's SNI routing), avirtedgeproxyd, and avirthostd (namespaces, workloads, services and enrolled peers).

## Internal names

| Form | Names | Routed through |
| --- | --- | --- |
| `<id>.<ns>.workload.host.avirt.internal`, `<alias>-<id>.<ns>.workload.host.avirt.internal`, `<generated>.<ns>.workload.host.avirt.internal` | A workload on this host | This host's avirtedgeproxyd |
| `<name>.<ns>.svc.host.avirt.internal`, `<alias>.<ns>.svc.host.avirt.internal` | A service on this host | This host's avirtedgeproxyd |
| `<id>.workload.host.avirt.internal`, `<alias>-<id>.workload.host.avirt.internal`, `<generated>.workload.host.avirt.internal`, `<name>.svc.host.avirt.internal`, `<alias>.svc.host.avirt.internal` | The same entity as the long form in the `default` namespace | This host's avirtedgeproxyd |
| `<id>.<ns>.workload.<host>`, `<alias>-<id>.<ns>.workload.<host>`, `<generated>.<ns>.workload.<host>`, `<name>.<ns>.svc.<host>`, `<alias>.<ns>.svc.<host>` | A workload or service on an `active` peer, where `<host>` is that peer's fully qualified host name | That peer's avirtedgeproxyd |

- **Workload name forms.** A workload's first DNS label is its `<id>`, `<alias>-<id>` if it has an alias, or its `<generated>` name. These three forms refer to the same workload.
  - `<id>` is the workload's short ID, exactly 8 lowercase hexadecimal characters (`[0-9a-f]{8}`). This name applies only to DNS. The workload's canonical ID is its UUIDv7, which appears in SPIFFE IDs and Cedar entities (see [workload IDs](./components.md#workload-ids)).
  - `<alias>-<id>` parses unambiguously, because `<id>` is always the last 8 characters.
  - `<generated>` is a hyphenated `<adverb>-<adjective>-<noun>` name, such as `quietly-brave-otter`, which avirthostd always generates at creation with the `petname` crate. The name uses only lowercase `a`-`z` and hyphens. It must be unique per host and namespace among live workloads: on a collision avirthostd generates another. The word lists limit the longest combination to 63 characters. The noun list excludes 8-character words made only of hex digits, so a generated name never parses as `<alias>-<id>`.
- **Service name forms.** A service's first DNS label is its `<name>` or one of its aliases. These forms refer to the same service.
- **DNS label syntax.** Every DNS label in an internal name is lowercase: `a`-`z`, `0`-`9` and `-`, at most 63 characters. avirthostd validates aliases, service names, and namespace names against that form. `<host>` is a fully qualified name of at least two such labels, none starting or ending with `-`, with an internationalized name in its A-label (`xn--`) form and no trailing dot (see [peers' host names](./peering.md#peers-host-names)).
- **Aliases grant nothing.** Name resolution maps every form of a name to the same entity (see [egress](./egress-policies.md#name-resolution)), and authorization uses the entity, keyed by the workload's ID or the service's name. Authorization uses the workload's ID and excludes both its alias and its generated name.

DNS limits each label to 63 octets and a whole name to 253 characters in text form, without the trailing dot. These caps follow from them:

| Item | Cap | Derivation |
| --- | --- | --- |
| Workload alias | 54 characters | 63, less the hyphen and the 8-character ID |
| Generated workload name | 63 characters | One label, guaranteed by the word lists |
| Service name, service alias | 63 characters | One label |
| Namespace name | 63 characters | One label |
| Local name | 156 characters at most | 63 + 1 + 63 + 1 + 28 for `workload.host.avirt.internal`, within 253 |
| Peer's host name (`<host>`) | 116 characters | `<alias>-<id>.<ns>.workload.` takes at most 63 + 1 + 63 + 1 + 8 + 1 = 137 characters, and 253 − 137 = 116. The `svc` form would allow 121, so 116 is the binding cap. |

At creation, avirthostd validates each alias, service name, and namespace name. Enrollment validates a peer's host name (see [enrolling](./peering.md#enrolling)). These checks reject a name that exceeds its cap or breaks the label syntax. They never truncate a name.

`host` in the local forms is a literal label, in the way `host.docker.internal` is. Only avirtdnsd answers these names.

avirtmediatord sends every connection to an internal name, local or peered, to this host's avirtedgeproxyd through avirthostd's SNI routing, whatever address the name resolved to, so it never needs a peer's address. avirtedgeproxyd resolves the name to its canonical entity (see [egress](./egress-policies.md#name-resolution)). It routes a local one to the destination workload's avirtmediatord. For a peered one, it routes to the peer's avirtedgeproxyd at the address avirtdnsd returns for it. Connections to a workload name can use only the ports that workload publishes (see [workload topology](./workload-topology.md#ingress)).

## Synthetic answers

avirtmediatord maintains a bounded, persistent table for each workload, using `198.18.0.0/15` and a ULA `/64`. Across restarts, the mediator never reuses an address for another name during the workload's lifetime. Allocation fails when the table is full. The mediator refuses unknown synthetic addresses and canonicalizes IPv4-mapped IPv6 addresses before lookup. Only the table or an explicit CONNECT authority supplies a flow name. SNI and Host on real-address flows remain observations. The edge resolves the name and checks the real upstream address with `private-dest`.

TCP and UDP queries to port 53 at any address reach `/dns`, including UDP read from the TUN. Each query has its own stream and event. Both the mediator and edge enforce `resolve`, including CNAME names. For external queries, the edge parses requests rather than relaying arbitrary DNS bytes. Supported query types include A, AAAA, HTTPS, SVCB, and CNAME. Queries have size and rate limits. MX and TXT require permission.

HTTPS and SVCB rewriting handles AliasMode, TargetName, `port`, synthetic address hints, and the consistency of `mandatory` and `no-default-alpn`. The synthesizer removes `ech` and, for intercepted names, `h3`. HTTP responses for intercepted names omit `Alt-Svc`. Rewritten answers prevent guest DNSSEC validation. Permitted encrypted DNS endpoints and encoded names remain possible channels under policy.

## Workload DNS policy and the DNS stream

The workload's resolver, the `resolve` check on every query and the public-name path are in [egress](./egress.md#workload-dns). The Cedar `resolve` action and the resolution of an internal name to its canonical entity are in [egress policies](./egress-policies.md#name-resolution). The per-query `/dns` HTTP/2 streams with `application/dns-message` bodies and their 16-stream limit are in [workload topology](./workload-topology.md#http2-streams). Workload IDs, including the short ID in `<id>`, are in [components](./components.md#workload-ids).

# Open questions

1. **Reuse of generated names.** Whether a new workload can reuse a generated name after its workload ends is open. Reuse risks confusion through DNS caches and old references. Prohibiting reuse slowly exhausts a namespace's name space.
2. **IPv6 allocation.** The exact ULA `/64` for synthetic answers remains a configuration choice.
