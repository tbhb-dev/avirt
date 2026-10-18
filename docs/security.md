---
type: Security Design
title: Security principles
description: avirt's security thesis and scope, expectations of external layers, extension points, and core security principles. Each concrete property has a status of Met, In Progress, Future, or Out of Scope against the current code.
tags: [avirt, security, principles, threat-model]
implementation: partial
generated: { by: codex/gpt-6, at: 2026-10-18T05:20:59Z }
---

# Overview

avirt gives each agent only an identity and a sandbox container. Real credentials and direct network access remain outside the agent, whose authority requires policy grants.

On an Apple silicon Mac, avirt runs untrusted ACP agents. avirt approves each permission request automatically (see [ACP](./acp.md#permission-requests)). The agent can run any tool without human approval. avirt treats the agent and everything it runs as hostile. The implemented system runs on a single-user Mac with one avirthostd per user. The planned design adds the Docker Desktop runtime, peers whose workloads connect to each other through their avirtedgeproxyd instances (see [peering](./peering.md)), and the all-in-one server image that runs the host services in one container (see [components](./components.md)). This document defines avirt's security principles and the status of each property. See [trust boundaries](./trust-boundaries.md) for the detailed threat model.

# Security scope

avirt provides the base for agent security:

- **Sandboxing.** Each sandbox container runs in its own VM with no network interface other than loopback, and the agent runs under a reduced user, capability, seccomp and Landlock profile (see [workload hardening](./workload-hardening.md) and [Landlock](./landlock.md)).
- **Identity.** avirthostd issues SPIFFE identities to sandbox containers and mediators, and to host services. Peers federate their trust domains (see [identity](./identity.md)).
- **Egress and ingress mediation.** Each byte the agent sends out passes through its workload's mediator, and each workload-to-workload, service and off-host connection passes through avirtedgeproxyd, the edge proxy (see [egress](./egress.md) and [networking](./networking.md)).
- **Credential brokering.** avirtsecretsd holds the real credentials, the sandbox container holds placeholders, and the gateways and avirtmediatord inject credentials on their bindings only (see [egress credentials](./egress-credentials.md)).
- **Policy.** Cedar policy governs egress, ingress, DNS, TLS interception, workload management and workspaces, and the policy prover checks each policy set before it's distributed (see [egress policies](./egress-policies.md)).
- **Audit.** avirthostd's workload log records each JSON-RPC line, each connection and each control-plane decision, labelled with the component that observed it (see [trust boundaries](./trust-boundaries.md#src) and [observability](./observability.md#audit)).

avirt expects the layers outside it to provide:

- **The host.** The integrity of macOS, of the user account avirthostd runs as, and of the files that store the operator's own credentials, such as `~/.codex/auth.json` and the environment avirthostd starts with.
- **The hypervisor and kernels.** The isolation of Virtualization.framework VMs, and on Docker Desktop of the shared Linux kernel (see [compute runtimes](./compute-runtimes.md#docker-desktop-runtime)).
- **Upstream authorization.** What a model provider, MCP server, git host or registry lets a credential do once avirt has injected it.
- **Content judgement.** Judging permitted requests through prompt injection detection, data loss prevention, and review of the agent's output. These checks use the extension points below.
- **Tool approvals.** Human approval of individual agent tool calls. avirt approves each request, and the sandbox container is the security boundary.

# Security extensibility

Status: planned. avirt's enforcement points accept extensions where content inspection and external authorization decisions belong:

- **Mediator data plane.** avirtmediatord evaluates workload policy and inspects intercepted requests. It checks placeholders and injects credentials in its own process. Request and response inspection extensions can add checks at this point (see [egress](./egress.md#how-avirtmediatord-handles-egress)).
- **Edge proxy.** avirtedgeproxyd evaluates host policy for workload, service, peer and external connections in its own process. External authorization extensions can add checks to these connections.
- **Cedar policy.** Host and workload policy sets, with invariants that the prover checks before avirtpolicyd builds any policy snapshot (see [egress policies](./egress-policies.md#the-prover)).
- **Secret providers.** avirtsecretsd reads real credentials from the providers that the host configuration names, and is the only component that does (see [egress credentials](./egress-credentials.md#status-planned)).

An extension adds checks and never removes one. Cedar decisions, the `private-dest` check, `Host` and SNI matching, the `credential-misplaced` check and credential bindings apply whatever an extension decides.

# Core security principles

Status legend:

- **Met.** Implemented in the current code.
- **In Progress.** Partially implemented in the current code.
- **Future.** Specified in the planned design and not yet implemented.
- **Out of Scope.** Not avirt's job, and left to a layer outside it.

## No credentials in the sandbox container

The sandbox container holds placeholders and the workload's MITM CA certificate, neither of which is a secret. avirtsecretsd stores each real credential. Gateways and avirtmediatord inject it outside the sandbox container on its own binding only.

- **Met.** avirthostd refuses a launch whose agent credential is missing, and passes credentials into the sandbox container by variable name, never on argv or in the image ([egress credentials](./egress-credentials.md#status-implemented)).
- **Met.** A workload receives only its own agent's credentials: a Codex sandbox container doesn't contain `CLAUDE_CODE_OAUTH_TOKEN` ([egress credentials](./egress-credentials.md#status-implemented)).
- **Future.** The sandbox container holds placeholders only, and a placeholder grants nothing outside its own workload, binding, and destinations. Today the agent can read the Claude token in its environment and the Codex `auth.json` in its mounted home ([placeholders](./egress-credentials.md#placeholders)).
- **Future.** Only avirtmodelgwd and avirtmcpgwd inject model and MCP credentials. The mediator lacks long-lived real credentials ([bindings](./egress-credentials.md#bindings)).
- **Future.** avirtmediatord's `credential-misplaced` check rejects any issued placeholder found outside its bound header, in both policy modes ([placeholders](./egress-credentials.md#placeholders)).
- **Future.** Each credential has exactly one owner that refreshes it. The Codex refresh token never leaves avirtsecretsd ([Codex refresh](./egress-credentials.md#codex-refresh)).
- **Future.** avirthostd holds credential binding references and no credential values ([host services](./components.md#host-services)).

## Default-deny egress through one global PEP

The agent's only way out is its workload's mediator, each connection needs an explicit permit, and avirtedgeproxyd is the policy enforcement point for each connection that leaves a workload for another workload, a service, or the internet.

- **Met.** The sandbox container has no network interface other than loopback, and its only path out is avirtinitd's TCP proxy to `proxy.sock`, the mediator's CONNECT proxy ([egress](./egress.md#path)).
- **Met.** The mediator accepts only `CONNECT host:port` with an explicit port and a canonical hostname or IP literal ([CONNECT app](./egress.md#pingora-connect-app)).
- **Met.** The mediator refuses each resolved address in a non-global range or assigned to one of the host's own interfaces, which it reads on each connection ([`private-dest` check](./egress.md#private-dest)).
- **Future.** Enforce mode requires an explicit Cedar permit for a connection, request, or DNS query. Today the mediator allows each destination that passes the `private-dest` check under rule `allow-all` ([decision semantics](./egress-policies.md#decision-semantics)).
- **Future.** avirtedgeproxyd is the policy enforcement point for each workload-to-workload, service, host-local service and external connection, and the only host service that opens connections off the host ([avirtedgeproxyd](./components.md#avirtedgeproxyd), [networking](./networking.md#restrictions-on-host-connections)).
- **Future.** avirtedgeproxyd runs on the host. The macOS content filter permits its binary and blocks non-loopback flows from other avirt-signed processes. Linux permits new off-host flows only from its UID ([restrictions on host connections](./networking.md#restrictions-on-host-connections)).
- **Future.** avirtedgeproxyd refuses the host's own interface addresses, which it reads with `getifaddrs` on every connection, as well as the non-global ranges ([edge proxy egress](./egress.md#edge-proxy-egress)).
- **Future.** Workload DNS goes through the `resolve` action, which applies as evaluated in audit mode too, and the mediator logs each query ([workload DNS](./egress.md#workload-dns)).
- **Future.** The `Host` of a request under TLS interception must equal its CONNECT host, and the SNI of a connection under TLS passthrough must equal its CONNECT host ([how avirtmediatord handles egress](./egress.md#how-avirtmediatord-handles-egress)).
- **Future.** Inbound connections reach only the ports a workload publishes, through avirtedgeproxyd and under the destination workload's `ingress` policy ([ingress](./workload-topology.md#ingress)).

## Strong workload identity

Each connection across a container or host boundary authenticates with a short-lived SPIFFE identity bound to one workload and one mediator generation. A VM can request only identities that avirt issued to it.

- **Met.** Each workload's connections to avirthostd authenticate with per-workload random secrets. Only avirtinitd possesses the connection token (`AVIRT_CONNECTION_TOKEN`), which it removes from the agent's environment. The mediator alone possesses the mediator secret ([identity](./identity.md#status-implemented)).
- **Future.** Each listener reachable across a container or host boundary requires mutual TLS with SPIFFE X.509-SVIDs from the first byte, and authorizes the caller's exact ID ([listeners](./trust-boundaries.md#listeners)).
- **Future.** VMs lack a SPIFFE Workload API. avirthostd delivers each container's bootstrap SVID at creation and pushes each later SVID over an authenticated connection ([bootstrap](./identity.md#bootstrap)).
- **Future.** Components generate their long-lived keys where they use them. avirthostd zeroizes each bootstrap key after handing it to the runtime ([design invariants](./architecture.md#design-invariants)).
- **Future.** Workload and mediator SVIDs include the namespace, the never-reused UUIDv7, and the mediator generation. They live for 1 h. Each host listener refuses a superseded generation ([authorization](./identity.md#authorization)).
- **Future.** avirthostd issues a workload SVID only for a request signed with the current workload key. A compromised mediator can't get one ([rotation](./identity.md#rotation)).
- **Future.** uid 1000 never holds an SVID or its key, the trust bundle, a CA key or a real credential ([local exceptions](./trust-boundaries.md#local-exceptions)).
- **Future.** The SPIFFE CA and the workload MITM CA are separate. Endpoints accept certificates only from the CA they expect ([two separate CAs](./identity.md#two-separate-cas)).

## Defense in depth in the sandbox container

Independent layers confine the agent. Escaping one leaves the others in place.

- **Met.** Each sandbox container runs in its own VM under the Apple container CLI runtime ([compute runtimes](./compute-runtimes.md#apple-container-cli-runtime)).
- **Met.** The agent runs as uid 1000 with each capability set empty and `no_new_privs`, and its environment excludes the workload ID, prompt, and connection token ([agent process](./workload-hardening.md#agent-process)).
- **Met.** Seccomp denies `AF_VSOCK`, `AF_NETLINK` and `AF_PACKET` sockets and io_uring, to avirtinitd and everything it starts, before the agent starts ([seccomp](./workload-hardening.md#seccomp)).
- **Met.** `proxy.sock` and `mediator.sock` are `root:root 0600`, and the agent can't read `/proc/1/fd`, `/proc/1/mem` or `/proc/1/environ` or signal PID 1 ([what the agent can still do](./workload-hardening.md#residual-agent-access)).
- **Future.** `socket()` allows only `AF_UNIX`, `AF_INET` and `AF_INET6`, and the denylist blocks fileless exec, cross-process memory access, kernel exploit primitives, mounts and user namespaces ([denylist](./workload-hardening.md#denylist)).
- **Future.** Landlock confines the agent's filesystem writes to `/workspace`, `/home/agent` and `/tmp`, and the agent doesn't start without it ([Landlock](./landlock.md)).
- **Future.** Each of avirtnetd, avirtacp, avirtagent, avirtexec, and avirtguestd runs under its own UID and profile, and avirtinitd mounts `/proc` with `hidepid=invisible` ([security contexts](./workload-hardening.md#security-contexts)).
- **Future.** The agent and each tool it starts run in one cgroup that avirtinitd freezes and kills as a unit ([freeze and reconnect](./workload-topology.md#freeze-and-reconnect)).
- **Future.** Workspace and configuration layers exclude reserved paths. All layers exclude setuid bits, file capabilities, and device nodes ([reserved paths](./workload-hardening.md#reserved-paths), [content rules](./workspaces.md#content-rules)).
- **Future.** On Docker Desktop, where each workload shares one kernel, the runtime's capability and seccomp layers sit beneath avirtinitd's, and avirtbuilderd refuses command builds ([runtime outer layer](./workload-hardening.md#runtime-outer-layer)).
- **Out of Scope.** Approving individual tool calls inside the agent. avirt approves each permission request automatically ([ACP](./acp.md#permission-requests)).

## Bounded mediator compromise

A compromised mediator controls only its own workload's traffic. It can't access a real model or MCP credential. Access to another workload or the network must pass through avirtedgeproxyd.

- **Met.** The mediator starts from an empty environment that contains only its secret ([mediator secret](./identity.md#mediator-secret)).
- **Met.** avirthostd binds each mediator connection to the workload its connection header names, and serves only that workload's `POST /workloads/<id>/stream` route on it ([sockets](./workload-topology.md#sockets)).
- **Future.** The mediator runs in its own VM with no network interface other than loopback. Today it's a host process with the user's network access and file access ([mediator network](./egress.md#mediator-network)).
- **Future.** avirtmediatord handles traffic and lifecycle in one process. A parser bug exposes its SVID's private key and mediator-injected credentials inside the per-workload mediator VM, which has no network device ([mediator Landlock rules](./landlock.md#mediator), [compromise scope](./trust-boundaries.md#compromise-scope)).
- **Future.** The mediator has only placeholders and short-lived mediator-injected credentials. Long-lived real credentials for model or MCP traffic remain outside it ([bindings](./egress-credentials.md#bindings)).
- **Future.** avirthostd's SNI routing never connects one mediator to another, and each host listener refuses a superseded generation ([SNI routing](./workload-topology.md#sni-routing)).
- **Future.** The workload's MITM CA is name-constrained to the hosts its policy can intercept, and a workload lacks a MITM CA unless its policy permits interception ([MITM CA name constraints](./egress-policies.md#mitm-ca-name-constraints)).
- **Future.** avirthostd labels events that the mediator reports with `src: "mediator"`. Trust in these events must never exceed trust in the mediator ([the event's `src` field](./trust-boundaries.md#src)).

## Least privilege for host services

Each host service limits its state, secrets, and callers to those it needs. Local callers authenticate before acting.

- **Met.** avirthostd starts each sandbox container and each mediator from a cleared environment, adding only the variables the launch needs ([egress credentials](./egress-credentials.md#status-implemented)).
- **Met.** The runtime directory, workload directories and logs directory are 0700 and owned by the user, and each log file is 0600 ([log files](./hostd-api.md#log-files)).
- **Future.** Local clients authenticate before they act: avirtedgeproxyd's `edgeproxy.sock` is a user-owned 0600 socket, and avirthostd's admin socket checks peer credentials. Today the control API has no authentication, and any local process can launch workloads ([control API](./hostd-api.md#control-api), [listeners](./trust-boundaries.md#listeners)).
- **Future.** Each host service manages only its own state, accepts only the callers it lists, and in the all-in-one server image runs under its own UID and reads only its own secrets ([all-in-one server image](./components.md#all-in-one-server-image)).
- **Future.** avirtbuilderd lacks signing keys and host control API access. Each build of untrusted content runs in a disposable build job ([builds](./workspace-build.md#builds)).
- **Future.** zot accepts only avirtregistryd, over mutual TLS with a private CA held in memory ([workspace registry](./workspace-registry.md#trust-boundaries)).

## Policy-governed workload management with no escalation

A workload can manage other workloads only as policy permits, and never gains more authority than it holds.

- **Met.** A workload has no control-plane access: its HTTP/2 connection to its mediator reaches only its own `POST /workloads/<id>/stream` route ([sockets](./workload-topology.md#sockets)).
- **Future.** avirthostd authorizes each management call per action and per resource against the host policy snapshot, with denial as the default decision ([workload management](./components.md#workload-management)).
- **Future.** A workload creates workloads only in namespaces where it may, with bindings it holds, from workspace images it may launch, and with its own or an assignable policy reference ([workload management](./components.md#workload-management)).
- **Future.** Only avirthostd sets `avirt.internal/` labels, and a workload can't change its own namespace, labels, bindings or policy reference ([workload management](./components.md#workload-management)).
- **Future.** avirthostd records each workload's creator and root, and refuses a create past the delegation chain depth cap ([workload management](./components.md#workload-management)).
- **Future.** Host administration is never available to workload principals, and avirtadmin reaches avirthostd only on its `admin.sock` ([listeners](./trust-boundaries.md#listeners)).
- **Future.** Audit mode never widens control-plane authority ([modes](./egress-policies.md#modes)).
- **Future.** A build job has no `guest-cli` access ([build jobs](./workspace-build.md#build-jobs)).

## Fail closed

avirt denies or ends the workload if a check can't run, a dependency is missing, or state is uncertain.

- **Met.** Starting the agent requires avirthostd to accept the workload's HTTP/2 connection to its mediator and avirtinitd to apply the seccomp filters. Failure to apply a filter ends the workload ([connection to the mediator](./workload-topology.md#connection-to-the-mediator), [seccomp](./workload-hardening.md#seccomp)).
- **Met.** A stream POST with a missing or wrong token gets 403 before the workload's state changes, and avirthostd closes a mediator connection whose header lacks the right secret ([identity](./identity.md#status-implemented)).
- **Met.** Losing its mediator leaves a workload without an outbound path. A mediator shuts down on stdin EOF when avirthostd exits ([mediator process](./egress.md#mediator-process)).
- **Met.** Codex write-back replaces the host `auth.json` only with a strictly newer copy of the same login. avirthostd skips write-back unless it confirms that the workload has ended ([Codex](./egress-credentials.md#codex)).
- **Future.** No agent starts before the workload identity, the MITM CA, the placeholders and a valid policy snapshot are in place ([launch handshake](./workload-topology.md#launch-handshake)).
- **Future.** A missing or expired policy snapshot, a Cedar evaluation error and an internal name that resolves to no entity each deny, in both modes ([policy failure behavior](./egress-policies.md#policy-failure-behavior)).
- **Future.** A lost connection between the workload and its mediator freezes the agent, and the workload ends if mediator confirmation fails to complete within 30 s ([freeze and reconnect](./workload-topology.md#freeze-and-reconnect)).
- **Future.** Each side of each protocol refuses a version it doesn't support with a named error ([operational ownership](./components.md#operational-ownership)).

## Complete, tamper-evident audit

avirt records each agent action and each avirt decision with an authenticated identity. Dropping or rewriting a record leaves evidence.

- **Met.** The workload log records each JSON-RPC line and each proxied connection before `container_exit` ([log files](./hostd-api.md#log-files), [events](./egress.md#events)).
- **Met.** avirtinitd records each automatic approval as an `auto_approve` meta event ([ACP](./acp.md#permission-requests)).
- **In Progress.** avirt's own events exclude secret values: the Codex write-back logs a fixed reason string. Lines the agent produces can still contain its credentials ([Codex](./egress-credentials.md#codex)).
- **Future.** Each event includes `workload`, `gen`, `seq` and a `src` label that avirthostd stamps from the authenticated connection it arrived on ([the event's `src` field](./trust-boundaries.md#src)).
- **Future.** Each policy decision records the deciding rule and policy snapshot version, and each workload management call records the caller, the action, the resource and the delegation chain ([events](./egress.md#events), [workload management](./components.md#workload-management)).
- **Future.** Event loss never happens silently. A full event ring refuses new work with 503, and a workload at its log's hard cap ends ([resuming after reconnect](./workload-topology.md#resuming-after-reconnect)).
- **Future.** avirtauditd delivers the log as OCSF with `src` intact. Its backpressure never stalls a workload ([audit](./observability.md#audit)).
- **Future.** The workload log and each audit log are tamper-evident: each line includes a hash chain, and each sink receives signed checkpoints ([integrity](./observability.md#integrity)).

## Supply chain integrity of images and workspaces

avirt pins each image and workspace a workload runs by digest and verifies it at the point of use. Builds from untrusted content run only in a sandbox container.

- **In Progress.** avirtinitd builds with `--locked` from `Cargo.lock`, and each adapter installs with `npm ci` from its lockfile. Base image pins use tags, and the Node tarball has no checksum ([Claude adapter](./acp-claude.md#adapter), [Codex adapter](./acp-codex.md#adapter)).
- **Future.** The guest kernel and initfs pins include their URLs and sha256 hashes. avirt refuses a cache with a wrong hash ([guest artifacts](./build-and-platforms.md#guest-artifacts)).
- **Future.** avirthostd verifies each artifact's digest when writing it and when creating a workload from it, and each workspace digest it pulls against the resolved manifest ([operational ownership](./components.md#operational-ownership), [workspace registry](./workspace-registry.md#trust-boundaries)).
- **Future.** avirtbuilderd and avirthostd both check each workspace layer against the content rules, and a workspace composes only onto the base specified by its base image reference ([content rules](./workspaces.md#content-rules), [base image reference](./workspaces.md#base-image-reference)).
- **Future.** Only avirtbuilderd promotes a build result, with annotations it computes itself, and a build job writes only to its staging repository ([build pipeline](./workspace-build.md#build-pipeline)).
- **Future.** An image built from local sources never substitutes for a ref's image, and policy can forbid launching one ([images built from local sources](./workspaces.md#images-built-from-local-sources)).
- **Future.** Promoted workspace images include signatures, which `launch_workspace` can require. Whether and how to do this remain open ([workspace builds](./workspace-build.md#open-questions)).
- **Out of Scope.** The trustworthiness of the packages, repositories, and images an agent or a build fetches. avirt mediates and records those fetches and doesn't vet their content.

## Secrets never at rest in the clear

A secret exists in memory where it's used, and on disk only where an owner needs it, with the narrowest mode, for the shortest time.

- **Met.** avirt excludes secrets from argv and sandbox images ([egress credentials](./egress-credentials.md#status-implemented)).
- **Met.** The per-launch Codex home is 0700 with a 0600 `auth.json`, and it's removed on each exit path or left for the next startup cleanup ([Codex](./egress-credentials.md#codex)).
- **Future.** Only memory stores the SPIFFE issuing CA, each SVID key, and the workload MITM CA key. The SPIFFE root key is a 0600 file in the state directory ([issuer](./identity.md#issuer), [durable state](./lifecycle.md#durable-state)).
- **Future.** A gateway keeps a resolved credential in memory only, and for no longer than the credential's own lifetime ([credential broker API](./egress-credentials.md#credential-broker-api)).
- **Future.** The configuration layer excludes secret and placeholder values ([configuration layer](./workload-lifecycle.md#configuration-layer)).
- **Future.** Workspace images exclude secrets by a denylist at build time and again at launch ([secret exclusion](./workspaces.md#secret-exclusion)).
- **Future.** Encryption protects durable secrets at rest, including the SPIFFE root key and avirtsecretsd's credentials. No document specifies the mechanism yet (see [open questions](#open-questions)).
- **Out of Scope.** Protecting the operator's own credential files, such as `~/.codex/auth.json`, from other processes of the same user.

## Peering trust

Peered hosts trust each other's identities explicitly and narrowly: a peer authenticates as its own trust domain, and reaches only what this host's policy publishes to it.

- **Future.** Peers federate by explicit enrollment with a bundle obtained out of band, and never share a CA key ([federation](./identity.md#federation)).
- **Future.** The peer listener accepts only an enrolled peer's `/service/avirtedgeproxyd`, and only caller identities in that peer's own trust domain ([federation](./identity.md#federation)).
- **Future.** A peer reaches only the published ports, services and avirtapid routes that this host's host policy snapshot and its enrollment allow, and never avirthostd or the gateways ([avirtedgeproxyd](./components.md#avirtedgeproxyd), [peering](./peering.md#what-crosses-a-peering)).
- **Future.** Cross-host traffic passes both hosts' avirtedgeproxyd, and each evaluates its own host policy snapshot ([peering](./peering.md#evaluation-on-each-side)).
- **Future.** Managing a peer's workloads goes through that peer's avirtapid, under the peer's policy for the caller's fully qualified identity ([workload management](./components.md#workload-management)).
- **Future.** An operator can revoke a peer before its bundle expires, which closes each connection with it ([revocation and removal](./peering.md#revocation-and-removal)).
- **Out of Scope.** Deciding which hosts to peer with. Enrollment is the operator's decision.

# Open questions

1. **Secrets at rest.** Whether to encrypt durable secrets at rest, such as the SPIFFE root key in the macOS Keychain, remains open. Storage for avirtsecretsd's cached provider values is also open.
2. **Base image pinning.** Whether the avirt base, the harness bases and the mediator image pin their own base images by digest and their downloads by checksum is open.
3. **Extension callouts.** Request and response inspection, external authorization, their access under TLS passthrough, and their failure behavior remain unspecified.
4. **Restrictions on host connections.** Whether the Network Extension content filter enforces the restriction that only avirtedgeproxyd connects off the host on macOS needs a prototype. The property to test is that it drops each non-loopback flow from avirt-signed host processes (see [networking](./networking.md#open-questions)).

# Threat model

What each listener accepts, and what a compromised agent, root in the sandbox container, mediator, avirtedgeproxyd, build job, registry, builder or peer can and can't reach, is in [trust boundaries](./trust-boundaries.md).
