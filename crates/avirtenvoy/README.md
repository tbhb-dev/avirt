---
type: README
title: avirtenvoy
description: "The Envoy dynamic module that checks CONNECT destinations and reports connection events to avirtmediatord."
tags: [avirt, crate, avirtenvoy]
implementation: partial
generated: { by: claude-code/claude-opus-5-5, at: 2026-10-18T01:00:00Z }
---

# avirtenvoy

## Status: implemented

The crate provides the Envoy HTTP dynamic module for avirtmediatord's optional Envoy engine. Shared destination policy lives in `avirt-connect`. Build with `cargo build --locked --manifest-path crates/avirtenvoy/Cargo.toml --target-dir target/envoy-module`. On a Linux host the output is `target/envoy-module/debug/libavirtenvoy.so`. The module ships only as a glibc Linux cdylib (see [Envoy delivery](../../docs/build-and-platforms.md#envoy-delivery)). The module is excluded from the root workspace.

The module uses `envoy-proxy-dynamic-modules-rust-sdk` from [Envoy v1.40.0](https://github.com/envoyproxy/envoy/tree/v1.40.0/source/extensions/dynamic_modules/sdk/rust), pinned to commit `9e5580933739775e8bce696a57a2e1bc99368f23`. The SDK is a git dependency. Building it requires libclang; Xcode supplies it on macOS. The module’s own `Cargo.lock` pins prettyplease 0.2.37, compatible with bindgen 0.73.2's syn 2 dependency.

Each CONNECT resolves through the system resolver with a ten-second deadline, checks every returned address against non-global ranges and current host interfaces, and passes the first checked address as a numeric authority to Envoy's dynamic forward proxy. Envoy therefore connects to a checked address without a second hostname resolution. The original authority remains in connection events. The module writes start, byte-count and completion snapshots to its stdout pipe; avirtmediatord converts them into the existing `EGRESS` connection records.

`mise run check-envoy` runs formatting, clippy and the SDK-independent stream-state tests for this excluded module.

See [egress](../../docs/egress.md#experimental-host-envoy-mode) for configuration and validation requirements.

## Status: planned

Runtime parity remains unverified. Address fallback, parser-generated error events, exact status parity and byte accounting under every shutdown path require the phase 4 gates. Cedar evaluation, TLS interception, credential injection, xDS, SPIFFE, use in avirtedgeproxyd's Envoy and in avirtmedproxyd's Envoy, and avirtmedproxyd's supervision of the module's Envoy are planned; see [components](../../docs/components.md).
