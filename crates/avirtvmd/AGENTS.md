---
type: Agent Instructions
title: "Agent instructions: avirtvmd"
description: "Guidance for AI coding agents working in the `avirtvmd` Swift package."
tags: [avirt, crate, avirtvmd]
generated: { by: claude-code/claude-sonnet-5-5, at: 2026-10-18T10:45:00Z }
---

# Agent instructions: avirtvmd

The per-workload VM host helper. It's a SwiftPM package rather than a Cargo crate, and it builds on macOS only. `avirthostd` starts it as a child process and talks to it over `v.sock`. All the Swift in avirt is in this package, so `avirthostd` links no Swift and has no virtualization entitlement.

## Layout

| Path | Holds |
| --- | --- |
| `Package.swift` | Swift 6.2 tools, a macOS 15 floor, and the Swift 6 language mode, warnings as errors and strict memory safety on every target |
| `avirtvmd.entitlements` | `com.apple.security.virtualization` and nothing else |
| `Sources/AvirtVMDCommand/` | The executable target, `Avirtvmd.swift` only: the ArgumentParser commands. Its product is `avirtvmd` |
| `Sources/AvirtVMD/Helper.swift` | `runHelper`, which orders startup: incarnation, lock, socket, then serving until stdin closes or a `Stop` arrives |
| `Sources/AvirtVMD/Control/` | `ControlServer` (the handshake and the requests), the length framing, `Incarnation`, the stdin watcher, and the peer user check |
| `Sources/AvirtVMD/Protocol/` | `avirtvmd.proto`, the wire contract, and `Messages.swift` with aliases for the generated types. `SwiftProtobufPlugin` generates the Swift at build time and needs `protoc` on `PATH` |
| `Sources/AvirtVMD/Prepare/` | The one-shot `prepare` steps: `fetch`, `extract`, `initfs`, and `rootfs`. Each writes only to the paths on its command line (`--out`, and `--store` for `initfs`) and prints one JSON object |
| `Sources/AvirtVMD/Sys.swift` | The C library calls that need an `unsafe` expression. No other file may use one |
| `Tests/AvirtVMDTests/` | The tests, and `fixtures/` with the golden protocol encodings |

## Invariants

- The helper reads one stdin line, 32 lowercase hexadecimal characters, within 5 seconds. After that, end-of-file on stdin means avirthostd has exited, and the helper stops and exits 0. Nothing else about a workload arrives on argv or in the environment.
- Exit 64 for bad flags or a bad incarnation line. Exit 69 when a precondition outside avirtvmd fails: another process holds the lock, or the socket can't bind. Exit 70 when a `prepare` step fails, with one line on stderr.
- Take an exclusive `flock` on `<dir>/vmd.lock` before binding, and hold it until the process ends. A free lock tells avirthostd that the helper and its VM have exited.
- Bind `<dir>/v.sock` with mode 0600 and refuse a connection from any user but this process's, judged by `LOCAL_PEERCRED`. Remove the socket on a clean exit.
- Every frame is a 4-byte big-endian length and one Protocol Buffers message of at most 1 MiB. `FrameDecoder` rejects a longer length before it buffers any of the body, which the `NIOExtras` decoder doesn't, so the package doesn't use `NIOExtras`.
- The helper sends `HelperHello` first on every connection, and the peer replies with `HostHello` or `Refusal` within 5 seconds. A version outside the helper's range gets `Refusal` with `REFUSAL_CODE_UNSUPPORTED_VERSION`. A connection that completes the handshake replaces the previous one.
- `Status` and `Stop` in the idle state always get an answer. `StartWorkload` fails until the VM host exists.
- Never log or print an incarnation, a prompt, a token, or an environment value.
- Don't use `@unchecked Sendable`, `nonisolated(unsafe)`, or an `unsafe` expression outside `Sys.swift`. Don't lower the language mode or the warning policy.

## Swift formatting

`.swift-format` is the output of `swift format dump-configuration` with `lineLength` set to 120 to match `rustfmt` and every rule on. No rule is off.

## Testing

```bash
mise run check-vmd
```

The task builds `target/debug/avirtvmd` and signs it, then checks the entitlement before `swift format lint --strict` and `swift test`. On Linux it prints a note and passes. Tests run the control server in process against a socket in a scratch directory, and run `runHelper` against a pipe in place of stdin.

The golden fixtures in `Tests/AvirtVMDTests/fixtures/` are the contract with avirthostd's Rust side. A `.txtpb` file holds a message in text format, and the matching `.bin` file holds its encoding. Names starting `host-` are `HostMessage`, and names starting `helper-` are `HelperMessage`. After changing `avirtvmd.proto`, regenerate every `.bin` file from the repository root with `protoc`:

```bash
for f in crates/avirtvmd/Tests/AvirtVMDTests/fixtures/*.txtpb; do
  case $(basename "$f") in host-*) type=HostMessage ;; *) type=HelperMessage ;; esac
  protoc --proto_path=crates/avirtvmd/Sources/AvirtVMD/Protocol --encode="avirt.vmd.v1.$type" avirtvmd.proto <"$f" >"${f%.txtpb}.bin"
done
```
