//! Shared code for avirt: the NDJSON log envelope, the h2c connection from a workload to
//! avirthostd, avirthostd itself and the workload-side avirtinitd harness.

pub mod api;
pub mod connection;
pub mod egress;
pub mod envelope;
pub mod h2c;
pub mod harness;
pub mod hostd;
pub mod state;
