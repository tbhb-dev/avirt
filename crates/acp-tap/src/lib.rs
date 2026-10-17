//! Shared code for the ACP container experiments: the NDJSON log envelope, the h2c uplink from
//! a container to the host daemon, the daemon itself and the container-side entrypoint harness.

pub mod daemon;
pub mod egress;
pub mod entrypoint;
pub mod envelope;
pub mod h2c;
pub mod state;
pub mod uplink;
