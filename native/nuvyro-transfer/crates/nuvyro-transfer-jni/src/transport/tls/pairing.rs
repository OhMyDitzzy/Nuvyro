use std::io::{self, Read, Write};

use nuvyro_transfer_core::pairing::PairingQr;
use nuvyro_transfer_core::protocol::{EncodedFrame, ErrorCode, FrameHeader, Message, MessageKind};
use rustls::ClientConnection;
use zeroize::Zeroizing;

use super::super::ConnectionContext;
use super::super::connector::PendingConnection;
use super::driver::{SocketReader, SocketWriter, wait};
use super::{TlsHandshakeError, TlsInterest, TlsIo, VerifiedTlsConnection, handshake_with};
use crate::pairing::{HandshakeAction, HandshakeGate, PairContext};

// The largest inbound pairing response is a connection Error (six-byte header + 35).
const RESPONSE_BYTES: usize = 41;
const TLS_OUTPUT_BYTES: usize = 1024;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum PairConnectionError {
    Transport(TlsHandshakeError),
    Protocol(ErrorCode),
}

impl From<TlsHandshakeError> for PairConnectionError {
    fn from(error: TlsHandshakeError) -> Self {
        Self::Transport(error)
    }
}

/// Keeps the authenticated socket private. Pairing grants no transfer or file capability.
pub(crate) struct PairedConnection<S> {
    _connection: VerifiedTlsConnection<S>,
    pair: PairContext,
}

impl<S> PairedConnection<S> {
    pub(crate) const fn pair(&self) -> PairContext {
        self.pair
    }
}

impl<S> std::fmt::Debug for PairedConnection<S> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PairedConnection").finish_non_exhaustive()
    }
}

/// One QR supplies the TLS pin, Hello SID and invitation on the same owned connection.
pub(in crate::transport) fn pair_outgoing_with<S, O, C>(
    mut pending: PendingConnection<S>,
    io: &O,
    qr: PairingQr,
    claimed_name: String,
    mut current: C,
) -> Result<PairedConnection<S>, PairConnectionError>
where
    O: TlsIo<S>,
    C: FnMut() -> Option<ConnectionContext>,
{
    validate_target(&mut pending, io, &qr, &mut current)?;
    let verified = handshake_with(pending, io, qr.spki_pin(), &mut current)?;
    pair_verified(verified, io, qr, claimed_name, current)
}

fn validate_target<S, O: TlsIo<S>>(
    pending: &mut PendingConnection<S>,
    io: &O,
    qr: &PairingQr,
    current: &mut impl FnMut() -> Option<ConnectionContext>,
) -> Result<(), PairConnectionError> {
    context_check(pending, io, current)?;
    if pending.context().expected_session() != qr.secure_session_id() {
        return Err(PairConnectionError::Protocol(ErrorCode::UnexpectedState));
    }
    Ok(())
}

fn pair_verified<S, O, C>(
    mut connection: VerifiedTlsConnection<S>,
    io: &O,
    qr: PairingQr,
    claimed_name: String,
    mut current: C,
) -> Result<PairedConnection<S>, PairConnectionError>
where
    O: TlsIo<S>,
    C: FnMut() -> Option<ConnectionContext>,
{
    context_check(&mut connection.pending, io, &mut current)?;
    let mut gate = HandshakeGate::outgoing(qr, claimed_name, io.now())
        .map_err(PairConnectionError::Protocol)?;
    connection
        .pending
        .enter_control_phase(current(), io.now(), gate.deadline())
        .map_err(|error| PairConnectionError::Transport(TlsHandshakeError::Context(error)))?;
    connection.client.set_buffer_limit(Some(TLS_OUTPUT_BYTES));
    // Reject application bytes already buffered alongside the TLS handshake.
    quiet(&mut connection.client)?;
    let result = drive_control(&mut connection, &mut gate, io, &mut current);
    let pair = match result {
        Ok(pair) => pair,
        Err(error) => {
            gate.abort();
            return Err(error);
        }
    };
    // Gate completion is terminal: its time checker no longer applies after Established.
    context_check(&mut connection.pending, io, &mut current)?;
    quiet(&mut connection.client)?;
    context_check(&mut connection.pending, io, &mut current)?;
    Ok(PairedConnection {
        _connection: connection,
        pair,
    })
}

fn drive_control<S, O: TlsIo<S>>(
    connection: &mut VerifiedTlsConnection<S>,
    gate: &mut HandshakeGate,
    io: &O,
    current: &mut impl FnMut() -> Option<ConnectionContext>,
) -> Result<PairContext, PairConnectionError> {
    let mut action = gate
        .initial_message(io.now())
        .map_err(PairConnectionError::Protocol)?;
    loop {
        match action {
            HandshakeAction::Send(frame) => {
                check(connection, gate, io, current)?;
                quiet(&mut connection.client)?;
                send_frame(connection, gate, io, current, &frame)?;
                // Drop the token-bearing frame as soon as TLS has flushed it.
                drop(frame);
                if let Some(pair) = gate.sent(io.now()).map_err(PairConnectionError::Protocol)? {
                    return Ok(pair);
                }
            }
            HandshakeAction::Established(pair) => return Ok(pair),
        }
        let (bytes, len) = read_frame(connection, gate, io, current)?;
        let header = FrameHeader::decode(bytes[..6].try_into().map_err(|_| malformed())?)
            .map_err(|_| malformed())?;
        let message = Message::decode(header, &bytes[6..len])
            .map_err(|error| PairConnectionError::Protocol(error.remote_code()))?;
        let previous_deadline = gate.deadline();
        action = gate
            .receive(message, io.now())
            .map_err(PairConnectionError::Protocol)?;
        if gate.deadline() != previous_deadline {
            connection
                .pending
                .enter_control_phase(current(), io.now(), gate.deadline())
                .map_err(|error| {
                    PairConnectionError::Transport(TlsHandshakeError::Context(error))
                })?;
        }
    }
}

fn send_frame<S, O: TlsIo<S>>(
    connection: &mut VerifiedTlsConnection<S>,
    gate: &mut HandshakeGate,
    io: &O,
    current: &mut impl FnMut() -> Option<ConnectionContext>,
    frame: &EncodedFrame,
) -> Result<(), PairConnectionError> {
    let mut offset = 0;
    while offset < frame.as_slice().len() {
        check(connection, gate, io, current)?;
        let result = connection
            .client
            .writer()
            .write(&frame.as_slice()[offset..]);
        check(connection, gate, io, current)?;
        match result {
            Ok(0) => return Err(TlsHandshakeError::InvalidProgress.into()),
            Ok(count) => offset += count,
            Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => {}
            Err(_) => return Err(TlsHandshakeError::SocketIo.into()),
        }
        flush_records(connection, gate, io, current)?;
    }
    flush_records(connection, gate, io, current)
}

fn flush_records<S, O: TlsIo<S>>(
    connection: &mut VerifiedTlsConnection<S>,
    gate: &mut HandshakeGate,
    io: &O,
    current: &mut impl FnMut() -> Option<ConnectionContext>,
) -> Result<(), PairConnectionError> {
    while connection.client.wants_write() {
        check(connection, gate, io, current)?;
        let socket = connection
            .pending
            .socket_mut(current(), io.now())
            .map_err(|error| PairConnectionError::Transport(TlsHandshakeError::Context(error)))?;
        let result = connection
            .client
            .write_tls(&mut SocketWriter { io, socket });
        check(connection, gate, io, current)?;
        match result {
            Ok(0) => return Err(TlsHandshakeError::InvalidProgress.into()),
            Ok(_) => {}
            Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                wait(&mut connection.pending, io, TlsInterest::Write, current)?;
                check(connection, gate, io, current)?;
            }
            Err(_) => return Err(TlsHandshakeError::SocketIo.into()),
        }
    }
    check(connection, gate, io, current)
}

fn read_frame<S, O: TlsIo<S>>(
    connection: &mut VerifiedTlsConnection<S>,
    gate: &mut HandshakeGate,
    io: &O,
    current: &mut impl FnMut() -> Option<ConnectionContext>,
) -> Result<(Zeroizing<[u8; RESPONSE_BYTES]>, usize), PairConnectionError> {
    let mut bytes = Zeroizing::new([0; RESPONSE_BYTES]);
    let mut used = 0;
    let mut wanted = 6;
    loop {
        check(connection, gate, io, current)?;
        flush_records(connection, gate, io, current)?;
        let result = connection.client.reader().read(&mut bytes[used..wanted]);
        check(connection, gate, io, current)?;
        match result {
            Ok(0) => return Err(TlsHandshakeError::PeerClosed.into()),
            Ok(count) => {
                used += count;
                if used != wanted {
                    continue;
                }
                if wanted != 6 {
                    return Ok((bytes, used));
                }
                let header = FrameHeader::decode(bytes[..6].try_into().map_err(|_| malformed())?)
                    .map_err(|_| malformed())?;
                if !matches!(
                    header.kind(),
                    MessageKind::SessionHelloAck | MessageKind::PairResult | MessageKind::Error
                ) {
                    return Err(PairConnectionError::Protocol(ErrorCode::UnexpectedState));
                }
                wanted = 6 + header.body_len() as usize;
                if wanted > RESPONSE_BYTES {
                    return Err(malformed());
                }
            }
            Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                let socket =
                    connection
                        .pending
                        .socket_mut(current(), io.now())
                        .map_err(|error| {
                            PairConnectionError::Transport(TlsHandshakeError::Context(error))
                        })?;
                let received = connection.client.read_tls(&mut SocketReader { io, socket });
                check(connection, gate, io, current)?;
                match received {
                    Ok(0) => return Err(TlsHandshakeError::PeerClosed.into()),
                    Ok(_) => {
                        let state = connection.client.process_new_packets().map_err(|_| {
                            PairConnectionError::Transport(TlsHandshakeError::ProtocolFailure)
                        })?;
                        check(connection, gate, io, current)?;
                        if state.peer_has_closed() {
                            return Err(TlsHandshakeError::PeerClosed.into());
                        }
                    }
                    Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
                    Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                        wait(&mut connection.pending, io, TlsInterest::Read, current)?;
                        check(connection, gate, io, current)?;
                    }
                    Err(_) => return Err(TlsHandshakeError::SocketIo.into()),
                }
            }
            Err(_) => return Err(TlsHandshakeError::SocketIo.into()),
        }
    }
}

fn quiet(client: &mut ClientConnection) -> Result<(), PairConnectionError> {
    let mut byte = [0];
    match client.reader().read(&mut byte) {
        Err(error) if error.kind() == io::ErrorKind::WouldBlock => Ok(()),
        Ok(0) => Err(TlsHandshakeError::PeerClosed.into()),
        Ok(_) => Err(PairConnectionError::Protocol(ErrorCode::UnexpectedState)),
        Err(_) => Err(TlsHandshakeError::SocketIo.into()),
    }
}

fn check<S, O: TlsIo<S>>(
    connection: &mut VerifiedTlsConnection<S>,
    gate: &mut HandshakeGate,
    io: &O,
    current: &mut impl FnMut() -> Option<ConnectionContext>,
) -> Result<(), PairConnectionError> {
    context_check(&mut connection.pending, io, current)?;
    gate.check_time(io.now())
        .map_err(PairConnectionError::Protocol)
}

fn context_check<S, O: TlsIo<S>>(
    pending: &mut PendingConnection<S>,
    io: &O,
    current: &mut impl FnMut() -> Option<ConnectionContext>,
) -> Result<(), PairConnectionError> {
    pending
        .check_context_at(current(), io.now())
        .map_err(|error| PairConnectionError::Transport(TlsHandshakeError::Context(error)))
}

fn malformed() -> PairConnectionError {
    PairConnectionError::Protocol(ErrorCode::MalformedFrame)
}
