---
type: Security Design
title: Workload hardening
description: How avirtinitd confines the agent inside the workload container, covering its user, capabilities, environment and seccomp filters, the reserved paths, the security contexts of the workload helpers with links to their Landlock rulesets, and what runs as root.
tags: [avirt, workload, seccomp, landlock, capabilities, security]
implementation: partial
generated: { by: claude-code/claude-opus-5-5, at: 2026-10-17T19:00:00Z }
---

# Status: implemented

## Container

The workload container runs with `--network none` and `--init`. With `--init` the runtime's init is PID 1 and avirtinitd is its direct child: init reaps orphaned processes and passes `container stop` to avirtinitd as SIGTERM. `avirtinitd` handles SIGTERM by recording a `terminated by SIGTERM` error, closing its HTTP/2 connection on `mediator.sock` and exiting with code 2. The image (`ubuntu:26.04`) has its own uid 1000 user removed, so that uid 1000 is `agent`. There is no `USER` line, and avirtinitd starts as root.

## Agent process

After avirthostd accepts `POST /workloads/<id>/stream` on the workload's HTTP/2 connection to its mediator, avirtinitd starts the adapter as `agent`:

```text
env -u AVIRT_WORKLOAD_ID -u AVIRT_PROMPT -u AVIRT_CWD -u AVIRT_CONNECTION_TOKEN -u AVIRT_AGENT
  setpriv --reuid=agent --regid=agent --init-groups --inh-caps=-all --bounding-set=-all --no-new-privs -- <adapter>
```

The agent has all five capability sets at zero and `NoNewPrivs: 1`. Its environment has `HOME=/home/agent` and the proxy variables below. It does not carry the workload ID, the prompt or the connection token (`AVIRT_CONNECTION_TOKEN`). Credentials and `CODEX_HOME` pass through.

```text
HTTPS_PROXY  https_proxy  HTTP_PROXY  http_proxy  ALL_PROXY  =  http://127.0.0.1:PORT
NO_PROXY     no_proxy                                         =  localhost,127.0.0.1
NODE_USE_ENV_PROXY=1
npm_config_proxy  npm_config_https_proxy                      =  http://127.0.0.1:PORT
```

Node also lets `HTTP_PROXY` serve HTTPS requests, so a bypass test has to unset all of them.

## Seccomp

The workload container shares the VM's network namespace with the runtime's vsock-to-Unix-socket forwarders, and the runtime applies no seccomp. Any process in it could otherwise dial the host-side forwarder ports directly and bypass the socket files' 0600 mode. After `POST /workloads/<id>/stream` is accepted and avirtinitd's TCP proxy to `proxy.sock` has started, and before the agent starts, avirtinitd applies two filters (`seccompiler`, default allow) to itself with all threads, and the agent inherits them:

- `socket()` with domain `AF_VSOCK`, `AF_NETLINK` or `AF_PACKET` returns `EAFNOSUPPORT`. `AF_UNIX`, `AF_INET` and `AF_INET6` stay, because the agent reaches avirtinitd's TCP proxy to `proxy.sock` over loopback.
- `io_uring_setup`, `io_uring_enter` and `io_uring_register` return `EPERM`, because `IORING_OP_SOCKET` creates sockets without calling `socket()`.

Claude Code, Codex, Node, npm and git work under these filters (Node falls back from io_uring to its thread pool). On x86_64 builds the filter also covers the x32 syscall numbers (the `0x4000_0000` bit).

## What the agent can still do

It can use the proxy, see its own filesystem plus the mounted `/workspace` or `CODEX_HOME`, and spend CPU. It cannot open a socket other than AF_UNIX, AF_INET or AF_INET6, and it has no route anywhere except loopback. It cannot open `proxy.sock` or `mediator.sock` (they are `root:root 0600`), read `/proc/1/fd`, `/proc/1/mem` or `/proc/1/environ`, or kill PID 1. Permission requests are auto-approved (see [permission requests](./acp.md#permission-requests)), so the agent runs any tool inside the workload with no human in the loop.

# Status: planned

## Socket-domain allowlist

`socket()` allows only `AF_UNIX`, `AF_INET` and `AF_INET6`, and `socketpair()` only `AF_UNIX`. Every other domain returns `EAFNOSUPPORT`. Constraint: a seccomp filter on the domain argument cannot restrict `AF_INET` to loopback, because it cannot dereference `connect()`'s sockaddr. Loopback-only is enforced by the workload having no network. `AF_NETLINK` stays denied. If `os.networkInterfaces()` fails for a client, `NETLINK_ROUTE` (protocol 0) is the one netlink protocol to allow.

## Denylist

| Syscalls | Effect | Known breakage |
| --- | --- | --- |
| `io_uring_*` | Blocks socket creation outside `socket()` | None (Node falls back) |
| `memfd_create`; `execveat` with `AT_EMPTY_PATH` | Blocks fileless exec, which would bypass [Landlock exec rules](./sandbox.md#guarantees-by-layer) | Rare tools such as Chromium and Playwright |
| `ptrace`, `process_vm_readv`, `process_vm_writev`, `pidfd_getfd`, `pidfd_send_signal` | Blocks cross-process memory and fd access | `strace`, `gdb` |
| `bpf`, `perf_event_open`, `userfaultfd` | Blocks kernel exploit primitives | None |
| `mount`, `fsopen`, `fsconfig`, `fsmount`, `fspick`, `move_mount`, `open_tree`, `umount2`, `pivot_root`, `setns` | Blocks remounting around Landlock | None |
| `unshare` and `clone` with `CLONE_NEWUSER` | Blocks capabilities in a new user namespace | `unshare -Urn`, and possibly Codex's own tool sandbox |
| `clone3` and `pidfd_open`, returning `ENOSYS` | `clone3` flags cannot be filtered, and glibc and libuv fall back | None |
| `kexec_*`, `init_module`, `finit_module`, `delete_module` | Module and kernel loading | None |

`seccomp(SECCOMP_SET_MODE_FILTER)` stays allowed. Stacked filters can only tighten, and Codex installs its own filters for tool commands. If Codex's tool sandbox needs user namespaces, the Codex security context allows `CLONE_NEWUSER`.

## Landlock

Landlock rulesets, the ABI each runtime's kernel provides and the per-process paths are in [sandbox](./sandbox.md).

## Reserved paths

The workload container's image layers are the avirt base, a harness base, the workspace layers and the configuration layer (see [workload lifecycle](./workload-lifecycle.md#layer-stack)). These paths belong to the avirt base, the harness base, the configuration layer or the boot itself. No workspace layer may touch them (see [content rules](./workspaces.md#content-rules)), and every one is read-only to uid 1000 under [the agent's ruleset](./sandbox.md#agent).

| Path | Owned by | Holds |
| --- | --- | --- |
| `/usr/local/bin/avirt*` | The avirt base | The workload guest binaries, owned by uid 0 |
| `/etc/passwd`, `/etc/group`, `/etc/shadow`, `/etc/gshadow` | The avirt base | uid 1000 `agent`, the helper UIDs and the `avirt-ws` group |
| `/etc/ssl/certs`, `/usr/local/share/ca-certificates`, `/usr/share/ca-certificates` | The avirt base | The system CA bundle, to which avirtinitd appends the MITM CA at boot |
| `/etc/resolv.conf`, `/etc/hosts` | The avirt base | Name resolution files, which avirtinitd rewrites at boot |
| `/opt/agent` | The harness base | The harness, its adapter and `harness.toml` |
| `/etc/avirt`, and the managed configuration paths `harness.toml` names | The configuration layer | `launch.toml`, the harness's managed settings and its MCP configuration |
| `/home/agent/.claude`, `/home/agent/.codex`, and every `config.user_paths` directory `harness.toml` names | The harness base and avirtinitd, at boot | The harness's user-level configuration, including the placeholder files avirtinitd writes (see [`harness.toml`](./acp.md#harnesstoml)). A workspace layer that touches them could mark `/workspace` trusted. |
| `/run`, `/proc`, `/sys`, `/dev` | The runtime and avirtinitd, at boot | `/run/avirt`, its Unix sockets and the mounted pseudo-filesystems. No layer holds them. |

The configuration layer writes only `/etc/avirt` and the managed configuration paths. avirtinitd launches every avirt binary and the adapter by absolute path, so nothing earlier on `PATH` replaces them.

## Security contexts

avirtinitd launches every workload process with that process's own security context, applied between fork and exec. No process inherits avirtinitd's security context, and avirtinitd applies no Landlock ruleset to itself, because every child would inherit it.

| Process | UID | Capabilities | Seccomp | Landlock |
| --- | --- | --- | --- | --- |
| avirtinitd | 0 | Until it has bound the DNS stub's sockets: also `CAP_NET_BIND_SERVICE`. At boot, until its `NETLINK_AUDIT` socket is open: also `CAP_AUDIT_READ`. Until `started`: `CAP_SETUID`, `CAP_SETGID`, `CAP_SETPCAP` and `CAP_KILL`. After it: `CAP_SETUID`, `CAP_SETGID` and `CAP_SETPCAP`, to restart avirtguestd. | The denylist, with `AF_VSOCK` denied | None |
| avirtnetd | `avirt-net` | None | The socket-domain allowlist and the denylist | [Its ruleset](./sandbox.md#avirtnetd) |
| Main workload process (avirtacp, avirtagent or avirtexec) | `avirt-run` | None | The socket-domain allowlist and the denylist | [Its ruleset](./sandbox.md#main-workload-process) |
| avirtguestd | `avirt-guest` | None | The socket-domain allowlist and the denylist | [Its ruleset](./sandbox.md#avirtguestd) |
| Agent, its tools, avirtmcp and avirtguest | 1000 | None, with `no_new_privs` | The socket-domain allowlist and the denylist | [The agent's ruleset](./sandbox.md#agent) |

A build job runs under the build security context instead, which permits the namespaces and mounts BuildKit's OCI worker needs and runs the build runner as root inside the build VM (see [workspaces](./workspace-build.md#build-jobs)). It is used only for build jobs, which have no `guest-cli` access and no MITM CA for build commands, and only on a runtime with a VM per workload.

Each process's Landlock ruleset is in [per-process rulesets](./sandbox.md#per-process-rulesets). Every process runs with `no_new_privs`. Every helper UID is distinct from 0 and from 1000, so uid 1000 cannot signal, trace or read the memory of any helper, and no helper can read another's sockets. The agent and every tool it starts run in the agent cgroup, which avirtinitd freezes and kills as a unit (see [workload topology](./workload-topology.md#freeze-and-reconnect)). The helpers run outside it.

The agent (uid 1000) and avirtguestd share the workspace group `avirt-ws`, and nothing else. avirtinitd makes each declared workspace root setgid to that group with default POSIX ACLs that grant the group read and write, so files the agent creates stay writable by avirtguestd. As their owner, the agent can still remove the group's access to its own files, and avirtguestd reports such a file as unreadable rather than overriding it: avirtguestd holds no `CAP_DAC_OVERRIDE` or `CAP_FOWNER`.

avirtinitd mounts `/proc` with `hidepid=invisible` before it starts any child, so uid 1000 cannot see the helpers' processes, command lines or file descriptors.

## What runs as root

Only avirtinitd. It needs root to:

- start the other workload processes under their own UIDs,
- create the agent cgroup, and freeze and kill it,
- write the system CA bundle and `/etc/resolv.conf`,
- bind the DNS stub resolver's port 53 sockets on `127.0.0.53`, which it hands to avirtnetd.

It holds no SVID and no key: the bootstrap SVID passes through it to avirtnetd on an inherited file descriptor. Its own seccomp filter, including the `AF_VSOCK` denial, applies to it before any child is forked. Every process it launches has `AF_VSOCK` denied, which keeps every workload process off any vsock listener, whether transient or gated by mutual TLS.

## Runtime outer layer

The runtime adds its native restrictions underneath avirtinitd's. On Docker Desktop that is:

- `--cap-drop ALL`, then `--cap-add SETUID,SETGID,SETPCAP,KILL,NET_BIND_SERVICE`. `SETPCAP` is needed for `setpriv --bounding-set=-all`, and `NET_BIND_SERVICE` for the DNS stub's port 53.
- `--security-opt no-new-privileges`.
- Docker's default seccomp profile, left in place. A custom profile would replace the default rather than add to it.

See [compute runtimes](./compute-runtimes.md).
