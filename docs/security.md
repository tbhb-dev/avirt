---
type: Security Design
title: Security principles
description: avirt's security thesis, its scope and what it expects of the layers outside it, its extension points, and its core security principles, each with concrete properties tagged Met, In Progress, Future or Out of Scope against the current code.
tags: [avirt, security, principles, threat-model]
implementation: partial
generated: { by: claude-code/claude-opus-5-5, at: 2026-10-17T23:30:00Z }
---

# Overview

avirt gives every agent an identity and a sandbox, and nothing more: no real credentials, no network of its own, and no authority that policy did not grant it.

avirt runs untrusted ACP agents on an Apple silicon Mac. Every permission request is approved automatically (see [ACP](./acp.md#permission-requests)), so the agent runs any tool with no human in the loop, and avirt treats the agent and everything it runs as hostile. The implemented system runs on a single-user Mac with one avirthostd per user. The planned design adds the Docker Desktop runtime, peers whose workloads reach each other through their avirtedgeproxyd instances (see [peering](./peering.md)), and the all-in-one server image that runs the host services in one container (see [components](./components.md)). This document defines avirt's security principles and the status of each property. The detailed threat model is in [trust boundaries](./trust-boundaries.md).

# Security scope

avirt provides the base for agent security:

- **Sandboxing.** Each workload container runs in its own VM with no network interface other than loopback, and the agent runs under a reduced user, capability, seccomp and Landlock profile (see [workload hardening](./workload-hardening.md) and [sandbox](./sandbox.md)).
- **Identity.** avirthostd issues a SPIFFE identity to every workload container, mediator and host service, and peers federate their trust domains (see [identity](./identity.md)).
- **Egress and ingress mediation.** Every byte the agent sends out passes through its workload's mediator, and every workload-to-workload, service and off-host connection passes through avirtedgeproxyd, the edge proxy (see [egress](./egress.md) and [networking](./networking.md)).
- **Credential brokering.** avirtsecretsd holds the real credentials, the workload container holds placeholders, and the gateways and avirtmedproxyd inject credentials on their bindings only (see [egress credentials](./egress-credentials.md)).
- **Policy.** Cedar policy governs egress, ingress, DNS, TLS interception, workload management and workspaces, and the policy prover checks every policy set before it is distributed (see [egress policies](./egress-policies.md)).
- **Audit.** avirthostd's workload log records every JSON-RPC line, every connection and every control-plane decision, labelled with the component that observed it (see [trust boundaries](./trust-boundaries.md#event-provenance) and [observability](./observability.md#audit)).

avirt expects the layers outside it to provide:

- **The host.** The integrity of macOS, of the user account avirthostd runs as, and of the files that hold the operator's own credentials, such as `~/.codex/auth.json` and the environment avirthostd starts with.
- **The hypervisor and kernels.** The isolation of Virtualization.framework VMs, and on Docker Desktop of the shared Linux kernel (see [compute runtimes](./compute-runtimes.md#docker-desktop-runtime)).
- **Upstream authorization.** What a model provider, MCP server, git host or registry lets a credential do once avirt has injected it.
- **Content judgement.** Whether a permitted request is a good idea: prompt injection detection, data loss prevention and review of the agent's output, which plug into the extension points below.
- **Tool approvals.** Human approval of individual agent tool calls. avirt approves every request, and the workload container is the security boundary.

# Security extensibility

Status: planned. avirt's enforcement points accept extensions where content inspection and external authorization decisions belong:

- **libavirtenvoy.** The Envoy dynamic module in avirtmedproxyd's Envoy and avirtedgeproxyd's Envoy. It evaluates Cedar policy locally, resolves avirt names to entities, runs the placeholder misuse check and injects credentials (see [egress](./egress.md#avirtmedproxyd-pipeline)).
- **Envoy `ext_proc` and `ext_authz` on avirtmedproxyd's Envoy.** Callouts on the agent's requests under TLS interception, for request and response inspection and for external authorization decisions per request.
- **Envoy `ext_proc` and `ext_authz` on avirtedgeproxyd's Envoy.** Callouts on workload-to-workload, service, peer and off-host connections, for external authorization of every connection the host carries.
- **Cedar policy.** Host and workload policy sets, with policy invariants that the prover checks before any policy snapshot is built (see [egress policies](./egress-policies.md#the-prover)).
- **Secret providers.** avirtsecretsd reads real credentials from the providers that the host configuration names, and is the only component that does (see [egress credentials](./egress-credentials.md#status-planned)).

An extension adds checks and never removes one: Cedar decisions, the private-address block, `Host` and SNI matching, the placeholder misuse check and credential bindings apply whatever an extension decides.

# Core security principles

Status legend:

- **Met.** Implemented in the current code.
- **In Progress.** Partially implemented in the current code.
- **Future.** Specified in the planned design and not yet implemented.
- **Out of Scope.** Not avirt's job, and left to a layer outside it.

## No credentials in the workload container

The workload container holds placeholders and the workload's MITM CA certificate, neither of which is a secret. Every real credential stays with avirtsecretsd and is injected outside the workload container, on its own binding only.

- **Met.** avirthostd refuses a launch whose agent credential is missing, and passes credentials into the workload container by variable name, never on argv or in the image ([egress credentials](./egress-credentials.md#status-implemented)).
- **Met.** A workload receives only its own agent's credentials: a Codex workload container does not carry `CLAUDE_CODE_OAUTH_TOKEN` ([egress credentials](./egress-credentials.md#status-implemented)).
- **Future.** The workload container holds placeholders only, and a placeholder grants nothing outside its own workload, binding and destinations. Today the agent can read the Claude token in its environment and the Codex `auth.json` in its mounted home ([placeholders](./egress-credentials.md#placeholders)).
- **Future.** Model and MCP credentials are injected only at avirtmodelgwd and avirtmcpgwd, and the mediator holds no long-lived real credential ([bindings](./egress-credentials.md#bindings)).
- **Future.** avirtmedproxyd's placeholder misuse check rejects any issued placeholder found outside its bound header, in both policy modes ([placeholders](./egress-credentials.md#placeholders)).
- **Future.** Each credential has exactly one owner that refreshes it, and the Codex refresh token never leaves avirtsecretsd ([Codex refresh](./egress-credentials.md#codex-refresh)).
- **Future.** avirthostd holds credential binding references and no credential values ([host services](./components.md#host-services)).

## Default-deny egress through a single global PEP

The agent's only way out is its workload's mediator, every connection needs an explicit permit, and avirtedgeproxyd is the one policy enforcement point for every connection that leaves a workload for another workload, a service or the internet.

- **Met.** The workload container has no network interface other than loopback, and its only path out is avirtinitd's TCP proxy to `proxy.sock`, the mediator's CONNECT proxy ([egress](./egress.md#path)).
- **Met.** The mediator accepts only `CONNECT host:port` with an explicit port and a canonical hostname or IP literal ([CONNECT app](./egress.md#connect-app)).
- **Met.** Every resolved address is refused when it is in a non-global range or assigned to one of the host's own interfaces, read on every connection ([private-address block](./egress.md#private-address-block-private-dest)).
- **Future.** A connection, request or DNS query is allowed only by an explicit Cedar permit in enforce mode. Today every destination that passes the private-address block is allowed under rule `allow-all` ([decision semantics](./egress-policies.md#decision-semantics)).
- **Future.** avirtedgeproxyd is the policy enforcement point for every workload-to-workload, service, host-local service and external connection, and the only host service that opens connections off the host ([avirtedgeproxyd](./components.md#avirtedgeproxyd), [networking](./networking.md#off-host-enforcement)).
- **Future.** Workload DNS goes through the `resolve` action, which applies as evaluated in audit mode too, and every query is logged ([workload DNS](./egress.md#workload-dns)).
- **Future.** The `Host` of a request under TLS interception must equal its CONNECT host, and the SNI of a connection under TLS passthrough must equal its CONNECT host ([avirtmedproxyd pipeline](./egress.md#avirtmedproxyd-pipeline)).
- **Future.** Inbound connections reach only the ports a workload publishes, through avirtedgeproxyd and under the destination workload's `ingress` policy ([ingress](./workload-topology.md#ingress)).

## Strong workload identity

Every connection across a container or host boundary is authenticated by a short-lived SPIFFE identity bound to one workload and one mediator generation, and no VM can ask for an identity it was not issued.

- **Met.** Each workload's connections to avirthostd are authenticated by per-workload random secrets: the connection token (`AVIRT_UPLINK_TOKEN`), held only by avirtinitd and removed from the agent's environment, and the mediator secret, held only by the mediator ([identity](./identity.md#status-implemented)).
- **Future.** Every listener reachable across a container or host boundary requires mutual TLS with SPIFFE X.509-SVIDs from the first byte, and authorizes the caller's exact ID ([listeners](./trust-boundaries.md#listeners)).
- **Future.** No SPIFFE Workload API exists in any VM. avirthostd delivers each container's bootstrap SVID at creation and pushes every later SVID over an authenticated connection ([bootstrap](./identity.md#bootstrap)).
- **Future.** Long-lived keys are generated where they are used, and avirthostd zeroizes each bootstrap key after handing it to the runtime ([design invariants](./architecture.md#design-invariants)).
- **Future.** Workload and mediator SVIDs carry the namespace, the never-reused UUIDv7 and the mediator generation, live for 1 h, and a superseded generation is refused at every host listener ([authorization](./identity.md#authorization)).
- **Future.** A workload SVID is issued only for a request signed with the current workload key, so a compromised mediator cannot obtain one ([rotation](./identity.md#rotation)).
- **Future.** uid 1000 never holds an SVID or its key, the SPIFFE bundle, a CA key or a real credential ([local exceptions](./trust-boundaries.md#local-exceptions)).
- **Future.** The SPIFFE CA and the workload MITM CA are separate, and a certificate from one is never accepted where the other is expected ([two separate CAs](./identity.md#two-separate-cas)).

## Defense in depth in the sandbox

The agent is confined by several independent layers, and escaping one leaves the others in place.

- **Met.** Each workload container runs in its own VM under the Apple container CLI runtime ([compute runtimes](./compute-runtimes.md#apple-container-cli-runtime)).
- **Met.** The agent runs as uid 1000 with every capability set empty and `no_new_privs`, and its environment carries no workload ID, prompt or connection token ([agent process](./workload-hardening.md#agent-process)).
- **Met.** Seccomp denies `AF_VSOCK`, `AF_NETLINK` and `AF_PACKET` sockets and io_uring, to avirtinitd and everything it starts, before the agent starts ([seccomp](./workload-hardening.md#seccomp)).
- **Met.** `proxy.sock` and `uplink.sock` are `root:root 0600`, and the agent cannot read `/proc/1/fd`, `/proc/1/mem` or `/proc/1/environ` or signal PID 1 ([what the agent can still do](./workload-hardening.md#what-the-agent-can-still-do)).
- **Future.** `socket()` allows only `AF_UNIX`, `AF_INET` and `AF_INET6`, and the denylist blocks fileless exec, cross-process memory access, kernel exploit primitives, mounts and user namespaces ([denylist](./workload-hardening.md#denylist)).
- **Future.** Landlock confines the agent's filesystem writes to `/workspace`, `/home/agent` and `/tmp`, and the agent does not start without it ([sandbox](./sandbox.md)).
- **Future.** Every workload helper runs under its own UID and profile, and `/proc` is mounted with `hidepid=invisible` ([security contexts](./workload-hardening.md#security-contexts)).
- **Future.** The agent and every tool it starts run in one cgroup that avirtinitd freezes and kills as a unit ([freeze and reconnect](./workload-topology.md#freeze-and-reconnect)).
- **Future.** No workspace or configuration layer touches a reserved path, and no layer carries a setuid bit, a file capability or a device node ([reserved paths](./workload-hardening.md#reserved-paths), [content rules](./workspaces.md#content-rules)).
- **Future.** On Docker Desktop, where every workload shares one kernel, the runtime's capability and seccomp layers sit beneath avirtinitd's, and command builds are refused ([runtime outer layer](./workload-hardening.md#runtime-outer-layer)).
- **Out of Scope.** Approving individual tool calls inside the agent. Every permission request is approved automatically ([ACP](./acp.md#permission-requests)).

## Bounded mediator compromise

A compromised mediator controls its own workload's traffic and nothing else: it cannot reach another workload, a real model or MCP credential, or the network except through avirtedgeproxyd.

- **Met.** The mediator starts from an empty environment that holds only its secret ([mediator secret](./identity.md#mediator-secret)).
- **Met.** avirthostd binds each mediator connection to the workload its connection header names, and serves only that workload's `POST /workloads/<id>/stream` route on it ([sockets](./workload-topology.md#sockets)).
- **Future.** The mediator runs in its own VM with no network interface other than loopback. Today it is a host process with the user's network access and file access ([mediator network](./egress.md#mediator-network)).
- **Future.** The mediator holds no long-lived real credential for model or MCP traffic, only placeholders and short-lived mediator-injected credentials ([bindings](./egress-credentials.md#bindings)).
- **Future.** avirthostd's SNI routing never connects one mediator to another, and every host listener refuses a superseded generation ([SNI routing](./workload-topology.md#sni-routing)).
- **Future.** The workload's MITM CA is name-constrained to the hosts its policy can intercept, and a workload whose policy permits no interception has none ([MITM CA name constraints](./egress-policies.md#mitm-ca-name-constraints)).
- **Future.** Events the mediator reports are labelled `src: "mediator"` and trusted only as far as the mediator is ([event provenance](./trust-boundaries.md#event-provenance)).

## Least privilege for host services

Each host service holds only the state, secrets and callers it needs, and a local caller is authenticated before it can act.

- **Met.** avirthostd starts each workload container and each mediator from a cleared environment, adding only the variables the launch needs ([egress credentials](./egress-credentials.md#status-implemented)).
- **Met.** The runtime directory, workload directories and logs directory are 0700 and owned by the user, and every log file is 0600 ([log files](./hostd-api.md#log-files)).
- **Future.** Local clients authenticate before they act: avirtedgeproxyd's local client listener is a user-owned 0600 socket, and avirthostd's admin socket checks peer credentials. Today the control API has no authentication, and any local process can launch workloads ([control API](./hostd-api.md#control-api), [listeners](./trust-boundaries.md#listeners)).
- **Future.** Each host service owns only its own state, accepts only the callers it lists, and in the all-in-one server image runs under its own UID and reads only its own secrets ([all-in-one server image](./components.md#all-in-one-server-image)).
- **Future.** avirtbuilderd holds no signing key and no host control API access, and every build of untrusted content runs in a disposable build job ([builds](./workspace-build.md#builds)).
- **Future.** zot accepts only avirtregistryd, over mutual TLS with a private CA held in memory ([workspace registry](./workspace-registry.md#trust-boundaries)).

## Policy-governed workload management with no escalation

A workload can manage other workloads only as policy permits, and never gains more authority than it holds.

- **Met.** A workload has no control-plane access: its HTTP/2 connection to its mediator reaches only its own `POST /workloads/<id>/stream` route ([sockets](./workload-topology.md#sockets)).
- **Future.** avirthostd authorizes every management call per action and per resource against the host policy snapshot, and nothing is permitted by default ([workload management](./components.md#workload-management)).
- **Future.** A workload creates workloads only in namespaces where it may, with bindings it holds, from workspace images it may launch, and with its own or an assignable policy reference ([workload management](./components.md#workload-management)).
- **Future.** Only avirthostd sets `avirt.internal/` labels, and a workload cannot change its own namespace, labels, bindings or policy reference ([workload management](./components.md#workload-management)).
- **Future.** avirthostd records each workload's creator and root, and refuses a create past the delegation chain depth cap ([workload management](./components.md#workload-management)).
- **Future.** Host administration is never available to workload principals, and avirtadmin reaches avirthostd only on its local admin socket ([listeners](./trust-boundaries.md#listeners)).
- **Future.** Audit mode never widens control-plane authority ([modes](./egress-policies.md#modes)).
- **Future.** A build job has no `guest-cli` access ([build jobs](./workspace-build.md#build-jobs)).

## Fail closed

When a check cannot run, a dependency is missing or state is uncertain, avirt denies or ends the workload.

- **Met.** No agent starts until avirthostd has accepted the workload's HTTP/2 connection to its mediator and the seccomp filters are applied, and a filter that cannot be applied ends the workload ([connection to the mediator](./workload-topology.md#connection-to-the-mediator), [seccomp](./workload-hardening.md#seccomp)).
- **Met.** A stream POST with a missing or wrong token gets 403 before the workload's state changes, and a mediator connection header without the right secret is closed ([identity](./identity.md#status-implemented)).
- **Met.** A workload whose mediator is gone has no path out, and a mediator never outlives avirthostd ([mediator process](./egress.md#mediator-process)).
- **Met.** The Codex write-back replaces the host `auth.json` only with a strictly newer copy of the same login, and is skipped when the workload cannot be confirmed gone ([Codex](./egress-credentials.md#codex)).
- **Future.** No agent starts before the workload identity, the MITM CA, the placeholders and a valid policy snapshot are in place ([launch handshake](./workload-topology.md#launch-handshake)).
- **Future.** A missing or expired policy snapshot, a Cedar evaluation error and an avirt name that resolves to no entity each deny, in both modes ([policy failure behavior](./egress-policies.md#policy-failure-behavior)).
- **Future.** A lost connection between the workload and its mediator freezes the agent, and the workload ends if no mediator is confirmed within 30 s ([freeze and reconnect](./workload-topology.md#freeze-and-reconnect)).
- **Future.** Each side of every protocol refuses a version it does not support with a named error ([operational ownership](./components.md#operational-ownership)).

## Complete, tamper-evident audit

Every action an agent takes and every decision avirt makes is recorded, attributed to an authenticated identity, and cannot be dropped or rewritten unnoticed.

- **Met.** The workload log records every JSON-RPC line and every proxied connection before `container_exit` ([log files](./hostd-api.md#log-files), [events](./egress.md#events)).
- **Met.** Every automatic approval is recorded as an `auto_approve` meta event ([ACP](./acp.md#permission-requests)).
- **In Progress.** avirt's own events never carry a secret value: the Codex write-back logs a fixed reason string. Lines the agent produces can still carry the credentials it holds ([Codex](./egress-credentials.md#codex)).
- **Future.** Every event carries `workload`, `gen`, `seq` and a `src` label that avirthostd stamps from the authenticated connection it arrived on ([event provenance](./trust-boundaries.md#event-provenance)).
- **Future.** Every policy decision records the deciding rule and policy snapshot version, and every workload management call records the caller, the action, the resource and the delegation chain ([events](./egress.md#events), [workload management](./components.md#workload-management)).
- **Future.** No event is dropped silently: a full event ring refuses new work with 503, and a workload at its log's hard cap ends ([resuming after reconnect](./workload-topology.md#resuming-after-reconnect)).
- **Future.** avirtauditd delivers the log as OCSF, keeping `src`, and its backpressure never stalls a workload ([audit](./observability.md#audit)).
- **Future.** The workload log and every audit log are tamper-evident: each line carries a hash chain, and signed checkpoints reach every sink ([integrity](./observability.md#integrity)).

## Supply-chain integrity of images and workspaces

Every image and workspace a workload runs is pinned by digest, verified where it is used, and built from untrusted content only in a sandbox.

- **In Progress.** avirtinitd builds with `--locked` from `Cargo.lock`, and each adapter installs with `npm ci` from its lockfile. The base images are pinned by tag, and the Node tarball has no checksum ([Claude adapter](./acp-claude.md#adapter), [Codex adapter](./acp-codex.md#adapter)).
- **Future.** The guest kernel and initfs are pinned by URL and sha256, and a cache with a wrong hash is refused ([guest artifacts](./build-and-platforms.md#guest-artifacts)).
- **Future.** avirthostd verifies every artifact's digest when it is written and when a workload is created from it, and every workspace digest it pulls against the resolved manifest ([operational ownership](./components.md#operational-ownership), [workspace registry](./workspace-registry.md#trust-boundaries)).
- **Future.** avirtbuilderd and avirthostd both check every workspace layer against the content rules, and a workspace composes only onto the base its base image reference names ([content rules](./workspaces.md#content-rules), [base image reference](./workspaces.md#base-image-reference)).
- **Future.** Only avirtbuilderd promotes a build result, with annotations it computes itself, and a build job writes only to its staging repository ([build pipeline](./workspace-build.md#build-pipeline)).
- **Future.** An image with local provenance never stands in for a ref's image, and policy can forbid launching one ([local provenance](./workspaces.md#local-provenance)).
- **Future.** Promoted workspace images are signed, and `launch_workspace` can require a signature. Whether and how is open ([workspace builds](./workspace-build.md#open-questions)).
- **Out of Scope.** The trustworthiness of the packages, repositories and images an agent or a build fetches. avirt mediates and records those fetches and does not vet their content.

## Secrets never at rest in the clear

A secret exists in memory where it is used, and on disk only where an owner needs it, with the narrowest mode, for the shortest time.

- **Met.** No secret is placed on argv or in a workload image ([egress credentials](./egress-credentials.md#status-implemented)).
- **Met.** The per-launch Codex home is 0700 with a 0600 `auth.json`, and it is removed on every exit path or left for the next startup cleanup ([Codex](./egress-credentials.md#codex)).
- **Future.** The SPIFFE issuing CA, every SVID key and the workload MITM CA key are held in memory only. The SPIFFE root key is a 0600 file in the state directory ([issuer](./identity.md#issuer), [durable state](./lifecycle.md#durable-state)).
- **Future.** A gateway keeps a resolved credential in memory only, and for no longer than the credential's own lifetime ([credential broker API](./egress-credentials.md#credential-broker-api-and-sds)).
- **Future.** The configuration layer holds no secret and no placeholder value ([configuration layer](./workload-lifecycle.md#configuration-layer)).
- **Future.** Workspace images exclude secrets by a denylist at build time and again at launch ([secret exclusion](./workspaces.md#secret-exclusion)).
- **Future.** Durable secrets, such as the SPIFFE root key and avirtsecretsd's credentials, are encrypted at rest. No document specifies the mechanism yet (see [open questions](#open-questions)).
- **Out of Scope.** Protecting the operator's own credential files, such as `~/.codex/auth.json`, from other processes of the same user.

## Peering trust

Peered hosts trust each other's identities explicitly and narrowly: a peer authenticates as its own trust domain, and reaches only what this host's policy publishes to it.

- **Future.** Peers federate by explicit enrollment with a bundle obtained out of band, and never share a CA key ([federation](./identity.md#federation)).
- **Future.** The peer listener accepts only an enrolled peer's `/service/avirtedgeproxyd`, and only caller identities in that peer's own trust domain ([federation](./identity.md#federation)).
- **Future.** A peer reaches only the published ports, services and avirtapid routes that this host's host policy snapshot and its enrollment allow, and never avirthostd or the gateways ([avirtedgeproxyd](./components.md#avirtedgeproxyd), [peering](./peering.md#what-crosses-a-peering)).
- **Future.** Cross-host traffic passes both hosts' avirtedgeproxyd, and each evaluates its own host policy snapshot ([peering](./peering.md#evaluation-on-each-side)).
- **Future.** Managing a peer's workloads goes through that peer's avirtapid, under the peer's policy for the caller's fully qualified identity ([workload management](./components.md#workload-management)).
- **Future.** An operator can revoke a peer before its bundle expires, which closes every connection with it ([revocation and removal](./peering.md#revocation-and-removal)).
- **Out of Scope.** Deciding which hosts to peer with. Enrollment is the operator's decision.

# Open questions

1. **Secrets at rest.** Whether durable secrets are encrypted at rest, such as the SPIFFE root key in the macOS Keychain, and how avirtsecretsd stores what it caches from providers, is open.
2. **Base image pinning.** Whether the avirt base, the harness bases and the mediator image pin their own base images by digest and their downloads by checksum is open.
3. **Extension callouts.** Which filter positions `ext_proc` and `ext_authz` take in each Envoy relative to libavirtenvoy, what they see under TLS passthrough, and how they fail, is open.
4. **Off-host enforcement.** Which host mechanism enforces that only avirtedgeproxyd opens connections off the host is open (see [off-host enforcement](./networking.md#off-host-enforcement)).

# Threat model

What each listener accepts, and what a compromised agent, guest root, mediator, avirtedgeproxyd, build job, registry, builder or peer can and cannot reach, is in [trust boundaries](./trust-boundaries.md).
