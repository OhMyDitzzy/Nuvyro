use std::cell::{Cell, RefCell};
use std::collections::VecDeque;
use std::io::{self, Read, Write};
use std::net::SocketAddr;
use std::rc::Rc;
use std::time::{Duration, Instant};

use nuvyro_transfer_core::pairing::PairingQr;
use nuvyro_transfer_core::protocol::{
    ErrorCode, ErrorScope, Hello, Id, Message, MessageKind, StreamDecoder, encode_message,
};
use rustls::ServerConnection;

use super::super::super::pairing::{PairConnectionError, pair_outgoing_with};
use super::super::super::{TlsInterest, TlsIo};
use crate::pairing::{HandshakeAction, HandshakeGate, PairContext, ReceiverIdentity};
use crate::transport::budget::ConnectionBudget;
use crate::transport::connector::{
    ConnectProgress, PendingConnection, PollProgress, SocketOps, connect_with,
};
use crate::transport::{ConnectAttempt, ConnectCancellation, ConnectionContext, NumericEndpoint};

#[derive(Clone, Copy, Eq, PartialEq)]
pub(super) enum Behavior {
    Normal,
    WrongAck,
    WrongResult,
    RefuseAuth,
    SilentHello,
    SilentAuth,
    DataHeader,
    OversizedHeader,
    PipelinedResult,
    ClosedResult,
    TrailingResult,
}

pub(super) struct Socket {
    server: RefCell<ServerConnection>,
    gate: RefCell<HandshakeGate>,
    decoder: RefCell<StreamDecoder>,
    inbound: RefCell<VecDeque<u8>>,
    drops: Rc<Cell<usize>>,
}

impl Drop for Socket {
    fn drop(&mut self) {
        self.drops.set(self.drops.get() + 1);
    }
}

pub(super) struct Wire {
    pub(super) identity: ReceiverIdentity,
    pub(super) budget: ConnectionBudget,
    pub(super) time: Cell<Instant>,
    pub(super) drops: Rc<Cell<usize>>,
    pub(super) behavior: Cell<Behavior>,
    pub(super) max_io: Cell<usize>,
    pub(super) seen: RefCell<Vec<MessageKind>>,
    pub(super) polls: RefCell<Vec<i32>>,
    pub(super) cancellation: RefCell<Option<ConnectCancellation>>,
    pub(super) cancel_after_frame: Cell<usize>,
    pub(super) invalidate: Cell<bool>,
    pub(super) invalidate_on_write: Cell<bool>,
    pub(super) advance_on_write: Cell<Duration>,
    pub(super) interrupt_read: Cell<bool>,
    pub(super) interrupt_poll: Cell<bool>,
    pub(super) block_write: Cell<bool>,
    pub(super) fail_write: Cell<bool>,
    pub(super) zero_write: Cell<bool>,
    pub(super) raw_eof: Cell<bool>,
    pub(super) rewind_on_read: Cell<bool>,
    pub(super) cancel_on_poll: Cell<bool>,
    pub(super) corrupt_response: Cell<bool>,
    pub(super) established: Cell<Option<PairContext>>,
}

impl Wire {
    pub(super) fn new() -> (Self, PairingQr) {
        let identity = ReceiverIdentity::create().unwrap();
        let (_, qr) = identity.create_invitation().unwrap();
        let scanned = PairingQr::decode(qr.as_bytes()).unwrap();
        (
            Self {
                identity,
                budget: ConnectionBudget::new(),
                time: Cell::new(Instant::now()),
                drops: Rc::new(Cell::new(0)),
                behavior: Cell::new(Behavior::Normal),
                max_io: Cell::new(usize::MAX),
                seen: RefCell::new(Vec::new()),
                polls: RefCell::new(Vec::new()),
                cancellation: RefCell::new(None),
                cancel_after_frame: Cell::new(usize::MAX),
                invalidate: Cell::new(false),
                invalidate_on_write: Cell::new(false),
                advance_on_write: Cell::new(Duration::ZERO),
                interrupt_read: Cell::new(false),
                interrupt_poll: Cell::new(false),
                block_write: Cell::new(false),
                fail_write: Cell::new(false),
                zero_write: Cell::new(false),
                raw_eof: Cell::new(false),
                rewind_on_read: Cell::new(false),
                cancel_on_poll: Cell::new(false),
                corrupt_response: Cell::new(false),
                established: Cell::new(None),
            },
            scanned,
        )
    }

    pub(super) fn context(&self) -> ConnectionContext {
        ConnectionContext::new(1, 2, 7, *self.identity.session().as_bytes()).unwrap()
    }

    pub(super) fn pending(&self, context: ConnectionContext) -> PendingConnection<Socket> {
        let attempt = ConnectAttempt::at(
            context,
            NumericEndpoint::new(&[192, 168, 42, 3], 0, 47739).unwrap(),
            Duration::from_secs(1),
            self.time.get(),
        )
        .unwrap();
        *self.cancellation.borrow_mut() = Some(attempt.cancellation());
        connect_with(self, &self.budget, attempt).unwrap()
    }

    pub(super) fn run(
        &self,
        qr: PairingQr,
    ) -> Result<super::super::super::PairedConnection<Socket>, PairConnectionError> {
        pair_outgoing_with(
            self.pending(self.context()),
            self,
            qr,
            "Sender".to_owned(),
            || {
                if self.invalidate.get() {
                    None
                } else {
                    Some(self.context())
                }
            },
        )
    }

    fn respond(&self, server: &mut ServerConnection, message: Message<'_>) {
        let bytes = encode_message(&message).unwrap();
        server.writer().write_all(bytes.as_slice()).unwrap();
    }

    fn peer(&self, socket: &Socket, server: &mut ServerConnection, bytes: &[u8]) {
        let decoded = socket.decoder.borrow_mut().push(bytes).unwrap();
        assert_eq!(decoded.consumed, bytes.len());
        let Some(frame) = decoded.frame else {
            return;
        };
        let message = frame.decode().unwrap();
        let kind = message.kind();
        self.seen.borrow_mut().push(kind);
        match (self.behavior.get(), kind) {
            (Behavior::WrongAck, MessageKind::SessionHello) => self.respond(
                server,
                Message::SessionHelloAck(Hello {
                    secure_session_id: Id::from_bytes([0x55; 16]).unwrap(),
                }),
            ),
            (Behavior::WrongResult, MessageKind::PairAuthenticate) => self.respond(
                server,
                Message::PairResult {
                    pair_id: Id::from_bytes([0x66; 16]).unwrap(),
                    secure_session_id: Id::from_bytes([0x55; 16]).unwrap(),
                },
            ),
            (Behavior::RefuseAuth, MessageKind::PairAuthenticate) => self.respond(
                server,
                Message::Error {
                    pair_id: None,
                    transfer_id: None,
                    scope: ErrorScope::Connection,
                    code: ErrorCode::AuthenticationFailed,
                },
            ),
            (Behavior::SilentHello, MessageKind::SessionHello)
            | (Behavior::SilentAuth, MessageKind::PairAuthenticate) => {}
            (Behavior::DataHeader, MessageKind::SessionHello) => {
                server.writer().write_all(&[0, 4, 0, 58, 2, 2]).unwrap();
            }
            (Behavior::OversizedHeader, MessageKind::SessionHello) => {
                server
                    .writer()
                    .write_all(&[0xff, 0xff, 0xff, 0xff, 0, 2])
                    .unwrap();
            }
            _ => {
                let action = socket.gate.borrow_mut().receive(message, Instant::now());
                let response = match action {
                    Ok(HandshakeAction::Send(response)) => response,
                    Ok(_) => panic!("expected control frame"),
                    Err(code) => {
                        self.respond(
                            server,
                            Message::Error {
                                pair_id: None,
                                transfer_id: None,
                                scope: ErrorScope::Connection,
                                code,
                            },
                        );
                        return;
                    }
                };
                let mut output = zeroize::Zeroizing::new(response.as_slice().to_vec());
                if self.behavior.get() == Behavior::PipelinedResult
                    && kind == MessageKind::SessionHello
                {
                    let extra = encode_message(&Message::PairResult {
                        pair_id: Id::from_bytes([0x66; 16]).unwrap(),
                        secure_session_id: self.identity.session(),
                    })
                    .unwrap();
                    output.extend_from_slice(extra.as_slice());
                }
                if kind == MessageKind::PairAuthenticate
                    && self.behavior.get() == Behavior::TrailingResult
                {
                    output.extend_from_slice(b"extra");
                }
                server.writer().write_all(&output).unwrap();
                if let Some(pair) = socket.gate.borrow_mut().sent(Instant::now()).unwrap() {
                    self.established.set(Some(pair));
                }
                if kind == MessageKind::PairAuthenticate
                    && self.behavior.get() == Behavior::ClosedResult
                {
                    server.send_close_notify();
                }
            }
        }
        if self.seen.borrow().len() >= self.cancel_after_frame.get() {
            self.cancellation.borrow().as_ref().unwrap().cancel();
        }
        if self.invalidate_on_write.get() {
            self.invalidate.set(true);
        }
        self.time.set(self.time.get() + self.advance_on_write.get());
    }
}

impl SocketOps for Wire {
    type Socket = Socket;
    fn now(&self) -> Instant {
        self.time.get()
    }
    fn open(&self, _: SocketAddr) -> io::Result<Socket> {
        Ok(Socket {
            server: RefCell::new(self.identity.new_server().unwrap()),
            gate: RefCell::new(self.identity.incoming_gate().unwrap()),
            decoder: RefCell::default(),
            inbound: RefCell::default(),
            drops: Rc::clone(&self.drops),
        })
    }
    fn bind_network(&self, _: &Socket, _: u64) -> io::Result<()> {
        Ok(())
    }
    fn begin_connect(&self, _: &Socket, _: SocketAddr) -> io::Result<ConnectProgress> {
        Ok(ConnectProgress::Connected)
    }
    fn poll(&self, _: &Socket, _: Duration) -> io::Result<PollProgress> {
        Ok(PollProgress::Ready)
    }
    fn verify_peer(&self, _: &Socket, _: SocketAddr) -> io::Result<()> {
        Ok(())
    }
}

impl TlsIo<Socket> for Wire {
    fn now(&self) -> Instant {
        self.time.get()
    }
    fn read(&self, socket: &Socket, bytes: &mut [u8]) -> io::Result<usize> {
        if !self.seen.borrow().is_empty() {
            if self.rewind_on_read.replace(false) {
                self.time.set(self.time.get() - Duration::from_millis(1));
            }
            if self.raw_eof.get() {
                return Ok(0);
            }
            if self.interrupt_read.replace(false) {
                return Err(io::ErrorKind::Interrupted.into());
            }
        }
        let mut queue = socket.inbound.borrow_mut();
        if queue.is_empty() {
            return Err(io::ErrorKind::WouldBlock.into());
        }
        let count = bytes.len().min(queue.len()).min(self.max_io.get());
        for byte in &mut bytes[..count] {
            *byte = queue.pop_front().unwrap();
        }
        Ok(count)
    }
    fn write(&self, socket: &Socket, bytes: &[u8]) -> io::Result<usize> {
        let mut server = socket.server.borrow_mut();
        let was_complete = !server.is_handshaking();
        if was_complete {
            if self.block_write.replace(false) {
                return Err(io::ErrorKind::WouldBlock.into());
            }
            if self.fail_write.get() {
                return Err(io::ErrorKind::BrokenPipe.into());
            }
            if self.zero_write.get() {
                return Ok(0);
            }
        }
        let count = bytes.len().min(self.max_io.get());
        let read = server.read_tls(&mut io::Cursor::new(&bytes[..count]))?;
        server
            .process_new_packets()
            .map_err(|_| io::ErrorKind::InvalidData)?;
        let mut plain = zeroize::Zeroizing::new([0; 184]);
        loop {
            match server.reader().read(&mut *plain) {
                Ok(0) => break,
                Ok(count) => self.peer(socket, &mut server, &plain[..count]),
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => break,
                Err(error) => return Err(error),
            }
        }
        while server.wants_write() {
            let mut records = Vec::new();
            server.write_tls(&mut records)?;
            if !self.seen.borrow().is_empty() && self.corrupt_response.get() {
                if let Some(last) = records.last_mut() {
                    *last ^= 1;
                }
            }
            socket.inbound.borrow_mut().extend(records);
        }
        Ok(read)
    }
    fn poll(&self, _: &Socket, _: TlsInterest, timeout: i32) -> io::Result<PollProgress> {
        assert!((1..=50).contains(&timeout));
        self.polls.borrow_mut().push(timeout);
        self.time
            .set(self.time.get() + Duration::from_millis(timeout as u64));
        if self.cancel_on_poll.get() {
            self.cancellation.borrow().as_ref().unwrap().cancel();
        }
        if self.interrupt_poll.replace(false) {
            return Err(io::ErrorKind::Interrupted.into());
        }
        Ok(PollProgress::Ready)
    }
}
