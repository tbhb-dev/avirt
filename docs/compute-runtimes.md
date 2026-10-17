---
type: Component Design
title: Compute runtimes
description: The runtime interface avirthostd uses for its per-actor primitives, the Containerization runtime that hosts Apple VMs inside avirthostd, the Docker Desktop runtime, and the Apple container CLI runtime the current code uses.
tags: [avirt, runtime, containerization, docker, apple-container]
implementation: partial
generated: { by: claude-code/claude-opus-5-5, at: 2026-10-17T19:00:00Z }
---

# Apple container CLI runtime

Status: implemented. It is removed once the Containerization runtime matches current behavior.

`avirthostd` drives the Apple `container` CLI directly. Constraints:

- It requires Apple `container` 1.5.0 or later on an Apple silicon Mac.
- `container` cannot be installed through mise.
- With a socket file that still exists but whose listener is gone, `connect()` succeeds and then hangs.
- A directory mount that contains sockets goes over virtiofs and cannot connect a socket in either direction.
- The runtime's guest is the kata-containers 3.32.0 arm64 kernel (`vmlinux-6.18.35-197-debug`) with Containerization 0.47.0's vminit.
- The runtime applies no seccomp, so avirtinitd applies its own filters (see [workload hardening](./workload-hardening.md)).

# Runtime interface

Status: planned.

`avirthostd` drives one `Runtime` per process, selected by `AVIRT_RUNTIME` (`containerization`, the default on macOS, or `docker`). Backends implement the trait in `avirt-core` and are compiled in by target and cargo feature (see [build and platforms](./build-and-platforms.md)).

```rust
#[async_trait]
pub trait Runtime: Send + Sync {
    fn props(&self) -> &RuntimeProps;
    /// Run the workload; return once the mediator can be wired to its link endpoint.
    async fn run_workload(&self, spec: &WorkloadSpec) -> Result<Running>;
    /// Run the mediator, connected to the workload's link endpoint and to its
    /// avirthostd, service and ingress channels.
    async fn run_mediator(&self, spec: &MediatorSpec, workload: &Running) -> Result<Running>;
    /// SIGTERM, `grace`, SIGKILL, delete. Idempotent.
    async fn remove(&self, c: &Running, grace: Duration) -> Result<()>;
    /// True only when the runtime confirms the workload or mediator no longer exists.
    async fn gone(&self, c: &Running) -> Result<bool>;
    /// Delete any link volume, channel socket or per-actor directory.
    async fn release(&self, actor: &ActorId) -> Result<()>;
    /// Remove every VM, container, network, volume and directory avirthostd could have left behind.
    async fn sweep(&self) -> Result<SweepReport>;
}

pub struct RuntimeProps {
    pub isolation: Isolation,          // VmPerWorkload | SharedKernel
    pub host_socket_paths: usize,      // per-actor host socket paths subject to the 104-byte limit
}
```

- **`Running`** holds the runtime handle plus the workload's or mediator's stdout, stderr and exit status.
- **Specs** are runtime-neutral: the image, the environment, the workspace mount, the labels (`avirt=1`, `avirt.actor`, `avirt.role`), and the extra capabilities the role needs. Neither spec has a network: the workload and the mediator have no interface other than loopback on every runtime.
- **Channels.** The runtime provides the link, the avirthostd channel, the service channel and the ingress channel, each presented inside the container as a Unix socket under `/run/avirt/` (see [actor topology](./actor-topology.md#runtime-channels)).
- **Hardening.** `avirtinitd` applies all agent hardening on every runtime, and the runtime adds only its native outer layer.
- **Freeze and resume** are performed by avirtinitd, the same way on every runtime.
- **Isolation.** `isolation` is written into the `start` meta event.

# Containerization runtime

Status: planned. This is the Apple runtime.

`avirthostd` hosts the VMs itself, through Apple's `containerization` package. The package is wrapped in a Swift shim exporting a `@_cdecl` C ABI and linked into avirthostd as a dylib.

## Host requirements

- The avirthostd binary carries the `com.apple.security.virtualization` entitlement and is ad-hoc signed for development. `cargo run` and `cargo test` go through a signing runner (see [build and platforms](./build-and-platforms.md)).
- The Swift shim is a SwiftPM `.dynamic` product, loaded through an `@executable_path` rpath.
- Containerization 0.47.0 is pinned. The kernel, the vminit image and the package version move together.

## Guest artifacts

- **Kernel.** The kata-containers 3.32.0 arm64 `vmlinux`, pinned by URL and by the release tarball's sha256 `8736c054d9223974735394f822000823baef509e1c33405ec798240fa9b6e4b5`. It is extracted once and cached in avirthostd's state root.
- **Init filesystem.** It comes from `ghcr.io/apple/containerization/vminit:0.47.0` through `initfsReference`, stored in avirthostd's own state root.
- **Images.** The workload and mediator images are unpacked into avirthostd's own image store, also in the state root.

## Networking

- **Workload VM:** no network.
- **Mediator VM:** no network. Its connections are the four channels below.

## Channels

Each channel is a vsock connection between avirthostd and a guest, which Containerization's in-guest socket relay presents as a Unix socket under `/run/avirt/`.

| Channel | vsock direction | Host side |
| --- | --- | --- |
| Link | The workload VM dials avirthostd's link listener on its own vsock device, and avirthostd dials into the current mediator VM | avirthostd splices the two connections at L4 |
| avirthostd channel | The mediator VM dials avirthostd's listener on its own vsock device | avirthostd |
| Service channel | The mediator VM dials avirthostd's listener on its own vsock device | The service relay |
| Ingress channel | avirthostd dials into the mediator VM | The service relay |

TLS runs end to end between the guests and the destinations, so avirthostd's splice and relay carry only ciphertext. TLS roles are fixed regardless of who dials (see [actor topology](./actor-topology.md)).

Constraints:

- The workload VM's vsock device has exactly one host listener, the link's. Every other host vsock listener is on a mediator VM's device.
- Every host vsock listener is reachable by any process in its mediator VM. The avirthostd channel listener runs the mutual TLS handshake before reading any application byte, and the service relay reads only the TLS ClientHello before splicing to a destination that does.
- Containerization's default launch applies no OCI seccomp, so avirtinitd applies the workload's filters itself, including the `AF_VSOCK` denial for every process it launches.
- There are no host socket files, so `host_socket_paths` is 0.

## Workload stdio

The workload runs with `terminal = false`, and avirthostd supplies separate non-blocking writers for stdout and stderr, which feed `logs/<actor-id>.stdio.ndjson`. The VM console comes from `bootLog`. avirtlogd tails both (see [components](./components.md#workload-logs)).

## State

Each VM's runtime directory is `containers/<id>/` in avirthostd's state root. On startup avirthostd's sweep removes every stale `containers/<id>/` directory and stops any VM it still owns (see [lifecycle](./lifecycle.md)).

```mermaid
flowchart LR
    subgraph hostd["avirthostd (one process)"]
        core["avirt-core server"]
        shim["Swift shim dylib (Containerization 0.47.0)"]
        splice["vsock splice and service relay"]
        core --- shim
        core --- splice
    end
    subgraph wvm["workload VM (no network)"]
        net["avirtnetd: link endpoint, TLS client"]
    end
    subgraph svm["mediator VM (no network)"]
        me["mediator Envoy: link and ingress listeners, TLS server"]
        med["avirtmediatord"]
    end
    hs["host components"]
    fd["front-door Envoy"]
    splice <-->|"link over vsock (ciphertext)"| net
    splice <-->|"link and ingress over vsock (ciphertext)"| me
    core <-->|"avirthostd channel over vsock, mTLS"| med
    core <-->|"avirthostd channel: uplink and events"| me
    me -->|"service channel over vsock"| splice
    splice -->|"by SNI (ciphertext)"| hs
    splice -->|"by SNI (ciphertext)"| fd
    fd -->|"external endpoints, peer hosts"| net2(("internet"))
```

# Docker Desktop runtime

Status: planned.

| Primitive | Docker Desktop 4.95.0 |
| --- | --- |
| Isolation | One shared linuxkit kernel (7.0.14) and one rootful daemon. A kernel escape is root over every actor's workload and mediator, including the MITM CA keys and short-lived credentials in mediator memory. |
| Link | A per-actor named volume `avirt-<id8>-link` at `/run/avirt/link` in both containers. The mediator Envoy listens on `link.sock` there and avirtnetd dials. The hop stays inside the VM. A 0600 socket gives uid 1000 `EACCES`. |
| avirthostd channel | A single-file bind mount of the host `actors/<id>/c.sock` at `/run/avirt/hostd.sock`, connected from the container. It appears as `root:root 0660` and reconnects after a listener restart. Directory mounts and container-side listeners do not cross to the host. The mediator's dial retries, because a transient gRPC `Canceled` can occur on this mount. |
| Service channel | A single-file bind mount of the host `actors/<id>/s.sock` at `/run/avirt/svc.sock`, served by the service relay, with the same properties as the avirthostd channel |
| Ingress channel | Open, because container-side listeners do not cross to the host (see [components](./components.md#open-questions)) |
| Host socket paths | `c.sock` and `s.sock` (104-byte limit) |
| Mediator network | `--network none` (only `lo`) |
| Workload network | `--network none` (only `lo`, no docker.sock, no `/run/host-services`) |
| Workload outer layer | `--cap-drop ALL --cap-add SETUID,SETGID,SETPCAP,KILL,NET_BIND_SERVICE --security-opt no-new-privileges`, with Docker's default seccomp profile left in place. The default profile blocks io_uring, bpf, userfaultfd, perf, mount, setns, unshare, `CLONE_NEWUSER`, keyctl and `AF_VSOCK`/`AF_PACKET`/`AF_ALG`. avirtinitd's filter adds ptrace, memfd_create, process_vm_* and `AF_NETLINK`. |
| Mediator outer layer | `--cap-drop ALL --cap-add SETUID,SETGID --security-opt no-new-privileges`, with Docker's default seccomp profile left in place. avirtinitd in mediator mode needs only `SETUID` and `SETGID`, to start avirtmediatord and Envoy under their own UIDs. |
| AF_VSOCK | No host surface, though the kernel has vsock. `avirtinitd` still denies it. |
| Landlock | ABI v8, unprivileged with `no_new_privs` |
| Logs | Docker's container log files, which avirtlogd reads (see [components](./components.md#workload-logs)) |
| Lifecycle | `docker run`, `rm -f`. avirtinitd is PID 1 in both containers. Containers and volumes are swept with `--filter label=avirt=1`. |

Docker Desktop actors must never use `--pid=host` or mount docker.sock. Each link volume is mounted only by its own actor's two containers.

```mermaid
flowchart LR
    subgraph mac["macOS host"]
        hostd["avirthostd and service relay"]
        fd["front-door Envoy"]
        csock{{"actors/ID/c.sock"}}
        ssock{{"actors/ID/s.sock"}}
        hostd --- csock
        hostd --- ssock
        hostd -->|"by SNI"| fd
    end
    subgraph vm["Docker Desktop VM (one shared kernel)"]
        subgraph wl["workload (--network none)"]
            net["avirtnetd"]
        end
        vol[("volume avirt-ID-link")]
        subgraph med["mediator (--network none)"]
            s["avirtmediatord, mediator Envoy"]
        end
        net ---|"dials link.sock"| vol
        s ---|"listens on link.sock"| vol
    end
    s -.->|"file bind mount"| csock
    s -.->|"file bind mount"| ssock
    fd --> inet(("internet"))
```
