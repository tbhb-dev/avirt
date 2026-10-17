---
type: Build Reference
title: Build and platforms
description: Build targets for the guest and host binaries, how the Swift Containerization shim is gated and linked, signing, the CI matrix, the toolchain, host privileges per runtime and guest artifact pins.
tags: [avirt, build, ci, platforms, signing, swift]
implementation: partial
generated: { by: claude-code/claude-opus-5-5, at: 2026-10-17T19:00:00Z }
---

# Status: implemented

## Experimental host mediator module

The host mediator can select Envoy v1.40.0 while retaining the relayed proxy and uplink sockets. Build and run instructions are in [egress](./egress.md#experimental-host-envoy-mode). The official Rust SDK is pinned to the same release commit; `cargo build --locked --manifest-path crates/avirtenvoy/Cargo.toml --target-dir target/envoy-module` builds the cdylib with libclang available. On macOS the cdylib links Envoy callback symbols with `-undefined dynamic_lookup` and is passed by absolute `.dylib` filename in the bootstrap.

The v1.40.0 [POSIX module loader](https://github.com/envoyproxy/envoy/blob/v1.40.0/source/extensions/dynamic_modules/posix/dynamic_modules.cc) uses `dlopen`; upstream [module build tests](https://github.com/envoyproxy/envoy/blob/v1.40.0/test/extensions/dynamic_modules/test_data/rust/test_data.bzl) account for macOS dylibs. A native macOS build must include the HTTP dynamic-module extension and export the SDK callback symbols. Native module compilation is supported by this crate; loading against a supplied darwin Envoy binary requires the bootstrap validation test and is unverified here.

Upstream v1.40.0 [release assets](https://github.com/envoyproxy/envoy/releases/tag/v1.40.0) provide Linux arm64 and x86_64 binaries, with no darwin binary. A matching native macOS arm64 Envoy must be built from the pinned source with the [upstream build prerequisites](https://github.com/envoyproxy/envoy/blob/v1.40.0/bazel/README.md#quick-start-bazel-build-for-developers) available: full Xcode, Bazelisk, coreutils, wget, libtool, Go, clang-format, autoconf and aspell. The upstream build command is `bazel build -c opt envoy`. Envoy is not installed or downloaded by avirt. A Homebrew, mise, aqua or func-e binary is usable only if its version and compiled extensions satisfy the bootstrap validation test. Linux Envoy in a container additionally needs a transport bridge for the host Unix streams and host-interface destination checks; that bridge is not implemented.

# Status: planned

## Targets

| Binary | Runs on | Targets |
| --- | --- | --- |
| `avirtinitd` | Linux guests (the workload) | `aarch64-unknown-linux-musl`, `x86_64-unknown-linux-musl` |
| `avirtmediatord` | Linux guests (the mediator) | `aarch64-unknown-linux-musl`, `x86_64-unknown-linux-musl` |
| `libavirtenvoy` | Envoy, as a dynamic module: the mediator Envoy in Linux guests and the front-door Envoy on the host | `aarch64-unknown-linux-gnu`, `x86_64-unknown-linux-gnu`, macOS arm64 |
| `avirthostd` | The host | macOS arm64, Linux x86_64 and arm64, Windows x86_64 and arm64 |
| `avirt` | The host | macOS arm64, Linux x86_64 and arm64, Windows x86_64 and arm64 |

The guest binaries do not depend on the host OS. `avirthostd` and the `avirt` CLI are built per host, side by side.

## Crates and runtime gating

| Crate | Contents |
| --- | --- |
| `crates/avirt-core` | The shared library, including the `Runtime` trait (see [compute runtimes](./compute-runtimes.md)) and the `api` module shared by `avirthostd` and the CLI |
| `crates/avirt` | The `avirt` operator CLI |
| `crates/avirthostd` | The host daemon |
| `crates/avirtmediatord` | The mediator |
| `crates/avirtinitd` | The workload entrypoint |
| `crates/avirtenvoy` | The Envoy dynamic module, a `cdylib` built as `libavirtenvoy`, that evaluates Cedar policy (embedding the `cedar-policy` crate) and injects HTTP and gRPC credentials |
| `crates/avirt-runtime-containerization` | The Containerization runtime: a Swift package and a `build.rs` |

`workloads/claude/` and `workloads/codex/` hold only the image build directories.

Runtime backends implement the `Runtime` trait from `avirt-core`. They are selected at build time by target `cfg` and by the cargo features `runtime-containerization` and `runtime-docker`, and at run time by `AVIRT_RUNTIME` among those compiled in.

The Containerization runtime is gated by target in two places:

- `crates/avirt-runtime-containerization` holds the Swift package, whose `.dynamic` product exports a `@_cdecl` C ABI. Its `build.rs` keys on `CARGO_CFG_TARGET_OS` (the target, not the host) and does nothing on non-macOS targets.
- `avirthostd` depends on that crate only under `[target.'cfg(target_os = "macos")'.dependencies]`.

```mermaid
flowchart LR
    core["avirt-core (Runtime trait)"]
    hostd["avirthostd"]
    rc["avirt-runtime-containerization (macOS targets)"]
    swift["Swift package: .dynamic product, @_cdecl C ABI"]
    cz["Containerization 0.47.0 (SwiftPM, pinned tag)"]
    dk["Docker runtime (feature runtime-docker)"]
    hostd --> core
    hostd -->|"cfg(target_os = macos), feature runtime-containerization"| rc
    hostd --> dk
    rc --> core
    dk --> core
    rc -->|"build.rs"| swift
    swift --> cz
```

## Linking

The shim dylib is loaded through an `@executable_path` rpath, so it ships next to the avirthostd binary. The Swift runtime comes from the OS.

## Containerization source

Apple publishes no prebuilt Containerization artifact, so the library is built from source with SwiftPM at the pinned tag (0.47.0). `build.rs` uses a prebuilt shim dylib when `AVIRT_SWIFTSHIM_DIR` points at one, and builds from source otherwise. CI publishes the dylib as a checksummed artifact.

## Signing

- **Development.** macOS avirthostd builds are ad-hoc signed with the `com.apple.security.virtualization` entitlement. `cargo run` and `cargo test` go through a signing runner configured in `.cargo/config.toml`, so test and run binaries carry the entitlement.
- **Distribution.** `avirthostd` is Developer ID signed and notarized.

## CI matrix

| Runner | Builds |
| --- | --- |
| macOS | The Swift dylib, then avirthostd, then signing, plus the macOS `avirt` CLI |
| Linux | The guest binaries for both architectures, and the Linux avirthostd and CLI |
| Windows, or Linux with `cargo-xwin` | The Windows avirthostd and CLI |

Constraint: the macOS avirthostd binary can only be built on a macOS host.

## Toolchain

mise pins Rust and defines the tasks on every OS: `serve` runs `avirthostd`, and `launch <agent> <prompt>` runs `avirt launch`. Swift comes from Xcode (`xcrun swift`), and the macOS tasks check that it is present.

## Host privileges per runtime

| Host | Runtime | Requirement |
| --- | --- | --- |
| macOS | Containerization | The avirthostd binary carries `com.apple.security.virtualization` |
| macOS | Docker Desktop | Docker Desktop running for the user |
| Linux | KVM microVM (planned) | Access to `/dev/kvm` |
| Windows | Hyper-V through HCS (planned) | Membership in Hyper-V Administrators |

## Guest artifacts

Kernel and initfs pins belong to each runtime. The Containerization runtime pins:

- the kata-containers 3.32.0 arm64 `vmlinux` by URL and tarball sha256 `8736c054d9223974735394f822000823baef509e1c33405ec798240fa9b6e4b5`;
- the initfs `ghcr.io/apple/containerization/vminit:0.47.0`.

Both move together with the Containerization package version. Docker Desktop supplies its own kernel.
