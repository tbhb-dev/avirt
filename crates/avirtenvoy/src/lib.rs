//! CONNECT destination policy and the optional Envoy dynamic-module entrypoint.
pub mod policy;

#[cfg(all(feature = "dynamic-module", not(test)))]
mod filter;
