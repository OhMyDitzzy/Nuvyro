//! Portable foundation for Nuvyro's session and transfer engine.
//!
//! Protocol contracts are defined here; networking, pairing, and file transfer
//! are not implemented yet.

#![forbid(unsafe_code)]

pub mod protocol;

/// The engine package version, independent of future wire and JNI ABI versions.
pub const ENGINE_VERSION: &str = env!("CARGO_PKG_VERSION");
