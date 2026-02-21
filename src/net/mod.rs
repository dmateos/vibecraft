//! Networking module root for protocol and shared transport-facing types.
//! Keeps reusable net definitions outside game-only code paths so binaries
//! can share message contracts without pulling full client runtime state.
pub mod protocol;
