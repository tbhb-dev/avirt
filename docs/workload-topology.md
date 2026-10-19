---
type: Protocol Specification
title: Workload topology and connections
description: The connections inside one workload and between workloads (workload container, mediator, avirthostd, host services, avirtedgeproxyd), the runtime-provided Unix sockets and avirthostd's SNI routing, the HTTP/2 streams on the workload's connection to its mediator, launch handshake, freeze, reconnect, resuming the `/envelopes` stream and port publication.
tags: [avirt, workload, protocol, http2]
implementation: partial
generated: { by: codex/gpt-6, at: 2026-10-18T05:20:59Z }
---

# Status: implemented

## Sockets

The workload container uses `--network none`. The runtime mounts two Unix socket files into it with `-v` from the workload's private directory `workloads/<id>/` (0700). For each mounted socket file, the runtime forwards connections to the host process listening on it. Both files are mode 0600 on the host, which maps through, so only root in the workload container can open them.

| Guest path | Host listener | Traffic |
| --- | --- | --- |
| `/run/avirt/mediator.sock` | Mediator | The workload's HTTP/2 connection to its mediator, which today transports only log envelopes, on its one request `POST /workloads/<id>/stream`. The mediator forwards its bytes unchanged to avirthostd. |
| `/run/avirt/proxy.sock` | Mediator | The CONNECT proxy (see [egress](./egress.md)) |

To connect to avirthostd, the mediator dials its socket `hostd/hostd.sock` (directory 0700, socket 0600). Each connection starts with a one-line connection header naming its kind, the workload and the mediator's secret (see [identity](./identity.md)), read within 5 s:

- `WORKLOAD <id> <secret>`: `avirthostd` binds the connection to that workload and serves only that workload's `POST /workloads/<id>/stream` route over it. The mediator then forwards the HTTP/2 bytes unchanged between avirtinitd (client) and avirthostd's stream handler (server).
- `EGRESS <id> <secret>`: plain NDJSON egress events, one per proxied connection. `avirthostd` wraps each in the log envelope with `dir: "egress"`. avirthostd accepts only one event connection per workload.

At startup, the mediator opens the `EGRESS` connection. It dials a `WORKLOAD` connection only when the workload's first bytes arrive on `mediator.sock`, so a readiness probe (connect and close) costs avirthostd nothing.

## Connection to the mediator

`avirtinitd` connects to `mediator.sock` within 5 s and completes the HTTP/2 handshake within 5 s. It sends `POST /workloads/<id>/stream` with its connection token (`AVIRT_CONNECTION_TOKEN`), then waits 4 s for the response headers. Retries wait 300 ms. The total limit is 15 s. A 403, 404, 409, or 410 ends retries. A socket file without a listener accepts a connection but never responds, so each step has a timeout. Starting the TCP proxy to `proxy.sock` and the agent requires avirthostd's acceptance of that request. Otherwise, `avirtinitd` fails with a message that identifies the mediator.

## TCP proxy to `proxy.sock`

`avirtinitd` listens on an ephemeral `127.0.0.1` port. It forwards each connection to `proxy.sock`. The agent connects to it through `HTTPS_PROXY` and the related variables (see [workload hardening](./workload-hardening.md)).

Forwarded sockets must never half-close. With a `-v` socket-file mount, sending `SHUT_WR` immediately after the last write loses the peer's reply. To avoid that loss, the TCP proxy omits forwarding a client's end-of-stream. After the client finishes, each read on the other direction has a 30 s idle timeout. When the host side ends or a read idles out, the proxy notifies the client and closes. The log can show a proxied connection lasting longer than the client's connection.

# Status: planned

## Runtime-provided sockets

The workload container and the mediator have no network interface other than loopback. Each connection they make is on a Unix socket that the compute runtime provides (see [compute runtimes](./compute-runtimes.md) and [networking](./networking.md)). The workload container has exactly one, for the workload's HTTP/2 connection to its mediator. The mediator has four.

| Socket | Container end | Host end | Initiated by | TLS client and server |
| --- | --- | --- | --- | --- |
| Workload connection | Workload container: `/run/avirt/mediator.sock`, dialed by avirtnetd. Mediator: `/run/avirt/workload.sock`, served by avirtmediatord. | avirthostd, which forwards the bytes unchanged | The workload side | avirtnetd, avirtmediatord |
| Connection to avirthostd | Mediator: `/run/avirt/hostd.sock` | avirthostd | avirtmediatord | The mediator process, avirthostd |
| Outbound socket to avirthostd | Mediator: `/run/avirt/svc.sock` | avirthostd's SNI routing | avirtmediatord | The mediator process, the destination |
| avirtmediatord's ingress listener | Mediator: `/run/avirt/ingress.sock`, served by avirtmediatord | avirthostd's SNI routing | avirthostd, for avirtedgeproxyd | avirtedgeproxyd, avirtmediatord |

- **Containerization.** A vsock connection between avirthostd and the guest backs each socket. The runtime's vsock-to-Unix-socket forwarder presents this connection as the Unix socket in the table. The workload VM's vsock device has exactly one host listener, for the workload's connection, which avirthostd forwards unchanged to the current mediator generation's `workload.sock`. This lets avirtnetd open a new connection to its mediator whenever it needs one. Each other host listener is on the mediator VM's device. seccomp denies `AF_VSOCK` to each workload process, so only the runtime's forwarder uses the workload VM's vsock device.
- **Docker Desktop.** The workload's connection goes through a per-workload Docker named volume. The mediator's connection to avirthostd and its outbound socket to avirthostd are single-file bind mounts of the host's `workloads/<id>/c.sock` and `workloads/<id>/s.sock`. avirtmediatord's ingress listener is open (see [components](./components.md#open-questions)).

Each vsock connection, bind mount, and named volume transports ciphertext only.

## SNI routing

avirthostd reads only the TLS ClientHello's SNI on each connection on a mediator's outbound socket, and forwards the connection unchanged to the destination the SNI names, if that destination is in the workload's allowed set. On both the outbound socket and `services/ingress-relay.sock`, missing or unknown SNI and destinations outside the allowed set cause avirthostd to close the connection before forwarding any bytes. It never decrypts TLS on these connections. Each destination authenticates the caller by its SVID and checks authorization (see [trust boundaries](./trust-boundaries.md#listeners)). The outbound socket permits these destinations:

| SNI | Destination | Allowed when |
| --- | --- | --- |
| `<name>.svc.<td>` | The host service's listener `services/<name>.sock` | Always, for avirtsecretsd, avirtpolicyd, avirtdnsd, avirtoteld, avirtmcpgwd and avirtmodelgwd, and for avirtregistryd from a build job's outbound socket only |
| `edgeproxy.svc.<td>` | avirtedgeproxyd's egress listener | Always |

avirthostd also serves one host-side listener for avirtedgeproxyd alone, `services/ingress-relay.sock`, checked with peer credentials (`LOCAL_PEERCRED` on macOS, `SO_PEERCRED` on Linux). A connection there specifies its destination by SNI `<uuid>.ingress.<td>`, and avirthostd forwards it unchanged to that workload's avirtmediatord ingress listener at its current generation. SNI routing excludes direct routes between mediators.

## HTTP/2 streams

| Layer | avirtmediatord | avirtnetd |
| --- | --- | --- |
| TLS 1.3 (mutual, SPIFFE) | Server | Client |
| HTTP/2 | Server | Client |

One HTTP/2 connection per mediator generation transports each stream. avirtnetd opens each stream, and avirtmediatord routes each one:

| Stream | Request | Opened for | Routed to |
| --- | --- | --- | --- |
| control | `POST /control`, always the first stream | avirtnetd | avirtmediatord, NDJSON messages in both directions for the life of the connection |
| envelopes | `POST /envelopes` with `avirt-resume-seq: N` | Envelopes from avirtinitd, the main workload process and avirtnetd, each containing a `seq` | avirthostd, `POST /workload/envelopes` |
| egress | `CONNECT host:port`, one per agent connection | avirtnetd's forward proxy | The egress pipeline (see [egress](./egress.md#how-avirtmediatord-handles-egress)) |
| guest CLI | `POST /guest-cli` | avirtguest's gRPC, through `guest.sock` | avirtedgeproxyd, then avirtapid, under the `guest-cli` principal (see [identity](./identity.md#trust-domain-and-ids)) |
| MCP | `POST /mcp` | avirtmcp, through `mcp.sock` | avirtmcpgwd |
| DNS | `POST /dns`, one stream per query, `content-type: application/dns-message` | avirtnetd's stub resolver, one DNS wire message in each request and response | avirtmediatord's DNS policy, then avirtdnsd for avirt hostnames or avirtedgeproxyd's DNS proxy for public names (see [egress](./egress.md#workload-dns)) |
| OTLP | `POST /otlp` | avirtnetd and the main workload process | avirtoteld, with `x-avirt-src: workload` (see [observability](./observability.md#opentelemetry-logs)) |
| inbound connection notices | `POST /inbound-notices`, long-lived | avirtnetd | avirtmediatord, which sends one notice per inbound connection |
| ingress | `CONNECT /ingress` with `:protocol: connect-tcp` (RFC 8441) with `avirt-ingress-id: I` | avirtnetd, accepting an inbound connection notice | The inbound connection waiting under `I` (see [ingress](#ingress)) |

An egress stream's response includes the pipeline's status, including avirtedgeproxyd's 403 `private-dest`, 502, and 504. The flow-control and admission rules below preserve control, DNS and event progress under saturation in either direction.

avirtnetd dials `/run/avirt/mediator.sock` with a 50 ms to 1 s backoff, because dials made before avirtmediatord listens fail, for up to the 30 s launch budget (see [lifecycle](./lifecycle.md#launch-order)). An ended workload container and one awaiting readiness look identical to the mediator. avirthostd must stop the mediator explicitly.

### Admission and flow control

Both endpoints explicitly limit concurrent HTTP/2 streams to 128. Across the active and draining connections, each workload has at most 96 bulk streams (egress and ingress together), 16 DNS queries, and 16 other streams. The last group reserves one slot each for `/control`, `/envelopes`, and `/inbound-notices`. Other routes share its remaining 13 slots. avirtnetd refuses work at a cap instead of queuing it. The mediator server enforces the same caps independently of the client.

Each endpoint sets the per-stream receive window to 256 KiB and the connection receive window to 48 MiB: `128 × 256 KiB + 16 MiB`. HTTP/2 has one shared connection window and can't reserve part of it for a stream. Unread stream data never exceeds the stream window. The receiver releases credit only as it consumes data, with separate bounds on request queues. The connection window must cover all stream windows plus the 16 MiB reserve, including credit that the receiver hasn't yet advertised.

Transport buffers and request queues have a combined limit of 64 MiB per workload at each endpoint, across both connections. Each endpoint refuses new streams before exceeding that budget. Control, DNS, and event streams must progress under bulk saturation in both directions with bounded memory.

A test with the real client and server stops bulk readers in each direction and exhausts the 96-stream limit. Measurements cover control round trips, DNS completions, envelope progress, and peak memory. Measurements include connection rotation and simultaneous DNS bursts. Passing this test is a prerequisite for using these settings.

avirtmediatord's server enforces a 16 KiB decoded header-list limit and a 10 s header deadline. It allows at most 16 streams awaiting request headers. A workload may open or reset at most 128 streams per second, with a burst of 128. Exceeding that rate closes the connection. Root inside the workload container can't bypass these server limits by replacing avirtnetd.

avirtedgeproxyd independently enforces 128 new streams per second per workload, burst 128, across its connections. Excess streams receive 503. Each server includes headers and TLS buffers in its memory budget. Stream buffers and request queues count toward the same budget.

### Timeouts and keepalive

Both ends enforce these stream timeouts in avirt code. Idle means no stream data bytes in either direction. HTTP/2 `PING` doesn't reset it. Each stream also ends at connection drain, identity expiry, or revocation.

| Stream | Idle timeout | Duration limit |
| --- | --- | --- |
| `/control` | 30 s. Each end sends a heartbeat on this stream every 10 s | Connection lifetime |
| `/envelopes` | 60 s. avirtnetd sends a heartbeat record with no payload every 20 s, excluded from the log | Connection lifetime |
| `/inbound-notices` | 30 s. The mediator sends a heartbeat notice with no payload every 10 s | Connection lifetime |
| `/dns` | 5 s | 10 s per query |
| Egress CONNECT and `/ingress` tunnels | 15 minutes | Connection lifetime |
| `/guest-cli`, `/mcp`, `/otlp` | 60 s | Connection lifetime |

The mediator sends HTTP/2 `PING` every 5 s with a 10 s acknowledgment timeout. avirtnetd sends `PING` every 10 s with a 10 s acknowledgment timeout. `PING` establishes transport health only. It doesn't establish stream progress. avirtedgeproxyd applies the same 15-minute tunnel idle timeout and the limit set by certificate expiry to egress tunnels.

Each egress stream has an independent half-close through HTTP/2 `END_STREAM`. avirtnetd and avirtmediatord propagate it to the corresponding write half while continuing to read the other direction. An end-to-end test must deliver a reply after the client half-closes. The test must exercise both directions through the actual runtime transport before avirtnetd removes its implemented idle-timeout fallback.

## Launch handshake

After the mutual TLS handshake (see [identity](./identity.md)):

1. avirtnetd → `{"t":"attach","last_gen":G,"proto":P}`. avirtmediatord refuses an unsupported control protocol version with `{"t":"refused","reason":"protocol"}`.
2. avirtmediatord → `{"t":"attached","gen":g}`, echoing the generation from its SVID.
3. avirtnetd rotates its bootstrap SVID (see [identity](./identity.md#rotation)).
4. avirtnetd opens the `/envelopes` stream, and avirthostd accepts it.
5. avirtmediatord → `{"t":"confirm","gen":g,"ca_pem":…,"env":{…placeholders…},"files":{"codex/auth.json":…},"mode":"audit","policy_version":V,"publications":[…]}`
6. avirtnetd hands the `confirm` contents to avirtinitd. avirtinitd installs the MITM CA and writes the files. It starts the main workload process and avirtguestd, and launches the agent at the main workload process's request (see [components](./components.md#workload)). avirtnetd replies `{"t":"started","pid":P}`.

avirtmediatord sends `confirm` only after fetching workload configuration and rotating its SVID. Its listeners must be ready. Readiness also requires avirtmediatord to load the policy snapshot and placeholders. It must load the MITM CA when the workload configuration includes one. avirthostd must report the accepted `/envelopes` stream with `{"t":"envelopes_accepted"}` on `GET /workload/control`.

avirtmediatord keeps readiness as internal state and sends `confirm` only while all conditions hold. Agent processes start only after `confirm`.

## Freeze and reconnect

avirtnetd distinguishes three transitions of its connection to the mediator:

- **Rotation.** After an SVID rotation, avirtnetd opens a new connection to the same generation with its current SVID. It sends `{"t":"reattach","gen":g}` on a new `/control` stream, then resumes the `/envelopes` stream by `seq` and reopens `/inbound-notices`. It stops opening streams on the old connection as soon as the new one attaches, then sends GOAWAY. At most one old connection drains for 30 s. Quotas cover both connections. Pending DNS queries may finish during the drain. Tunnels end at the drain deadline or certificate expiry, whichever comes first, and never migrate. Rotation leaves the processes running.
- **Reconnect.** When a connection fails, avirtnetd reconnects to the same generation in the same way. Until attachment to a connection of the current or a higher generation, avirtnetd reports the disconnected state to avirtinitd. avirtinitd adds a `connection` freeze reason.
- **Replacement.** A mediator with a higher generation completes the handshake, `attach` and `confirm`, and supersedes the older generation.

While any freeze reason exists, avirtinitd writes `1` to the agent cgroup's `cgroup.freeze` and waits until `cgroup.events` shows `frozen 1`. The freeze covers each process in the agent cgroup, including tools that left the agent's process group, and leaves avirtnetd, the main workload process, and avirtguestd running. If attachment and confirmation of a connection to the same or a higher generation complete within 30 s, avirtinitd removes the `connection` freeze reason. Missing the 30 s deadline makes avirtinitd write `1` to `cgroup.kill` and exit 2 with `error: mediator lost`. After removing the reason, avirtinitd checks for other freeze reasons. Only when none remain does it write `0` to `cgroup.freeze` and resume the agent. Egress connections open through the lost connection end. Clients retry them.

A workload container whose guest lacks a writable cgroup v2 hierarchy with `cgroup.freeze` and `cgroup.kill` doesn't start its agent.

`avirthostd` restarts a mediator that exits unexpectedly, at most twice per workload, each time with the generation incremented. Once confirmation of a newer generation completes, avirtnetd sends GOAWAY on any older connection, and avirthostd fences the older generation at each host listener (see [identity](./identity.md#authorization)).

## Resuming after reconnect

`avirthostd` writes each envelope to the workload log before counting its `seq` as written. It tracks the highest written `seq` and returns it as `avirt-last-seq` when the `/envelopes` stream resumes. avirtnetd replays from a 4 MiB ring of sent envelopes and writes a `meta` `envelope_gap` event when the ring no longer covers the gap. `avirthostd` deduplicates by `seq`. Envelopes on the `/envelopes` stream include `src: "workload"` (see [trust boundaries](./trust-boundaries.md#the-src-field)). avirthostd stamps `src`, `workload` and `gen` on each envelope and event from the authenticated connection it arrived on, overriding any value the producer supplied, and keeps one `seq` domain per producer. Each line avirthostd writes includes `prev`, the hash of the line before it (see [observability](./observability.md#integrity)). avirthostd caps each workload's log by size. Past a soft cap it slows the `/envelopes` stream and the workload's events through flow control, so producers refuse new work with 503 rather than drop records. At the hard cap it ends the workload with a `container_error` that identifies the cap.

## Ingress

A guest port is reachable from outside the workload container only through a port publication, and only through avirtedgeproxyd. avirthostd stores port publications as versioned state, separate from the immutable workload configuration. Launch requests or later API calls add them, subject to the `publish_port` action for each publication.

| Field | Meaning |
| --- | --- |
| `workload` | The publishing workload, and with it its short ID and namespace |
| `port` | A `127.0.0.1` port in the workload container, or the built-in `guestd` target, which avirtnetd forwards to `/run/avirt/streams/guestd.sock` |
| `protocol` | `tcp` for opaque bytes, or `http` |
| `expires` | When the publication ends, at the latest with the workload. avirtedgeproxyd and the destination workload's avirtmediatord both enforce it locally. |

A published port is reachable as `<name>.<ns>.workload.host.avirt.internal:<port>`, and a service record (see [components](./components.md#avirtedgeproxyd)) can name it as its backend. Who may connect is policy: avirtedgeproxyd's host policy snapshot and the destination workload's `ingress` action.

The destination workload's avirtmediatord receives each inbound connection from its own host's avirtedgeproxyd, on avirtmediatord's ingress listener:

- **Same host.** The source workload's avirtmediatord sends an h2 CONNECT stream with the avirt hostname as `:authority` to this host's avirtedgeproxyd. avirtedgeproxyd resolves the name to its entity and evaluates the host policy snapshot. It opens a new mutual TLS connection as `/service/avirtedgeproxyd` on the destination's ingress listener, with the source identity in `x-avirt-caller`.
- **Peered host.** The source workload's avirtmediatord sends the stream to its own avirtedgeproxyd, which forwards it to the peer's avirtedgeproxyd over mutual TLS with federated bundles (see [peering](./peering.md#workload-traffic)). The peer's avirtedgeproxyd evaluates its own host policy snapshot and connects to the destination as in the same-host path. The source avirtedgeproxyd connects to the peer's peer listener and sets `x-avirt-caller` to the originating workload's identity, in its own trust domain. The receiving avirtedgeproxyd accepts it only if that identity is in the trust domain of the peer's avirtedgeproxyd SVID that authenticated its connection, and otherwise refuses the stream with 403. Each avirtedgeproxyd sets `x-avirt-caller` itself on the stream it originates.
- **avirtapid.** avirtapid reaches services through avirtedgeproxyd in the same way.

The guest CLI stream takes the same first hop the other way round: avirtmediatord sends it to avirtedgeproxyd, which authorizes the `guest-cli` principal against the host policy snapshot and forwards it to avirtapid.

The destination workload's avirtmediatord accepts ingress connections only from `/service/avirtedgeproxyd`. It then:

1. evaluates the workload policy snapshot's `ingress` action for the caller in `x-avirt-caller` and the port.
2. sends the inbound connection notice `{"t":"ingress","id":I,"port":…}` on the `/inbound-notices` stream.
3. waits for avirtnetd's `CONNECT /ingress` with `:protocol: connect-tcp` (RFC 8441) with `avirt-ingress-id: I`, and forwards bytes between the two.

Each ingress ID contains 256 random bits and permits one use. avirtmediatord binds it to the destination workload and mediator generation. The ID also identifies the publication and pending connection. avirtmediatord consumes it atomically when the matching extended CONNECT arrives from that workload. An unknown, expired, reused, or mismatched ID gets 403. A pending connection expires after 10 s. At most 32 pending connections exist per workload and per mediator. This count covers active and draining connections. A new arrival at the cap gets 503. Expiry, disconnect, or publication revocation removes the pending entry. The HTTP/2 server advertises `SETTINGS_ENABLE_CONNECT_PROTOCOL`. `/ingress` uses RFC 8441 extended CONNECT with `:protocol: connect-tcp`, `:scheme: https`, the mediator authority and `:path: /ingress`. The protocol value comes from the [IETF CONNECT-TCP draft](https://datatracker.ietf.org/doc/draft-ietf-httpbis-connect-tcp/).

avirtnetd accepts a notice only for an unexpired port in its current publication set, which arrives in `confirm` and in later `{"t":"publications"}` control messages. A revoked or expired publication closes its open ingress streams. avirtnetd connects only to `127.0.0.1:<port>`, excluding its own forward proxy port, DNS stub, and any other port it holds. Within `/run/avirt`, it connects only to `streams/guestd.sock` for the `guestd` target. For that target it passes the caller's identity, so avirtguestd can check authorization for each operation.

## Sequence

```mermaid
sequenceDiagram
    participant C as avirthostd
    participant M as avirtmediatord (gen g)
    participant N as avirtnetd
    participant I as avirtinitd (root)
    participant A as agent (uid 1000)
    C->>I: runtime.run_workload (bootstrap SVID, root-only)
    I->>N: start, bootstrap SVID and key on an fd
    C->>M: runtime.run_mediator (sockets, bootstrap mediator/1)
    M->>C: mTLS, GET /workload, GET /workload/control
    M->>C: POST /workload/svid (mediator CSR)
    M->>M: policy snapshot and placeholders loaded
    loop until the h2 handshake succeeds
        N->>M: dial mediator.sock
    end
    N->>M: TLS 1.3, both SVIDs, authorization of the exact peer ID
    N->>M: control: attach(last_gen)
    M->>N: attached(g)
    Note over C,N: workload rotation, inner request signed by the workload key
    N->>M: POST /envelopes (resume-seq 0)
    M->>C: POST /workload/envelopes
    M->>N: confirm (CA cert, placeholders, files, publications)
    N->>I: confirm contents
    I->>A: CA install, launch in the agent cgroup with its security context
    N->>M: started(pid)
    A->>N: CONNECT through the forward proxy
    N->>M: h2 CONNECT stream
    M->>M: policy, TLS termination, routing
    M-->>C: events: conn and http lines
    Note over M: mediator exits unexpectedly
    I->>A: freeze the agent cgroup
    C->>M: runtime.run_mediator gen=2
    N->>M: new connection, mTLS with mediator/2, attach, confirm (same CA)
    N->>M: /envelopes resume (avirt-last-seq)
    I->>A: thaw the agent cgroup
    A-->>I: ACP session ends
    N->>M: /envelopes END_STREAM, control bye, GOAWAY
    C->>C: workload container exited
    C->>M: runtime.remove (SIGTERM, drain events)
    C->>C: container_exit, runtime.release
```

## Connection to avirthostd

`avirthostd` serves one listener per workload for the mediator's connection to avirthostd (see [runtime-provided sockets](#runtime-provided-sockets)). Its listener identity binds the workload. avirtmediatord is the h2 client over mutual TLS 1.3, authenticated by its SVID (see [identity](./identity.md#authorization)).

| Request | Caller | Purpose |
| --- | --- | --- |
| `GET /workload` | avirtmediatord | One JSON document per generation: `gen`, the namespace, the workload's short ID and alias, its labels, the policy reference, the MITM CA cert and key, the credential binding references, and the port publications. It excludes credential values. Held in memory only. |
| `GET /workload/control` | avirtmediatord | Long-lived. avirthostd pushes `rotate`, `bundle`, `publications` and `stop` as NDJSON. |
| `POST /workload/svid` | avirtmediatord | A CSR for its own ID, or a forwarded workload-signed request (see [identity](./identity.md#rotation)). Returns the SVID and the current bundle. |
| `POST /workload/envelopes` | avirtmediatord | The workload's `/envelopes` stream, forwarded body for body |
| `POST /workload/events` | avirtmediatord | NDJSON events, written as `dir: "egress"` lines with `src: "mediator"` |

Stderr from avirtmediatord goes to `logs/<workload-id>.mediator.log`. Losing the connection to avirthostd makes avirtmediatord exit, followed by the mediator container. Inside the workload container, the agent freezes. avirthostd restarts the mediator or tears the workload down.

# Open questions

1. **Session control stream.** Whether avirtacp's session control stream is its own HTTP/2 stream on the workload's connection to its mediator or a message type on `/control` is open (see [ACP](./acp.md#session-control-api)).
2. **OTLP stream limits.** The `/otlp` stream's flow-control share and per-workload rate limit are open (see [observability](./observability.md#open-questions)).
