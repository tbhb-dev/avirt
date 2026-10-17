---
type: Roadmap
title: Roadmap
description: The planned phases from the implemented system to the planned design, each with its changes, what it removes and its verification checklist, plus the regression suite, unscheduled runtimes and the open questions to resolve first.
tags: [agentvmm, roadmap, verification]
implementation: planned
generated: { by: claude-code/claude-opus-5-5, at: 2026-10-17T14:23:31Z }
---

# Status: planned

Each phase is one commit that passes `mise run check`, its own checklist and the regression suite below. Each phase also records the [performance metrics](./performance.md) for the stages it introduces.

- The Containerization runtime lands first. It is the foundation for the controller-spliced vsock link and SPIFFE mutual TLS.
- The Apple container CLI runtime is removed right after, once the Containerization runtime matches current behavior.
- The Docker Desktop runtime is built directly on the new link protocol.

# Regression suite

Every phase runs this suite on every runtime it touches. The bypass checks run inside the agent's own process tree, by asking the agent to run a script, because a process started from outside the agent's tree is not under its restrictions. Each check must fail, and fail fast:

- direct TCP (`curl --noproxy '*'`, bash `/dev/tcp`), UDP, DNS (`getent`, a Node resolver aimed at 1.1.1.1 and at the proxy host) and IPv6;
- Node `fetch` with every proxy variable unset;
- `/proc/self/status` shows capabilities at 0 and `NoNewPrivs: 1`;
- `su`, `mount` and the setuid binaries gain nothing, and `unshare -Urn` gets an empty network namespace;
- opening the actor's sockets directly gives `EACCES` (they are listed as `root:root 0600`), and no controller socket exists anywhere in the workload's filesystem;
- `/proc/1/fd`, `/proc/1/mem` and `/proc/1/environ` are unreadable, and `kill -9 1` is refused;
- CONNECT to `127.0.0.1`, `localhost`, `[::1]` and the host's own addresses gets 403 `private-dest`, and an absolute-URI `GET` gets 501 `plain-http`.

Positive checks, for each agent: a web fetch, `npm install left-pad` and a `git clone` of `octocat/Hello-World`. Each must produce allow lines in `logs/<actor-id>.ndjson` for the model API, example.com, registry.npmjs.org and github.com, all before `container_exit`.

Lifecycle checks:

- SIGINT mid-session leaves no supervisor, no actor directory and no VM or container;
- killing the supervisor mid-session ends the actor with a `container_error` (freeze and resume from phase 3 on);
- after the controller is killed with SIGKILL, the next controller start sweeps everything it left.

# Phases

| Phase | Changes | Removes | Verify |
| --- | --- | --- | --- |
| 1. Containerization runtime | The `Runtime` trait in `agentvmm-core`. `crates/agentvmm-runtime-containerization` with the Swift shim (`.dynamic`, `@_cdecl`), loaded through an `@executable_path` rpath. The signing runner for `cargo run` and `cargo test`. The pinned kernel (URL and sha256, extracted once and cached), `vminit:0.47.0` through `initfsReference`, and the controller's own image store and `containers/<id>/` directories in the state root. The implemented actor shape (host supervisor, relayed `proxy.sock` and `uplink.sock`) on Containerization. The runner's seccomp filters applied as before. `AGENTVMM_RUNTIME` selects `containerization` (default) or the CLI runtime until phase 2 removes it. | Nothing | The regression suite and positive checks pass for both agents with `container` absent from `PATH`. `codesign -d --entitlements -` on the controller and on test binaries shows `com.apple.security.virtualization`. A kernel cache with a wrong sha256 is refused and re-fetched. Inside the workload, `socket(AF_VSOCK)` gives errno 97. After the controller is killed with SIGKILL and restarted, no VM is running and no `containers/<id>/` directory remains, while the kernel, initfs and image caches remain. |
| 2. Remove the Apple container CLI runtime | Containerization is the only Apple runtime, once it matches current behavior | The CLI runtime, its `container run` invocations, the `-v` socket-file mounts, the label-based CLI sweep, and the `container` prerequisite | Parity: every check that passed on the CLI runtime passes on Containerization, including the regression suite, both agents' positive checks, `--cwd` mounts and the Codex `auth.json` write-back. The build and tests pass with no `container` binary installed. `rg 'container run'` finds nothing in the crates. |
| 3. Link protocol and SPIFFE over vsock (supervisor still on the host) | The runner listens on vsock and the controller dials it and splices it to the host supervisor. The h2 link (control, uplink, CONNECT), the controller's SPIFFE issuer, bootstrap SVIDs, mutual TLS with exact-ID authorization on the link and on the controller channel, the generation in the SVID path, controller-initiated CSR rotation over `GET /actor/control` with an immediate first rotation, freeze and resume, `seq` uplink. | The relayed `proxy.sock` and `uplink.sock`, `AGENTVMM_UPLINK_TOKEN`, `AGENTVMM_SUPERVISOR_SECRET`, the L4 uplink splice, and the `ACTOR` and `EGRESS` preambles | A connection to any controller vsock listener without a valid SVID fails before any application byte is read. A capture at the controller's splice shows only TLS records. `kill -9` of the supervisor mid-turn freezes the agent (`State: T`), a restart resumes it, and the log has no gap and no duplicates. A supervisor SVID with an old generation, another actor's id or a MITM-CA signature is refused. With the SVID lifetime set to 2 minutes, rotation happens over new connections and long CONNECT streams survive until expiry. `env` in the agent shows no `AGENTVMM_SVID*`. Positive web, npm and git checks. |
| 4. Supervisor in its own VM | The supervisor VM on a `VmnetNetwork` NAT interface, nft, capability drop, public DNS resolvers, `deny_addrs` from the controller, the controller channel over vsock, supervisor VMs in the sweep | The host-process supervisor and its stdin-EOF lifetime | From inside the supervisor, as root after the drop: the NAT gateway, the host, the host's LAN IP, the LAN router, peer VMs on the NAT subnet, `10/8` and the host's IPv6 addresses are unreachable, DNS works, and `nft list` shows the drop sets. CONNECT to the host IP gives 403. Throughput is at least 60 MB/s. Killing the controller with SIGKILL and restarting it leaves no VMs and no directories. |
| 5. Docker Desktop runtime | `runtime-docker`, built directly on the link protocol: the per-actor link volume, the file-mounted controller socket with dial retry, the Docker DNS exception, the outer capability and seccomp layer, labelled containers, networks and volumes in the sweep, `isolation` in the `start` meta event | Nothing | The regression suite and the phase 3-4 checks pass under Docker. From the workload, a vsock scan gets `EPERM` and `/run/agentvmm/link` gives `EACCES`. From the supervisor, `host.docker.internal` (including services bound only to the Mac's loopback), the LAN IP and the router are unreachable while DNS works. `docker ps -a --filter label=agentvmm=1` is empty after the controller is killed with SIGKILL and restarted. |
| 6. TLS termination, audit mode | The controller's MITM CA, the leaf cache, the rustls `HttpProxy` path, `tls: skip`, the `http` events, CA delivery in `confirm` | Nothing | Claude, Codex, npm, git and curl work through termination, and the `http` lines show method, host, path and status. `openssl s_client` inside the workload shows the actor CA. A pinned test client fails, and passes with `tls: skip`. The CA key appears in no workload file or environment. Codex's WSS session works through termination. |
| 7. Claude placeholders | Credential bindings, exact-match substitution, the leak scan, the placeholder in the agent's environment | The real `CLAUDE_CODE_OAUTH_TOKEN` in the workload | `env` and `/proc/*/environ` in the workload show only `sk-ant-oat01-agentvmmph…`. A prompt that curls the placeholder to example.com gets 403 `credential-misplaced`. A placeholder in a body sent to api.anthropic.com gets 403. A CRLF in the real value is refused. A full Claude ACP session works. |
| 8. Codex placeholders and refresh | The placeholder `auth.json` in `confirm`, the refresh intercept, controller refresh with flock single-flight | The `auth.json` seed and write-back, the `CODEX_HOME` mount, the scratch homes | The workload's `auth.json` holds only placeholders. Forcing a refresh (`last_refresh` backdated) rewrites the host file, the agent receives new placeholders, and the ACP session continues. Two concurrent actors both survive a refresh. The account and plan still show in `auth_methods` and the logs. |
| 9. Seccomp allowlist and Landlock | The socket-domain allowlist and the deny list, Landlock as a hard requirement, the runner's capability drop | The denylist-only filter | `socket(AF_RXRPC)` gives errno 97. `memfd_create`, `ptrace` and `unshare -U` give `EPERM`. Writing `/etc`, `/opt/agent` or `/run/agentvmm` gives `EACCES`. Codex workspace-write tool commands still run, and Node, npm and git still pass, on both runtimes. |
| 10. Enforce mode | `mode: enforce`, `default: deny`, an allowlist built from phase 6's `http` logs | The `allow-all` rule | Non-listed hosts get 403 with a `deny` line, and listed flows still work. |

# Unscheduled

- **Linux host runtime.** A KVM microVM per workload and supervisor with vhost-vsock, mapping to the controller-spliced vsock link. It needs `/dev/kvm`.
- **Windows host runtime.** Hyper-V through HCS with hvsock, mapping to the same controller-spliced link. It needs Hyper-V Administrators membership.
- **Pure-Rust Apple runtime (deferred).** VMs through `objc2-virtualization`, with a Rust guest init in place of vminitd, replacing the Swift shim.

# Open questions

Resolve these before building the phases that depend on them:

1. **Codex with a placeholder `auth.json`, end to end (phase 8).** Check the synthesized refresh response through Codex's token persistence, its reload from disk on a 401, and codex-acp's login path, using a hand-made placeholder `auth.json` against a stub supervisor and then a forced real refresh.
2. **Codex's own tool sandbox under the planned seccomp filter (phase 9).** Does workspace-write need `CLONE_NEWUSER` or nested seccomp? Docker's default profile also blocks `unshare`, so the answer affects both runtimes.
3. **The Pingora rustls path on linux-arm64 (phase 6).** `HttpProxy::new` with a per-tunnel `Acceptor` and the `SslDigest` extension, h2 upstreams, and Codex's WSS upgrade through it. Check with a terminated `curl --http1.1` and a `wss://` echo.
4. **The SPIFFE verifier and rotation (phase 3).** Run conformance tests against the X.509-SVID rules (chain to bundle, exact-ID authorization, name constraints, no web roots), plus a soak with a 2-minute lifetime and long streams across rotations.
5. **Landlock in the running Apple guest (phase 9).** `landlock_create_ruleset(NULL, 0, LANDLOCK_CREATE_RULESET_VERSION)` must return at least 6 inside a workload, since a kernel `lsm=` argument could drop Landlock.
6. **CA coverage under termination (phase 6).** Does Claude Code's `NODE_EXTRA_CA_CERTS` path cover every client it uses, and which `/api/*` calls does it make at startup?
7. **Freeze completeness (phase 3).** The SIGSTOP rescan races `fork`. If cgroup v2 is writable in the guest, use `cgroup.freeze`. Check with a fork bomb under a supervisor kill.
8. **Host address drift.** An interface address added while an actor runs is not in `deny6`. The private ranges cover the LAN.
