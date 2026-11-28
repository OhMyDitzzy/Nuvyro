//! ABI 1 session bridge with explicit scoped receiver attachment and bounded event delivery.

#[cfg(panic = "abort")]
compile_error!("The JNI lifecycle boundary requires panic=unwind");

mod boundary;
mod command;
pub mod contract;
mod engine;
mod events;
mod exports;
#[cfg(any(target_os = "android", test))]
mod pairing;
mod receiver_command;
mod registry;
#[cfg(any(target_os = "android", test))]
mod transport;

pub use nuvyro_transfer_core::ENGINE_VERSION;
