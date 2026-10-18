---
type: Agent Instructions
title: "Agent instructions: avirtenvoy"
description: "Guidance for AI coding agents working in the `avirtenvoy` crate."
tags: [avirt, crate, avirtenvoy]
generated: { by: claude-code/claude-sonnet-5-5, at: 2026-10-17T19:16:42Z }
---

# Agent instructions: avirtenvoy

The crate builds the Envoy v1.40.0 HTTP filter, which checks resolved addresses using `avirt-connect` and emits connection snapshots to avirtmediatord. Build with `cargo build --locked --manifest-path crates/avirtenvoy/Cargo.toml --target-dir target/envoy-module`. See the crate README for build constraints and runtime validation.

## Status: planned

Cedar policy, credential injection and use in avirtedgeproxyd's Envoy and in avirtmedproxyd's Envoy are planned.
