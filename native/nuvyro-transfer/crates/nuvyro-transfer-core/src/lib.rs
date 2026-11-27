//! Portable foundation for Nuvyro's session and transfer engine.
//!
//! Bounded wire decoding and stream integrity primitives. Transport,
//! authentication, consent, and storage are separate responsibilities.

#![forbid(unsafe_code)]

pub mod integrity;
pub mod pairing;
pub mod protocol;

/// The engine package version, independent of wire and JNI ABI versions.
pub const ENGINE_VERSION: &str = env!("CARGO_PKG_VERSION");
