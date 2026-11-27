mod handshake;
mod invitation;
mod session;

pub(crate) use handshake::{HandshakeAction, HandshakeGate, PairContext};
pub(crate) use invitation::{InvitationStore, PairingError, SensitiveQr};
pub(crate) use session::ReceiverIdentity;

use nuvyro_transfer_core::protocol::{ErrorCode, Id};

fn random_id() -> Result<Id, ErrorCode> {
    let mut bytes = [0; 16];
    getrandom::fill(&mut bytes).map_err(|_| ErrorCode::InternalFailure)?;
    Id::from_bytes(bytes).map_err(|_| ErrorCode::InternalFailure)
}
