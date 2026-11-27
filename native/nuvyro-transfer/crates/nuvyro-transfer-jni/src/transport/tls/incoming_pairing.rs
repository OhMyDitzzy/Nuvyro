use nuvyro_transfer_core::protocol::{ErrorCode, FrameHeader, Message, MessageKind};

use super::server::{ReceiverTlsIo, check as lifecycle_check};
use super::{IncomingTlsError, VerifiedIncoming, handshake_incoming_with};
use crate::pairing::{HandshakeAction, HandshakeGate, PairContext, ReceiverIdentity};
use crate::transport::{PendingIncoming, ReceiverContext};

mod io_control;
use io_control::{check, flush_frame, quiet, read_frame};

// PairAuthenticate: six-byte header, 16-byte ID, 32-byte token, u16 length, 128-byte name.
const REQUEST_BYTES: usize = 184;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum IncomingPairError {
    Transport(IncomingTlsError),
    Protocol(ErrorCode),
}

impl From<IncomingTlsError> for IncomingPairError {
    fn from(error: IncomingTlsError) -> Self {
        Self::Transport(error)
    }
}

/// Connection authentication only. The claimed display name is not a permanent identity.
pub(crate) struct PairedIncoming<S> {
    _connection: VerifiedIncoming<S>,
    pair: PairContext,
    claimed_name: String,
}

impl<S> PairedIncoming<S> {
    pub(crate) const fn pair(&self) -> PairContext {
        self.pair
    }

    pub(crate) fn claimed_name(&self) -> &str {
        &self.claimed_name
    }
}

impl<S> std::fmt::Debug for PairedIncoming<S> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PairedIncoming").finish_non_exhaustive()
    }
}

pub(in crate::transport) fn pair_incoming_with<S, O, C>(
    pending: PendingIncoming<S>,
    io: &O,
    identity: &ReceiverIdentity,
    mut current: C,
) -> Result<PairedIncoming<S>, IncomingPairError>
where
    O: ReceiverTlsIo<S>,
    C: FnMut() -> Option<ReceiverContext>,
{
    let mut connection = handshake_incoming_with(pending, io, identity, &mut current)?;
    lifecycle_check(&mut connection.pending, io, identity, &mut current)?;
    let mut gate = identity
        .incoming_gate_at(io.wall_now(), io.now())
        .map_err(IncomingPairError::Protocol)?;
    connection
        .pending
        .enter_control_phase(current(), io.now(), gate.deadline())
        .map_err(context_error)?;
    let result = drive(&mut connection, &mut gate, io, identity, &mut current);
    match result {
        Ok((pair, claimed_name)) => Ok(PairedIncoming {
            _connection: connection,
            pair,
            claimed_name,
        }),
        Err(error) => {
            gate.abort();
            Err(error)
        }
    }
}

fn drive<S, O: ReceiverTlsIo<S>>(
    connection: &mut VerifiedIncoming<S>,
    gate: &mut HandshakeGate,
    io: &O,
    identity: &ReceiverIdentity,
    current: &mut impl FnMut() -> Option<ReceiverContext>,
) -> Result<(PairContext, String), IncomingPairError> {
    let (ack, _) = receive(
        connection,
        gate,
        io,
        identity,
        current,
        MessageKind::SessionHello,
    )?;
    let HandshakeAction::Send(ack) = ack else {
        return Err(unexpected());
    };
    flush_frame(connection, gate, io, identity, current, &ack)?;
    drop(ack);
    check(connection, gate, io, identity, current)?;
    if gate
        .sent(io.now())
        .map_err(IncomingPairError::Protocol)?
        .is_some()
    {
        return Err(unexpected());
    }
    connection
        .pending
        .enter_control_phase(current(), io.now(), gate.deadline())
        .map_err(context_error)?;
    let (result, name) = receive(
        connection,
        gate,
        io,
        identity,
        current,
        MessageKind::PairAuthenticate,
    )?;
    let HandshakeAction::Send(result) = result else {
        return Err(unexpected());
    };
    let name = name.ok_or_else(unexpected)?;
    // Authentication already consumed the invitation, even if reservation or sending fails.
    let reservation = connection
        .pending
        .reserve_pair(current(), io.now())
        .map_err(context_error)?;
    flush_frame(connection, gate, io, identity, current, &result)?;
    drop(result);
    check(connection, gate, io, identity, current)?;
    let pair = gate
        .sent(io.now())
        .map_err(IncomingPairError::Protocol)?
        .ok_or_else(unexpected)?;
    // A terminal gate cannot be polled again; lifecycle/deadline checks still apply.
    lifecycle_check(&mut connection.pending, io, identity, current)?;
    quiet(&mut connection.server)?;
    lifecycle_check(&mut connection.pending, io, identity, current)?;
    connection
        .pending
        .promote_pair(reservation, current(), io.now())
        .map_err(context_error)?;
    lifecycle_check(&mut connection.pending, io, identity, current)?;
    Ok((pair, name))
}

fn receive<S, O: ReceiverTlsIo<S>>(
    connection: &mut VerifiedIncoming<S>,
    gate: &mut HandshakeGate,
    io: &O,
    identity: &ReceiverIdentity,
    current: &mut impl FnMut() -> Option<ReceiverContext>,
    expected: MessageKind,
) -> Result<(HandshakeAction, Option<String>), IncomingPairError> {
    let (bytes, len) = read_frame(connection, gate, io, identity, current, expected)?;
    check(connection, gate, io, identity, current)?;
    quiet(&mut connection.server)?;
    check(connection, gate, io, identity, current)?;
    let header = FrameHeader::decode(bytes[..6].try_into().map_err(|_| malformed())?)
        .map_err(|_| malformed())?;
    let message = Message::decode(header, &bytes[6..len])
        .map_err(|error| IncomingPairError::Protocol(error.remote_code()))?;
    let name = match &message {
        Message::PairAuthenticate {
            claimed_sender_name,
            ..
        } => Some(*claimed_sender_name),
        _ => None,
    };
    let action = gate
        .receive(message, io.now())
        .map_err(IncomingPairError::Protocol)?;
    let name = name
        .map(|name| {
            let mut owned = String::new();
            owned
                .try_reserve_exact(name.len())
                .map_err(|_| IncomingPairError::Protocol(ErrorCode::InternalFailure))?;
            owned.push_str(name);
            Ok::<String, IncomingPairError>(owned)
        })
        .transpose()?;
    // Token-bearing fixed storage is cleared before any response is queued.
    drop(bytes);
    Ok((action, name))
}

fn context_error(error: crate::transport::ReceiverError) -> IncomingPairError {
    IncomingTlsError::Context(error).into()
}

fn malformed() -> IncomingPairError {
    IncomingPairError::Protocol(ErrorCode::MalformedFrame)
}

fn unexpected() -> IncomingPairError {
    IncomingPairError::Protocol(ErrorCode::UnexpectedState)
}
