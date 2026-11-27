use std::cell::{Cell, RefCell};
use std::io::{self, Read, Write};
use std::time::{Duration, Instant, SystemTime};

use nuvyro_transfer_core::pairing::PairingQr;
use nuvyro_transfer_core::protocol::{
    EncodedFrame, ErrorCode, ErrorScope, Hello, Message, MessageKind, StreamDecoder, encode_message,
};
use zeroize::Zeroizing;

use super::super::wire::{Event, Fault, Operation, Socket, Wire};
use crate::pairing::{HandshakeAction, HandshakeGate, PairContext, ReceiverIdentity};
use crate::transport::ReceiverAdmission;
use crate::transport::connector::PollProgress;
use crate::transport::tls::incoming_pairing::pair_incoming_with;
use crate::transport::tls::{IncomingPairError, PairedIncoming, ReceiverTlsIo, TlsInterest, TlsIo};

#[derive(Clone, Copy, Eq, PartialEq)]
pub(super) enum Behavior {
    Normal,
    SilentHello,
    SilentAuth,
    WrongHello,
    WrongToken,
    WrongInvitation,
    AuthFirst,
    DuplicateHello,
    ReverseAck,
    DataHeader,
    OversizedHeader,
    PipelinedAuth,
    TrailingHello,
    TrailingAuth,
    BadName,
    RemoteError,
    CloseHello,
    CloseAuth,
    CorruptAuth,
    TruncatedAuth,
}

pub(super) struct Peer<'a> {
    pub(super) tls: Wire<'a>,
    gate: RefCell<Option<HandshakeGate>>,
    decoder: RefCell<StreamDecoder>,
    started: Cell<bool>,
    awaiting_sent: Cell<bool>,
    auth: EncodedFrame,
    pub(super) behavior: Cell<Behavior>,
    pub(super) seen: RefCell<Vec<MessageKind>>,
    pub(super) established: Cell<Option<PairContext>>,
    pub(super) delay_ack: Cell<Duration>,
    pub(super) delay_auth: Cell<Duration>,
    pub(super) after_ack: Cell<Option<(Operation, Event)>>,
    pub(super) fault_after_ack: Cell<Option<(Operation, Fault)>>,
    pub(super) cancel_after_result: Cell<bool>,
    pub(super) step_after_ack: Cell<Duration>,
    pub(super) claimed_name: RefCell<String>,
}

impl<'a> Peer<'a> {
    pub(super) fn new(identity: &'a ReceiverIdentity, admission: &'a ReceiverAdmission) -> Self {
        let (_, qr) = identity.create_invitation().unwrap();
        Self::from_qr(
            identity,
            admission,
            PairingQr::decode(qr.as_bytes()).unwrap(),
        )
    }

    pub(super) fn from_qr(
        identity: &'a ReceiverIdentity,
        admission: &'a ReceiverAdmission,
        qr: PairingQr,
    ) -> Self {
        let auth = encode_message(&Message::PairAuthenticate {
            invitation_id: qr.invitation_id(),
            token: qr.token().expose_secret(),
            claimed_sender_name: "Sender",
        })
        .unwrap();
        Self {
            tls: Wire::from_qr(identity, admission, qr),
            gate: RefCell::new(None),
            decoder: RefCell::default(),
            started: Cell::new(false),
            awaiting_sent: Cell::new(false),
            auth,
            behavior: Cell::new(Behavior::Normal),
            seen: RefCell::default(),
            established: Cell::new(None),
            delay_ack: Cell::new(Duration::ZERO),
            delay_auth: Cell::new(Duration::ZERO),
            after_ack: Cell::new(None),
            fault_after_ack: Cell::new(None),
            cancel_after_result: Cell::new(false),
            step_after_ack: Cell::new(Duration::ZERO),
            claimed_name: RefCell::new("Sender".to_owned()),
        }
    }

    pub(super) fn run(&self) -> Result<PairedIncoming<Socket>, IncomingPairError> {
        let pending = self
            .tls
            .admission
            .admit(self.tls.socket(), self.tls.cancellation.clone(), || {
                self.tls.time.get()
            })
            .unwrap();
        pair_incoming_with(pending, self, self.tls.identity, || self.tls.context.get())
    }

    fn start(&self) {
        if !self.tls.peer_complete.get() || self.started.replace(true) {
            return;
        }
        if self.behavior.get() == Behavior::SilentHello {
            return;
        }
        let qr = self.tls.qr.borrow_mut().take().unwrap();
        let mut gate =
            HandshakeGate::outgoing(qr, self.claimed_name.borrow().clone(), Instant::now())
                .unwrap();
        let HandshakeAction::Send(frame) = gate.initial_message(Instant::now()).unwrap() else {
            panic!("Hello required");
        };
        *self.gate.borrow_mut() = Some(gate);
        self.queue(&frame, MessageKind::SessionHello);
    }

    fn queue(&self, frame: &EncodedFrame, kind: MessageKind) {
        let behavior = self.behavior.get();
        if kind == MessageKind::PairAuthenticate && behavior == Behavior::SilentAuth {
            return;
        }
        if (kind == MessageKind::SessionHello && behavior == Behavior::CloseHello)
            || (kind == MessageKind::PairAuthenticate && behavior == Behavior::CloseAuth)
        {
            self.tls.client.borrow_mut().send_close_notify();
            return;
        }
        let mut bytes = Zeroizing::new(frame.as_slice().to_vec());
        if kind == MessageKind::SessionHello {
            match behavior {
                Behavior::WrongHello => bytes[10] ^= 1,
                Behavior::AuthFirst => {
                    bytes.clear();
                    bytes.extend_from_slice(self.auth.as_slice());
                }
                Behavior::ReverseAck => {
                    bytes.clear();
                    bytes.extend_from_slice(
                        encode_message(&Message::SessionHelloAck(Hello {
                            secure_session_id: self.tls.identity.session(),
                        }))
                        .unwrap()
                        .as_slice(),
                    );
                }
                Behavior::DataHeader => {
                    bytes.clear();
                    bytes.extend_from_slice(&[0, 4, 0, 58, 2, 2]);
                }
                Behavior::OversizedHeader => {
                    bytes.clear();
                    bytes.extend_from_slice(&[0xff, 0xff, 0xff, 0xff, 0, 1]);
                }
                Behavior::PipelinedAuth => bytes.extend_from_slice(self.auth.as_slice()),
                Behavior::TrailingHello => bytes.push(0x99),
                Behavior::RemoteError => {
                    bytes.clear();
                    bytes.extend_from_slice(
                        encode_message(&Message::Error {
                            pair_id: None,
                            transfer_id: None,
                            code: ErrorCode::AuthenticationFailed,
                            scope: ErrorScope::Connection,
                        })
                        .unwrap()
                        .as_slice(),
                    );
                }
                _ => {}
            }
        } else {
            match behavior {
                Behavior::WrongToken => bytes[22] ^= 1,
                Behavior::WrongInvitation => bytes[6] ^= 1,
                Behavior::DuplicateHello => {
                    bytes.clear();
                    bytes.extend_from_slice(
                        encode_message(&Message::SessionHello(Hello {
                            secure_session_id: self.tls.identity.session(),
                        }))
                        .unwrap()
                        .as_slice(),
                    );
                }
                Behavior::TrailingAuth => bytes.push(0x99),
                Behavior::BadName => *bytes.last_mut().unwrap() = b'\n',
                Behavior::CorruptAuth => self.tls.corrupt_finished.set(true),
                Behavior::TruncatedAuth => bytes.truncate(20),
                _ => {}
            }
        }
        self.tls
            .client
            .borrow_mut()
            .writer()
            .write_all(&bytes)
            .unwrap();
        self.awaiting_sent.set(true);
    }

    fn sent(&self) {
        if self.awaiting_sent.get()
            && !self.tls.client.borrow().wants_write()
            && self.tls.inbound.borrow().is_empty()
        {
            self.awaiting_sent.set(false);
            assert!(
                self.gate
                    .borrow_mut()
                    .as_mut()
                    .unwrap()
                    .sent(Instant::now())
                    .unwrap()
                    .is_none()
            );
        }
    }

    fn receive(&self, message: Message<'_>) {
        let kind = message.kind();
        self.seen.borrow_mut().push(kind);
        let action = self
            .gate
            .borrow_mut()
            .as_mut()
            .unwrap()
            .receive(message, Instant::now())
            .unwrap();
        match action {
            HandshakeAction::Send(frame) => {
                assert_eq!(kind, MessageKind::SessionHelloAck);
                self.tls.time.set(self.now() + self.delay_ack.get());
                self.queue(&frame, MessageKind::PairAuthenticate);
                self.tls.hook.set(self.after_ack.get());
                if let Some((operation, fault)) = self.fault_after_ack.get() {
                    match operation {
                        Operation::Read => self.tls.read_fault.set(fault),
                        Operation::Write => self.tls.write_fault.set(fault),
                        Operation::Poll => {
                            self.tls
                                .read_fault
                                .set(if matches!(fault, Fault::InterruptedOnce) {
                                    Fault::WouldBlockOnce
                                } else {
                                    Fault::AlwaysBlock
                                });
                            self.tls.poll_fault.set(fault);
                        }
                    }
                }
                self.tls.step.set(self.step_after_ack.get());
            }
            HandshakeAction::Established(pair) => {
                assert_eq!(kind, MessageKind::PairResult);
                self.established.set(Some(pair));
                if self.cancel_after_result.get() {
                    self.tls.cancellation.cancel();
                }
            }
        }
    }
}

impl ReceiverTlsIo<Socket> for Peer<'_> {
    fn wall_now(&self) -> SystemTime {
        self.tls.wall_now()
    }
}

impl TlsIo<Socket> for Peer<'_> {
    fn now(&self) -> Instant {
        self.tls.now()
    }

    fn read(&self, socket: &Socket, bytes: &mut [u8]) -> io::Result<usize> {
        self.start();
        if !self.seen.borrow().is_empty() {
            self.tls
                .time
                .set(self.now() + self.delay_auth.replace(Duration::ZERO));
        }
        let result = self.tls.read(socket, bytes);
        self.sent();
        if self.behavior.get() == Behavior::TruncatedAuth
            && !self.seen.borrow().is_empty()
            && result
                .as_ref()
                .is_err_and(|e| e.kind() == io::ErrorKind::WouldBlock)
        {
            return Ok(0);
        }
        result
    }

    fn write(&self, socket: &Socket, bytes: &[u8]) -> io::Result<usize> {
        let result = self.tls.write(socket, bytes)?;
        let mut plain = Zeroizing::new([0; 184]);
        loop {
            let received = self.tls.client.borrow_mut().reader().read(&mut *plain);
            match received {
                Ok(0) => break,
                Ok(count) => {
                    let mut used = 0;
                    while used < count {
                        let output = self.decoder.borrow_mut().push(&plain[used..count]).unwrap();
                        used += output.consumed;
                        if let Some(frame) = output.frame {
                            self.receive(frame.decode().unwrap());
                        }
                    }
                }
                Err(e) if e.kind() == io::ErrorKind::WouldBlock => break,
                Err(e) => return Err(e),
            }
        }
        Ok(result)
    }

    fn poll(
        &self,
        socket: &Socket,
        interest: TlsInterest,
        timeout: i32,
    ) -> io::Result<PollProgress> {
        self.tls.poll(socket, interest, timeout)
    }
}
