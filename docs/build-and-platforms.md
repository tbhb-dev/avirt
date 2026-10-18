---
type: Build Reference
title: Build and platforms
description: Build targets for the guest and host binaries, how the Swift Containerization shim is gated and linked, Envoy delivery and the libavirtenvoy build, signing, the CI matrix, the toolchain, host privileges per runtime and guest artifact pins.
tags: [avirt, build, ci, platforms, signing, swift]
implementation: partial
generated: { by: codex/gpt-6, at: 2026-10-18T02:37:39Z }
---

# Status: implemented

## Workspace layout

The root workspace includes `avirt-connect`, the CONNECT authority parser and destination checks shared by Pingora and Envoy. `avirtenvoy` is an excluded standalone workspace with its own `Cargo.lock`. Root workspace builds exclude the Envoy SDK, bindgen, and libclang. `mise run check` checks the root workspace. `mise run check-envoy` checks the excluded module with the SDK git source and libclang available. On a Linux host the module builds into `target/envoy-module/debug/libavirtenvoy.so`.

## Experimental host mediator module

The host mediator can select Envoy v1.40.0 while retaining the relayed `proxy.sock` and `mediator.sock`. Build and run instructions are in [egress](./egress.md#experimental-host-envoy-mode). The module pins the official Rust SDK to the same release commit. `cargo build --locked --manifest-path crates/avirtenvoy/Cargo.toml --target-dir target/envoy-module` builds the cdylib with libclang available, and the bootstrap names it by absolute filename.

Envoy's v1.40.0 [POSIX module loader](https://github.com/envoyproxy/envoy/blob/v1.40.0/source/extensions/dynamic_modules/posix/dynamic_modules.cc) uses `dlopen` to load the module into Envoy's own process. Upstream v1.40.0 [release assets](https://github.com/envoyproxy/envoy/releases/tag/v1.40.0) provide Linux arm64 and x86_64 binaries only. avirt neither distributes nor runs a darwin Envoy. In the implemented mode the operator supplies the Envoy binary by path. avirt neither installs nor downloads it. Only a binary whose version and compiled extensions meet the bootstrap validation test is usable.

# Status: planned

## Targets

| Binary | Runs on | Targets |
| --- | --- | --- |
| `avirtinitd` | Linux guests (the workload container) | `aarch64-unknown-linux-musl`, `x86_64-unknown-linux-musl` |
| `avirtnetd`, `avirtguestd`, `avirtacp`, `avirtagent`, `avirtexec`, `avirtmcp`, `avirtguest` | Linux guests (the workload container), from the avirt base image | `aarch64-unknown-linux-musl`, `x86_64-unknown-linux-musl` |
| `avirtmediatord` | Linux guests (the mediator) | `aarch64-unknown-linux-musl`, `x86_64-unknown-linux-musl` |
| `libavirtenvoy` | Envoy, as a <!-- Envoy extension name. --><!-- vale ai-tells.OverusedVocabulary = NO -->dynamic module<!-- vale ai-tells.OverusedVocabulary = YES -->: avirtmedproxyd's Envoy in the mediator and avirtedgeproxyd's Envoy in the edge VM, both Linux (see [Envoy delivery](#envoy-delivery)) | `aarch64-unknown-linux-gnu`, `x86_64-unknown-linux-gnu` |
| `avirthostd` | The host | macOS arm64, Linux x86_64 and arm64, and Windows x86_64 and arm64 |
| `avirt` | The host | macOS arm64, Linux x86_64 and arm64, and Windows x86_64 and arm64 |
| `avirtsecrets-1password` | The host, as avirtsecretsd's provider plugin (see [1Password](./secrets-1password.md#plugin)) | macOS arm64 built with cgo (`CGO_ENABLED=1`), and Linux x86_64 and arm64 for the all-in-one server image built without it |

The guest binaries don't depend on the host OS. Each host build produces `avirthostd` and the `avirt` command-line tool side by side.

## Crates and runtime gating

| Crate | Contents |
| --- | --- |
| `crates/avirt-connect` | CONNECT authority parsing and destination checks shared by both CONNECT engines |
| `crates/avirt-core` | The shared library, including the `Runtime` trait (see [compute runtimes](./compute-runtimes.md)) and the `api` module shared by `avirthostd` and the command-line tool |
| `crates/avirt` | The `avirt` <!-- Preserve the exact term from the glossary. --><!-- vale project.WordList = NO -->operator CLI<!-- vale project.WordList = YES --> |
| `crates/avirthostd` | The host daemon |
| `crates/avirtmediatord` | The mediator |
| `crates/avirtinitd` | The workload container entrypoint |
| `crates/avirtenvoy` | The Envoy <!-- Envoy extension name. --><!-- vale ai-tells.OverusedVocabulary = NO -->dynamic module<!-- vale ai-tells.OverusedVocabulary = YES -->, a `cdylib` built as `libavirtenvoy` and excluded from the root workspace, that checks CONNECT destinations and reports connection events. Cedar policy and credential injection have planned status |
| `crates/avirt-runtime-containerization` | The Containerization runtime: a Swift package and a `build.rs` |

`workloads/` contains only the image build directories (see [workload images](#workload-images)).

Runtime backends conform to the `Runtime` trait from `avirt-core`. During a build, target `cfg` and the cargo features `runtime-containerization` and `runtime-docker` select them. During execution, `AVIRT_RUNTIME` selects among those compiled in.

The build uses two target checks to control inclusion of the Containerization runtime:

- `crates/avirt-runtime-containerization` contains the Swift package, whose `.dynamic` product exports a `@_cdecl` C ABI. Its `build.rs` keys on `CARGO_CFG_TARGET_OS` (the target, not the host) and does nothing on non-macOS targets.

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

## Workload images

Every workload image derives from the avirt base image (see [workload lifecycle](./workload-lifecycle.md#base-images)).

| Image | Directory | `FROM` | Adds |
| --- | --- | --- | --- |
| avirt base, `avirt-workload-base` | `workloads/base/` | `ubuntu:26.04` | `ca-certificates`, the `agent` user, the helper users and the `avirt-ws` group, `/workspace`, and every workload guest binary in the targets table |
| Claude harness base, `avirt-workload-claude` | `workloads/claude/` | The avirt base | Node, claude-agent-acp and `harness.toml`, under `/opt/agent` |
| Codex harness base, `avirt-workload-codex` | `workloads/codex/` | The avirt base | Node, codex-acp and `harness.toml`, under `/opt/agent` |
| avirt builder | `workloads/builder/` | The avirt base | `git`, avirt's layer tool and BuildKit |

`mise run build-images` builds the avirt base first and then every image derived from it. The image build fails a derived image whose layer chain doesn't begin with the avirt base's `diff_ids`, and a harness base with any entry or whiteout outside `/opt/agent`. avirthostd supplies harness configuration at launch as the configuration layer. Workload images exclude it.

The implemented images don't have this split: each Containerfile builds from `ubuntu:26.04` and compiles avirtinitd itself (see [Claude](./acp-claude.md#adapter) and [Codex](./acp-codex.md#adapter)).

## Envoy delivery

Envoy runs only on Linux. avirt neither distributes nor runs a darwin Envoy.

- **Image.** Every Envoy avirt runs comes from the official `envoyproxy/envoy:distroless-v1.40.0` image, pinned by index digest `sha256:23b72ac1fa94908b2ca7ebf194cc6083cd0d4913210c7d7453e4b2008ca26c84`. Its base is `gcr.io/distroless/base-nossl-debian13`, which has glibc.
- **Verification.** Upstream omits cosign signatures, attestations, and SBOMs for the image. CI checks the Envoy binary in the image against the release's PGP-clearsigned `checksums.txt.asc`, verified with the Envoy maintainers key `0AFCE836BA4D1D35763C8523D8CDC3750181F31F` that's committed in the Envoy repository.
- **Pins.** The Envoy version, the image digest and the SDK rev, the v1.40.0 tag commit `9e5580933739775e8bce696a57a2e1bc99368f23`, move together in one change.
- **libavirtenvoy.** Build libavirtenvoy only as a glibc Linux cdylib, `aarch64-unknown-linux-gnu` and, for amd64, `x86_64-unknown-linux-gnu`, and never for musl, because Envoy loads it into its glibc process with `dlopen`. Constraint: its glibc symbol versions must not exceed those of the runtime image's glibc.
- **Upgrade gate.** The pinned Envoy must export every undefined `envoy_dynamic_module_callback_*` symbol in the module, and Envoy's own `--mode validate` must pass inside the image.
- **License.** Include Envoy's Apache-2.0 `LICENSE` and `NOTICE` in every image containing Envoy.

## Linking

avirthostd loads the shim dylib through an `@executable_path` rpath. The distribution places the dylib next to the avirthostd binary. The Swift runtime comes from the OS.

## Containerization source

Apple doesn't publish a prebuilt Containerization artifact. Build the library from source with SwiftPM at the pinned tag (0.47.0). `build.rs` uses a prebuilt shim dylib when `AVIRT_SWIFTSHIM_DIR` points at one, and builds from source otherwise. CI publishes the dylib as a checksummed artifact.

## Signing

- **Development.** macOS avirthostd builds use an ad-hoc signature with the `com.apple.security.virtualization` entitlement. `cargo run` and `cargo test` go through a signing runner configured in `.cargo/config.toml`, so test and run binaries contain the entitlement.
- **Distribution.** Sign `avirthostd` with Developer ID and notarize it.
- **Secrets broker.** Sign `avirtsecretsd` and `avirtsecrets-1password` with Developer ID and notarize them. Each has a stable signing identifier that keeps its designated requirements unchanged across releases (see [macOS Keychain](./secrets-keychain.md#code-signing-and-entitlements)).

## CI matrix

| Runner | Builds |
| --- | --- |
| macOS | The Swift dylib, then avirthostd, then signing, plus the macOS `avirt` command-line tool |
| Linux | The guest binaries for both architectures, libavirtenvoy for both glibc targets with the Envoy upgrade gate, the Envoy image checksum check, and the Linux avirthostd and command-line tool |
| Windows, or Linux with `cargo-xwin` | The Windows avirthostd and command-line tool |

Constraint: only a macOS host can build the macOS avirthostd binary.

## Toolchain

mise pins Rust and defines the tasks on every OS: `serve` runs `avirthostd`, and `launch <agent> <prompt>` runs `avirt launch`. Swift comes from Xcode (`xcrun swift`), and the macOS tasks check that it's present.

## Host privileges per runtime

| Host | Runtime | Constraint |
| --- | --- | --- |
| macOS | Containerization | The avirthostd binary contains `com.apple.security.virtualization` |
| macOS | Docker Desktop | Docker Desktop running for the user |
| Linux | KVM microVM (planned) | Access to `/dev/kvm` |
| Windows | Hyper-V through HCS (planned) | Membership in Hyper-V Administrators |

## Guest artifacts

Kernel and initfs pins belong to each runtime. The Containerization runtime pins:

- the Kata Containers 3.32.0 arm64 `vmlinux` by URL and tarball sha256 `8736c054d9223974735394f822000823baef509e1c33405ec798240fa9b6e4b5`.
- the initfs `ghcr.io/apple/containerization/vminit:0.47.0`.

Both move together with the Containerization package version. Docker Desktop supplies its own kernel.

# Open questions

1. **Envoy on Windows.** How a Windows host runs avirtedgeproxyd's Envoy is open. The edge VM would likely run in WSL2 or Hyper-V.
