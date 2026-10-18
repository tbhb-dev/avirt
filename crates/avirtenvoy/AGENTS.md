---
type: Agent Instructions
title: "Agent instructions: avirtenvoy"
description: "Guidance for AI coding agents working in the `avirtenvoy` crate."
tags: [avirt, crate, avirtenvoy]
generated: { by: codex/gpt-6, at: 2026-10-18T02:56:31Z }
---

# Agent instructions: avirtenvoy

The crate builds the Envoy v1.40.0 HTTP filter, which checks resolved addresses using `avirt-connect` and emits connection snapshots to avirtmediatord. Build with `cargo build --locked --manifest-path crates/avirtenvoy/Cargo.toml --target-dir target/envoy-module`. See the crate README for build constraints and runtime validation.

## Status: planned

Plans include Cedar policy and credential injection, plus use in avirtedgeproxyd's Envoy and in avirtmedproxyd's Envoy.
