---
type: Component Design
title: Egress and TLS termination
description: How the agent's network traffic leaves the workload container, the mediator's CONNECT proxy and private-address block, the planned avirtmedproxyd pipeline, avirtedgeproxyd as policy enforcement point, workload DNS, websockets, TLS termination with the workload's MITM CA and its delivery into the workload container, where policy is evaluated on the data path, the event format and the mediator's lack of a network.
tags: [avirt, egress, proxy, pingora, envoy, tls]
implementation: partial
generated: { by: claude-code/claude-opus-5-5, at: 2026-10-18T01:00:00Z }
---

# Status: implemented

## Path

The workload container runs with `--network none`, so TCP, UDP, DNS and IPv6 all fail inside it and only loopback works. The agent's only way out is avirtinitd's TCP proxy to `proxy.sock`, which forwards to the mediator's CONNECT proxy on `proxy.sock` (see [workload topology](./workload-topology.md)). Every client must honour the proxy variables. Claude Code, Codex, Node `fetch` and `https` (with `NODE_USE_ENV_PROXY=1`), npm, git and curl all do. `git://` and `ssh://` do not work.

## Mediator process

```text
avirtmediatord --dir DIR --hostd-sock PATH --workload ID
```

It runs one Pingora 0.9.0 `Server` with two Unix-socket services at 0600: the CONNECT app on `proxy.sock`, and on `mediator.sock` a TCP proxy that forwards the bytes of avirtinitd's HTTP/2 connection unchanged to avirthostd. Stdin EOF or SIGINT means fast shutdown and SIGTERM graceful shutdown with no grace-period delay and at most one second for service runtimes to drain. Either way it first flushes every open connection as an event with `error: "shutdown"`. Stdin EOF ties its lifetime to avirthostd's, so a SIGKILLed avirthostd takes every mediator down with it.

## Experimental host Envoy mode

Pingora is the default engine. `avirtmediatord --proxy-engine envoy --envoy-bin /absolute/path/to/envoy --envoy-module /absolute/path/to/libavirtenvoy.so` selects the experimental Envoy v1.40.0 engine. The equivalent environment variables are `AVIRT_PROXY_ENGINE=envoy`, `AVIRT_ENVOY_BIN` and `AVIRT_ENVOY_MODULE`; avirthostd forwards only these additional configuration variables to its mediator child. The remaining mediator arguments and the secret source are unchanged.

The mediator writes a static `envoy.json` at 0600 in the workload's directory and starts Envoy through a guardian child. Envoy listens on `envoy.sock` at 0600. The existing `proxy.sock` accept loop relays bytes to that listener; Envoy accepts HTTP/1.1 CONNECT only, the avirtenvoy HTTP filter checks destinations, and a dynamic forward proxy cluster opens raw TCP tunnels. The module and filter are mandatory local bootstrap resources; a load failure prevents listener readiness. The `mediator.sock` TCP proxy, the `WORKLOAD` and `EGRESS` connection headers, secrets and workload relay are unchanged.

The guardian receives no mediator secret and watches a pipe held by avirtmediatord. EOF kills and reaps Envoy, including when avirtmediatord is killed. Envoy and its guardian run in their own process group, separate from avirtmediatord. Envoy exit shuts down avirtmediatord with exit status 1. On shutdown, avirtmediatord closes the guardian pipe, kills the entire Envoy process group and drains the module event reader before the relay connections close and before closing EGRESS. The stdin watch runs on a dedicated thread, so shutdown does not wait for avirthostd to close stdin. Open streams are reported with `error: "shutdown"` using their latest module snapshot.

The module checks all resolved addresses with the shared Pingora destination policy and reads current host interfaces on every resolution. It replaces the upstream authority with the first checked numeric address, so the forward proxy cannot resolve the original name again. No fallback to later addresses is implemented. Each stream starts with `deny`/`undecided`; it becomes `allow`/`allow-all` only after destination checks and the authority rewrite succeed. DNS failures retain `allow`/`allow-all` with error `"dns"`. A stream that completes before a decision has error `"aborted"`; an open undecided stream flushed at shutdown remains `deny`/`undecided` with error `"shutdown"`. Body callbacks accumulate byte counts and emit progress snapshots at most once per second per stream; completion always emits final totals. Module snapshots retain the original host and are converted to the existing `conn` event shape with the workload stamped by avirtmediatord.

Runtime parity is unverified. Envoy may reject a malformed HTTP head before the module runs, in which case there is no module connection event. Envoy's upstream failure statuses and plain-method classification differ from Pingora's status table below. Exact event counts, byte counts, early tunnel data, long streams, and shutdown behavior require live validation. This mode remains opt-in; the phase 4 parity gates are pending.

To prepare an end-to-end run, build the mediator and host daemon with `cargo build --locked -p avirtmediatord -p avirthostd`, build the module with `cargo build --locked --manifest-path crates/avirtenvoy/Cargo.toml --target-dir target/envoy-module`, and supply an Envoy v1.40.0 binary with the HTTP dynamic-module and dynamic forward proxy extensions. Constraint: avirt ships and runs no darwin Envoy, so this mode runs only where avirtmediatord runs on a Linux host (see [Envoy delivery](./build-and-platforms.md#envoy-delivery)). The bootstrap validation test runs only when both `AVIRT_ENVOY_BIN` and `AVIRT_ENVOY_MODULE` are set; otherwise it prints `skipped: set AVIRT_ENVOY_BIN and AVIRT_ENVOY_MODULE` and returns, without searching PATH. Set both variables, then run `cargo test -p avirtmediatord envoy_validates_bootstrap_when_available -- --nocapture` to validate that the supplied binary loads the generated bootstrap and module. Start avirthostd with those variables and `AVIRT_PROXY_ENGINE=envoy`, then use the normal launch command. Run the regression suite and both agents' positive checks before claiming parity.

### Status: planned

HTTP/2 CONNECT, generation restart, fencing and workload freeze are planned.

Interim dev mode, experimental. Before the mediator moves into its own VM (see [roadmap](./roadmap.md#phases)), `AVIRT_PROXY_ENGINE=envoy` has avirtmediatord run Envoy in a `container run --publish-socket` container on `container`'s default network. Known gap: avirthostd does not yet push the host's interface addresses in this mode, so libavirtenvoy refuses the non-global ranges but not the host's own global addresses.

## Pingora CONNECT app

Connections are never reused, and there is no peer check, because the socket's mode is the access control.

1. Read the request head (at most 8 KiB, 10 s timeout) and parse it with `httparse`. A connection that closes before sending a byte (a readiness probe) is ignored. Bytes after the blank line, such as an early TLS ClientHello, are forwarded upstream.
2. Accept only `CONNECT host:port` with an explicit port of 1-65535 written as 1 to 5 ASCII digits, with no sign or leading zero, and a hostname or IP literal, lowercased, with any trailing dot stripped. An authority that looks like IPv4 must be a canonical dotted quad, so `2130706433`, `0177.0.0.1`, `0x7f000001` and `127.1` are malformed.
3. Resolve the name with `tokio::net::lookup_host`, with a 10 s timeout, and apply the private-address block below to every resolved address. One refused address refuses the whole CONNECT.
4. Dial the addresses in order, with a 10 s timeout for dialing. Reply `200 Connection Established`, flush, and copy in both directions with byte counts until either side closes or shutdown fires. There is no idle timeout, because model API streams are long-lived.

| Status | Cause |
| --- | --- |
| 400 | Malformed head or authority (`bad-request`) |
| 403 | `private-dest` |
| 501 | Any method other than CONNECT (`plain-http` for an absolute `http://` URI, else `unsupported-method`) |
| 502 | DNS or dial failure |
| 504 | Connect timeout |

Every other destination is allowed and logged under rule `allow-all`.

## Private-address block (`private-dest`)

A resolved address is refused when it is in any non-global range: loopback, RFC 1918, CGNAT, link-local, benchmarking, documentation, multicast, reserved and broadcast, and for IPv6 also unique-local, site-local, Teredo, discard, NAT64 (`64:ff9b::/96`, `64:ff9b:1::/48`), 6to4 (through its embedded IPv4) and IPv4-mapped or IPv4-compatible addresses. IPv6 also refuses IPv4-translated `::ffff:0:0:0/96`, benchmarking `2001:2::/48`, ORCHID `2001:10::/28`, ORCHIDv2 `2001:20::/28`, documentation `3fff::/20` and segment-routing SIDs `5f00::/16`. Every address assigned to one of the host's own interfaces is also refused, global ones included, read with `getifaddrs` on each CONNECT. If host interfaces cannot be read, the connection is refused with 403, rule `private-dest` and error `"host addresses"`.

## Events

Each proxied connection becomes one `dir: "egress"` line in `logs/<workload-id>.ndjson`, written when the connection closes. Connections still open at shutdown are flushed with `error: "shutdown"`.

```json
{"ts":"…","dir":"egress","msg":{"event":"conn","workload":"7e759259","started":"…","method":"CONNECT","host":"api.anthropic.com","port":443,"decision":"allow","rule":"allow-all","bytes_up":1234,"bytes_down":56789,"duration_ms":4200,"error":null}}
```

To list the hosts a set of workloads used: `jq -r 'select(.dir=="egress") | .msg | "\(.host):\(.port)"' logs/*.ndjson | sort | uniq -c`.


# Status: planned

## Transport scope

The workload container has no network interface other than loopback, and neither does the mediator. Supported workload egress is proxy-aware traffic: a client honours the proxy variables and opens `CONNECT host:port` to avirtnetd's forward proxy. Name resolution inside the workload container behaves as expected through avirtnetd's DNS stub resolver (see [workload DNS](#workload-dns)). Workload processes also use the named HTTP/2 streams in [workload topology](./workload-topology.md#http2-streams) for the guest CLI, MCP and ingress. How each connection type crosses the hosts is in [networking](./networking.md). Transparent TCP and UDP are separate planned features, each needing its own security design and checks, and nothing in this document provides them.

## Path

Each connection type's hops, policy enforcement points and identities are in [connection types](./networking.md#connection-types).

Every connection from avirtmedproxyd's Envoy to avirtmodelgwd, avirtmcpgwd or avirtedgeproxyd leaves on the mediator's outbound socket to avirthostd, which routes it by TLS SNI without terminating TLS (see [SNI routing](./workload-topology.md#sni-routing)), with mutual TLS and SPIFFE identities.

## avirtmedproxyd pipeline

avirtmedproxyd's Envoy serves the workload's HTTP/2 connection to its mediator as an h2 listener on `/run/avirt/workload.sock` and terminates each agent `CONNECT` stream. libavirtenvoy runs these steps as Envoy filters:

1. **Authority.** It parses the authority with the same rules as the implemented CONNECT app: an explicit port of 1-65535 written as 1 to 5 ASCII digits with no sign or leading zero, a hostname or IP literal, lowercased, with any trailing dot stripped, and a canonical dotted quad for anything that looks like IPv4, and canonicalizes the host (see [external host canonicalization](./egress-policies.md#external-host-canonicalization)). A malformed authority gets 400 `bad-request`. avirtnetd's forward proxy answers any method other than CONNECT with 501, `plain-http` for an absolute `http://` URI and `unsupported-method` otherwise. An IP-literal authority is an external endpoint whose resource has an `ip` attribute and no `host`, so no hostname rule and no `like` pattern can match it: it is permitted only by a policy that names that exact address in `resource.ip`.
2. **Classification.** It classifies the destination as one of:
   - an avirt name (see [DNS](./dns.md#avirt-names)), which it resolves to its canonical entity (see [name resolution](./egress-policies.md#name-resolution)); a name that resolves to no entity is denied before policy evaluation;
   - a model API host that avirtmodelgwd serves, such as `api.anthropic.com`, `chatgpt.com`, `api.openai.com` and `auth.openai.com`;
   - an external endpoint, for any other name or IP literal.

   An authority that names an avirt host service, such as `avirtsecretsd.svc.<td>`, is refused with 403: the agent never reaches a host service through avirtnetd's forward proxy. Host services are reached only by the mediator's own processes and by the named HTTP/2 streams (`/mcp` to avirtmcpgwd, `/guest-cli` through avirtedgeproxyd).
3. **`connect`.** It evaluates the `connect` action against the workload policy snapshot. A deny in enforce mode gets 403 with the body `egress denied: <host>:<port>`.
4. **`intercept` or `passthrough`.** For an external endpoint or model API host on port 443 it evaluates `intercept`. If it is permitted, Envoy replies `200` and terminates the agent's TLS with a leaf certificate from the workload's MITM CA, with ALPN `http/1.1`, or `h2` and `http/1.1` for a destination that one of the workload's gRPC bindings names (see [egress credentials](./egress-credentials.md#grpc)). Otherwise, and on any other port, it evaluates `passthrough`, and the connection takes TLS passthrough only if that is permitted; with neither permitted the connection is denied. Every avirt name takes TLS passthrough, authorized at avirtedgeproxyd.
5. **Requests.** On a connection under TLS interception, for each request:
   - `Host` must equal the CONNECT host, else 421;
   - it canonicalizes the path and evaluates `request` with the method and path (see [request path canonicalization](./egress-policies.md#request-path-canonicalization));
   - it runs the Codex refresh intercept, an exact endpoint and schema match evaluated before the placeholder misuse check (see [egress credentials](./egress-credentials.md#codex-refresh));
   - it runs the placeholder misuse check (see [egress credentials](./egress-credentials.md#placeholders));
   - it injects any credential whose binding names the mediator as its credential injector.
6. **Upstream.** Model API requests go to avirtmodelgwd over a new mutual TLS connection with the original host as `:authority`. MCP reaches avirtmcpgwd on the `/mcp` stream, not through CONNECT. For a request to any other external endpoint, Envoy opens its own TLS connection to the upstream: it opens an h2 CONNECT stream to avirtedgeproxyd with `:authority` set to the original `host:port`, and runs a new TLS session to the upstream inside it, verifying the upstream against bundled webpki roots, with ALPN `h2` and `http/1.1`. A request that carries `Upgrade` forces `http/1.1`, because an h1 upgrade cannot be bridged onto h2.
7. **TLS passthrough.** A passthrough connection's bytes are forwarded unchanged to avirtedgeproxyd, inside an h2 CONNECT stream that carries the original authority. That covers every avirt name, local or peered: avirtmedproxyd never connects to another mediator or to a peer itself. For an external endpoint, Envoy's TLS inspector reads the agent's first bytes before forwarding any. They must be a ClientHello whose SNI equals the CONNECT host, so an allowed host on a shared CDN address cannot carry a request for another name. A connection that does not start with TLS is closed, unless a `passthrough` permit for that destination has `context.plaintext` true. No credential is ever injected under TLS passthrough.
8. **Events.** It writes the `conn` and `http` events.

Constraints:

- avirtmedproxyd resolves no external names, because it has no network and no resolver of its own. avirtedgeproxyd resolves them when it dials.
- Downstream (agent-facing) TLS is HTTP/1.1, except for a destination that a gRPC binding names, which also offers h2. Upstream may be h2.

## Edge proxy egress

avirtedgeproxyd's egress listener accepts h2 CONNECT streams over mutual TLS from avirtmedproxyd's Envoys and from host services. It is the policy enforcement point for every workload-to-workload connection on this host and between hosts, for services, for host-local services and for external endpoints. For each stream, libavirtenvoy:

1. authorizes the caller's SVID: an avirtmedproxyd Envoy of a live workload at its current generation, or a host service that the host policy snapshot lists. A call from avirtapid, avirtmodelgwd, avirtmcpgwd or avirtregistryd made on behalf of a caller carries that caller in `x-avirt-caller`, and the host policy snapshot is evaluated for both the service and the originating principal (see [components](./components.md#avirtedgeproxyd));
2. resolves an avirt authority to its canonical entity, denying one that resolves to none, and canonicalizes an external host;
3. evaluates the host policy snapshot's `connect` for the caller and the destination entity;
4. routes by destination:
   - a local workload's published port, or a service whose backend is one: a new mutual TLS connection as `/service/avirtedgeproxyd` on the destination's avirtmedproxyd ingress listener, with the caller's identity in `x-avirt-caller` (see [workload topology](./workload-topology.md#ingress));
   - a service whose backend is an authorized host-local service: a dial to the backend address in the record, the one host address the private-address block lets through, and only for that record;
   - the `/guest-cli` stream from an avirtmedproxyd Envoy: to avirtapid, with `x-avirt-caller` set to the `guest-cli` principal (see [identity](./identity.md#trust-domain-and-ids));
   - a peered avirt name: the stream is forwarded to the peer's peer listener, dialing the addresses in that peer's [enrollment record](./peering.md#enrollment-record) in order, authenticated as the enrolled peer's `/service/avirtedgeproxyd` (see [peering](./peering.md));
   - an external endpoint: it resolves the name and applies the private-address block below to every resolved address;
5. dials, with a 10 s timeout, only the addresses it checked, with no re-resolution in any later layer, and checks every retry's address again; then replies `200`, and relays bytes in both directions with no idle timeout, because model API streams are long-lived.

| Status | Cause |
| --- | --- |
| 403 | `private-dest`, an avirt name that resolves to no entity, or a caller or destination the host policy snapshot denies in enforce mode |
| 502 | DNS or dial failure |
| 504 | Connect timeout |

The private-address block refuses an address in any non-global range listed under [private-address block](#private-address-block-private-dest), and any address assigned to one of the host's own interfaces, global ones included. avirthostd pushes the addresses of the host's own interfaces to avirtedgeproxyd and pushes an update whenever they change, so an address added while a workload runs is covered. The interfaces inside the edge VM are not the source (see [edge VM network](./networking.md#edge-vm-network)). A dial to a peer's peer listener is exempt from the non-global range refusal, private and CGNAT ranges included, but only for the addresses in that peer's enrollment record, and never for an address of the host's own interfaces.

avirtedgeproxyd never sees the plaintext of a connection whose TLS avirtmedproxyd or a gateway opens to the upstream: that TLS session runs inside the tunnel. Workload-to-workload bytes are opaque to it too. It records one `conn` event per connection, attributed to the caller's workload, with `src: "edgeproxy"`.

## Workload DNS

Workload processes get normal DNS answers, for avirt names and for public names, because many tools resolve a name before they connect.

1. avirtinitd binds UDP and TCP port 53 on `127.0.0.53` and avirtnetd's forward proxy listener on `127.0.0.1` as root, hands the sockets to avirtnetd, and writes `/etc/resolv.conf` to point at `127.0.0.53`.
2. avirtnetd's stub resolver accepts each query and sends it to the mediator on the `/dns` stream (see [workload topology](./workload-topology.md#http2-streams)).
3. libavirtenvoy in avirtmedproxyd's Envoy evaluates the `resolve` action for the name. A name the workload may not reach gets `NXDOMAIN`. `resolve` is always enforced, in audit mode too, so a denied name never leaves the mediator.
4. It sends avirt names to avirtdnsd. It sends public names to avirtedgeproxyd's DNS proxy, which is not a resolver tunnel: avirtedgeproxyd parses each query, evaluates the host policy snapshot's `resolve` for the caller and the name, resolves it itself, evaluates `resolve` again for every name in a CNAME chain, and returns a size-limited answer. A compromised mediator therefore cannot bypass the host's DNS policy.
5. It records a `dns` event for every query.

Public names get their real answers. Answers for avirt names follow avirtdnsd's model: they carry avirtedgeproxyd's address. A proxy-aware client still sends the name in its CONNECT, which is what avirtmedproxyd routes by, and a CONNECT to a bare avirtedgeproxyd address carries no name to route by and is refused. Direct DNS to any other address fails, because the workload has no network. DNS is an exfiltration path, so every query is subject to policy and logged.

## Websockets

After termination the downstream connection is h1, so a websocket upgrade arrives as an ordinary `GET` with `Upgrade: websocket`. It passes through the request steps, where the placeholder misuse check runs and the request is logged. A model API upgrade goes to avirtmodelgwd, which injects the credential on the upgrade request. Codex's Responses API runs over WSS with its auth headers on the upgrade request. On a `101`, both sides switch to relaying bytes. Frames are neither substituted nor scanned. The `http` event is written when the upgraded connection closes, with its byte counts.

## MITM CA and leaf certificates

`avirthostd` mints one MITM CA per workload (rcgen, ECDSA P-256, CN `avirt workload <uuid>`, 7-day validity, `KeyCertSign` only), so that it survives mediator restarts, and passes the cert and key to avirtmediatord in `GET /workload`. avirtmediatord hands them to avirtmedproxyd, which serves them to libavirtenvoy over its Envoy's local SDS socket. libavirtenvoy mints leaves on demand and caches up to 256 per host: P-256, SAN set to the host, 24 h validity, chain of leaf plus CA. The CA key never enters the workload container.

The CA is scoped to what the workload's policy can intercept. When it creates the workload, avirthostd asks avirtpolicyd's prover for the set of hosts the workload policy snapshot can permit `intercept` for (see [MITM CA name constraints](./egress-policies.md#mitm-ca-name-constraints)):

- **A bounded set.** The CA carries X.509 name constraints whose permitted DNS subtrees are exactly those hosts and domains, so a leaf for any other name fails validation in the agent.
- **An empty set.** avirthostd issues no MITM CA, `confirm` carries none, and every external connection must be permitted by `passthrough`.
- **An unbounded set.** The CA carries no name constraints.

libavirtenvoy never mints a leaf outside the CA's constraints. A later policy snapshot that permits `intercept` for a host outside them cannot terminate that host's TLS: the connection takes TLS passthrough if `passthrough` is permitted for it, and is denied otherwise.

## CA delivery into the workload

The CA cert arrives in `confirm`. Before the agent starts, avirtinitd:

1. writes `/run/avirt/ca.pem`,
2. appends it to `/etc/ssl/certs/ca-certificates.crt` (image-local, as root),
3. writes `/run/avirt/ca-bundle.pem` (system bundle plus the MITM CA).

It then sets `NODE_EXTRA_CA_CERTS=/run/avirt/ca.pem`, and sets `SSL_CERT_FILE`, `CURL_CA_BUNDLE`, `REQUESTS_CA_BUNDLE`, `GIT_SSL_CAINFO`, `npm_config_cafile` and `CODEX_CA_CERTIFICATE` all to `/run/avirt/ca-bundle.pem`.

| Client | Trust behavior |
| --- | --- |
| Claude Code 2.1.300 | No certificate pinning. `CLAUDE_CODE_CERT_STORE` defaults to `bundled,system`, and it appends `NODE_EXTRA_CA_CERTS`. It does not load `SSL_CERT_FILE`. |
| Codex 0.160.1 | No pinning. rustls with native roots. `CODEX_CA_CERTIFICATE` takes precedence over `SSL_CERT_FILE`, and both its HTTP and websocket clients add the bundle. |
| npm | Honors `NODE_EXTRA_CA_CERTS` and `cafile`. No pinning. |
| git | Honors `GIT_SSL_CAINFO`. No pinning. |

A client that rejects the MITM CA needs a policy that forbids `intercept` and permits `passthrough` for its hosts. Those hosts are then audited at L4 only and get no credential injection.

## Events

Each connection becomes one `conn` event, each intercepted request one `http` event, and each workload DNS query one `dns` event. libavirtenvoy in avirtmedproxyd's Envoy posts them to avirthostd on `POST /workload/events` (`src: "mediator"`), avirtedgeproxyd posts its own `conn` and `dns` events (`src: "edgeproxy"`), and avirtmodelgwd and avirtmcpgwd post the credential outcome of every request they serve (`src: "gateway"`). Each producer queues its events in a bounded ring and never blocks a worker thread on delivery, and numbers them in its own `seq` domain so avirthostd can deduplicate them across reconnects. While a workload's ring is full, its Envoy refuses that workload's new connections, requests and DNS queries with 503 until the ring drains, so no event is dropped silently. avirtedgeproxyd keeps one ring per workload, so one workload's backlog never stalls another's. Every event carries `workload`, `gen` and `seq` (see [trust boundaries](./trust-boundaries.md#event-provenance)).

The `http` event has these fields:

- `conn_id`, `method`, `host`,
- `path` (query string dropped) and `query: bool`,
- `status`, `bytes_up`, `bytes_down`, `duration_ms`,
- `decision` (`allow`, `deny` or `would-deny`), `rule` (the id of the Cedar policy that decided), `mode`, `policy_version`,
- `cred: [{name, location, action: "injected"|"rejected"}]`, for credentials avirtmedproxyd injects itself. The gateways' own events record theirs.

The `dns` event has `name`, `qtype`, `rcode`, `decision`, `rule` and `policy_version`.

Secret values, placeholder values, header values and bodies are never logged. How these events become audit records, and their severities, are in [observability](./observability.md#audit-events) and [severity](./observability.md#severity).

## Policy

Egress policy is Cedar, evaluated locally in each Envoy from a versioned policy snapshot: avirtmedproxyd's Envoy evaluates the workload policy snapshot's `resolve`, `connect`, `intercept`, `passthrough`, `request` and `ingress`, and avirtedgeproxyd's Envoy evaluates the host policy snapshot's `resolve` and `connect`. The schema, name resolution, the action semantics, modes, policy snapshots, revocation, the prover and failure behavior are in [egress policies](./egress-policies.md). Credential bindings and injection are in [egress credentials](./egress-credentials.md).

## Mediator network

The mediator has no network interface other than loopback, on every runtime. It has no NAT, no bridge, no resolver and no firewall ruleset, and it runs without `CAP_NET_ADMIN`. Its only connections are the runtime-provided Unix sockets in [workload topology](./workload-topology.md#runtime-provided-sockets). The private-address block runs at avirtedgeproxyd, where external names are resolved, and avirtedgeproxyd is the only host service that opens connections off the host, from the edge VM (see [edge VM](./components.md#edge-vm)).
