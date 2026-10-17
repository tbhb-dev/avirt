---
type: Roadmap
title: Roadmap
description: The planned phases from the implemented system to the planned design, each with its changes, what it removes and its verification checklist, plus the regression suite, the Envoy transition and its parity gates, the workload process split, unscheduled runtimes, components and operational work, and the open questions to resolve first.
tags: [avirt, roadmap, verification, envoy]
implementation: planned
generated: { by: claude-code/claude-opus-5-5, at: 2026-10-17T19:00:00Z }
---

# Status: planned

Each phase is one commit that passes `mise run check`, its own checklist and the regression suite below. Each phase also records the [performance metrics](./performance.md) for the stages it introduces.

- The Containerization runtime lands first. It is the foundation for the avirthostd-spliced vsock link and SPIFFE mutual TLS.
- The Apple container CLI runtime is removed right after, once the Containerization runtime matches current behavior.
- The mediator's data plane moves to Envoy with libavirtenvoy in phase 4, before TLS interception, policy or credential work is built, and every later phase builds on Envoy.
- The Docker Desktop runtime is built directly on the new link protocol and the networkless mediator.

# Regression suite

Every phase runs this suite on every runtime it touches. The bypass checks run inside the agent's own process tree, by asking the agent to run a script, because a process started from outside the agent's tree is not under its restrictions. Each check must fail, and fail fast:

- direct TCP (`curl --noproxy '*'`, bash `/dev/tcp`), UDP, DNS (`getent`, a Node resolver aimed at 1.1.1.1 and at the proxy host) and IPv6. Once the workload DNS stub exists, `getent` of an allowed name resolves through it and appears as a `dns` event, a name the policy denies gets `NXDOMAIN`, and DNS aimed at any other address still fails;
- Node `fetch` with every proxy variable unset;
- `/proc/self/status` shows capabilities at 0 and `NoNewPrivs: 1`;
- `su`, `mount` and the setuid binaries gain nothing, and `unshare -Urn` gets an empty network namespace;
- opening the actor's sockets directly gives `EACCES` (they are listed as `root:root 0600`, or owned by a helper UID with mode 0600 once the helpers exist), and no avirthostd socket exists anywhere in the workload's filesystem;
- `/proc/1/fd`, `/proc/1/mem` and `/proc/1/environ` are unreadable, and `kill -9 1` is refused;
- CONNECT to `127.0.0.1`, `localhost`, `[::1]` and the host's own addresses gets 403 `private-dest`, and an absolute-URI `GET` gets 501 `plain-http`.

Positive checks, for each agent: a web fetch, `npm install left-pad` and a `git clone` of `octocat/Hello-World`. Each must produce allow lines in `logs/<actor-id>.ndjson` for the model API, example.com, registry.npmjs.org and github.com, all before `container_exit`.

Lifecycle checks:

- SIGINT mid-session leaves no mediator, no actor directory and no VM or container;
- killing the mediator mid-session ends the actor with a `container_error` (freeze and resume from phase 3 on);
- after avirthostd is killed with SIGKILL, the next avirthostd start sweeps everything it left, and leaves the durable state directory untouched.

# Phases

| Phase | Changes | Removes | Verify |
| --- | --- | --- | --- |
| 1. Containerization runtime | The `Runtime` trait in `avirt-core`. `crates/avirt-runtime-containerization` with the Swift shim (`.dynamic`, `@_cdecl`), loaded through an `@executable_path` rpath. The signing runner for `cargo run` and `cargo test`. The pinned kernel (URL and sha256, extracted once and cached), `vminit:0.47.0` through `initfsReference`, and avirthostd's own image store and `containers/<id>/` directories in the state root. The implemented actor shape (host mediator, relayed `proxy.sock` and `uplink.sock`) on Containerization. avirtinitd's seccomp filters applied as before. `AVIRT_RUNTIME` selects `containerization` (default) or the CLI runtime until phase 2 removes it. | Nothing | The regression suite and positive checks pass for both agents with `container` absent from `PATH`. `codesign -d --entitlements -` on avirthostd and on test binaries shows `com.apple.security.virtualization`. A kernel cache with a wrong sha256 is refused and re-fetched. Inside the workload, `socket(AF_VSOCK)` gives errno 97. After avirthostd is killed with SIGKILL and restarted, no VM is running and no `containers/<id>/` directory remains, while the kernel, initfs and image caches remain. |
| 2. Remove the Apple container CLI runtime | Containerization is the only Apple runtime, once it matches current behavior | The CLI runtime, its `container run` invocations, the `-v` socket-file mounts, the label-based CLI sweep, and the `container` prerequisite | Parity: every check that passed on the CLI runtime passes on Containerization, including the regression suite, both agents' positive checks, `--cwd` mounts and the Codex `auth.json` write-back. The build and tests pass with no `container` binary installed. `rg 'container run'` finds nothing in the crates. |
| 3. Link protocol and SPIFFE over vsock (mediator still on the host) | The workload's link endpoint on vsock (avirtinitd, until the avirtnetd item below moves it), which avirthostd dials and splices to the host mediator. The h2 link (control, uplink, CONNECT) with avirtmediatord splicing each CONNECT stream into its existing CONNECT app. avirthostd's SPIFFE issuer with the installation trust domain, the durable root and the in-memory issuing CA. Bootstrap SVIDs, mutual TLS with exact-ID authorization on the link and on the avirthostd channel, the generation in the SVID path, avirthostd-initiated rotation over `GET /actor/control` with an immediate first rotation, workload-signed inner requests for workload rotation, the agent cgroup with `cgroup.freeze` and `cgroup.kill`, and the `seq` uplink. | The relayed `proxy.sock` and `uplink.sock`, `AVIRT_UPLINK_TOKEN`, `AVIRT_MEDIATOR_SECRET`, the L4 uplink splice, and the `ACTOR` and `EGRESS` preambles | A connection to any avirthostd vsock listener without a valid SVID fails before any application byte is read. A capture at avirthostd's splice shows only TLS records. `kill -9` of the mediator mid-turn freezes the agent cgroup (`frozen 1` in `cgroup.events`), a fork bomb in the agent is frozen with it, a restart resumes it, and the log has no gap and no duplicates. A mediator SVID with an old generation, another actor's id or a MITM-CA signature is refused. A relayed workload CSR without a valid inner signature, or with a stale nonce, is refused. After avirthostd restarts, the trust domain and root are unchanged. With the SVID lifetime set to 2 minutes, rotation happens over new connections and long CONNECT streams survive until expiry. `env` in the agent shows no `AVIRT_SVID*`. Positive web, npm and git checks. |
| 4. Envoy transition (mediator still on the host) | The `libavirtenvoy` crate. The mediator Envoy per actor, started beside avirtmediatord, terminating the link as TLS and h2 server and serving the agent's CONNECT streams. avirtmediatord as the control agent: local xDS and SDS, the Envoy's key and `/envoy` SVID, the link control stream. The front-door Envoy with its egress listener, dynamic forward proxy and the destination check in libavirtenvoy, with `getifaddrs` on every connection. avirthostd's service relay with SNI routing to the front-door Envoy. Generation fencing at the front-door Envoy. `conn` events from both Envoys with `src`. | Pingora and the CONNECT app in avirtmediatord, and every upstream dial from the mediator | The destination checks, websockets and restart fencing gates below. The regression suite and positive checks for both agents. Throughput through both Envoys is at least 60 MB/s. |
| 5. Mediator in its own VM | The mediator VM with no network interface, avirtinitd in mediator mode starting avirtmediatord and Envoy under their own UIDs, the avirthostd, service and ingress channels over vsock through the in-guest socket relay, the service relay's routes to host components, mediator VMs in the sweep | The host-process mediator and its stdin-EOF lifetime | Inside the mediator, `ip -o link` lists only `lo` and a `connect()` to the host's address fails with `ENETUNREACH`. The service relay closes a connection with an unknown SNI before forwarding a byte. Killing Envoy alone ends that generation, and the next generation resumes the agent. The restart fencing gate passes with the mediator in its VM. Throughput is at least 60 MB/s. Killing avirthostd with SIGKILL and restarting it leaves no VMs and no directories. |
| 6. Docker Desktop runtime | `runtime-docker`, built directly on the link protocol: the per-actor link volume, the file-mounted avirthostd and service channel sockets with dial retry, `--network none` for both containers, the outer capability and seccomp layers, labelled containers and volumes in the sweep, `isolation` in the `start` meta event | Nothing | The regression suite and the phase 3-5 checks pass under Docker, except those that need the ingress channel. From the workload, a vsock scan gets `EPERM` and `/run/avirt/link` gives `EACCES` to uid 1000. Inside the mediator, `ip -o link` lists only `lo`, and `host.docker.internal` is unreachable. `docker ps -a --filter label=avirt=1` is empty after avirthostd is killed with SIGKILL and restarted. |
| 7. Cedar policy, audit mode | avirtpolicyd with the Cedar policy prover gating every snapshot, the avirt Cedar schema, versioned snapshots with expiry and hash, libavirtenvoy's embedded `cedar-policy` evaluation in the mediator Envoy and the front-door Envoy, the policy reference in actor configuration, `rule` and `policy_version` in events | The `allow-all` rule | The policy failure behavior gate below. Every connection's event names the deciding Cedar policy and snapshot version. |
| 8. TLS interception, audit mode | avirthostd's MITM CA, leaf minting in libavirtenvoy, the `intercept` and `request` actions, re-originated TLS inside front-door tunnels, the `http` events, CA delivery in `confirm` | Nothing | The TLS interception gate below, and the websockets gate through termination. |
| 9. avirtsecretsd, avirtmodelgwd and Claude placeholders | avirtsecretsd with bindings, placeholder issuance and the broker API. avirtmodelgwd with injection for Anthropic bindings. Model API routing and the leak scan in libavirtenvoy. The placeholder in the agent's environment. | The real `CLAUDE_CODE_OAUTH_TOKEN` in the workload and in avirthostd's environment | The credential isolation gate below. A full Claude ACP session works. |
| 10. Codex placeholders and refresh | The placeholder `auth.json` from avirtsecretsd in `confirm`, the refresh intercept in libavirtenvoy, avirtmodelgwd's refresh endpoint and Codex bindings, avirtsecretsd's refresh with flock single-flight and atomic persist | The `auth.json` seed and write-back, the `CODEX_HOME` mount, the scratch homes | The credential refresh gate below. The workload's `auth.json` holds only placeholders. The account and plan still show in `auth_methods` and the logs. |
| 11. Seccomp allowlist, Landlock and process profiles | The socket-domain allowlist and the deny list, Landlock as a hard requirement, the per-process launch profiles and helper UIDs, avirtinitd's reduced capability set | The denylist-only filter | `socket(AF_RXRPC)` gives errno 97. `memfd_create`, `ptrace` and `unshare -U` give `EPERM`. Writing `/etc`, `/opt/agent` or `/run/avirt` gives `EACCES`. From uid 1000, `kill` of any helper gives `EPERM` and every helper's `/proc/<pid>/mem` is unreadable. `ps` shows each helper under its own UID. Codex workspace-write tool commands still run, and Node, npm and git still pass, on both runtimes. |
| 12. Enforce mode | Snapshots with `mode: enforce`, and a policy built from phase 8's `http` logs | Audit mode as the default | Non-listed hosts get 403 with a `deny` line, and listed flows still work. |

# Envoy transition

## Status: implemented

An opt-in [host Envoy mode](./egress.md#experimental-host-envoy-mode) provides a static CONNECT bootstrap, an avirtenvoy destination filter, child supervision and conversion of module events to the existing egress envelope. Pingora remains the default. This slice uses the relayed `proxy.sock` and `uplink.sock` transport and has no phase 3 dependency. Shared address-policy unit tests and structural bootstrap tests cover the implementation; a bootstrap validation test runs only when an Envoy binary is supplied. It advances the destination-check and data-plane portions of phase 4 without satisfying a runtime parity gate.

## Status: planned

Phase 4 moves the mediator's data plane to Envoy with libavirtenvoy and removes Pingora from avirtmediatord. Each later phase builds its part of the egress, policy and credential design on Envoy. These parity gates must pass, on every runtime the phase touches, before the phase is done:

| Gate | Phase | Pass condition |
| --- | --- | --- |
| Destination checks | 4 | Every destination case gives the same status and event as the implemented CONNECT app: 400 `bad-request` for each malformed authority (including `2130706433`, `0177.0.0.1`, `0x7f000001` and `127.1`), 403 `private-dest` for every non-global range and for each of the host's own addresses (including one added while the actor runs), 501 `plain-http` and `unsupported-method`, 502 and 504. |
| Websockets | 4, 8 | A `wss://` echo works through an opaque tunnel in phase 4, and through termination in phase 8 with the upgrade request logged. Codex's WSS session works through termination with its credential injected at avirtmodelgwd from phase 10. |
| Restart fencing | 4, 5 | `kill -9` of the mediator Envoy, and separately of avirtmediatord, mid-turn freezes the agent cgroup, the next generation resumes it, and the log has no gap and no duplicates. The superseded generation's SVIDs are refused at the link, the avirthostd channel, the front-door Envoy and every host component. |
| Policy failure behavior | 7 | With avirtpolicyd stopped, the current snapshot holds until it expires, and then new connections and requests are denied in both modes. A snapshot with a wrong hash, an unsupported schema or a lower version is rejected whole. A mediator without a snapshot never sends `confirm`. A Cedar evaluation error denies. In audit mode, `private-dest`, `Host` mismatch and malformed authorities are still refused. |
| TLS interception | 8 | Claude, Codex, npm, git and curl work through termination, and the `http` lines show method, host, path and status. `openssl s_client` inside the workload shows the actor CA. A pinned test client fails, and passes when `intercept` is forbidden and `passthrough` permitted for its host. A passthrough connection whose SNI differs from its CONNECT host is closed. An actor whose policy permits no `intercept` gets no MITM CA, and a name-constrained CA is refused by the agent for a host outside its constraints. The CA key appears in no workload file or environment. A capture at the front-door Envoy shows only TLS records inside re-originated tunnels. |
| Credential isolation | 9 | `env` and `/proc/*/environ` in the workload show only `sk-ant-oat01-avirtph…`. A prompt that curls the placeholder to example.com gets 403 `credential-misplaced`, and a placeholder in a body sent to api.anthropic.com gets 403. A CRLF in the real value is refused. The real token is absent from the mediator container's environment and files and from a core dump of the mediator Envoy. The broker refuses a request for another actor's binding, and a request from a superseded generation. |
| Credential refresh | 10 | Forcing a refresh (`last_refresh` backdated) rewrites the host file through avirtsecretsd, the agent receives new placeholders, and the ACP session continues. Two concurrent actors both survive a refresh. An old placeholder works for 60 s and then gets 403. A refresh request that does not match the intercept's endpoint and schema meets the leak scan. avirthostd never opens the host `auth.json`. |

# Workload process split

Status: planned. These items split avirtinitd's current functions into the workload components in [components](./components.md). They are not assigned to phases. Each item passes `mise run check`, its own checks and the regression suite.

| Item | Changes | Removes | Verify |
| --- | --- | --- | --- |
| avirtacp | avirtacp as a separate binary that avirtinitd starts and supervises as a child. avirtacp carries the ACP client, asks avirtinitd to launch the adapter as uid 1000 in the agent cgroup, receives its stdio pipes, and taps every JSON-RPC line. | The ACP client in avirtinitd | The regression suite and positive checks pass for both agents. In the workload, avirtacp is a child of avirtinitd, and the adapter is in the agent cgroup. `cargo tree -p avirtinitd` shows no `agent-client-protocol`. The log still records every JSON-RPC line before `container_exit`. |
| avirtnetd | avirtnetd as a separate binary that avirtinitd starts and supervises as a child. avirtnetd carries the link endpoint and the workload SVID, the uplink with its ring, the loopback egress proxy, and the stream sockets. | The link endpoint, the SVID and the proxy relay in avirtinitd | The regression suite and positive checks pass for both agents. The loopback proxy port and the link socket are owned by avirtnetd, and avirtinitd holds no listening socket and no key. Killing avirtnetd freezes and then ends the actor. |

# Unscheduled

- **Linux host runtime.** A KVM microVM per workload and mediator with vhost-vsock, mapping to the avirthostd-spliced vsock link. It needs `/dev/kvm`.
- **Windows host runtime.** Hyper-V through HCS with hvsock, mapping to the same avirthostd-spliced link. It needs Hyper-V Administrators membership.
- **Pure-Rust Apple runtime (deferred).** VMs through `objc2-virtualization`, with a Rust guest init in place of vminitd, replacing the Swift shim.

Status: planned. These items from [components](./components.md) have no phase or item yet:

- **avirtinitd role modes.** `avirtinitd` as PID 1 in the workload, and the server mode in the all-in-one server image.
- **Workload components.** avirtagent, avirtexec, avirtguestd, avirtmcp and avirtguest, plus avirtnetd's ingress gateway and guest control gRPC API.
- **Namespaces and labels.** Namespaces in avirthostd, label key authorization, `Workload` in `Namespace` in the Cedar schema, and the namespace in actor SPIFFE IDs.
- **avirt names and workload DNS.** avirtdnsd's name zone, avirtnetd's DNS stub resolver, the link DNS stream, `resolve` policy and `dns` events.
- **The front-door Envoy as policy enforcement point.** Port publications, virtual service records, host-local service backends, the ingress channel, name-to-entity resolution, and workload-to-workload connections through the front-door Envoy on the same host and across peered hosts.
- **Host request path.** avirtproxyd, the front-door Envoy's local client and peer listeners, avirt and avirtguest reaching avirtproxyd over gRPC and avirtwebd over REST through it, and avirtadmin on avirthostd's local admin socket.
- **Web UI.** The frontend app, its static assets bundled into avirtwebd, and avirtwebd's authentication and MFA methods.
- **Host services.** avirtadmin, avirtwebd, avirtdnsd, avirtmcpgwd and avirtbuilderd, the issuer socket for host components, and peer federation.
- **Observability.** avirtlogd tailing the host-written workload log files (avirthostd's non-blocking stdio writers and `logs/<actor-id>.stdio.ndjson` on Containerization, Docker's container log files on Docker Desktop), avirtauditd consuming avirthostd's actor log by offset, and avirtoteld.
- **All-in-one server image.** The host components in one container under avirtinitd in server mode.
- **Workload management.** Workloads creating, listing, inspecting, stopping and deleting workloads through avirtproxyd under Cedar policy, with the escalation constraints and lineage labels in [components](./components.md#workload-management).
- **Canonical workload IDs.** UUIDv7 actor IDs in SPIFFE IDs, Cedar entities and log records, with short IDs from their random bits and clash handling.
- **avirtregistryd.** The wrapped OCI registry for workspace images, its secondary index and search API, avirtbuilderd pushing to it and avirthostd pulling from it by digest.
- **Operational ownership.** Resource admission and accounting, the artifact store with digest verification and garbage collection, sandboxed build jobs, and protocol versioning with draining (see [components](./components.md#operational-ownership)).

# Open questions

Resolve these before building the phases that depend on them:

1. **Codex with a placeholder `auth.json`, end to end (phase 10).** Check the synthesized refresh response through Codex's token persistence, its reload from disk on a 401, and codex-acp's login path, using a hand-made placeholder `auth.json` against a stub gateway and then a forced real refresh.
2. **Codex's own tool sandbox under the planned seccomp filter (phase 11).** Does workspace-write need `CLONE_NEWUSER` or nested seccomp? Docker's default profile also blocks `unshare`, so the answer affects both runtimes.
3. **libavirtenvoy on Envoy's dynamic module interface (phases 4 and 8).** Check h2 CONNECT termination on a Unix-socket listener, per-SNI leaf certificate selection from the module, Cedar evaluation latency per request, websocket upgrades through termination, and the module's compatibility with the pinned Envoy version on linux-arm64 and macOS arm64. Check with a terminated `curl --http1.1` and a `wss://` echo.
4. **The SPIFFE verifier and rotation (phase 3).** Run conformance tests against the X.509-SVID rules (chain to bundle through the issuing CA, exact-ID authorization, name constraints, no web roots) and against the workload-signed inner request, plus a soak with a 2-minute lifetime and long streams across rotations.
5. **Landlock in the running Apple guest (phase 11).** `landlock_create_ruleset(NULL, 0, LANDLOCK_CREATE_RULESET_VERSION)` must return at least 6 inside a workload, since a kernel `lsm=` argument could drop Landlock.
6. **CA coverage under termination (phase 8).** Does Claude Code's `NODE_EXTRA_CA_CERTS` path cover every client it uses, and which `/api/*` calls does it make at startup?
7. **cgroup v2 in the guests (phases 3 and 6).** Does each runtime's guest give avirtinitd a writable cgroup v2 hierarchy with `cgroup.freeze` and `cgroup.kill`? Docker mounts `/sys/fs/cgroup` read-only by default. Check with a fork bomb under a mediator kill.
8. **Containerization channel presentation (phases 3 and 5).** Does Containerization's in-guest socket relay present vsock connections in both directions as the Unix sockets the channels need, with the ownership and modes the helper UIDs require?
