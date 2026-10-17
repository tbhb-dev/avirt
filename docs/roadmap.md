---
type: Roadmap
title: Roadmap
description: The planned phases from the implemented system to the planned design, each with its changes, what it removes and its verification checklist, plus the regression suite, the workload process split, unscheduled runtimes and components, and the open questions to resolve first.
tags: [avirt, roadmap, verification]
implementation: planned
generated: { by: claude-code/claude-opus-5-5, at: 2026-10-17T14:23:31Z }
---

# Status: planned

Each phase is one commit that passes `mise run check`, its own checklist and the regression suite below. Each phase also records the [performance metrics](./performance.md) for the stages it introduces.

- The Containerization runtime lands first. It is the foundation for the avirthostd-spliced vsock link and SPIFFE mutual TLS.
- The Apple container CLI runtime is removed right after, once the Containerization runtime matches current behavior.
- The Docker Desktop runtime is built directly on the new link protocol.

# Regression suite

Every phase runs this suite on every runtime it touches. The bypass checks run inside the agent's own process tree, by asking the agent to run a script, because a process started from outside the agent's tree is not under its restrictions. Each check must fail, and fail fast:

- direct TCP (`curl --noproxy '*'`, bash `/dev/tcp`), UDP, DNS (`getent`, a Node resolver aimed at 1.1.1.1 and at the proxy host) and IPv6;
- Node `fetch` with every proxy variable unset;
- `/proc/self/status` shows capabilities at 0 and `NoNewPrivs: 1`;
- `su`, `mount` and the setuid binaries gain nothing, and `unshare -Urn` gets an empty network namespace;
- opening the actor's sockets directly gives `EACCES` (they are listed as `root:root 0600`), and no avirthostd socket exists anywhere in the workload's filesystem;
- `/proc/1/fd`, `/proc/1/mem` and `/proc/1/environ` are unreadable, and `kill -9 1` is refused;
- CONNECT to `127.0.0.1`, `localhost`, `[::1]` and the host's own addresses gets 403 `private-dest`, and an absolute-URI `GET` gets 501 `plain-http`.

Positive checks, for each agent: a web fetch, `npm install left-pad` and a `git clone` of `octocat/Hello-World`. Each must produce allow lines in `logs/<actor-id>.ndjson` for the model API, example.com, registry.npmjs.org and github.com, all before `container_exit`.

Lifecycle checks:

- SIGINT mid-session leaves no mediator, no actor directory and no VM or container;
- killing the mediator mid-session ends the actor with a `container_error` (freeze and resume from phase 3 on);
- after avirthostd is killed with SIGKILL, the next avirthostd start sweeps everything it left.

# Phases

| Phase | Changes | Removes | Verify |
| --- | --- | --- | --- |
| 1. Containerization runtime | The `Runtime` trait in `avirt-core`. `crates/avirt-runtime-containerization` with the Swift shim (`.dynamic`, `@_cdecl`), loaded through an `@executable_path` rpath. The signing runner for `cargo run` and `cargo test`. The pinned kernel (URL and sha256, extracted once and cached), `vminit:0.47.0` through `initfsReference`, and avirthostd's own image store and `containers/<id>/` directories in the state root. The implemented actor shape (host mediator, relayed `proxy.sock` and `uplink.sock`) on Containerization. avirtinitd's seccomp filters applied as before. `AVIRT_RUNTIME` selects `containerization` (default) or the CLI runtime until phase 2 removes it. | Nothing | The regression suite and positive checks pass for both agents with `container` absent from `PATH`. `codesign -d --entitlements -` on avirthostd and on test binaries shows `com.apple.security.virtualization`. A kernel cache with a wrong sha256 is refused and re-fetched. Inside the workload, `socket(AF_VSOCK)` gives errno 97. After avirthostd is killed with SIGKILL and restarted, no VM is running and no `containers/<id>/` directory remains, while the kernel, initfs and image caches remain. |
| 2. Remove the Apple container CLI runtime | Containerization is the only Apple runtime, once it matches current behavior | The CLI runtime, its `container run` invocations, the `-v` socket-file mounts, the label-based CLI sweep, and the `container` prerequisite | Parity: every check that passed on the CLI runtime passes on Containerization, including the regression suite, both agents' positive checks, `--cwd` mounts and the Codex `auth.json` write-back. The build and tests pass with no `container` binary installed. `rg 'container run'` finds nothing in the crates. |
| 3. Link protocol and SPIFFE over vsock (mediator still on the host) | avirtinitd listens on vsock and avirthostd dials it and splices it to the host mediator. The h2 link (control, uplink, CONNECT), avirthostd's SPIFFE issuer, bootstrap SVIDs, mutual TLS with exact-ID authorization on the link and on the avirthostd channel, the generation in the SVID path, avirthostd-initiated CSR rotation over `GET /actor/control` with an immediate first rotation, freeze and resume, `seq` uplink. | The relayed `proxy.sock` and `uplink.sock`, `AVIRT_UPLINK_TOKEN`, `AVIRT_MEDIATOR_SECRET`, the L4 uplink splice, and the `ACTOR` and `EGRESS` preambles | A connection to any avirthostd vsock listener without a valid SVID fails before any application byte is read. A capture at avirthostd's splice shows only TLS records. `kill -9` of the mediator mid-turn freezes the agent (`State: T`), a restart resumes it, and the log has no gap and no duplicates. A mediator SVID with an old generation, another actor's id or a MITM-CA signature is refused. With the SVID lifetime set to 2 minutes, rotation happens over new connections and long CONNECT streams survive until expiry. `env` in the agent shows no `AVIRT_SVID*`. Positive web, npm and git checks. |
| 4. Mediator in its own VM | The mediator VM on a `VmnetNetwork` NAT interface, nft, capability drop, public DNS resolvers, `deny_addrs` from avirthostd, the avirthostd channel over vsock, mediator VMs in the sweep | The host-process mediator and its stdin-EOF lifetime | From inside the mediator, as root after the drop: the NAT gateway, the host, the host's LAN IP, the LAN router, peer VMs on the NAT subnet, `10/8` and the host's IPv6 addresses are unreachable, DNS works, and `nft list` shows the drop sets. CONNECT to the host IP gives 403. Throughput is at least 60 MB/s. Killing avirthostd with SIGKILL and restarting it leaves no VMs and no directories. |
| 5. Docker Desktop runtime | `runtime-docker`, built directly on the link protocol: the per-actor link volume, the file-mounted avirthostd socket with dial retry, the Docker DNS exception, the outer capability and seccomp layer, labelled containers, networks and volumes in the sweep, `isolation` in the `start` meta event | Nothing | The regression suite and the phase 3-4 checks pass under Docker. From the workload, a vsock scan gets `EPERM` and `/run/avirt/link` gives `EACCES`. From the mediator, `host.docker.internal` (including services bound only to the Mac's loopback), the LAN IP and the router are unreachable while DNS works. `docker ps -a --filter label=avirt=1` is empty after avirthostd is killed with SIGKILL and restarted. |
| 6. TLS termination, audit mode | avirthostd's MITM CA, the leaf cache, the rustls `HttpProxy` path, `tls: skip`, the `http` events, CA delivery in `confirm` | Nothing | Claude, Codex, npm, git and curl work through termination, and the `http` lines show method, host, path and status. `openssl s_client` inside the workload shows the actor CA. A pinned test client fails, and passes with `tls: skip`. The CA key appears in no workload file or environment. Codex's WSS session works through termination. |
| 7. Claude placeholders | Credential bindings, exact-match substitution, the leak scan, the placeholder in the agent's environment | The real `CLAUDE_CODE_OAUTH_TOKEN` in the workload | `env` and `/proc/*/environ` in the workload show only `sk-ant-oat01-avirtph…`. A prompt that curls the placeholder to example.com gets 403 `credential-misplaced`. A placeholder in a body sent to api.anthropic.com gets 403. A CRLF in the real value is refused. A full Claude ACP session works. |
| 8. Codex placeholders and refresh | The placeholder `auth.json` in `confirm`, the refresh intercept, avirthostd refresh with flock single-flight | The `auth.json` seed and write-back, the `CODEX_HOME` mount, the scratch homes | The workload's `auth.json` holds only placeholders. Forcing a refresh (`last_refresh` backdated) rewrites the host file, the agent receives new placeholders, and the ACP session continues. Two concurrent actors both survive a refresh. The account and plan still show in `auth_methods` and the logs. |
| 9. Seccomp allowlist and Landlock | The socket-domain allowlist and the deny list, Landlock as a hard requirement, avirtinitd's capability drop | The denylist-only filter | `socket(AF_RXRPC)` gives errno 97. `memfd_create`, `ptrace` and `unshare -U` give `EPERM`. Writing `/etc`, `/opt/agent` or `/run/avirt` gives `EACCES`. Codex workspace-write tool commands still run, and Node, npm and git still pass, on both runtimes. |
| 10. Enforce mode | `mode: enforce`, `default: deny`, an allowlist built from phase 6's `http` logs | The `allow-all` rule | Non-listed hosts get 403 with a `deny` line, and listed flows still work. |

# Workload process split

Status: planned. These items split avirtinitd's current functions into the workload components in [components](./components.md). They are not assigned to phases. Each item passes `mise run check`, its own checks and the regression suite.

| Item | Changes | Removes | Verify |
| --- | --- | --- | --- |
| avirtacp | avirtacp as a separate binary that avirtinitd starts and supervises as a child. avirtacp carries the ACP client: it spawns the adapter as uid 1000 and taps every JSON-RPC line. | The ACP client in avirtinitd | The regression suite and positive checks pass for both agents. In the workload, avirtacp is a child of avirtinitd and the adapter is a child of avirtacp. `cargo tree -p avirtinitd` shows no `agent-client-protocol`. The log still records every JSON-RPC line before `container_exit`. |
| avirtnetd | avirtnetd as a separate binary that avirtinitd starts and supervises as a child. avirtnetd carries the loopback egress relay to the mediator. | The proxy relay in avirtinitd | The regression suite and positive checks pass for both agents. The loopback relay port is owned by avirtnetd, and avirtinitd holds no listening socket for it. |

# Unscheduled

- **Linux host runtime.** A KVM microVM per workload and mediator with vhost-vsock, mapping to the avirthostd-spliced vsock link. It needs `/dev/kvm`.
- **Windows host runtime.** Hyper-V through HCS with hvsock, mapping to the same avirthostd-spliced link. It needs Hyper-V Administrators membership.
- **Pure-Rust Apple runtime (deferred).** VMs through `objc2-virtualization`, with a Rust guest init in place of vminitd, replacing the Swift shim.

Status: planned. These components from [components](./components.md) have no phase or item yet:

- **avirtinitd as PID 1 and supervisor.** `avirtinitd` as PID 1 in the workload with the Landlock sandbox, and as the supervisor in the mediator container and the all-in-one server image.
- **Workload components.** avirtagent, avirtexec, avirtguestd, avirtmcp and avirtguest, plus avirtnetd's ingress gateway and guest control gRPC API.
- **Mediator container.** `avirtmediatord` in its own container under avirtinitd, Envoy Proxy, and the libavirtenvoy dynamic module.
- **Host request path.** avirtproxyd, the Envoy instances that front avirthostd, avirtproxyd, avirtmcpgwd and avirtmodelgwd, and avirt talking to avirtproxyd.
- **Host services.** avirtadmin, avirtdnsd, avirtmcpgwd, avirtmodelgwd, avirtbuilderd, avirtpolicyd and avirtsecretsd, and avirthostd's management of workspaces, secrets and Cedar policies.
- **Observability.** avirtlogd, avirtauditd and avirtoteld.
- **All-in-one server image.** The host components in one container under avirtinitd.

# Open questions

Resolve these before building the phases that depend on them:

1. **Codex with a placeholder `auth.json`, end to end (phase 8).** Check the synthesized refresh response through Codex's token persistence, its reload from disk on a 401, and codex-acp's login path, using a hand-made placeholder `auth.json` against a stub mediator and then a forced real refresh.
2. **Codex's own tool sandbox under the planned seccomp filter (phase 9).** Does workspace-write need `CLONE_NEWUSER` or nested seccomp? Docker's default profile also blocks `unshare`, so the answer affects both runtimes.
3. **The Pingora rustls path on linux-arm64 (phase 6).** `HttpProxy::new` with a per-tunnel `Acceptor` and the `SslDigest` extension, h2 upstreams, and Codex's WSS upgrade through it. Check with a terminated `curl --http1.1` and a `wss://` echo.
4. **The SPIFFE verifier and rotation (phase 3).** Run conformance tests against the X.509-SVID rules (chain to bundle, exact-ID authorization, name constraints, no web roots), plus a soak with a 2-minute lifetime and long streams across rotations.
5. **Landlock in the running Apple guest (phase 9).** `landlock_create_ruleset(NULL, 0, LANDLOCK_CREATE_RULESET_VERSION)` must return at least 6 inside a workload, since a kernel `lsm=` argument could drop Landlock.
6. **CA coverage under termination (phase 6).** Does Claude Code's `NODE_EXTRA_CA_CERTS` path cover every client it uses, and which `/api/*` calls does it make at startup?
7. **Freeze completeness (phase 3).** The SIGSTOP rescan races `fork`. If cgroup v2 is writable in the guest, use `cgroup.freeze`. Check with a fork bomb under a mediator kill.
8. **Host address drift.** An interface address added while an actor runs is not in `deny6`. The private ranges cover the LAN.
