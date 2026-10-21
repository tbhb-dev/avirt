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
| `Sources/AvirtVMD/Control/` | `ControlServer` (the handshake and the requests), the length framing, `Incarnation`, the stdin watcher, the peer user check, and the peer signature check |
| `Sources/AvirtVMD/Protocol/` | `avirtvmd.proto`, the wire contract, and `Messages.swift` with aliases for the generated types. `SwiftProtobufPlugin` generates the Swift at build time and needs `protoc` on `PATH` |
| `Sources/AvirtVMD/Host/` | `WorkloadHost`, the workload's lifecycle (idle, starting, running, exited, stopping), `LaunchSpec`, and `launchContainer`, which boots the VM through Containerization |
| `Sources/AvirtVMD/Stdio/` | `StdioLog`, the never-block ring and the NDJSON writer for the workload's stdout, stderr, and console |
| `Sources/AvirtVMD/Prepare/` | The one-shot `prepare` steps: `fetch`, `extract`, `initfs`, and `rootfs`. Each writes only to the paths on its command line (`--out`, and `--store` for `initfs`) and prints one JSON object |
| `Sources/AvirtVMD/Sys.swift` | The C library calls that need an `unsafe` expression. No other file may use one |
| `Tests/AvirtVMDTests/` | The tests, and `fixtures/` with the golden protocol encodings |

## Conditions

- The helper reads one stdin line, 32 lowercase hexadecimal characters, within 5 seconds. After that, end-of-file on stdin means avirthostd has exited, and the helper stops and exits 0. Nothing else about a workload arrives on argv or in the environment.
- Exit 64 for bad flags or a bad incarnation line. Exit 69 when a precondition outside avirtvmd fails: another process holds the lock, or the socket can't bind. Exit 70 when a `prepare` step fails, with one line on stderr.
- Take an exclusive `flock` on `<dir>/vmd.lock` before binding, and hold it until the process ends. A free lock tells avirthostd that the helper and its VM have exited.
- Bind `<dir>/v.sock` with mode 0600 and refuse a connection from any user but this process's, judged by `LOCAL_PEERCRED`. Remove the socket on a clean exit.
- Choose the peer policy once, before binding, from the helper's own code signature. A helper signed with a Developer ID certificate admits only a peer whose code, found by `LOCAL_PEERTOKEN`, has the helper's team identifier and the signing identifier `dev.avirt.hostd`, and it refuses any other peer with `Refusal` and `REFUSAL_CODE_WRONG_PEER` before the hello. A helper with any other signature checks the user ID alone and logs that to stderr. Exit 70 when the helper can't read its own signing information or its signature meets Apple's Developer ID rule for apps but has no team identifier.
- Every frame is a 4-byte big-endian length and one Protocol Buffers message of at most 1 MiB. `FrameDecoder` rejects a longer length before it buffers any of the body, which the `NIOExtras` decoder doesn't, so the package doesn't use `NIOExtras`.
- The helper sends `HelperHello` first on every connection, and the peer replies with `HostHello` or `Refusal` within 5 seconds. A version outside the helper's range gets `Refusal` with `REFUSAL_CODE_UNSUPPORTED_VERSION`. A connection that completes the handshake replaces the previous one.
- `Status` and `Stop` in the idle state always get an answer. `StartWorkload` with the digest of a start under way or running answers `Started` once the workload runs, and another digest fails.
- Starting a workload creates `container_dir` with mode 0700 and clones the read-only root filesystem into it with APFS `clonefile`. When the clone fails, the start fails, with no fallback to a full copy.
- The VM has no network. The container gets only the sockets and shares in the request, and `useInit` is on.
- The stdio log is `O_EXCL` with mode 0600, and writing to it can't block: a chunk that doesn't fit the 4 MiB ring counts toward `dropped_bytes`. Each record has `ts`, `workload`, `ns`, `short_id`, `stream`, `partial`, and `text` or `b64`. A line over 64 KiB comes out in pieces, and the file stops growing at 256 MiB.
- When the VM fails to start after its stdio log opens, the helper removes `container_dir` and renames the stdio log to `<path>.failed`, which keeps the boot log and frees the path for the next start.
- Stop the workload (SIGTERM, then a forced stop) before the helper exits, on `Stop` and on end-of-file on stdin.
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
