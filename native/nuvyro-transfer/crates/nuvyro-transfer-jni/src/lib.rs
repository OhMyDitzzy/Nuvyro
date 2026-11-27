//! ABI 1 lifecycle bridge. Local sessions do not advertise, pair, or transfer files.

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
mod registry;
#[cfg(any(target_os = "android", test))]
mod transport;

pub use nuvyro_transfer_core::ENGINE_VERSION;
