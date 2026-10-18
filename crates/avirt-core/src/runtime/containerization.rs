//! The runtime that hosts each workload's VM in a per-workload `avirtvmd` helper process.

pub mod control;
pub mod intent;
pub mod lifecycle;
pub mod proto;
#[cfg(test)]
mod testing;
