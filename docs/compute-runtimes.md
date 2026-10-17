---
type: Component Design
title: Compute runtimes
description: The runtime interface the controller uses for its per-actor primitives, the Containerization runtime that hosts Apple VMs inside the controller, the Docker Desktop runtime, and the Apple container CLI runtime the current code uses.
tags: [agentvmm, runtime, containerization, docker, apple-container]
implementation: partial
generated: { by: claude-code/claude-opus-5-5, at: 2026-10-17T14:23:31Z }
---

# Apple container CLI runtime

Status: implemented. It is removed once the Containerization runtime matches current behavior.

The controller drives the Apple `container` CLI directly. Constraints:

- It requires Apple `container` 1.5.0 or later on an Apple silicon Mac.
- `container` cannot be installed through mise.
- With a socket file that still exists but whose listener is gone, `connect()` succeeds and then hangs.
- A directory mount that contains sockets goes over virtiofs and cannot connect a socket in either direction.
- The runtime's guest is the kata-containers 3.32.0 arm64 kernel (`vmlinux-6.18.35-197-debug`) with Containerization 0.47.0's vminit.
- The runtime applies no seccomp, so the runner applies its own filters (see [workload hardening](./workload-hardening.md)).

# Runtime interface

Status: planned.

The controller drives one `Runtime` per process, selected by `AGENTVMM_RUNTIME` (`containerization`, the default on macOS, or `docker`). Backends implement the trait in `agentvmm-core` and are compiled in by target and cargo feature (see [build and platforms](./build-and-platforms.md)).

```rust
#[async_trait]
pub trait Runtime: Send + Sync {
    fn props(&self) -> &RuntimeProps;
    /// Run the workload; return once the supervisor can be wired to its link endpoint.
    async fn run_workload(&self, spec: &WorkloadSpec) -> Result<Running>;
    /// Create the actor network and run the supervisor, connected to the
    /// workload's link endpoint and to the controller channel.
    async fn run_supervisor(&self, spec: &SupervisorSpec, workload: &Running) -> Result<Running>;
    /// SIGTERM, `grace`, SIGKILL, delete. Idempotent.
    async fn remove(&self, c: &Running, grace: Duration) -> Result<()>;
    /// True only when the runtime confirms the workload or supervisor no longer exists.
    async fn gone(&self, c: &Running) -> Result<bool>;
    /// Delete the actor's network and any link volume or per-actor directory.
    async fn release(&self, actor: &ActorId) -> Result<()>;
    /// Remove every VM, container, network, volume and directory the controller could have left behind.
    async fn sweep(&self) -> Result<SweepReport>;
}

pub struct RuntimeProps {
    pub isolation: Isolation,          // VmPerWorkload | SharedKernel
    pub host_socket_paths: usize,      // per-actor host socket paths subject to the 104-byte limit
    pub dns_allow: Vec<SocketAddr>,    // the supervisor's nft resolver exception
    pub deny_extra: Vec<IpNet>,        // runtime-specific additions to deny4 and deny6
}
```

- **`Running`** holds the runtime handle plus the workload's or supervisor's stdout, stderr and exit status.
- **Specs** are runtime-neutral: the image, the environment, the workspace mount, the labels (`agentvmm=1`, `agentvmm.actor`, `agentvmm.role`), and the extra capabilities the role needs.
- **Hardening.** The runner applies all agent hardening on every runtime, and the runtime adds only its native outer layer.
- **Freeze and resume** are performed by the runner, the same way on every runtime.
- **Isolation.** `isolation` is written into the `start` meta event.

# Containerization runtime

Status: planned. This is the Apple runtime.

The controller hosts the VMs itself, through Apple's `containerization` package. The package is wrapped in a Swift shim exporting a `@_cdecl` C ABI and linked into the controller as a dylib.

## Host requirements

- The controller binary carries the `com.apple.security.virtualization` entitlement and is ad-hoc signed for development. `cargo run` and `cargo test` go through a signing runner (see [build and platforms](./build-and-platforms.md)).
- The Swift shim is a SwiftPM `.dynamic` product, loaded through an `@executable_path` rpath.
- Containerization 0.47.0 is pinned. The kernel, the vminit image and the package version move together.

## Guest artifacts

- **Kernel.** The kata-containers 3.32.0 arm64 `vmlinux`, pinned by URL and by the release tarball's sha256 `8736c054d9223974735394f822000823baef509e1c33405ec798240fa9b6e4b5`. It is extracted once and cached in the controller's state root.
- **Init filesystem.** It comes from `ghcr.io/apple/containerization/vminit:0.47.0` through `initfsReference`, stored in the controller's own state root.
- **Images.** The workload and supervisor images are unpacked into the controller's own image store, also in the state root.

## Networking

- **Workload VM:** no network.
- **Supervisor VM:** a `VmnetNetwork` NAT interface, which can reach the host, the host's LAN, peer VMs and the internet. The supervisor's nftables rules (see [egress](./egress.md)) therefore apply in full: the NAT subnet and gateway fall under the private ranges, and peer VMs with them. DNS goes to public resolvers (`dns_allow = [1.1.1.1:53, 9.9.9.9:53]`) configured through Containerization's DNS configuration.

## Link and controller channel

The link is vsock spliced by the controller. Each guest listens on a vsock port, and the controller dials both and splices the two connections at L4. TLS runs end to end between the runner and the supervisor, so the controller relays only ciphertext. The controller channel is also vsock, dialed by the controller into the supervisor VM. In both cases TLS roles are fixed regardless of who dials (see [actor topology](./actor-topology.md)).

Constraints:

- Every host vsock listener the controller opens is reachable by any guest uid, including 1000. Each one runs the mutual TLS handshake before reading any application byte.
- Containerization's default launch applies no OCI seccomp, so the runner applies the workload's filters itself, including the `AF_VSOCK` denial.
- There are no host socket files, so `host_socket_paths` is 0.

## State

Each VM's runtime directory is `containers/<id>/` in the controller's state root. On startup the controller's sweep removes every stale `containers/<id>/` directory and stops any VM it still owns (see [lifecycle](./lifecycle.md)).

```mermaid
flowchart LR
    subgraph ctl["agentvmm-controller (one process)"]
        core["controller"]
        shim["Swift shim dylib (Containerization 0.47.0)"]
        splice["vsock splice"]
        core --- shim
        core --- splice
    end
    subgraph wvm["workload VM (no network)"]
        runner["runner: vsock listener, TLS server"]
    end
    subgraph svm["supervisor VM (VmnetNetwork NAT, nft)"]
        sup["agentvmm-supervisor: vsock listeners, TLS client"]
    end
    splice <-->|"vsock (ciphertext)"| runner
    splice <-->|"vsock (ciphertext)"| sup
    core <-->|"controller channel over vsock, mTLS"| sup
    sup --> net(("internet"))
```

# Docker Desktop runtime

Status: planned.

| Primitive | Docker Desktop 4.95.0 |
| --- | --- |
| Isolation | One shared linuxkit kernel (7.0.14) and one rootful daemon. A kernel escape is root over every actor's workload and supervisor, including the CA keys and access tokens in supervisor memory. |
| Link | A per-actor named volume `agentvmm-<id8>-link` at `/run/agentvmm/link` in both containers. The runner listens on `link.sock` there and the supervisor dials. The hop stays inside the VM. A root 0600 socket gives uid 1000 `EACCES`. |
| Controller channel | A single-file bind mount of the host `actors/<id>/c.sock` at `/run/agentvmm/controller.sock`, connected from the container. It appears as `root:root 0660` and reconnects after a listener restart. Directory mounts and container-side listeners do not cross to the host. The supervisor's dial retries, because a transient gRPC `Canceled` can occur on this mount. |
| Host socket paths | `c.sock` (104-byte limit) |
| Actor network | `docker network create --label agentvmm=1 agentvmm-<id8>`, a bridge network isolated from other networks. Embedded DNS at 127.0.0.11 forwards to `192.168.65.7`, so `dns_allow = [192.168.65.7:53]`. `host.docker.internal` (192.168.65.254) reaches services bound only to the Mac's 127.0.0.1. The `192.168.0.0/16` drop blocks it, as it does the LAN IP and router. `--internal` networks have no DNS and no internet. |
| Workload network | `--network none` (only `lo`, no docker.sock, no `/run/host-services`) |
| Workload outer layer | `--cap-drop ALL --cap-add SETUID,SETGID,SETPCAP,KILL --security-opt no-new-privileges`, with Docker's default seccomp profile left in place. The default profile blocks io_uring, bpf, userfaultfd, perf, mount, setns, unshare, `CLONE_NEWUSER`, keyctl and `AF_VSOCK`/`AF_PACKET`/`AF_ALG`. The runner's filter adds ptrace, memfd_create, process_vm_* and `AF_NETLINK`. |
| Supervisor outer layer | `--cap-add CAP_NET_ADMIN`, dropped after nft |
| AF_VSOCK | No host surface, though the kernel has vsock. The runner still denies it. |
| Landlock | ABI v8, unprivileged with `no_new_privs` |
| Lifecycle | `docker run`, `--init`, `rm -f`. Containers, networks and volumes are swept with `--filter label=agentvmm=1`. |

Docker Desktop actors must never use `--pid=host` or mount docker.sock. Each link volume is mounted only by its own actor's two containers.

```mermaid
flowchart LR
    subgraph mac["macOS host"]
        ctl["agentvmm-controller"]
        csock{{"actors/ID/c.sock"}}
        ctl --- csock
    end
    subgraph vm["Docker Desktop VM (one shared kernel)"]
        subgraph wl["workload (--network none)"]
            runner["runner"]
        end
        vol[("volume agentvmm-ID-link")]
        subgraph sup["supervisor (bridge agentvmm-ID, nft)"]
            s["agentvmm-supervisor"]
        end
        runner ---|"listens on link.sock"| vol
        s ---|"dials link.sock"| vol
    end
    s -.->|"file bind mount"| csock
    s --> net(("internet"))
```
