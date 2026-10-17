---
type: README
title: avirtenvoy
description: "The Envoy dynamic module that evaluates Cedar policy and injects HTTP and gRPC credentials."
tags: [avirt, crate, avirtenvoy]
implementation: partial
generated: { by: claude-code/claude-sonnet-5-5, at: 2026-10-17T19:16:42Z }
---

# avirtenvoy

## Status: implemented

The crate provides the CONNECT authority parser and destination classification shared with Pingora, plus an optional Envoy HTTP dynamic module for the host mediator. Build the module with `cargo build --locked -p avirtenvoy --features dynamic-module`. The output is `target/debug/libavirtenvoy.dylib` on macOS and `target/debug/libavirtenvoy.so` on Linux. The default build exposes the shared policy library but has no Envoy entrypoint. Use a separate target directory, such as `--target-dir target/envoy-module`, for a module that must remain available while running ordinary workspace builds or tests.

The module uses `envoy-proxy-dynamic-modules-rust-sdk` from [Envoy v1.40.0](https://github.com/envoyproxy/envoy/tree/v1.40.0/source/extensions/dynamic_modules/sdk/rust), pinned to commit `9e5580933739775e8bce696a57a2e1bc99368f23`. The SDK is a git dependency. Building it requires libclang; Xcode supplies it on macOS. `Cargo.lock` pins prettyplease 0.2.37, compatible with bindgen 0.73.2's syn 2 dependency.

Each CONNECT resolves through the system resolver with a ten-second deadline, checks every returned address against non-global ranges and current host interfaces, and passes the first checked address as a numeric authority to Envoy's dynamic forward proxy. Envoy therefore connects to a checked address without a second hostname resolution. The original authority remains in connection events. The module writes start, byte-count and completion snapshots to its stdout pipe; avirtmediatord converts them into the existing EGRESS connection records.

See [egress](../../docs/egress.md#experimental-host-envoy-mode) for configuration and validation requirements.

## Status: planned

Runtime parity remains unverified. Address fallback, parser-generated error events, exact status parity and byte accounting under every shutdown path require the phase 4 gates. Cedar evaluation, TLS interception, credential injection, xDS, SPIFFE and the front-door Envoy are planned; see [components](../../docs/components.md).
