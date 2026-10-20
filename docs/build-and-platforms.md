---
type: Build Reference
title: Build and platforms
description: Build targets for the guest and host binaries, how the avirtvmd helper is built and signed, the Rust data-plane stack, signing, the CI matrix, the toolchain, host privileges per runtime and guest artifact pins.
tags: [avirt, build, ci, platforms, signing, swift]
implementation: partial
generated: { by: codex/gpt-6, at: 2026-10-18T05:10:25Z }
---

# Status: implemented

## Workspace layout

The root workspace includes `avirt-connect` for CONNECT authority parsing and destination checks. `mise run check` checks that workspace. `crates/avirtvmd` is a SwiftPM package outside it. On macOS, `mise run build-vmd` builds the package and signs it with the `com.apple.security.virtualization` entitlement. avirthostd starts the result from `AVIRT_VMD_BIN` or from the file next to its own binary. The implemented mediator uses the [Pingora CONNECT app](./egress.md#pingora-connect-app).

Runtime backends conform to the `Runtime` trait from `avirt-core`. At run time `AVIRT_RUNTIME` selects `container` or `containerization`, and Docker isn't a choice. Because the Containerization backend starts the avirtvmd helper for each workload, avirthostd links no Swift.

## Prose checks

Vale's banned-term rules block `mise run lint`. Judge the prose in context: preserve its meaning and grammar, and reserve glossary entries for real terms.

To review candidate terms in added prose by hand, run `mise run lint-jargon`. It exits 0 whatever it finds. A bad `--base` exits 1. It runs outside lint and pre-commit, and `mise run check` runs its tests.

## Guest artifacts

Kernel and initfs pins belong to each runtime. The Containerization runtime pins:

- the Kata Containers 3.32.0 arm64 `vmlinux` by URL and tarball sha256 `8736c054d9223974735394f822000823baef509e1c33405ec798240fa9b6e4b5`, and the extracted `vmlinux` by sha256 `fb2cfb79eb1ae19447a85d75682d7fa5cfec97e24beb2609a492b806e8072c8d`.
- the initfs `ghcr.io/apple/containerization/vminit:0.47.0`.

Both move together with the Containerization package version. Status: planned. Docker Desktop supplies its own kernel.

## Sandbox image archives

The `mise run build-image` task uses `container image save` to write sandbox images as OCI image layout archives for avirthostd. avirthostd imports each archive into `images/` and verifies the sha256 of every blob before storing it. `container build` builds the images. The Containerization runtime runs workloads without the `container` command.

# Status: planned

## Targets

| Binary | Runs on | Targets |
| --- | --- | --- |
| `avirtinitd` | Linux guests (the sandbox container) | `aarch64-unknown-linux-musl`, `x86_64-unknown-linux-musl` |
| `avirtnetd`, `avirtguestd`, `avirtacp`, `avirtagent`, `avirtexec`, `avirtmcp`, `avirtguest` | Linux guests (the sandbox container), from the avirt base image | `aarch64-unknown-linux-musl`, `x86_64-unknown-linux-musl` |
| `avirtmediatord` | Linux guests (the mediator) | `aarch64-unknown-linux-musl`, `x86_64-unknown-linux-musl` |
| `avirthostd`, `avirtedgeproxyd`, `avirtapid` | The host | macOS arm64, Linux x86_64 and arm64, and Windows x86_64 and arm64 |
| `avirt` | The host | macOS arm64, Linux x86_64 and arm64, and Windows x86_64 and arm64 |
| `avirtsecrets-1password` | The host, as avirtsecretsd's provider plugin (see [1Password](./secrets-1password.md#plugin)) | macOS arm64 built with cgo (`CGO_ENABLED=1`), and Linux x86_64 and arm64 for the all-in-one server image built without it |

The guest binaries don't depend on the host OS. Each host build produces `avirthostd` and the `avirt` command-line tool side by side.

## Crates and runtime gating

| Crate | Contents |
| --- | --- |
| `crates/avirt-connect` | CONNECT authority parsing and destination checks |
| `crates/avirt-core` | The shared library, including the `Runtime` trait (see [compute runtimes](./compute-runtimes.md)) and the `api` module shared by `avirthostd` and the command-line tool |
| `crates/avirt` | The `avirt` operator CLI |
| `crates/avirthostd` | The host daemon |
| `crates/avirtmediatord` | The mediator |
| `crates/avirtinitd` | The sandbox container entrypoint |
| `crates/avirtvmd` | The per-workload VM host helper: a Swift package that builds on macOS only |

`workloads/` contains only the image build directories (see [sandbox images](#sandbox-images)).

## Sandbox images

Every sandbox image derives from the avirt base image (see [workload lifecycle](./workload-lifecycle.md#base-images)).

| Image | Directory | `FROM` | Adds |
| --- | --- | --- | --- |
| avirt base, `avirt-workload-base` | `workloads/base/` | `ubuntu:26.04` | `ca-certificates`, the `agent` user, the per-process UIDs and the `avirt-ws` group, `/workspace`, and every workload guest binary in the targets table |
| Claude harness base, `avirt-workload-claude` | `workloads/claude/` | The avirt base | Node, claude-agent-acp and `harness.toml`, under `/opt/agent` |
| Codex harness base, `avirt-workload-codex` | `workloads/codex/` | The avirt base | Node, codex-acp and `harness.toml`, under `/opt/agent` |
| avirt builder | `workloads/builder/` | The avirt base | `git`, avirt's layer tool and BuildKit |

`mise run build-images` builds the avirt base first and then every image derived from it. The image build fails a derived image whose layer chain doesn't begin with the avirt base's `diff_ids`, and a harness base with any entry or whiteout outside `/opt/agent`. avirthostd supplies harness configuration at launch as the configuration layer. Sandbox images exclude it.

The implemented images don't have this split: each Containerfile builds from `ubuntu:26.04` and compiles avirtinitd itself (see [Claude](./acp-claude.md#adapter) and [Codex](./acp-codex.md#adapter)).

## Rust data plane

Status: planned. avirtmediatord and avirtedgeproxyd use hyper 1.x for HTTP, h2 0.4.20 for HTTP/2, rustls 0.23 with the ring provider for TLS, tokio-rustls for asynchronous TLS and rcgen for certificate generation. Cedar evaluation runs in-process with `cedar-policy`. SPIFFE verification uses `SpiffeId`, `X509Svid` and `X509Bundle` from `spiffe`, with avirt's verifier, which checks the exact peer ID. It doesn't use a Workload API source.

The mediator binary targets Linux. avirtedgeproxyd is a host binary, including on macOS, and is a host service in the all-in-one image. HTTP parsing, upgrades, flow control, and TLS use the libraries. avirt provides policy, credentials, routing, limits, and lifecycle. The transport checks in [workload topology](./workload-topology.md#admission-and-flow-control) and [half-close behavior](./workload-topology.md#timeouts-and-keepalive) verify the system's properties.

## Containerization source

Apple doesn't publish a prebuilt Containerization artifact. SwiftPM builds the library from source at the pinned tag (0.47.0) when it builds avirtvmd.

## Signing

Status: implemented.

- **Development.** The avirtvmd build signs the helper with the `com.apple.security.virtualization` entitlement. avirthostd and its tests don't need it.
- **Control peer.** A Developer ID-signed avirtvmd requires control-peer code to meet Apple's Developer ID rule for apps. The peer's leaf-certificate team organizational unit must equal avirtvmd's team identifier, and its signing identifier must equal `dev.avirt.hostd`. avirtvmd doesn't check or set its own `dev.avirt.vmd` signing identifier.
- **Verification.** Tests cover the code-signing rule text, policy selection, and real audit-token checks against the test process's code. Positive acceptance of a Developer ID-signed avirthostd remains unverified because no Developer ID build has run.

Status: planned.

- **Distribution.** A Developer ID build pipeline signs `avirthostd` as `dev.avirt.hostd` and signs `avirtvmd` with the `com.apple.security.virtualization` entitlement, both with a Developer ID Application certificate, and notarizes the binaries.
- **Manual acceptance.** The owner signs `target/debug/avirthostd` with `--identifier dev.avirt.hostd` and `target/debug/avirtvmd` with the virtualization entitlement, using their Developer ID Application certificate for both, and runs one end-to-end launch. The launch must succeed.
- **Manual rejection.** The owner signs avirthostd with another identifier and launches again. avirtvmd must refuse the connection, and the launch must fail with the refusal.
- **Secrets broker.** The pipeline signs `avirtsecretsd` and `avirtsecrets-1password` with Developer ID and notarizes them. Each has a stable signing identifier that keeps its designated requirements unchanged across releases (see [macOS Keychain](./secrets-keychain.md#code-signing-and-entitlements)).

## CI matrix

| Runner | Builds |
| --- | --- |
| macOS | avirtvmd, then its signing, then avirthostd, plus avirtedgeproxyd, avirtapid and the macOS `avirt` command-line tool |
| Linux | The guest binaries for both architectures, the Linux avirthostd and command-line tool |
| Windows, or Linux with `cargo-xwin` | The Windows avirthostd and command-line tool |

Constraint: only a macOS host can build the macOS avirthostd binary.

## Toolchain

mise pins Rust and defines the tasks on every OS: `serve` runs `avirthostd`, and `launch <agent> <prompt>` runs `avirt launch`. Swift comes from Xcode (`xcrun swift`), and the macOS tasks check that it's present.

## Host privileges per runtime

| Host | Runtime | Constraint |
| --- | --- | --- |
| macOS | Containerization | The avirtvmd binary contains `com.apple.security.virtualization` |
| macOS | Docker Desktop | Docker Desktop running for the user |
| Linux | KVM microVM (planned) | Access to `/dev/kvm` |
| Windows | Hyper-V through HCS (planned) | Membership in Hyper-V Administrators |

# Open questions

1. **Restricting off-host connections on Windows.** The host rule that restricts new off-host connections to avirtedgeproxyd on Windows remains unspecified.
