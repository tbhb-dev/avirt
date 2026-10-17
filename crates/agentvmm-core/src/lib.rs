//! Shared code for agentvmm-poc: the NDJSON log envelope, the h2c uplink from a workload to the
//! controller, the controller itself and the workload-side runner harness.

pub mod api;
pub mod controller;
pub mod egress;
pub mod envelope;
pub mod h2c;
pub mod harness;
pub mod state;
pub mod uplink;
