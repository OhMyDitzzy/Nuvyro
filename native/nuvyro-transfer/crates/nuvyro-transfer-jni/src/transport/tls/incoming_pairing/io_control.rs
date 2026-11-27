use std::io::{self, Read, Write};

use nuvyro_transfer_core::protocol::{EncodedFrame, FrameHeader, MessageKind};
use rustls::ServerConnection;
use zeroize::Zeroizing;

use super::{IncomingPairError, REQUEST_BYTES, malformed, unexpected};
use crate::pairing::{HandshakeGate, ReceiverIdentity};
use crate::transport::ReceiverContext;
use crate::transport::tls::driver::{SocketReader, SocketWriter, TlsInterest};
use crate::transport::tls::server::{ReceiverTlsIo, check as lifecycle_check, wait};
use crate::transport::tls::{IncomingTlsError, VerifiedIncoming};

pub(super) fn check<S, O: ReceiverTlsIo<S>>(
    connection: &mut VerifiedIncoming<S>,
    gate: &mut HandshakeGate,
    io: &O,
    identity: &ReceiverIdentity,
    current: &mut impl FnMut() -> Option<ReceiverContext>,
) -> Result<(), IncomingPairError> {
    lifecycle_check(&mut connection.pending, io, identity, current)?;
    gate.check_time(io.now())
        .map_err(IncomingPairError::Protocol)
}

pub(super) fn flush_frame<S, O: ReceiverTlsIo<S>>(
    connection: &mut VerifiedIncoming<S>,
    gate: &mut HandshakeGate,
    io: &O,
    identity: &ReceiverIdentity,
    current: &mut impl FnMut() -> Option<ReceiverContext>,
    frame: &EncodedFrame,
) -> Result<(), IncomingPairError> {
    check(connection, gate, io, identity, current)?;
    quiet(&mut connection.server)?;
    let mut offset = 0;
    while offset < frame.as_slice().len() {
        check(connection, gate, io, identity, current)?;
        let result = connection
            .server
            .writer()
            .write(&frame.as_slice()[offset..]);
        check(connection, gate, io, identity, current)?;
        match result {
            Ok(0) => return Err(IncomingTlsError::InvalidProgress.into()),
            Ok(count) => offset += count,
            Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => {}
            Err(_) => return Err(IncomingTlsError::SocketIo.into()),
        }
        flush_records(connection, gate, io, identity, current)?;
    }
    flush_records(connection, gate, io, identity, current)
}

fn flush_records<S, O: ReceiverTlsIo<S>>(
    connection: &mut VerifiedIncoming<S>,
    gate: &mut HandshakeGate,
    io: &O,
    identity: &ReceiverIdentity,
    current: &mut impl FnMut() -> Option<ReceiverContext>,
) -> Result<(), IncomingPairError> {
    while connection.server.wants_write() {
        check(connection, gate, io, identity, current)?;
        let result = {
            let socket = connection
                .pending
                .socket_mut(current(), io.now())
                .map_err(super::context_error)?;
            connection
                .server
                .write_tls(&mut SocketWriter { io, socket })
        };
        check(connection, gate, io, identity, current)?;
        match result {
            Ok(0) => return Err(IncomingTlsError::InvalidProgress.into()),
            Ok(_) => {}
            Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                wait(
                    &mut connection.pending,
                    io,
                    identity,
                    TlsInterest::Write,
                    current,
                )?;
                check(connection, gate, io, identity, current)?;
            }
            Err(_) => return Err(IncomingTlsError::SocketIo.into()),
        }
    }
    check(connection, gate, io, identity, current)
}

pub(super) fn read_frame<S, O: ReceiverTlsIo<S>>(
    connection: &mut VerifiedIncoming<S>,
    gate: &mut HandshakeGate,
    io: &O,
    identity: &ReceiverIdentity,
    current: &mut impl FnMut() -> Option<ReceiverContext>,
    expected: MessageKind,
) -> Result<(Zeroizing<[u8; REQUEST_BYTES]>, usize), IncomingPairError> {
    let mut bytes = Zeroizing::new([0; REQUEST_BYTES]);
    let mut used = 0;
    let mut wanted = 6;
    loop {
        check(connection, gate, io, identity, current)?;
        flush_records(connection, gate, io, identity, current)?;
        let result = connection.server.reader().read(&mut bytes[used..wanted]);
        check(connection, gate, io, identity, current)?;
        match result {
            Ok(0) => return Err(IncomingTlsError::PeerClosed.into()),
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
                if header.kind() != expected && header.kind() != MessageKind::Error {
                    return Err(unexpected());
                }
                wanted = 6 + header.body_len() as usize;
                if wanted > REQUEST_BYTES {
                    return Err(malformed());
                }
            }
            Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                let received = {
                    let socket = connection
                        .pending
                        .socket_mut(current(), io.now())
                        .map_err(super::context_error)?;
                    connection.server.read_tls(&mut SocketReader { io, socket })
                };
                check(connection, gate, io, identity, current)?;
                match received {
                    Ok(0) => return Err(IncomingTlsError::PeerClosed.into()),
                    Ok(_) => {
                        let state = connection.server.process_new_packets().map_err(|_| {
                            IncomingPairError::from(IncomingTlsError::ProtocolFailure)
                        })?;
                        check(connection, gate, io, identity, current)?;
                        if state.peer_has_closed() {
                            return Err(IncomingTlsError::PeerClosed.into());
                        }
                    }
                    Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
                    Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                        wait(
                            &mut connection.pending,
                            io,
                            identity,
                            TlsInterest::Read,
                            current,
                        )?;
                        check(connection, gate, io, identity, current)?;
                    }
                    Err(_) => return Err(IncomingTlsError::SocketIo.into()),
                }
            }
            Err(_) => return Err(IncomingTlsError::SocketIo.into()),
        }
    }
}

pub(super) fn quiet(server: &mut ServerConnection) -> Result<(), IncomingPairError> {
    let mut byte = Zeroizing::new([0]);
    match server.reader().read(&mut *byte) {
        Err(error) if error.kind() == io::ErrorKind::WouldBlock => Ok(()),
        Ok(0) => Err(IncomingTlsError::PeerClosed.into()),
        Ok(_) => Err(unexpected()),
        Err(_) => Err(IncomingTlsError::SocketIo.into()),
    }
}
