use std::io;
use std::time::SystemTime;

use nuvyro_transfer_core::protocol::{ErrorCode, TLS_ALPN};
use rustls::{ProtocolVersion, ServerConnection};

use super::TlsPolicyError;
use super::driver::{SocketReader, SocketWriter, TlsInterest, TlsIo};
use crate::pairing::ReceiverIdentity;
use crate::transport::connector::{PollProgress, poll_timeout};
use crate::transport::receiver::{PendingIncoming, ReceiverContext, ReceiverError};

pub(in crate::transport) trait ReceiverTlsIo<S>: TlsIo<S> {
    fn wall_now(&self) -> SystemTime {
        SystemTime::now()
    }
}

/// TLS is complete, but the sender has not authenticated an invitation.
/// The unpaired permit stays owned here; the owner must keep checking lifecycle on later I/O.
pub(crate) struct VerifiedIncoming<S> {
    pub(super) pending: PendingIncoming<S>,
    pub(super) server: ServerConnection,
}

impl<S> std::fmt::Debug for VerifiedIncoming<S> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("VerifiedIncoming").finish_non_exhaustive()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum IncomingTlsError {
    Context(ReceiverError),
    Identity(ErrorCode),
    Policy(TlsPolicyError),
    SocketIo,
    PeerClosed,
    ProtocolFailure,
    InvalidProgress,
}

/// Accepts only a pre-admitted socket. No listener, thread, or application I/O is started.
pub(in crate::transport) fn handshake_incoming_with<S, O, C>(
    mut pending: PendingIncoming<S>,
    io: &O,
    identity: &ReceiverIdentity,
    mut current: C,
) -> Result<VerifiedIncoming<S>, IncomingTlsError>
where
    O: ReceiverTlsIo<S>,
    C: FnMut() -> Option<ReceiverContext>,
{
    check(&mut pending, io, identity, &mut current)?;
    let mut server = identity.new_server().map_err(IncomingTlsError::Identity)?;
    server.set_buffer_limit(Some(1024));
    loop {
        check(&mut pending, io, identity, &mut current)?;
        if server.wants_write() {
            let result = {
                let socket = pending
                    .socket_mut(current(), io.now())
                    .map_err(IncomingTlsError::Context)?;
                server.write_tls(&mut SocketWriter { io, socket })
            };
            check(&mut pending, io, identity, &mut current)?;
            match result {
                Ok(0) => return Err(IncomingTlsError::InvalidProgress),
                Ok(_) => continue,
                Err(e) if e.kind() == io::ErrorKind::WouldBlock => {
                    wait(&mut pending, io, identity, TlsInterest::Write, &mut current)?;
                }
                Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
                Err(_) => return Err(IncomingTlsError::SocketIo),
            }
        } else if !server.is_handshaking() {
            break;
        } else {
            let result = {
                let socket = pending
                    .socket_mut(current(), io.now())
                    .map_err(IncomingTlsError::Context)?;
                server.read_tls(&mut SocketReader { io, socket })
            };
            check(&mut pending, io, identity, &mut current)?;
            match result {
                Ok(0) => return Err(IncomingTlsError::PeerClosed),
                Ok(_) => {
                    let state = server
                        .process_new_packets()
                        .map_err(|_| IncomingTlsError::ProtocolFailure)?;
                    check(&mut pending, io, identity, &mut current)?;
                    if state.peer_has_closed() {
                        return Err(IncomingTlsError::PeerClosed);
                    }
                }
                Err(e) if e.kind() == io::ErrorKind::WouldBlock => {
                    wait(&mut pending, io, identity, TlsInterest::Read, &mut current)?;
                }
                Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
                Err(_) => return Err(IncomingTlsError::SocketIo),
            }
        }
    }
    check(&mut pending, io, identity, &mut current)?;
    check_profile(&server).map_err(IncomingTlsError::Policy)?;
    check(&mut pending, io, identity, &mut current)?;
    Ok(VerifiedIncoming { pending, server })
}

fn check_profile(server: &ServerConnection) -> Result<(), TlsPolicyError> {
    if server.is_handshaking() {
        Err(TlsPolicyError::HandshakeIncomplete)
    } else if server.protocol_version() != Some(ProtocolVersion::TLSv1_3) {
        Err(TlsPolicyError::ProtocolMismatch)
    } else if server.alpn_protocol() != Some(TLS_ALPN) {
        Err(TlsPolicyError::AlpnMismatch)
    } else {
        Ok(())
    }
}

fn check<S, O: ReceiverTlsIo<S>>(
    pending: &mut PendingIncoming<S>,
    io: &O,
    identity: &ReceiverIdentity,
    current: &mut impl FnMut() -> Option<ReceiverContext>,
) -> Result<(), IncomingTlsError> {
    pending
        .check_context_at(current(), io.now())
        .map_err(IncomingTlsError::Context)?;
    if pending.context().session() != identity.session() {
        return Err(IncomingTlsError::Context(ReceiverError::StaleContext));
    }
    identity
        .check_live_at(io.wall_now(), io.now())
        .map_err(IncomingTlsError::Identity)
}

fn wait<S, O: ReceiverTlsIo<S>>(
    pending: &mut PendingIncoming<S>,
    io: &O,
    identity: &ReceiverIdentity,
    interest: TlsInterest,
    current: &mut impl FnMut() -> Option<ReceiverContext>,
) -> Result<(), IncomingTlsError> {
    loop {
        check(pending, io, identity, current)?;
        let window = pending
            .poll_window(current(), io.now())
            .map_err(IncomingTlsError::Context)?;
        // poll_window returns (0, 50 ms]; round up the final fraction of a millisecond.
        let timeout = poll_timeout(window).map_err(|_| IncomingTlsError::InvalidProgress)?;
        let result = {
            let socket = pending
                .socket_mut(current(), io.now())
                .map_err(IncomingTlsError::Context)?;
            io.poll(socket, interest, timeout)
        };
        check(pending, io, identity, current)?;
        match result {
            Ok(PollProgress::Ready) => return Ok(()),
            Ok(PollProgress::Pending) => {}
            Err(e) if e.kind() == io::ErrorKind::Interrupted => {}
            Err(_) => return Err(IncomingTlsError::SocketIo),
        }
    }
}
