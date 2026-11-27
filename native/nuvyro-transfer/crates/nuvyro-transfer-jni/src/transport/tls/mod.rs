//! Pinned TLS client policy and private handshake driver. No invitation or file access.

mod certificate;
mod client;
mod driver;
mod error;
mod verifier;

#[cfg(test)]
mod tests;

pub(super) use client::{check_negotiated_profile, new_client};
#[cfg(test)]
pub(super) use driver::test_handshake_with_client;
pub(super) use driver::{
    TlsHandshakeError, TlsInterest, TlsIo, VerifiedTlsConnection, handshake_with,
};
pub(super) use error::TlsPolicyError;

const SERVER_NAME: &str = "nuvyro.local";
const MAX_CERTIFICATE_BYTES: usize = 4096;
const MAX_CERTIFICATE_VALIDITY_SECONDS: i64 = 86_400 + 300;
