//! Untrusted pairing QR input; decoding does not authenticate a peer.

mod error;
mod input;
mod qr;

pub use error::PairingQrError;
pub use input::{InvitationToken, PairingQr, SpkiPin};
