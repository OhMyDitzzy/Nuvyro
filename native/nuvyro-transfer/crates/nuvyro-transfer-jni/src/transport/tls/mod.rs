//! Pinned TLS establishment and invitation control. Sockets and file access stay private.

mod certificate;
mod client;
mod driver;
mod error;
mod incoming_pairing;
mod pairing;
mod server;
mod verifier;

#[cfg(test)]
mod tests;

pub(crate) use client::{check_negotiated_profile, new_client};
#[cfg(test)]
use driver::test_handshake_with_client;
pub(super) use driver::{
    TlsHandshakeError, TlsInterest, TlsIo, VerifiedTlsConnection, handshake_with,
};
pub(super) use error::TlsPolicyError;
pub(in crate::transport) use incoming_pairing::pair_incoming_with;
pub(crate) use incoming_pairing::{IncomingPairError, PairedIncoming};
pub(super) use pairing::pair_outgoing_with;
pub(crate) use pairing::{PairConnectionError, PairedConnection};
pub(crate) use server::{IncomingTlsError, VerifiedIncoming};
pub(in crate::transport) use server::{ReceiverTlsIo, handshake_incoming_with};

const SERVER_NAME: &str = "nuvyro.local";
const MAX_CERTIFICATE_BYTES: usize = 4096;
const MAX_CERTIFICATE_VALIDITY_SECONDS: i64 = 86_400 + 300;
