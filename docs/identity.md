---
type: Security Design
title: Identity and mTLS
description: How the runner, the supervisor and the controller authenticate each other, the SPIFFE trust domain and IDs, SVID bootstrap and rotation, and the separation of the SPIFFE CA from the MITM CA.
tags: [agentvmm, identity, spiffe, mtls, security]
implementation: partial
generated: { by: claude-code/claude-opus-5-5, at: 2026-10-17T14:23:31Z }
---

# Status: implemented

## Uplink token

The controller draws a random 128-bit token per actor and passes it only to the runner (`-e AGENTVMM_UPLINK_TOKEN`, inherited by name, removed from the agent's environment). The stream POST must carry it in `x-agentvmm-token`. A missing or wrong token gets a 403 before the actor's state changes.

## Supervisor secret

Each supervisor gets a random secret in its environment (`AGENTVMM_SUPERVISOR_SECRET`, never in the workload's) and presents it in both preambles on the controller socket (`ACTOR <id> <secret>` and `EGRESS <id> <secret>`). A preamble without the right secret is closed, and nothing it sent reaches the log. The supervisor starts from an empty environment: only `PATH`, `HOME`, `TMPDIR` and its secret.

# Status: planned

Both hops (runner and supervisor, supervisor and controller) use mutual TLS 1.3 with SPIFFE X.509-SVIDs. This replaces the uplink token and the supervisor secret.

## Trust domain and IDs

The trust domain is per install: `agentvmm.<host>`, where `<host>` is the lowercased short hostname reduced to `[a-z0-9-]`.

| Workload | SPIFFE ID | Lifetime |
| --- | --- | --- |
| Controller | `spiffe://agentvmm.<host>/controller` | Controller process |
| Runner (in the workload) | `spiffe://agentvmm.<host>/actor/<id>/workload` | Actor |
| Supervisor | `spiffe://agentvmm.<host>/actor/<id>/supervisor/<gen>` | One supervisor generation |

The actor id and the supervisor generation are part of the signed URI. Actor ids are never reused.

## Issuer

The controller holds the trust-domain CA: ECDSA P-256, generated at controller startup, kept in memory only, valid for 7 days, and regenerated on every start (the startup sweep ends every older actor). It signs CSRs into X.509-SVIDs with:

- exactly one URI SAN,
- `CA:false`,
- `digitalSignature` key usage,
- `serverAuth` and `clientAuth` extended key usage,
- a 1 h lifetime.

The CA certificate is name-constrained: the permitted URI subtree is `agentvmm.<host>`, and no dNSName subtree is permitted.

Verification on every side uses the `spiffe` crate's types (`SpiffeId`, `X509Svid`, `X509Bundle`) with a rustls verifier that chains only to the SPIFFE bundle and then authorizes the exact peer ID. Constraint: `spiffe-rustls` cannot be used, because it builds its verifiers only from an `X509Source`, which always requires a Workload API endpoint, and no Workload API exists (invariant 1 in [architecture](./architecture.md)).

## Attestation

The controller creates exactly two workloads per actor and records which runtime handle, link and controller channel belong to which actor and generation. It signs a CSR only for the ID its own records assign to the channel the CSR arrived on:

- `/actor/<id>/supervisor/<gen>`, on that supervisor's controller channel;
- `/actor/<id>/workload`, relayed by that actor's current supervisor.

## Bootstrap

The controller mints one bootstrap SVID and key per workload (10-minute lifetime), hands them to the runtime when it creates the workload, and zeroizes its copy.

| Runtime | Supervisor | Workload |
| --- | --- | --- |
| Containerization | A root-only (0400) file written into the VM's root filesystem before boot, or the init process's environment | The same, delivered to the root runner |
| Docker Desktop | `AGENTVMM_SVID`, `AGENTVMM_SVID_KEY`, `AGENTVMM_BUNDLE`, inherited by name, never on argv | The same, read by the root runner |

The runner and the supervisor read the material, unlink the file or scrub the variables, and keep it in memory. uid 1000 cannot obtain it:

- reading `/proc/<runner>/environ` or the runner's memory requires a matching uid or `CAP_SYS_PTRACE`, and the agent's seccomp filter denies `ptrace` and `process_vm_readv`;
- the file is removed before the agent starts;
- the agent's environment is built without it.

## Rotation

The controller initiates every rotation over `GET /actor/control`, pushing `{"t":"rotate","target":"supervisor"|"workload"}` and `{"t":"bundle"}`.

1. The target generates a fresh P-256 key in memory and sends a CSR. The supervisor posts its own to `POST /actor/svid`. The runner sends its own on the link control stream, and the supervisor relays it.
2. The controller authorizes the CSR as above and returns the SVID, with the bundle, down the same path.

The first rotation of both workloads happens as soon as the link is attached, so the bootstrap keys are retired within seconds. Later rotations happen at 50 % of the lifetime. A rotated SVID takes effect on a new connection, because TLS 1.3 has no renegotiation: the supervisor opens a new link stream and a new controller stream, both sides present their current SVIDs, new streams move to the new connection, and the old connection gets GOAWAY. Streams already open on it may finish.

At the peer certificate's `not_after`, either side closes the connection and its streams end. Clients retry, and the uplink resumes by `seq`. If the controller is unreachable, rotation cannot happen and the supervisor exits, so the workload freezes and the actor ends after 30 s.

```mermaid
sequenceDiagram
    participant C as controller (SPIFFE CA)
    participant S as supervisor
    participant R as runner (root)
    C->>S: bootstrap SVID and key at creation (root-only), controller zeroizes
    C->>R: bootstrap SVID and key at creation (root-only), controller zeroizes
    S->>C: mTLS controller channel, GET /actor/control
    S->>R: mTLS link (bootstrap SVIDs)
    C->>S: control: rotate supervisor
    S->>S: new key in memory
    S->>C: POST /actor/svid (CSR)
    C-->>S: SVID
    C->>S: control: rotate workload
    S->>R: link control: rotate
    R->>R: new key in memory
    R->>S: CSR
    S->>C: POST /actor/svid (relayed CSR)
    C-->>S: SVID
    S-->>R: SVID
    S->>R: new link connection with current SVIDs, GOAWAY on the old one
```

## Authorization

- **Link:** the runner requires the peer to be `…/actor/<own id>/supervisor/<g>` with `g ≥ last_gen`. A higher `g` supersedes the older connection, and an equal `g` is a rotation. The supervisor requires exactly `…/actor/<own id>/workload`. A stale endpoint from another actor fails on the actor id, and an old supervisor fails on `g`.
- **Controller channel:** the controller presents `/controller` and requires `/actor/<id>/supervisor/<g>` for the channel's recorded actor. A controller vsock listener completes this handshake before reading any application byte.

## Two separate CAs

| | SPIFFE CA | Actor MITM CA |
| --- | --- | --- |
| Key lives in | The controller only | The controller and the actor's supervisor |
| Trusted by | The SVID verifiers in the runner, the supervisor and the controller, each configured with the SPIFFE bundle alone and never with system or web roots | The agent's trust store and CA variables |
| Signs | SVIDs (URI SAN, name-constrained to `agentvmm.<host>`) | Upstream leaf certificates (DNS SAN, `serverAuth` only) |

The SPIFFE bundle is never written to `/etc/ssl` or to any file in the workload. A certificate signed by one CA is never accepted where the other is expected.

```mermaid
flowchart LR
    subgraph ctl["controller"]
        sca["SPIFFE CA key"]
        mca["actor MITM CA"]
    end
    sca -->|"bootstrap SVID"| wl["runner: .../actor/ID/workload"]
    sca -->|"bootstrap SVID, then CSR-signed SVIDs"| sup["supervisor: .../actor/ID/supervisor/G"]
    sup -->|"relays the runner's CSR"| sca
    mca -->|"cert and key, GET /actor"| sup
    sup -->|"cert only, confirm"| agent["agent trust store"]
    wl <-->|"mTLS, exact IDs"| sup
    sup <-->|"mTLS, controller channel"| ctl
```
