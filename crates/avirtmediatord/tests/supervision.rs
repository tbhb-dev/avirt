//! Supervision checks use a fake avirthostd. They don't launch workloads.

// This test target doesn't use the binary's dependencies.
use async_trait as _;
use avirt_connect as _;
use chrono as _;
use clap as _;
use httparse as _;
use pingora_core as _;
use serde_json as _;
use tokio as _;

#[cfg(test)]
#[path = "supervision/tests.rs"]
mod tests;
