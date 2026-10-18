---
type: Security Design
title: Workload hardening
description: How avirtinitd confines the agent inside the sandbox container, covering its user, capabilities, environment and seccomp filters, the reserved paths, the security contexts of avirtnetd, avirtacp, avirtagent, avirtexec, and avirtguestd with links to their Landlock rulesets, and what runs as root.
tags: [avirt, workload, seccomp, landlock, capabilities, security]
implementation: partial
generated: { by: codex/gpt-6, at: 2026-10-18T04:29:26Z }
---

# Status: implemented

## Container

The sandbox container runs with `--network none` and `--init`. With `--init` the runtime's init is PID 1 and avirtinitd is its direct child: init reaps orphaned processes and passes `container stop` to avirtinitd as SIGTERM. `avirtinitd` handles SIGTERM by recording a `terminated by SIGTERM` error, closing its HTTP/2 connection on `mediator.sock` and exiting with code 2. The image build removes `ubuntu:26.04`'s uid 1000 user and assigns uid 1000 to `agent`. The Containerfile omits `USER`, and avirtinitd starts as root.

## Agent process

After avirthostd accepts `POST /workloads/<id>/stream` on the workload's HTTP/2 connection to its mediator, avirtinitd starts the adapter as `agent`:

```text
env -u AVIRT_WORKLOAD_ID -u AVIRT_PROMPT -u AVIRT_CWD -u AVIRT_CONNECTION_TOKEN -u AVIRT_AGENT
  setpriv --reuid=agent --regid=agent --init-groups --inh-caps=-all --bounding-set=-all --no-new-privs -- <adapter>
```

The agent has all five capability sets at zero and `NoNewPrivs: 1`. Its environment has `HOME=/home/agent` and the proxy variables below. It doesn't contain the workload ID, the prompt or the connection token (`AVIRT_CONNECTION_TOKEN`). Credentials and `CODEX_HOME` pass through.

```text
HTTPS_PROXY  https_proxy  HTTP_PROXY  http_proxy  ALL_PROXY  =  http://127.0.0.1:PORT
NO_PROXY     no_proxy                                         =  localhost,127.0.0.1
NODE_USE_ENV_PROXY=1
npm_config_proxy  npm_config_https_proxy                      =  http://127.0.0.1:PORT
```

Node also lets `HTTP_PROXY` serve HTTPS requests, so a bypass test has to unset every proxy variable.

## Seccomp

The sandbox container shares the VM's network namespace with the runtime's vsock-to-Unix-socket forwarders, and the runtime omits seccomp. Any process in it could otherwise dial the host-side forwarder ports directly and bypass the socket files' 0600 mode. After avirthostd accepts `POST /workloads/<id>/stream` and avirtinitd starts its TCP proxy to `proxy.sock`, but before the agent starts, avirtinitd applies two filters (`seccompiler`, default allow) to itself with all threads, and the agent inherits them:

- `socket()` with domain `AF_VSOCK`, `AF_NETLINK` or `AF_PACKET` returns `EAFNOSUPPORT`. `AF_UNIX`, `AF_INET` and `AF_INET6` stay, because the agent connects to avirtinitd's TCP proxy to `proxy.sock` over loopback.
- `io_uring_setup`, `io_uring_enter` and `io_uring_register` return `EPERM`, because `IORING_OP_SOCKET` creates sockets without calling `socket()`.

Claude Code, Codex, Node, npm, and git work under these filters (Node falls back from io_uring to its thread pool). On x86_64 builds the filter also covers the x32 syscall numbers (the `0x4000_0000` bit).

## Residual agent access

The agent can spend CPU and use the proxy. Its visible files include its own filesystem and the mounted `/workspace` or `CODEX_HOME`. It can't open a socket other than AF_UNIX, AF_INET or AF_INET6, and it has no route anywhere except loopback. It can't open `proxy.sock` or `mediator.sock` (they're `root:root 0600`), read `/proc/1/fd`, `/proc/1/mem` or `/proc/1/environ`, or send a fatal signal to PID 1. avirtinitd automatically approves permission requests (see [permission requests](./acp.md#permission-requests)). The agent can run any tool inside the workload without human approval.

# Status: planned

## Socket-domain allowlist

`socket()` allows only `AF_UNIX`, `AF_INET` and `AF_INET6`, and `socketpair()` only `AF_UNIX`. Every other domain returns `EAFNOSUPPORT`. Constraint: a seccomp filter on the domain argument can't restrict `AF_INET` to loopback, because it can't dereference `connect()`'s sockaddr. The sandbox VM has no network device. Its TUN and nftables REDIRECT send flows through avirtnetd, so ordinary TCP, UDP and unprivileged ICMP echo work under policy. `AF_NETLINK`, `AF_PACKET` and `AF_VSOCK` stay denied. Seccomp denies `IPPROTO_MPTCP`, and avirtinitd sets `net.mptcp.enabled=0`. `ping_group_range` permits the agent's unprivileged `SOCK_DGRAM` echo sockets. If `os.networkInterfaces()` fails for a client, `NETLINK_ROUTE` (protocol 0) is the one netlink protocol to allow.

## Denylist

| Syscalls | Effect | Known breakage |
| --- | --- | --- |
| `io_uring_*` | Blocks socket creation outside `socket()` | None (Node falls back) |
| `memfd_create` and `execveat` with `AT_EMPTY_PATH` | Blocks fileless exec, which would bypass [Landlock exec rules](./landlock.md#guarantees-by-layer) | Rare tools such as Chromium and Playwright |
| `ptrace`, `process_vm_readv`, `process_vm_writev`, `pidfd_getfd`, `pidfd_send_signal` | Blocks cross-process memory and fd access | `strace`, `gdb` |
| `bpf`, `perf_event_open`, `userfaultfd` | Blocks kernel exploit primitives | None |
| `mount`, `fsopen`, `fsconfig`, `fsmount`, `fspick`, `move_mount`, `open_tree`, `umount2`, `pivot_root`, `setns` | Blocks remounting around Landlock | None |
| `unshare` and `clone` with `CLONE_NEWUSER` | Blocks capabilities in a new user namespace | `unshare -Urn`, and possibly Codex's sandbox |
| `clone3` and `pidfd_open`, returning `ENOSYS` | The filter can't inspect `clone3` flags. glibc and libuv fall back | None |
| `kexec_*`, `init_module`, `finit_module`, `delete_module` | Module and kernel loading | None |

`seccomp(SECCOMP_SET_MODE_FILTER)` stays allowed. Stacked filters can only tighten, and Codex installs its own filters for tool commands. If Codex's sandbox needs user namespaces, the Codex security context allows `CLONE_NEWUSER`.

## Landlock

Landlock rulesets, the ABI each runtime's kernel provides and the per-process paths are in [Landlock](./landlock.md).

## Reserved paths

The sandbox container's image layers are the avirt base, a harness base, the workspace layers and the configuration layer (see [workload lifecycle](./workload-lifecycle.md#layer-stack)). The table lists the paths each layer or boot owns. Workspace layers must avoid these paths (see [content rules](./workspaces.md#content-rules)), and every one is read-only to uid 1000 under [the agent's ruleset](./landlock.md#agent).

| Path | Owned by | Holds |
| --- | --- | --- |
| `/usr/local/bin/avirt*` | The avirt base | The workload guest binaries, owned by uid 0 |
| `/etc/passwd`, `/etc/group`, `/etc/shadow`, `/etc/gshadow` | The avirt base | uid 1000 `agent`, the per-process UIDs and the `avirt-ws` group |
| `/etc/ssl/certs`, `/usr/local/share/ca-certificates`, `/usr/share/ca-certificates` | The avirt base | The system CA bundle, to which avirtinitd appends the MITM CA at boot |
| `/etc/resolv.conf`, `/etc/hosts` | The avirt base | Name resolution files, which avirtinitd rewrites at boot |
| `/opt/agent` | The harness base | The harness, its adapter and `harness.toml` |
| `/etc/avirt`, and the managed configuration paths `harness.toml` names | The configuration layer | `launch.toml`, the harness's managed settings and its MCP configuration |
| `/home/agent/.claude`, `/home/agent/.codex`, and every `config.user_paths` directory `harness.toml` names | The harness base and avirtinitd, at boot | The harness's user-level configuration, including the placeholder files avirtinitd writes (see [`harness.toml`](./acp.md#harnesstoml)). A workspace layer that touches them could mark `/workspace` trusted. |
| `/run`, `/proc`, `/sys`, `/dev` | The runtime and avirtinitd, at boot | `/run/avirt`, its Unix sockets and the mounted pseudo-filesystems. Layers exclude them. |

The configuration layer writes only `/etc/avirt` and the managed configuration paths. avirtinitd launches every avirt binary and the adapter by absolute path, so nothing earlier on `PATH` replaces them.

## Security contexts

avirtinitd launches every workload process with that process's own security context, which avirtinitd applies between fork and exec. Processes don't inherit avirtinitd's security context. avirtinitd omits a Landlock ruleset for itself because every child would inherit it.

| Process | UID | Capabilities | Seccomp | Landlock |
| --- | --- | --- | --- | --- |
| avirtinitd | 0 | During TUN and nftables setup: `CAP_NET_ADMIN`, dropped from the bounding set before starting other processes. Until it has bound the DNS stub's sockets: also `CAP_NET_BIND_SERVICE`. At boot, until its `NETLINK_AUDIT` socket is open: also `CAP_AUDIT_READ`. Until `started`: `CAP_SETUID`, `CAP_SETGID`, `CAP_SETPCAP` and `CAP_KILL`. After it: `CAP_SETUID`, `CAP_SETGID` and `CAP_SETPCAP`, to restart avirtguestd. | The denylist, with `AF_VSOCK` denied | None |
| avirtnetd | `avirt-net` | None | The socket-domain allowlist and the denylist | [Its ruleset](./landlock.md#avirtnetd) |
| Main workload process (avirtacp, avirtagent or avirtexec) | `avirt-run` | None | The socket-domain allowlist and the denylist | [Its ruleset](./landlock.md#main-workload-process) |
| avirtguestd | `avirt-guest` | None | The socket-domain allowlist and the denylist | [Its ruleset](./landlock.md#avirtguestd) |
| Agent, its tools, avirtmcp, and avirtguest | 1000 | None, with `no_new_privs` | The socket-domain allowlist and the denylist | [The agent's ruleset](./landlock.md#agent) |

A build job runs under the build security context instead, which permits the namespaces and mounts BuildKit's OCI worker needs and runs the build runner as root inside the build VM (see [workspaces](./workspace-build.md#build-jobs)). Only build jobs use this security context, and only on a runtime with a VM per workload. These jobs lack `guest-cli` access and a MITM CA for build commands.

Each process's Landlock ruleset is in [per-process rulesets](./landlock.md#per-process-rulesets). `no_new_privs` applies to all processes. avirtnetd, avirtacp, avirtagent, avirtexec, and avirtguestd run with per-process UIDs distinct from 0 and 1000. uid 1000 can't send signals to these processes. It also can't trace these processes or read their memory. avirtnetd, avirtacp, avirtagent, avirtexec, and avirtguestd can't read each other's sockets. The agent and every tool it starts run in the agent cgroup, which avirtinitd freezes and kills as a unit (see [workload topology](./workload-topology.md#freeze-and-reconnect)). These processes run outside it.

The agent (uid 1000) and avirtguestd share only the workspace group `avirt-ws`. avirtinitd makes each declared workspace root setgid to that group with default POSIX ACLs that grant the group read and write, allowing avirtguestd to write files the agent creates. As their owner, the agent can still remove the group's access to its own files, and avirtguestd reports such a file as unreadable rather than overriding it: avirtguestd lacks `CAP_DAC_OVERRIDE` and `CAP_FOWNER`.

avirtinitd mounts `/proc` with `hidepid=invisible` before it starts any child, so uid 1000 can't see the processes, command lines, or file descriptors of avirtnetd, avirtacp, avirtagent, avirtexec, and avirtguestd.

## What runs as root

avirtinitd is the only process running as root. It needs root to:

- start the other workload processes under their own UIDs,
- create the agent cgroup, with access to both `cgroup.freeze` and `cgroup.kill`,
- write the system CA bundle and `/etc/resolv.conf`,
- create the TUN, configure IPv4 and IPv6 routes, and install nftables REDIRECT rules,
- bind the transparent TCP, optional forward-proxy and DNS sockets, and hand them and the TUN descriptor to avirtnetd.

It has no stored SVID or key. The bootstrap SVID passes through it to avirtnetd on an inherited file descriptor. Its own seccomp filter, including the `AF_VSOCK` denial, applies before it forks any child. avirtinitd denies `AF_VSOCK` to every process it launches. Workload processes can't connect to any vsock listener. This includes transient listeners and those requiring mutual TLS.

## Runtime outer layer

The runtime adds its native restrictions underneath avirtinitd's. On Docker Desktop that's:

- `--cap-drop ALL`, then `--cap-add SETUID,SETGID,SETPCAP,KILL,NET_BIND_SERVICE,NET_ADMIN`. `setpriv --bounding-set=-all` needs `SETPCAP`, and the DNS stub's port 53 needs `NET_BIND_SERVICE`.
- `/dev/net/tun` for avirtinitd's setup. Docker Desktop support for this configuration still needs a runtime check.
- `--security-opt no-new-privileges`.
- Docker's default seccomp profile, left in place. A custom profile would replace the default rather than add to it.

See [compute runtimes](./compute-runtimes.md).
