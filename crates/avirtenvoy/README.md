---
type: README
title: avirtenvoy
description: "The Envoy module built as a shared library that checks CONNECT destinations and reports connection events to avirtmediatord."
tags: [avirt, crate, avirtenvoy]
implementation: partial
generated: { by: codex/gpt-6, at: 2026-10-18T02:56:31Z }
---

# avirtenvoy

## Status: implemented

The crate provides the Envoy HTTP <!-- vale ai-tells.OverusedVocabulary = NO -->dynamic<!-- vale ai-tells.OverusedVocabulary = YES --> module for avirtmediatord's optional Envoy engine. Shared destination policy is in `avirt-connect`. Build with `cargo build --locked --manifest-path crates/avirtenvoy/Cargo.toml --target-dir target/envoy-module`. On a Linux host the output is `target/envoy-module/debug/libavirtenvoy.so`. Distribution is only as a glibc Linux cdylib (see [Envoy delivery](../../docs/build-and-platforms.md#envoy-delivery)). The root workspace excludes this module.

`avirtenvoy` uses `envoy-proxy-dynamic-modules-rust-sdk` from [Envoy v1.40.0](https://github.com/envoyproxy/envoy/tree/v1.40.0/source/extensions/dynamic_modules/sdk/rust), pinned to commit `9e5580933739775e8bce696a57a2e1bc99368f23`. The SDK is a git dependency. Building it requires libclang. Xcode supplies it on macOS. The module’s own `Cargo.lock` pins prettyplease 0.2.37, compatible with bindgen 0.73.2's syn 2 dependency.

For each CONNECT, the module resolves through the system resolver with a ten-second deadline. It checks every returned address against non-global ranges and current host interfaces. Envoy's <!-- vale ai-tells.OverusedVocabulary = NO -->dynamic<!-- vale ai-tells.OverusedVocabulary = YES --> forward proxy receives the first checked address as a numeric authority. Envoy connects to a checked address without a second hostname resolution. The original authority remains in connection events. The module writes start, byte-count, and completion snapshots to its stdout pipe. avirtmediatord converts them into the existing `EGRESS` connection records.

`mise run check-envoy` runs formatting, clippy, and the SDK-independent stream-state tests for this excluded module.

See [egress](../../docs/egress.md#experimental-host-envoy-mode) for configuration and validation requirements.

## Status: planned

Tests have yet to verify runtime parity. Address fallback, parser-generated error events, exact status parity and byte accounting under every shutdown path require the phase 4 gates. Plans include Cedar evaluation, TLS interception, credential injection, xDS, and SPIFFE. They also include use in avirtedgeproxyd's Envoy and in avirtmedproxyd's Envoy, and avirtmedproxyd's supervision of the module's Envoy. See [components](../../docs/components.md).
