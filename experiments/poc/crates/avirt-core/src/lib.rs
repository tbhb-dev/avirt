//! Shared code for avirt includes the NDJSON log envelope and the h2c connection from a workload
//! to avirthostd. It also includes avirthostd itself and the workload-side avirtinitd harness.

pub mod api;
pub mod connection;
pub mod console;
pub mod egress;
pub mod envelope;
pub mod h2c;
pub mod harness;
pub mod hostd;
pub mod runtime;
pub mod state;

extern crate alloc;
