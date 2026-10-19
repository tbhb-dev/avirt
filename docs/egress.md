---
type: Component Design
title: Egress and TLS termination
description: Authorization of TCP, UDP, and ICMP flows, protocol inspection, private-dest checks, synthetic addresses, websockets, TLS certificates, and events in avirtmediatord and avirtedgeproxyd.
tags: [avirt, egress, proxy, tls, networking]
implementation: partial
generated: { by: codex/gpt-6, at: 2026-10-18T12:00:00Z }
---

# Status: implemented

## Path

The sandbox container has no network beyond loopback: the Apple container CLI runtime starts it with `--network none`, and Containerization boots a VM with no network device. TCP, UDP, DNS, and IPv6 all fail inside it. The agent's only way out is avirtinitd's TCP proxy to `proxy.sock`, which forwards to the mediator's CONNECT proxy on `proxy.sock` (see [workload topology](./workload-topology.md)). Every client must honour the proxy variables. Claude Code, Codex, Node `fetch` and `https` (with `NODE_USE_ENV_PROXY=1`), npm, git, and curl all do. `git://` and `ssh://` don't work.

## Mediator process

```text
avirtmediatord --dir DIR --hostd-sock PATH --workload ID
```

It runs one Pingora 0.9.0 `Server` with two Unix-socket services at 0600: the CONNECT app on `proxy.sock`, and on `mediator.sock` a TCP proxy that forwards the bytes of avirtinitd's HTTP/2 connection unchanged to avirthostd. Stdin EOF or SIGINT means fast shutdown and SIGTERM graceful shutdown with no grace-period delay and at most one second for service runtimes to drain. Either way it first flushes every open connection as an event with `error: "shutdown"`. Stdin EOF ties its lifetime to avirthostd's. Stopping avirthostd with SIGKILL also stops every mediator.

## Pingora CONNECT app

Pingora never reuses connections or checks peers because the socket's mode controls access.

1. Read the request head (at most 8 KiB, 10 s timeout) and parse it with `httparse`. Ignore a connection that closes before sending a byte (a readiness probe). Forward bytes after the blank line upstream, including an early TLS ClientHello.
2. Accept only `CONNECT host:port` with an explicit port of 1-65535 written as 1 to 5 ASCII digits without a sign or leading zero. Accept a hostname or IP literal, convert it to lowercase, and strip any trailing dot. An authority that looks like IPv4 must be a canonical dotted quad, so reject `2130706433`, `0177.0.0.1`, `0x7f000001`, and `127.1` as malformed.
3. Resolve the name with `tokio::net::lookup_host`, with a 10 s timeout, and apply the private-address block below to every resolved address. One refused address refuses the whole CONNECT.
4. Dial the addresses in order, with a 10 s timeout for dialing. Reply `200 Connection Established`, flush, and copy in both directions with byte counts until either side closes or shutdown fires. Omit an idle timeout because model API streams are long-lived.

| Status | Cause |
| --- | --- |
| 400 | Malformed head or authority (`bad-request`) |
| 403 | `private-dest` |
| 501 | Any method other than CONNECT (`plain-http` for an absolute `http://` URI, else `unsupported-method`) |
| 502 | DNS or dial failure |
| 504 | Connection timeout |

The proxy allows and logs other destinations under rule `allow-all`.

## Private-address block (`private-dest`)

The private-address block refuses resolved addresses in non-global ranges: loopback, RFC 1918, CGNAT, link-local, benchmarking, documentation, multicast, reserved and broadcast, and for IPv6 also unique-local, site-local, Teredo, discard, NAT64 (`64:ff9b::/96`, `64:ff9b:1::/48`), 6to4 (through its embedded IPv4) and IPv4-mapped or IPv4-compatible addresses. IPv6 also refuses IPv4-translated `::ffff:0:0:0/96`, benchmarking `2001:2::/48`, ORCHID `2001:10::/28`, ORCHIDv2 `2001:20::/28`, documentation `3fff::/20` and segment-routing SIDs `5f00::/16`. The block also refuses addresses on the host's own interfaces, including global addresses. It reads them with `getifaddrs` on each CONNECT. Failure to read host interfaces refuses the connection with 403. Its event uses rule `private-dest` and error `"host addresses"`.

## Events

Each proxied connection becomes one `dir: "egress"` line in `logs/<workload-id>.ndjson`, written when the connection closes. The mediator flushes connections still open at shutdown with `error: "shutdown"`.

```json
{"ts":"…","dir":"egress","msg":{"event":"conn","workload":"7e759259","started":"…","method":"CONNECT","host":"api.anthropic.com","port":443,"decision":"allow","rule":"allow-all","bytes_up":1234,"bytes_down":56789,"duration_ms":4200,"error":null}}
```

To list the hosts a set of workloads used: `jq -r 'select(.dir=="egress") | .msg | "\(.host):\(.port)"' logs/*.ndjson | sort | uniq -c`.

# Status: planned

## Path

On Containerization, the sandbox VM has no network device. avirtinitd configures the guest TUN and nftables REDIRECT rules, and avirtnetd transfers every TCP, UDP, and ICMP flow to avirtmediatord. DNS uses one POST `/dns` stream per query. Clients may still use the loopback forward proxy. avirt doesn't set proxy variables by default. [Guest networking](./networking.md#guest-networking) specifies transparent listeners, routing, process privileges, and error handling.

Every outbound connection from avirtmediatord passes through a runtime-provided Unix socket and avirthostd's SNI routing. Mutual TLS authenticates both ends. avirtmediatord delegates external resolution and sockets to avirtedgeproxyd. avirtedgeproxyd performs those operations after its own authorization checks.

## Mediator process

avirtmediatord runs as one process. Traffic handling and lifecycle share its UID and SVID, `/ns/<ns>/workload/<uuid>/mediator/<gen>`. It serves the workload listener at `/run/avirt/workload.sock`, where it applies the workload policy snapshot while storing the synthetic address table and interception keys for that workload. The table persists across mediator restarts and never reassigns an address to another name within the workload's lifetime.

## Transport scope

TCP uses one HTTP/2 CONNECT stream per flow. UDP uses CONNECT-UDP with `DATAGRAM` capsules and an RFC 9298 Context ID, with one immutable destination per stream. Ping uses extended CONNECT with `:protocol` `icmp` and `{type, id, seq, payload}` messages. Messages omit the destination. [HTTP/2 streams](./networking.md#http2-streams) defines framing, capsule length and buffer limits, flow control, and explicit drain before close.

## How avirtmediatord handles egress

1. **Destination.** Use the canonical authority or IP address. A synthetic address maps to a name only through the mediator's synthetic address table. Refuse an unknown synthetic address. A CONNECT authority supplies a name directly. Other addresses stay IP destinations even if a client supplies SNI or Host. `name_from` is `dns`, `connect` or `none`.
2. **Entity.** Resolve avirt hostnames to Workload or Service entities and refuse unresolved names. Ordinary flows can't target host services. MCP and guest CLI streams reach only their designated service paths.
3. **Connection policy.** Check `connect` with the destination protocol and port, using port 0 for ICMP. The principal comes from the authenticated workload identity, never its source address.
4. **Protocol.** Select SSH, HTTP, or TLS from port and policy before dialing. Port 22 or an SSH binding selects SSH. Skip probe dials. Check `intercept` for TLS or SSH at any permitted port, or `passthrough` with the detected protocol.
5. **Inspection.** Under TLS interception, present a leaf certificate from the workload's MITM CA and offer both `h2` and `http/1.1`. Check authorization and `credential-misplaced` on one canonical request. Serialize it with only the credential its binding permits. Check the exact Codex refresh match before the placeholder scan. On cleartext HTTP flows, act as the HTTP server with authorization per request and no injection. SSH uses the binding's pinned host key, authentication with a placeholder key, and allowed channel operations.
6. **Upstream.** Send model requests to avirtmodelgwd and MCP to avirtmcpgwd on their authenticated service paths. Other traffic reaches avirtedgeproxyd on a stream for its protocol. Intercepted TLS opens a separate upstream TLS connection within the edge tunnel and verifies its certificate. Every request, redirect and request on a pooled connection requires authorization.
7. **Passthrough.** On named TLS and QUIC flows, require matching SNI and refuse `encrypted_client_hello`. A passthrough permit grants access to the endpoint and anything it tunnels. HTTP Host must equal the flow's hostname, otherwise return 421. An SNI mismatch closes the connection.
8. **Events and close.** Record decisions and flow outcomes. A tunnel must shut down its sending side and drain the peer with a deadline, recording the outcome in its log. Dropping a stream proves nothing about delivery.

[HTTP and TLS](./networking.md#http-and-tls) specifies request ambiguity checks, gRPC refusals, and streaming scans. [QUIC](./networking.md#quic) defines Initial parsing and bounds, and [SSH and git](./networking.md#ssh-and-git) defines channel restrictions and per-binding git HTTPS rewrites. [ICMP echo](./networking.md#icmp-echo) specifies echo validation, reply matching, and rate limits.

## Edge proxy egress

avirtedgeproxyd accepts CONNECT, CONNECT-UDP and `icmp` streams from authorized mediators and host services, and from enrolled peers on its peer listener. It verifies the caller's SVID and current generation before evaluating the host policy snapshot. On authorized delegated calls it also checks the originating principal in `x-avirt-caller`.

avirt hostnames resolve to canonical entities. Local workload ports use the destination mediator's ingress listener. A service can use its authorized backend, and an avirt hostname under a peer's host name routes through that peer's avirtedgeproxyd. The edge resolves external names and checks every resolved address before dialing. Retries repeat the checks and never trigger an unchecked re-resolution. TCP and UDP use separate sockets per flow. UDP sockets connect to their fixed destination. ICMP uses unprivileged datagram sockets.

## `private-dest`

The edge refuses non-global addresses and addresses assigned to the host's own interfaces for CONNECT, CONNECT-UDP and `icmp` streams, and forwarded HTTP. The full [private-address block](#private-address-block-private-dest) applies, including refusal of IPv4-mapped and IPv4-compatible literals on flows to literal IP addresses outside the synthetic ranges. A watcher tracks host addresses. The mediator translates synthetic addresses through its table before the edge checks the real upstream address, and an authorized service record allows only its recorded backend. A peer connection may use enrolled peer addresses, but never an address of the host itself.

| Status | Cause |
| --- | --- |
| 403 | `private-dest`, an unknown avirt hostname, or a denied caller or destination |
| 502 | DNS or dial failure |
| 504 | Connection timeout |

avirtedgeproxyd doesn't decrypt the upstream TLS opened by a mediator or gateway. It records its own connection events with `src: "edgeproxy"`.

## Workload DNS

All TCP and UDP port 53 traffic reaches avirtnetd's DNS stub resolver, including queries sent to an arbitrary address and UDP port 53 read from the TUN. Each query opens a separate `POST /dns` stream with `application/dns-message` request and response bodies and a separate concurrency limit. `resolve` applies at both the mediator and edge, including every name in a CNAME chain.

The mediator returns synthetic answers for both external and avirt hostnames. The synthetic address table has a size limit. Allocations fail when full. DNS rewrites include HTTPS and SVCB address hints, AliasMode, TargetName and port, with consistent `mandatory` and `no-default-alpn` fields. The synthesizer removes `ech` and omits `h3` for intercepted names. Guest DNSSEC validation fails on rewritten answers. [Names and DNS](./networking.md#names-and-dns) specifies the table and residual channels. [DNS](./dns.md) defines avirt hostnames.

## Websockets

HTTP/1 upgrades and HTTP/2 extended CONNECT under RFC 8441 pass request policy and placeholder checks. Websocket upgrades use HTTP/1 upstream. avirtmediatord strips `permessage-deflate` and refuses unnegotiated extensions. It unmasks client data and scans across fragments and continuation frames with a bounded matcher, scanning control frames separately. A placeholder hit closes with 1008 and records rejection of client credentials. Both relay directions preserve frame boundaries, including when injecting a Close. Encoding a placeholder can evade scanning. Credential injection applies to the opening request only.

## MITM CA and leaf certificates

`avirthostd` issues one MITM CA per workload (ECDSA P-256, CN `avirt workload <uuid>`, 7-day validity, `KeyCertSign` only), so that it persists across mediator restarts, and passes the cert and key to avirtmediatord in `GET /workload`. avirtmediatord issues leaves in-process on demand and caches up to 256 per host. Each uses P-256 with the host as SAN, 24 h validity, and a chain of leaf plus CA. The CA key never enters the sandbox container.

Workload policy defines the CA's interception scope. When it creates the workload, avirthostd asks avirtpolicyd's prover for the set of hosts the workload policy snapshot can permit `intercept` for (see [MITM CA name constraints](./egress-policies.md#mitm-ca-name-constraints)):

- **A bounded set.** The CA contains X.509 name constraints whose permitted DNS subtrees are exactly those hosts and domains, so a leaf for any other name fails validation in the agent.
- **An empty set.** avirthostd leaves the workload without a MITM CA and omits it from `confirm`. External connections require a `passthrough` permit.
- **An unbounded set.** The CA contains no name constraints.

avirtmediatord never issues a leaf outside the CA's constraints. A later policy snapshot that permits `intercept` for a host outside them can't intercept that host's TLS. The connection uses TLS passthrough if policy permits `passthrough`. Otherwise, avirtmediatord denies it.

## CA delivery into the workload

The CA cert arrives in `confirm`. Before the agent starts, avirtinitd:

1. writes `/run/avirt/ca.pem`,
2. appends it to `/etc/ssl/certs/ca-certificates.crt` (image-local, as root),
3. writes `/run/avirt/ca-bundle.pem` (system bundle plus the MITM CA).

It then sets `NODE_EXTRA_CA_CERTS=/run/avirt/ca.pem`, and sets `SSL_CERT_FILE`, `CURL_CA_BUNDLE`, `REQUESTS_CA_BUNDLE`, `GIT_SSL_CAINFO`, `npm_config_cafile` and `CODEX_CA_CERTIFICATE` all to `/run/avirt/ca-bundle.pem`.

| Client | Trust behavior |
| --- | --- |
| Claude Code 2.1.300 | No certificate pinning. `CLAUDE_CODE_CERT_STORE` defaults to `bundled,system`, and it appends `NODE_EXTRA_CA_CERTS`. It doesn't load `SSL_CERT_FILE`. |
| Codex 0.160.1 | Uses native roots, without pinning. `CODEX_CA_CERTIFICATE` takes precedence over `SSL_CERT_FILE`, and both its HTTP and websocket clients add the bundle. |
| npm | Honors `NODE_EXTRA_CA_CERTS` and `cafile` without pinning. |
| git | Honors `GIT_SSL_CAINFO` without pinning. |

A client that rejects the MITM CA needs a policy that forbids `intercept` and permits `passthrough` for its hosts. avirt audits those hosts only at L4 and omits credential injection.

## Events

A flow produces one `conn` event with `proto` (`tcp`, `udp` or `icmp`), `dst_ip` and `name_from` (`dns`, `connect` or `none`). Intercepted requests produce one `http` event apiece, and workload DNS queries produce one `dns` event apiece. avirtmediatord posts them to avirthostd on `POST /workload/events` (`src: "mediator"`). avirtedgeproxyd posts its own `conn` and `dns` events (`src: "edgeproxy"`), and avirtmodelgwd and avirtmcpgwd post the credential outcome of every request they serve (`src: "gateway"`). Each producer queues its events in a bounded ring and never blocks a worker thread on delivery, and numbers them in its own `seq` domain so avirthostd can deduplicate them across reconnects. When a workload's ring fills, the proxy process refuses new connections, requests, and DNS queries for that workload with 503 until the ring drains. This prevents silent event loss. avirtedgeproxyd keeps one ring per workload, so one workload's backlog never stalls another's. Every event contains `workload`, `gen` and `seq` (see [trust boundaries](./trust-boundaries.md#src)).

The `http` event has these fields:

- `conn_id`, `method`, `host`,
- `path` (query string dropped) and `query: bool`,
- `status`, `bytes_up`, `bytes_down`, `duration_ms`,
- `decision` (`allow`, `deny` or `would-deny`), `rule` (the id of the Cedar policy that decided), `mode`, `policy_version`,
- `cred: [{name, location, action: "injected"|"rejected"}]`, for credentials avirtmediatord injects itself. The gateways' own events record theirs.

The `dns` event has `name`, `qtype`, `rcode`, `decision`, `rule` and `policy_version`.

Logs exclude secret values, placeholder values, header values, and bodies. How these events become audit records, and their severity levels, are in [observability](./observability.md#audit-events) and [severity](./observability.md#severity).

## Policy

Egress policy is Cedar, evaluated locally in avirtmediatord and avirtedgeproxyd each from a versioned policy snapshot: avirtmediatord evaluates the workload policy snapshot's `resolve`, `connect`, `intercept`, `passthrough`, `request` and `ingress`, and avirtedgeproxyd evaluates the host policy snapshot's `resolve` and `connect`. The schema, name resolution, the action semantics, modes, policy snapshots, revocation, the prover and failure behavior are in [egress policies](./egress-policies.md). Credential bindings and injection are in [egress credentials](./egress-credentials.md).

## Mediator network

The mediator has no network interface other than loopback, on every runtime. It runs without NAT, bridging, a resolver, a firewall ruleset, or `CAP_NET_ADMIN`. Its only connections are the runtime-provided Unix sockets in [workload topology](./workload-topology.md#runtime-provided-sockets). The `private-dest` check runs at avirtedgeproxyd, which resolves external names, and avirtedgeproxyd is the only host service that opens connections off the host (see [where avirtedgeproxyd runs](./components.md#where-avirtedgeproxyd-runs)).
