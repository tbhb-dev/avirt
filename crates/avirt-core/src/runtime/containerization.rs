//! The runtime that hosts each workload's VM in a per-workload `avirtvmd` helper process.

pub mod cache;
pub mod control;
pub mod digest;
pub mod images;
pub mod intent;
pub mod lifecycle;
pub mod proto;
pub mod reconcile;
pub mod request;
#[cfg(test)]
mod testing;
pub mod workload;
