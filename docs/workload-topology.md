---
type: Protocol Specification
title: Workload topology and connections
description: The connections inside one workload and between workloads (workload container, mediator, avirthostd, host services, avirtedgeproxyd), the runtime-provided Unix sockets and avirthostd's SNI routing, the HTTP/2 streams on the workload's connection to its mediator, launch handshake, freeze, reconnect, resuming the `/envelopes` stream and port publication.
tags: [avirt, workload, protocol, http2]
implementation: partial
generated: { by: claude-code/claude-opus-5-5, at: 2026-10-17T19:00:00Z }
---

# Status: implemented

## Sockets

The workload container runs with `--network none` and two Unix socket files mounted with `-v` from the workload's private directory `workloads/<id>/` (0700). The container runtime forwards a mounted socket file to the host process listening on it. Both files are mode 0600 on the host, which maps through, so only root in the workload container can open them.

| Guest path | Host listener | Carries |
| --- | --- | --- |
| `/run/avirt/mediator.sock` | Mediator | The workload's HTTP/2 connection to its mediator, which today carries only log envelopes, on its one request `POST /workloads/<id>/stream`. The mediator forwards its bytes unchanged to avirthostd. |
| `/run/avirt/proxy.sock` | Mediator | The CONNECT proxy (see [egress](./egress.md)) |

The mediator dials avirthostd's socket `hostd/hostd.sock` (directory 0700, socket 0600). Each connection starts with a one-line connection header naming its kind, the workload and the mediator's secret (see [identity](./identity.md)), read within 5 s:

- `WORKLOAD <id> <secret>`: `avirthostd` binds the connection to that workload and serves only that workload's `POST /workloads/<id>/stream` route over it. The mediator then forwards the HTTP/2 bytes unchanged between avirtinitd (client) and avirthostd's stream handler (server).
- `EGRESS <id> <secret>`: plain NDJSON egress events, one per proxied connection. `avirthostd` wraps each in the log envelope with `dir: "egress"`. Only one event connection per workload is accepted.

The mediator opens the `EGRESS` connection at startup. It dials a `WORKLOAD` connection only when the workload's first bytes arrive on `mediator.sock`, so a readiness probe (connect and close) costs avirthostd nothing.

## Connection to the mediator

`avirtinitd` connects to `mediator.sock` within 5 s, completes the HTTP/2 handshake within 5 s, sends `POST /workloads/<id>/stream` with its connection token (`AVIRT_CONNECTION_TOKEN`), and waits 4 s for the response headers. A failed attempt is retried after 300 ms, for 15 s overall. A 403, 404, 409 or 410 is final and is not retried. A socket file whose listener is gone accepts a connect and then says nothing, so every step has a timeout. `avirtinitd` starts neither its TCP proxy to `proxy.sock` nor the agent until avirthostd has accepted that request, and fails with a message naming the mediator otherwise.

## TCP proxy to `proxy.sock`

`avirtinitd` listens on an ephemeral `127.0.0.1` port and forwards each connection to `proxy.sock`. The agent reaches it through `HTTPS_PROXY` and the related variables (see [workload hardening](./workload-hardening.md)).

Constraint: nothing on a forwarded socket may half-close. With a `-v` socket-file mount, the peer's reply to a `SHUT_WR` sent immediately after the last write is lost. The TCP proxy therefore does not forward a client's end-of-stream. After the client finishes, each read on the other direction has a 30 s idle timeout. When the host side ends or a read idles out, the client is told and the proxy closes. A proxied connection can therefore appear in the log longer than the client held it.

# Status: planned

## Runtime-provided sockets

The workload container and the mediator have no network interface other than loopback. Every connection they make is on a Unix socket that the compute runtime provides (see [compute runtimes](./compute-runtimes.md) and [networking](./networking.md)). The workload container has exactly one, for the workload's HTTP/2 connection to its mediator. The mediator has four.

| Socket | Container end | Host end | Initiated by | TLS client and server |
| --- | --- | --- | --- | --- |
| Workload connection | Workload container: `/run/avirt/mediator.sock`, dialed by avirtnetd. Mediator: `/run/avirt/workload.sock`, served by avirtmedproxyd's Envoy. | avirthostd, which forwards the bytes unchanged | The workload side | avirtnetd, avirtmedproxyd's Envoy |
| Connection to avirthostd | Mediator: `/run/avirt/hostd.sock` | avirthostd | avirtmediatord and avirtmedproxyd's Envoy | The mediator process, avirthostd |
| Outbound socket to avirthostd | Mediator: `/run/avirt/svc.sock` | avirthostd's SNI routing | avirtmediatord and avirtmedproxyd's Envoy | The mediator process, the destination |
| avirtmedproxyd's ingress listener | Mediator: `/run/avirt/ingress.sock`, served by avirtmedproxyd's Envoy | avirthostd's SNI routing | avirthostd, for avirtedgeproxyd | avirtedgeproxyd's Envoy, avirtmedproxyd's Envoy |

- **Containerization.** Each socket is backed by a vsock connection between avirthostd and the guest, which the runtime's vsock-to-Unix-socket forwarder presents as the Unix socket above. The workload VM's vsock device has exactly one host listener, for the workload's connection, which avirthostd forwards unchanged to the current mediator generation's `workload.sock`; avirtnetd can therefore open a new connection to its mediator whenever it needs one. Every other host listener is on the mediator VM's device. `AF_VSOCK` is denied to every workload process, so only the runtime's forwarder uses the workload VM's vsock device.
- **Docker Desktop.** The workload's connection goes through a per-workload Docker named volume. The mediator's connection to avirthostd and its outbound socket to avirthostd are single-file bind mounts of the host's `workloads/<id>/c.sock` and `workloads/<id>/s.sock`. avirtmedproxyd's ingress listener is open (see [components](./components.md#open-questions)).

Every vsock connection, bind mount and named volume carries ciphertext only.

## SNI routing

avirthostd reads only the TLS ClientHello's SNI on each connection on a mediator's outbound socket, and forwards the connection unchanged to the destination the SNI names, if that destination is in the workload's allowed set:

| SNI | Destination | Allowed when |
| --- | --- | --- |
| `<name>.svc.<td>` | The host service's listener `services/<name>.sock` | Always, for avirtsecretsd, avirtpolicyd, avirtdnsd, avirtoteld, avirtmcpgwd and avirtmodelgwd, and for avirtregistryd from a build job's outbound socket only |
| `edgeproxy.svc.<td>` | avirtedgeproxyd's egress listener | Always |

avirthostd also serves one host-side listener for avirtedgeproxyd's Envoy alone, `services/ingress-relay.sock`, checked with peer credentials (`LOCAL_PEERCRED` on macOS, `SO_PEERCRED` on Linux). A connection there names its destination by SNI `<uuid>.ingress.<td>`, and avirthostd forwards it unchanged to that workload's avirtmedproxyd ingress listener at its current generation.

A connection with no SNI, an unknown SNI or a destination outside the set is closed before any byte is forwarded. avirthostd never terminates TLS on these connections. The destination authenticates the caller by its SVID and authorizes it (see [trust boundaries](./trust-boundaries.md#listeners)). avirthostd never routes one mediator to another. Only avirtedgeproxyd reaches a mediator's ingress listener, through `services/ingress-relay.sock`.

## HTTP/2 streams

| Layer | avirtmedproxyd's Envoy | avirtnetd |
| --- | --- | --- |
| TLS 1.3 (mutual, SPIFFE) | Server | Client |
| HTTP/2 | Server | Client |

One HTTP/2 connection per mediator generation carries every stream. avirtnetd opens every stream, and avirtmedproxyd's Envoy routes each one:

| Stream | Request | Opened for | Routed to |
| --- | --- | --- | --- |
| control | `POST /control`, always the first stream | avirtnetd | avirtmediatord, NDJSON messages in both directions for the life of the connection |
| envelopes | `POST /envelopes` with `avirt-resume-seq: N` | Envelopes from avirtinitd, the main workload process and avirtnetd, each carrying a `seq` | avirthostd, `POST /workload/envelopes` |
| egress | `CONNECT host:port`, one per agent connection | avirtnetd's forward proxy | The egress pipeline (see [egress](./egress.md#avirtmedproxyd-pipeline)) |
| guest CLI | `POST /guest-cli` | avirtguest's gRPC, through `guest.sock` | avirtedgeproxyd, then avirtapid, under the `guest-cli` principal (see [identity](./identity.md#trust-domain-and-ids)) |
| MCP | `POST /mcp` | avirtmcp, through `mcp.sock` | avirtmcpgwd |
| DNS | `POST /dns`, long-lived | avirtnetd's stub resolver, with length-prefixed DNS messages in both directions | libavirtenvoy's DNS policy, then avirtdnsd for avirt names or avirtedgeproxyd's DNS proxy for public names (see [egress](./egress.md#workload-dns)) |
| OTLP | `POST /otlp` | avirtnetd and the main workload process | avirtoteld, with `x-avirt-src: workload` (see [observability](./observability.md#opentelemetry-logs)) |
| inbound connection notices | `POST /inbound-notices`, long-lived | avirtnetd | libavirtenvoy, which sends one notice per inbound connection |
| ingress | `POST /ingress` with `avirt-ingress-id: I` | avirtnetd, accepting an inbound connection notice | The inbound connection waiting under `I` (see [ingress](#ingress)) |

An egress stream's response carries the pipeline's status, including avirtedgeproxyd's 403 `private-dest`, 502 and 504. The `/control`, `/envelopes`, `/dns` and `/inbound-notices` streams have reserved flow-control windows, so bulk egress and ingress streams cannot starve them.

avirtnetd dials `/run/avirt/mediator.sock` with a 50 ms to 1 s backoff, because dials made before avirtmedproxyd's Envoy listens fail, for up to the 30 s launch budget (see [lifecycle](./lifecycle.md#launch-order)). A mediator cannot tell a workload container that is gone from one that is not ready, so avirthostd stops it explicitly.

## Launch handshake

After the mutual TLS handshake (see [identity](./identity.md)):

1. avirtnetd → `{"t":"attach","last_gen":G,"proto":P}`. A control protocol version that avirtmediatord does not support is refused with `{"t":"refused","reason":"protocol"}`.
2. avirtmediatord → `{"t":"attached","gen":g}`, echoing the generation from its SVID.
3. The first rotation of every SVID (see [identity](./identity.md#rotation)).
4. avirtnetd opens the `/envelopes` stream, and avirthostd accepts it.
5. avirtmediatord → `{"t":"confirm","gen":g,"ca_pem":…,"env":{…placeholders…},"files":{"codex/auth.json":…},"mode":"audit","policy_version":V,"publications":[…]}`
6. avirtnetd hands the `confirm` contents to avirtinitd. avirtinitd installs the MITM CA, writes the files, starts the main workload process and avirtguestd, and launches the agent at the main workload process's request (see [components](./components.md#workload)). avirtnetd replies `{"t":"started","pid":P}`.

avirtmediatord sends `confirm` only once all of these hold: its workload configuration fetch succeeded, its own SVID and the SVID of avirtmedproxyd's Envoy are rotated, avirtmedproxyd's Envoy has its listeners, libavirtenvoy holds a valid policy snapshot and the workload's placeholders, the MITM CA is loaded, and avirthostd has reported the accepted `/envelopes` stream with `{"t":"envelopes_accepted"}` on `GET /workload/control`. libavirtenvoy reports its policy snapshot and placeholder readiness to avirtmediatord on a local status socket. No agent process exists before `confirm`.

## Freeze and reconnect

avirtnetd distinguishes three transitions of its connection to the mediator:

- **Rotation.** After an SVID rotation, avirtnetd opens a new connection to the same generation with its current SVID, sends `{"t":"reattach","gen":g}` on a new `/control` stream, resumes the `/envelopes` stream by `seq` and reopens its `/dns` and `/inbound-notices` streams, and only then sends GOAWAY on the old connection. Streams already open on the old connection may finish. Nothing is frozen.
- **Reconnect.** When a connection fails, avirtnetd reconnects to the same generation in the same way. While no connection of the current or a higher generation is attached, avirtnetd reports it to avirtinitd, which adds a `connection` freeze reason.
- **Replacement.** A mediator with a higher generation completes the handshake, `attach` and `confirm`, and supersedes the older generation.

While any freeze reason is set, avirtinitd writes `1` to the agent cgroup's `cgroup.freeze` and waits until `cgroup.events` shows `frozen 1`. The freeze covers every process in the agent cgroup, including tools that left the agent's process group, and leaves avirtnetd, the main workload process and avirtguestd running. If a connection to the same or a higher generation is attached and confirmed within 30 s, avirtinitd removes the `connection` freeze reason, and once no reason remains it writes `0` and the agent resumes. Otherwise it writes `1` to `cgroup.kill` and exits 2 with `error: mediator lost`. Egress connections open through the lost connection end, and clients retry them.

A workload container whose guest offers no writable cgroup v2 hierarchy with `cgroup.freeze` and `cgroup.kill` does not start its agent.

`avirthostd` restarts a mediator that exits unexpectedly, at most twice per workload, each time with the generation incremented. Once a newer generation is confirmed, avirtnetd sends GOAWAY on any older connection, and avirthostd fences the older generation at every host listener (see [identity](./identity.md#authorization)).

## Resuming after reconnect

`avirthostd` writes each envelope to the workload log before it counts its `seq` as written, tracks the highest `seq` it has written, and returns it as `avirt-last-seq` when the `/envelopes` stream resumes. avirtnetd replays from a 4 MiB ring of sent envelopes and writes a `meta` `envelope_gap` event when the ring no longer covers the gap. `avirthostd` deduplicates by `seq`. Envelopes on the `/envelopes` stream carry `src: "workload"` (see [trust boundaries](./trust-boundaries.md#event-provenance)). avirthostd stamps `src`, `workload` and `gen` on every envelope and event from the authenticated connection it arrived on, overriding any value the producer supplied, and keeps one `seq` domain per producer. Every line avirthostd writes carries `prev`, the hash of the line before it (see [observability](./observability.md#integrity)). avirthostd caps each workload's log by size. Past a soft cap it slows the `/envelopes` stream and the workload's events through flow control, so producers refuse new work with 503 rather than drop records. At the hard cap it ends the workload with a `container_error` that names the cap.

## Ingress

A guest port is reachable from outside the workload container only through a port publication, and only through avirtedgeproxyd. Port publications are versioned state in avirthostd, separate from the immutable workload configuration. They are set at launch or added through avirthostd's API, each authorized by the `publish_port` action.

| Field | Meaning |
| --- | --- |
| `workload` | The publishing workload, and with it its short ID and namespace |
| `port` | A `127.0.0.1` port in the workload container, or the built-in `guestd` target, which avirtnetd forwards to `/run/avirt/streams/guestd.sock` |
| `protocol` | `tcp` for opaque bytes, or `http` |
| `expires` | When the publication ends, at the latest with the workload. avirtedgeproxyd and the destination avirtmedproxyd both enforce it locally. |

A published port is reachable as `<name>.<ns>.workload.host.avirt.internal:<port>`, and a service record (see [components](./components.md#avirtedgeproxyd)) can name it as its backend. Who may connect is policy: avirtedgeproxyd's host policy snapshot and the destination workload's `ingress` action.

Every inbound connection reaches the destination avirtmedproxyd from the avirtedgeproxyd of its own host, on avirtmedproxyd's ingress listener:

- **Same host.** The source avirtmedproxyd sends an h2 CONNECT stream with the avirt name as `:authority` to this host's avirtedgeproxyd. avirtedgeproxyd resolves the name to its entity, evaluates the host policy snapshot, and opens a new mutual TLS connection as `/service/avirtedgeproxyd` on the destination's ingress listener, carrying the source identity in `x-avirt-caller`.
- **Peered host.** The source avirtmedproxyd sends the stream to its own avirtedgeproxyd, which forwards it to the peer's avirtedgeproxyd over mutual TLS with federated bundles (see [peering](./peering.md#workload-traffic)). The peer's avirtedgeproxyd evaluates its own host policy snapshot and connects to the destination as above. The source avirtedgeproxyd connects to the peer's peer listener and sets `x-avirt-caller` to the originating workload's identity, in its own trust domain. The receiving avirtedgeproxyd accepts it only if that identity is in the trust domain of the peer's avirtedgeproxyd SVID that carried it, and otherwise refuses the stream with 403. Each avirtedgeproxyd sets `x-avirt-caller` itself on the stream it originates.
- **avirtapid.** avirtapid reaches services through avirtedgeproxyd in the same way.

The guest CLI stream takes the same first hop the other way round: avirtmedproxyd sends it to avirtedgeproxyd, which authorizes the `guest-cli` principal against the host policy snapshot and forwards it to avirtapid.

The destination avirtmedproxyd accepts ingress connections only from `/service/avirtedgeproxyd`. It then:

1. evaluates the workload policy snapshot's `ingress` action for the caller in `x-avirt-caller` and the port;
2. sends the inbound connection notice `{"t":"ingress","id":I,"port":…}` on the `/inbound-notices` stream;
3. waits for avirtnetd's `POST /ingress` with `avirt-ingress-id: I`, and forwards bytes between the two.

avirtnetd accepts a notice only for an unexpired port in its current publication set, which arrives in `confirm` and in later `{"t":"publications"}` control messages. A revoked or expired publication closes its open ingress streams. avirtnetd connects only to `127.0.0.1:<port>`, never to its own forward proxy port, its DNS stub or any other port it holds, and never to a socket under `/run/avirt` except `streams/guestd.sock` for the `guestd` target. For that target it passes the caller's identity, so avirtguestd can authorize each operation.

## Sequence

```mermaid
sequenceDiagram
    participant C as avirthostd
    participant M as avirtmediatord (gen g)
    participant P as avirtmedproxyd (gen g)
    participant E as avirtmedproxyd's Envoy (gen g)
    participant N as avirtnetd
    participant I as avirtinitd (root)
    participant A as agent (uid 1000)
    C->>I: runtime.run_workload (bootstrap SVID, root-only)
    I->>N: start, bootstrap SVID and key on an fd
    C->>M: runtime.run_mediator (sockets, bootstrap mediator/1)
    M->>C: mTLS, GET /workload, GET /workload/control
    M->>C: POST /workload/svid (own and medproxy CSRs)
    M->>P: medproxy SVID and key
    P->>E: listeners and SVID over local xDS and SDS
    E->>E: policy snapshot and placeholders loaded
    loop until the h2 handshake succeeds
        N->>E: dial mediator.sock
    end
    N->>E: TLS 1.3, both SVIDs, exact-ID authorization
    N->>M: control: attach(last_gen)
    M->>N: attached(g)
    Note over C,N: workload rotation, inner request signed by the workload key
    N->>E: POST /envelopes (resume-seq 0)
    E->>C: POST /workload/envelopes
    M->>N: confirm (CA cert, placeholders, files, publications)
    N->>I: confirm contents
    I->>A: CA install, launch in the agent cgroup with its security context
    N->>M: started(pid)
    A->>N: CONNECT through the forward proxy
    N->>E: h2 CONNECT stream
    E->>E: policy, TLS termination, routing
    E-->>C: events: conn and http lines
    Note over M,E: mediator exits unexpectedly
    I->>A: freeze the agent cgroup
    C->>M: runtime.run_mediator gen=2
    N->>E: new connection, mTLS with mediator/2/medproxy, attach, confirm (same CA)
    N->>E: /envelopes resume (avirt-last-seq)
    I->>A: thaw the agent cgroup
    A-->>I: ACP session ends
    N->>E: /envelopes END_STREAM, control bye, GOAWAY
    C->>C: workload container exited
    C->>M: runtime.remove (SIGTERM, drain events)
    C->>C: container_exit, runtime.release
```

## Connection to avirthostd

`avirthostd` serves one listener per workload for the mediator's connection to avirthostd (see [runtime-provided sockets](#runtime-provided-sockets)). The listener's identity binds the workload. avirtmediatord and avirtmedproxyd's Envoy are h2 clients over mutual TLS 1.3, each with its own SVID and its own routes (see [identity](./identity.md#authorization)).

| Request | Caller | Purpose |
| --- | --- | --- |
| `GET /workload` | avirtmediatord | One JSON document per generation: `gen`, the namespace, the workload's short ID and alias, its labels, the policy reference, the MITM CA cert and key, the credential binding references, and the port publications. It carries no credential value. Held in memory only. |
| `GET /workload/control` | avirtmediatord | Long-lived. avirthostd pushes `rotate`, `bundle`, `publications` and `stop` as NDJSON. |
| `POST /workload/svid` | avirtmediatord | A CSR for its own ID or for avirtmedproxyd's Envoy's, or a forwarded workload-signed request (see [identity](./identity.md#rotation)). Returns the SVID and the current bundle. |
| `POST /workload/envelopes` | avirtmedproxyd's Envoy | The workload's `/envelopes` stream, forwarded body for body |
| `POST /workload/events` | avirtmedproxyd's Envoy | NDJSON events, written as `dir: "egress"` lines with `src: "mediator"` |

The stderr of avirtmediatord, avirtmedproxyd and avirtmedproxyd's Envoy goes to `logs/<workload-id>.mediator.log`. Losing the connection to avirthostd is fatal to avirtmediatord: it exits, the mediator container exits, the agent in the workload container freezes, and avirthostd restarts the mediator or tears the workload down.

# Open questions

1. **Session control stream.** Whether avirtacp's session control stream is its own HTTP/2 stream on the workload's connection to its mediator or a message type on `/control` is open (see [ACP](./acp.md#session-control-api)).
2. **OTLP stream limits.** The `/otlp` stream's flow-control share and per-workload rate limit are open (see [observability](./observability.md#open-questions)).
