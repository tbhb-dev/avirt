---
type: Component Design
title: Egress, TLS termination and policy
description: How the agent's network traffic leaves the workload, the supervisor's CONNECT proxy and destination checks, TLS termination in Pingora, the egress policy file, the event format and the supervisor's network.
tags: [agentvmm, egress, proxy, pingora, tls, policy]
implementation: partial
generated: { by: claude-code/claude-opus-5-5, at: 2026-10-17T14:23:31Z }
---

# Status: implemented

## Path

The workload runs with `--network none`, so TCP, UDP, DNS and IPv6 all fail inside it and only loopback works. The agent's only way out is the runner's loopback relay, which forwards to the supervisor's CONNECT proxy on `proxy.sock` (see [actor topology](./actor-topology.md)). Every client must honour the proxy variables. Claude Code, Codex, Node `fetch` and `https` (with `NODE_USE_ENV_PROXY=1`), npm, git and curl all do. `git://` and `ssh://` do not work.

## Supervisor process

```text
agentvmm-supervisor --dir DIR --controller-sock PATH --actor ID
```

It runs one Pingora 0.9.0 `Server` with two Unix-socket services at 0600: the CONNECT app on `proxy.sock` and the uplink splice on `uplink.sock`. Stdin EOF or SIGINT means fast shutdown and SIGTERM graceful shutdown. Either way it first flushes every open connection as an event with `error: "shutdown"`. Stdin EOF ties its lifetime to the controller's, so a SIGKILLed controller takes every supervisor down with it.

## CONNECT app

Connections are never reused, and there is no peer check, because the socket's mode is the access control.

1. Read the request head (at most 8 KiB, 10 s timeout) and parse it with `httparse`. A connection that closes before sending a byte (a readiness probe) is ignored. Bytes after the blank line, such as an early TLS ClientHello, are forwarded upstream.
2. Accept only `CONNECT host:port` with an explicit port of 1-65535 and a hostname or IP literal, lowercased, with any trailing dot stripped. An authority that looks like IPv4 must be a canonical dotted quad, so `2130706433`, `0177.0.0.1`, `0x7f000001` and `127.1` are malformed.
3. Resolve the name with `tokio::net::lookup_host` and apply the destination check below.
4. Dial each address, 10 s overall. Reply `200 Connection Established`, flush, and copy in both directions with byte counts until either side closes or shutdown fires. There is no idle timeout, because model API streams are long-lived.

| Status | Cause |
| --- | --- |
| 400 | Malformed head or authority (`bad-request`) |
| 403 | `private-dest` |
| 501 | Any method other than CONNECT (`plain-http` for an absolute `http://` URI, else `unsupported-method`) |
| 502 | DNS or dial failure |
| 504 | Connect timeout |

Every other destination is allowed and logged under rule `allow-all`.

## Destination check (`private-dest`)

A resolved address is refused when it is in any non-global range: loopback, RFC 1918, CGNAT, link-local, benchmarking, documentation, multicast, reserved and broadcast, and for IPv6 also unique-local, site-local, Teredo, discard, NAT64 (`64:ff9b::/96`, `64:ff9b:1::/48`), 6to4 (through its embedded IPv4) and IPv4-mapped or IPv4-compatible addresses. Every address assigned to one of the host's own interfaces is also refused, global ones included, read with `getifaddrs` on each CONNECT.

## Events

Each proxied connection becomes one `dir: "egress"` line in `logs/<actor-id>.ndjson`, written when the connection closes. Connections still open at shutdown are flushed with `error: "shutdown"`.

```json
{"ts":"…","dir":"egress","msg":{"event":"conn","actor":"7e759259","started":"…","method":"CONNECT","host":"api.anthropic.com","port":443,"decision":"allow","rule":"allow-all","bytes_up":1234,"bytes_down":56789,"duration_ms":4200,"error":null}}
```

To list the hosts a set of actors used: `jq -r 'select(.dir=="egress") | .msg | "\(.host):\(.port)"' logs/*.ndjson | sort | uniq -c`.

# Status: planned

## Supervisor pipeline

All of this runs inside one `agentvmm-supervisor` process:

1. The **mux** accepts each h2 CONNECT stream from the link and splices it into the local CONNECT listener on `/run/supervisor/connect.sock`.
2. The **CONNECT app** parses the head, normalizes the authority, resolves it, applies `private-dest` against the host addresses passed in by the controller (`deny_addrs`), and evaluates the policy at L4. A rule with `tls: skip`, or a port other than 443, takes the opaque path described above.
3. Otherwise it replies `200`, flushes, and accepts TLS with an `Acceptor` built per tunnel from `TlsSettings::with_callbacks(TunnelInfo{authority, conn_id})`, `set_cert_resolver(cache)` and ALPN `http/1.1`. `handshake_complete_callback` stores `TunnelInfo` in the `SslDigest` extension.
4. The TLS stream goes to one shared `Arc<HttpProxy<Egress>>` built with `HttpProxy::new` and driven through `ServerApp::process_new`.
5. `Egress: ProxyHttp`:
   - `request_filter` reads `TunnelInfo` and requires `Host` to equal the CONNECT host (else 421). It evaluates the L7 rules, substitutes credentials (see [credentials](./credentials.md)), and runs the leak scan.
   - `request_body_filter` scans the body for issued placeholders, carrying the last `max_placeholder_len - 1` bytes between chunks.
   - `upstream_peer` returns `HttpPeer(resolved address, tls=true, sni=host)` with ALPN `H2H1`, verifying against bundled webpki roots. When the request carries `Upgrade`, it forces ALPN `H1`, because an h1 upgrade cannot be bridged onto h2.
   - `logging` writes the `http` event.

Constraints:

- Pingora's TLS backend is rustls (`pingora-rustls`, which uses `ring`), which builds on macOS and linux-arm64 without cmake, Go or a system OpenSSL.
- The rustls backend never invokes `certificate_callback`, so leaf certificates are selected through `set_cert_resolver`.
- Downstream (agent-facing) TLS is HTTP/1.1 only. Upstream may be h2.

## Websockets

After termination the downstream connection is h1, so a websocket upgrade arrives as an ordinary `GET` with `Upgrade: websocket`. It passes through `request_filter`, where the credential is substituted, the leak scan runs and the request is logged. On a `101`, Pingora switches both sides to upgraded-body mode and relays bytes. Frames are neither substituted nor scanned. The `http` event is written when the upgraded connection closes, with its byte counts. Codex's Responses API runs over WSS with its auth headers on the upgrade request.

## MITM CA and leaf certificates

The controller mints one MITM CA per actor (rcgen, ECDSA P-256, CN `agentvmm actor <id>`, 7-day validity, `KeyCertSign` only) so that it survives supervisor restarts, and passes the cert and key to the supervisor in `GET /actor`. The supervisor caches up to 256 leaves per host: P-256, SAN set to the host, 24 h validity, chain of leaf plus CA. The CA key never enters the workload.

## CA delivery into the workload

The CA cert arrives in `confirm`. Before the agent starts, the runner:

1. writes `/run/agentvmm/ca.pem`,
2. appends it to `/etc/ssl/certs/ca-certificates.crt` (image-local, as root),
3. writes `/run/agentvmm/ca-bundle.pem` (system bundle plus the actor CA).

It then sets `NODE_EXTRA_CA_CERTS=/run/agentvmm/ca.pem`, and sets `SSL_CERT_FILE`, `CURL_CA_BUNDLE`, `REQUESTS_CA_BUNDLE`, `GIT_SSL_CAINFO`, `npm_config_cafile` and `CODEX_CA_CERTIFICATE` all to `/run/agentvmm/ca-bundle.pem`.

| Client | Trust behavior |
| --- | --- |
| Claude Code 2.1.300 | No certificate pinning. `CLAUDE_CODE_CERT_STORE` defaults to `bundled,system`, and it appends `NODE_EXTRA_CA_CERTS`. It does not load `SSL_CERT_FILE`. |
| Codex 0.160.1 | No pinning. rustls with native roots. `CODEX_CA_CERTIFICATE` takes precedence over `SSL_CERT_FILE`, and both its HTTP and websocket clients add the bundle. |
| npm | Honors `NODE_EXTRA_CA_CERTS` and `cafile`. No pinning. |
| git | Honors `GIT_SSL_CAINFO`. No pinning. |

A client that rejects the actor CA gets a `tls: skip` rule. That host is then audited at L4 only and gets no credential injection.

## Per-request event

`dir: "egress"`, `event: "http"`, with these fields:

- `conn_id`, `method`, `host`,
- `path` (query string dropped) and `query: bool`,
- `status`, `bytes_up`, `bytes_down`, `duration_ms`,
- `decision` (`allow`, `deny` or `would-deny`), `rule`, `mode`,
- `cred: [{name, location, action: "injected"|"rejected"}]`.

Secret values, placeholder values, header values and bodies are never logged. The per-connection `conn` event stays.

## Policy file

The controller loads one YAML policy at startup (default `policy/default.yaml`, overridden by `AGENTVMM_POLICY`) and serves it to each supervisor in `GET /actor`. A new policy applies to actors launched after the controller loads it.

```yaml
version: 1
mode: audit                 # audit: log would-deny, allow. enforce: deny.
rules:                      # first match wins; no match = default
  - name: anthropic
    host: api.anthropic.com
    ports: [443]
    http: [{methods: [POST, GET], path: "/v1/*"}, {path: "/api/*"}]
  - name: oai-content
    suffix: oaiusercontent.com
    tls: skip               # opaque CONNECT, L4 rules only, no credentials
  - name: npm
    host: registry.npmjs.org
default: allow              # enforce mode requires default: deny
credentials:
  - name: claude-oauth
    source: env:CLAUDE_CODE_OAUTH_TOKEN      # resolved by the controller, never the supervisor
    workload: {env: CLAUDE_CODE_OAUTH_TOKEN, shape: "sk-ant-oat01-"}
    inject: {header: authorization, value: "Bearer {}"}
    bind: [{host: api.anthropic.com, port: 443, paths: ["/v1/*", "/api/*"]}]
```

Matching rules:

- `host` is an exact match on the normalized CONNECT host.
- `suffix` matches the name itself or any subdomain (`h == s`, or `h` ends with `"." + s`).
- An IP literal matches only an identical `host` entry.
- The first matching rule wins, and its `name` becomes the event's `rule`.

`private-dest` and credential bindings apply in both modes. In enforce mode, no match returns 403 with the body `egress denied: <host>:<port>` and a `decision: "deny"` event with `rule: null`. Codex downloads large artifacts from several `*.oaiusercontent.com` hosts at startup, so an enforce-mode allowlist has to include them.

## Supervisor network

The supervisor runs on its own per-actor network with NAT egress: a `VmnetNetwork` NAT interface on the Containerization runtime, or a labelled bridge network on Docker Desktop (see [compute runtimes](./compute-runtimes.md)). Its image is `ubuntu:26.04` with `nftables`, `ca-certificates` and `agentvmm-supervisor`, and it runs with `CAP_NET_ADMIN`.

`agentvmm-supervisor` starts as root and fetches `GET /actor`. It loads the ruleset below through `nft -f -` and reads it back with `nft -j list ruleset`, refusing to continue unless the drop sets are present. It then clears every capability set, sets `no_new_privs`, and stays uid 0, because Docker's file-mounted controller socket maps in as root-owned.

```text
table inet agentvmm {
  set deny4 { type ipv4_addr; flags interval; elements = { 0.0.0.0/8, 10.0.0.0/8, 100.64.0.0/10,
    127.0.0.0/8, 169.254.0.0/16, 172.16.0.0/12, 192.168.0.0/16, 198.18.0.0/15, 224.0.0.0/3,
    $GATEWAY, $HOST_V4... } }
  set deny6 { type ipv6_addr; flags interval; elements = { ::/127, ::ffff:0:0/96, 64:ff9b::/96,
    64:ff9b:1::/48, 2002::/16, fc00::/7, fe80::/10, ff00::/8, $HOST_V6... } }
  chain out { type filter hook output priority 0; policy accept;
    oif lo accept
    meta l4proto { tcp, udp } th dport 53 ip daddr { $DNS_ALLOW } accept
    ip daddr @deny4 drop
    ip6 daddr @deny6 drop
    meta l4proto udp drop }
  chain in { type filter hook input priority 0; policy drop;
    iif lo accept
    ct state established,related accept }
}
```

- `$DNS_ALLOW` and any extra deny entries come from the runtime: `1.1.1.1` and `9.9.9.9` on the Containerization runtime, and `192.168.65.7:53` on Docker Desktop.
- `$HOST_V4` and `$HOST_V6` are the controller's `getifaddrs` at launch, global addresses included. The LAN router falls under the private ranges.
- The NAT gateway is in `deny4` on both runtimes. On the Containerization runtime, the NAT subnet's peer VMs fall under `192.168.0.0/16`.
- A host address added while an actor runs is not picked up.
- The CONNECT app's `private-dest` check still runs against the same set.
