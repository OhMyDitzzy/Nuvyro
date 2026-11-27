//! Pinned TLS client policy. No sockets, invitation authentication, or file access.

mod certificate;
mod client;
mod error;
mod verifier;

#[cfg(test)]
mod tests;

pub(super) use client::{check_negotiated_profile, new_client};
pub(super) use error::TlsPolicyError;

const SERVER_NAME: &str = "nuvyro.local";
const MAX_CERTIFICATE_BYTES: usize = 4096;
const MAX_CERTIFICATE_VALIDITY_SECONDS: i64 = 86_400 + 300;
