//! Shared crate surface for binaries and split-target experiments.
//! Exposes stable modules used by the game client and standalone tools
//! (server/protocol probes) to reduce duplication across binaries.
pub mod config;
pub mod core_sim;
pub mod net;
pub mod world;
