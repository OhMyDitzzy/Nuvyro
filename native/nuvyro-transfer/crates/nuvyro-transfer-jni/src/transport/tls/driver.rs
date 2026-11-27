use std::io::{self, Read, Write};
use std::time::Instant;

use nuvyro_transfer_core::pairing::SpkiPin;
use rustls::ClientConnection;

use super::super::connector::{PendingConnection, PollProgress, poll_timeout};
use super::super::{ConnectError, ConnectionContext};
use super::{TlsPolicyError, check_negotiated_profile, new_client};

/// A private readiness direction for the nonblocking socket owned by the worker.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::transport) enum TlsInterest {
    Read,
    Write,
}

/// Performs only bounded socket I/O. The descriptor remains owned by PendingConnection.
pub(in crate::transport) trait TlsIo<S> {
    fn now(&self) -> Instant;
    fn read(&self, socket: &S, bytes: &mut [u8]) -> io::Result<usize>;
    fn write(&self, socket: &S, bytes: &[u8]) -> io::Result<usize>;
    fn poll(&self, socket: &S, interest: TlsInterest, timeout_ms: i32) -> io::Result<PollProgress>;
}

/// Owns the still-private socket and verified TLS state. It deliberately exposes no I/O API;
/// session hello and invitation checks must run before a later layer can send application data.
pub(crate) struct VerifiedTlsConnection<S> {
    pub(super) pending: PendingConnection<S>,
    pub(super) client: ClientConnection,
}

impl<S> std::fmt::Debug for VerifiedTlsConnection<S> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("VerifiedTlsConnection")
            .finish_non_exhaustive()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum TlsHandshakeError {
    Context(ConnectError),
    Policy(TlsPolicyError),
    SocketIo,
    PeerClosed,
    ProtocolFailure,
    InvalidProgress,
}

/// Completes one TLS 1.3 client handshake using the connect attempt's original deadline.
/// `current_context` must return a fresh owner snapshot; None means the session/route is gone.
/// Every socket operation is nonblocking and followed by cancellation, deadline, and context
/// checks. Any error drops the socket and its connection-budget permit.
pub(in crate::transport) fn handshake_with<S, O, C>(
    pending: PendingConnection<S>,
    socket_io: &O,
    pin: &SpkiPin,
    current_context: C,
) -> Result<VerifiedTlsConnection<S>, TlsHandshakeError>
where
    O: TlsIo<S>,
    C: FnMut() -> Option<ConnectionContext>,
{
    let client = new_client(pin).map_err(TlsHandshakeError::Policy)?;
    drive_handshake(pending, socket_io, client, current_context)
}

#[cfg(test)]
pub(super) fn test_handshake_with_client<S, O, C>(
    pending: PendingConnection<S>,
    socket_io: &O,
    client: ClientConnection,
    current_context: C,
) -> Result<VerifiedTlsConnection<S>, TlsHandshakeError>
where
    O: TlsIo<S>,
    C: FnMut() -> Option<ConnectionContext>,
{
    drive_handshake(pending, socket_io, client, current_context)
}

fn drive_handshake<S, O, C>(
    mut pending: PendingConnection<S>,
    socket_io: &O,
    mut client: ClientConnection,
    mut current_context: C,
) -> Result<VerifiedTlsConnection<S>, TlsHandshakeError>
where
    O: TlsIo<S>,
    C: FnMut() -> Option<ConnectionContext>,
{
    check(&mut pending, socket_io, &mut current_context)?;

    loop {
        check(&mut pending, socket_io, &mut current_context)?;
        if client.wants_write() {
            let result = {
                let socket = pending
                    .socket_mut(current_context(), socket_io.now())
                    .map_err(TlsHandshakeError::Context)?;
                client.write_tls(&mut SocketWriter {
                    io: socket_io,
                    socket,
                })
            };
            check(&mut pending, socket_io, &mut current_context)?;
            match result {
                Ok(0) => return Err(TlsHandshakeError::InvalidProgress),
                Ok(_) => continue,
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                    wait(
                        &mut pending,
                        socket_io,
                        TlsInterest::Write,
                        &mut current_context,
                    )?;
                }
                Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
                Err(_) => return Err(TlsHandshakeError::SocketIo),
            }
        } else if !client.is_handshaking() {
            break;
        } else {
            let result = {
                let socket = pending
                    .socket_mut(current_context(), socket_io.now())
                    .map_err(TlsHandshakeError::Context)?;
                client.read_tls(&mut SocketReader {
                    io: socket_io,
                    socket,
                })
            };
            check(&mut pending, socket_io, &mut current_context)?;
            match result {
                Ok(0) => return Err(TlsHandshakeError::PeerClosed),
                Ok(_) => {
                    client
                        .process_new_packets()
                        .map_err(|_| TlsHandshakeError::ProtocolFailure)?;
                    check(&mut pending, socket_io, &mut current_context)?;
                }
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                    wait(
                        &mut pending,
                        socket_io,
                        TlsInterest::Read,
                        &mut current_context,
                    )?;
                }
                Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
                Err(_) => return Err(TlsHandshakeError::SocketIo),
            }
        }
    }

    check(&mut pending, socket_io, &mut current_context)?;
    check_negotiated_profile(&client).map_err(TlsHandshakeError::Policy)?;
    check(&mut pending, socket_io, &mut current_context)?;
    Ok(VerifiedTlsConnection { pending, client })
}

fn check<S, O, C>(
    pending: &mut PendingConnection<S>,
    io: &O,
    current_context: &mut C,
) -> Result<(), TlsHandshakeError>
where
    O: TlsIo<S>,
    C: FnMut() -> Option<ConnectionContext>,
{
    pending
        .check_context_at(current_context(), io.now())
        .map_err(TlsHandshakeError::Context)
}

pub(super) fn wait<S, O, C>(
    pending: &mut PendingConnection<S>,
    io: &O,
    interest: TlsInterest,
    current_context: &mut C,
) -> Result<(), TlsHandshakeError>
where
    O: TlsIo<S>,
    C: FnMut() -> Option<ConnectionContext>,
{
    loop {
        let now = io.now();
        let window = pending
            .poll_window(current_context(), now)
            .map_err(TlsHandshakeError::Context)?;
        let timeout_ms = poll_timeout(window).map_err(TlsHandshakeError::Context)?;
        let progress = {
            let socket = pending
                .socket_mut(current_context(), io.now())
                .map_err(TlsHandshakeError::Context)?;
            io.poll(socket, interest, timeout_ms)
        };
        check(pending, io, current_context)?;
        match progress {
            Ok(PollProgress::Ready) => return Ok(()),
            Ok(PollProgress::Pending) => {}
            Err(error) if error.kind() == io::ErrorKind::Interrupted => {}
            Err(_) => return Err(TlsHandshakeError::SocketIo),
        }
    }
}

pub(super) struct SocketReader<'a, S, O> {
    pub(super) io: &'a O,
    pub(super) socket: &'a S,
}

impl<S, O: TlsIo<S>> Read for SocketReader<'_, S, O> {
    fn read(&mut self, bytes: &mut [u8]) -> io::Result<usize> {
        self.io.read(self.socket, bytes)
    }
}

pub(super) struct SocketWriter<'a, S, O> {
    pub(super) io: &'a O,
    pub(super) socket: &'a S,
}

impl<S, O: TlsIo<S>> Write for SocketWriter<'_, S, O> {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.io.write(self.socket, bytes)
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
