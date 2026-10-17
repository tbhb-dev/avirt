---
type: Security Design
title: Workload hardening
description: How the runner confines the agent inside the workload, covering its user, capabilities, environment, seccomp filters and Landlock rules, plus what runs as root.
tags: [agentvmm, workload, seccomp, landlock, capabilities, security]
implementation: partial
generated: { by: claude-code/claude-opus-5-5, at: 2026-10-17T14:23:31Z }
---

# Status: implemented

## Container

The workload runs with `--network none` and `--init`. With `--init` the runtime's init is PID 1 and the runner is its direct child: init reaps orphaned processes and passes `container stop` to the runner as SIGTERM. The runner handles SIGTERM by recording a `terminated by SIGTERM` error, closing the uplink and exiting with code 2. The image (`ubuntu:26.04`) has its own uid 1000 user removed, so that uid 1000 is `agent`. There is no `USER` line, and the runner starts as root.

## Agent process

After the controller accepts the uplink, the runner starts the adapter as `agent`:

```text
env -u AGENTVMM_ACTOR_ID -u AGENTVMM_PROMPT -u AGENTVMM_CWD -u AGENTVMM_UPLINK_TOKEN -u AGENTVMM_AGENT
  setpriv --reuid=agent --regid=agent --init-groups --inh-caps=-all --bounding-set=-all --no-new-privs -- <adapter>
```

The agent has all five capability sets at zero and `NoNewPrivs: 1`. Its environment has `HOME=/home/agent` and the proxy variables below. It does not carry the actor id, the prompt or the uplink token. Credentials and `CODEX_HOME` pass through.

```text
HTTPS_PROXY  https_proxy  HTTP_PROXY  http_proxy  ALL_PROXY  =  http://127.0.0.1:PORT
NO_PROXY     no_proxy                                         =  localhost,127.0.0.1
NODE_USE_ENV_PROXY=1
npm_config_proxy  npm_config_https_proxy                      =  http://127.0.0.1:PORT
```

Node also lets `HTTP_PROXY` serve HTTPS requests, so a bypass test has to unset all of them.

## Seccomp

The workload shares the VM's network namespace with the runtime's vsock relays, and the runtime applies no seccomp. Any process in it could otherwise dial the host-side relay ports directly and bypass the socket files' 0600 mode. After the uplink is accepted and the proxy relay has started, and before the agent starts, the runner applies two filters (`seccompiler`, default allow) to itself with all threads, and the agent inherits them:

- `socket()` with domain `AF_VSOCK`, `AF_NETLINK` or `AF_PACKET` returns `EAFNOSUPPORT`. `AF_UNIX`, `AF_INET` and `AF_INET6` stay, because the agent reaches the proxy relay over loopback.
- `io_uring_setup`, `io_uring_enter` and `io_uring_register` return `EPERM`, because `IORING_OP_SOCKET` creates sockets without calling `socket()`.

Claude Code, Codex, Node, npm and git work under these filters (Node falls back from io_uring to its thread pool). On x86_64 builds the filter also covers the x32 syscall numbers (the `0x4000_0000` bit).

## What the agent can still do

It can use the proxy, see its own filesystem plus the mounted `/workspace` or `CODEX_HOME`, and spend CPU. It cannot open a socket other than AF_UNIX, AF_INET or AF_INET6, and it has no route anywhere except loopback. It cannot open `proxy.sock` or `uplink.sock` (they are `root:root 0600`), read `/proc/1/fd`, `/proc/1/mem` or `/proc/1/environ`, or kill PID 1. Permission requests are auto-approved (see [agents](./agents.md)), so the agent runs any tool inside the workload with no human in the loop.

# Status: planned

## Socket-domain allowlist

`socket()` allows only `AF_UNIX`, `AF_INET` and `AF_INET6`, and `socketpair()` only `AF_UNIX`. Every other domain returns `EAFNOSUPPORT`. Constraint: a seccomp filter on the domain argument cannot restrict `AF_INET` to loopback, because it cannot dereference `connect()`'s sockaddr. Loopback-only is enforced by the workload having no network. `AF_NETLINK` stays denied. If `os.networkInterfaces()` fails for a client, `NETLINK_ROUTE` (protocol 0) is the one netlink protocol to allow.

## Deny list

| Syscalls | Effect | Known breakage |
| --- | --- | --- |
| `io_uring_*` | Blocks socket creation outside `socket()` | None (Node falls back) |
| `memfd_create`; `execveat` with `AT_EMPTY_PATH` | Blocks fileless exec, which would bypass Landlock exec rules | Rare tools such as Chromium and Playwright |
| `ptrace`, `process_vm_readv`, `process_vm_writev`, `pidfd_getfd`, `pidfd_send_signal` | Blocks cross-process memory and fd access | `strace`, `gdb` |
| `bpf`, `perf_event_open`, `userfaultfd` | Blocks kernel exploit primitives | None |
| `mount`, `fsopen`, `fsconfig`, `fsmount`, `fspick`, `move_mount`, `open_tree`, `umount2`, `pivot_root`, `setns` | Blocks remounting around Landlock | None |
| `unshare` and `clone` with `CLONE_NEWUSER` | Blocks capabilities in a new user namespace | `unshare -Urn`, and possibly Codex's own tool sandbox |
| `clone3` and `pidfd_open`, returning `ENOSYS` | `clone3` flags cannot be filtered, and glibc and libuv fall back | None |
| `kexec_*`, `init_module`, `finit_module`, `delete_module` | Module and kernel loading | None |

`seccomp(SECCOMP_SET_MODE_FILTER)` stays allowed. Stacked filters can only tighten, and Codex installs its own filters for tool commands. If Codex's tool sandbox needs user namespaces, the Codex profile allows `CLONE_NEWUSER`.

## Landlock

The kata-containers 3.32.0 arm64 guest kernel (`vmlinux-6.18.35-197-debug`) that the Apple runtimes boot is built with `CONFIG_SECURITY_LANDLOCK=y` and `CONFIG_LSM` starting with `landlock`, which provides at least ABI v6. Docker Desktop's kernel provides ABI v8. The runner requests the ABI v3 filesystem rights plus the ABI v6 scopes (abstract Unix sockets and signals) as a hard requirement, so the agent does not start without Landlock.

| | Read-only | Read-write |
| --- | --- | --- |
| Both agents | `/usr` `/lib` `/bin` `/sbin` `/etc` `/opt/agent` `/proc` `/sys/fs/cgroup` `/dev/urandom` `/dev/random` `/run/agentvmm/ca.pem` `/run/agentvmm/ca-bundle.pem` | `/workspace` `/home/agent` `/tmp` `/dev/null` `/dev/shm` `/dev/zero` |
| Codex | | `/home/agent/.codex` (under home) |

## What runs as root

Only the runner. It needs root to:

- bind the link endpoint,
- write the system CA bundle,
- `setpriv` the agent,
- signal uid 1000 processes when it freezes them.

After `started`, it drops every capability except `CAP_KILL` and sets `no_new_privs`. Its own seccomp filter, including the `AF_VSOCK` denial, applies to it before the agent is forked. On every runtime the agent's filter denies `AF_VSOCK`, which keeps uid 1000 off any host vsock listener, whether transient or gated by mutual TLS.

## Runtime outer layer

The runtime adds its native restrictions underneath the runner's. On Docker Desktop that is:

- `--cap-drop ALL`, then `--cap-add SETUID,SETGID,SETPCAP,KILL`. `SETPCAP` is needed for `setpriv --bounding-set=-all`.
- `--security-opt no-new-privileges`.
- Docker's default seccomp profile, left in place. A custom profile would replace the default rather than add to it.

See [compute runtimes](./compute-runtimes.md).
