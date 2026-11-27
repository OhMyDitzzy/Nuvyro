use std::cell::{Cell, RefCell};
use std::collections::VecDeque;
use std::io::{self, Cursor};
use std::rc::Rc;
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime};

use nuvyro_transfer_core::pairing::PairingQr;
use rustls::ClientConnection;

use crate::pairing::ReceiverIdentity;
use crate::transport::connector::PollProgress;
use crate::transport::tls::client::client_config;
use crate::transport::tls::server::{ReceiverTlsIo, VerifiedIncoming, handshake_incoming_with};
use crate::transport::tls::{IncomingTlsError, TlsInterest, TlsIo};
use crate::transport::{ConnectCancellation, ReceiverAdmission, ReceiverContext};

#[derive(Clone, Copy, PartialEq)]
pub(super) enum Operation {
    Read,
    Write,
    Poll,
}

#[derive(Clone, Copy)]
pub(super) enum Event {
    Cancel,
    CloseIdentity,
    CloseAdmission,
    StaleNative,
    StaleRoute,
    MissingContext,
    BackwardClock,
    Deadline,
    ExpiredWall,
    EarlierWall,
}

#[derive(Clone, Copy, Default)]
pub(super) enum Fault {
    #[default]
    None,
    InterruptedOnce,
    AlwaysInterrupted,
    WouldBlockOnce,
    AlwaysBlock,
    Error,
    Zero,
}

pub(super) struct Socket(Rc<Cell<usize>>);

impl Drop for Socket {
    fn drop(&mut self) {
        self.0.set(self.0.get() + 1);
    }
}

pub(super) struct Wire<'a> {
    pub(super) identity: &'a ReceiverIdentity,
    pub(super) admission: &'a ReceiverAdmission,
    pub(super) context: Cell<Option<ReceiverContext>>,
    pub(super) time: Cell<Instant>,
    wall: Cell<SystemTime>,
    pub(super) cancellation: ConnectCancellation,
    pub(super) drops: Rc<Cell<usize>>,
    pub(super) reads: Cell<usize>,
    pub(super) writes: Cell<usize>,
    pub(super) windows: RefCell<Vec<(TlsInterest, i32)>>,
    pub(super) fragment: Cell<usize>,
    pub(super) read_fault: Cell<Fault>,
    pub(super) write_fault: Cell<Fault>,
    pub(super) poll_fault: Cell<Fault>,
    pub(super) hook: Cell<Option<(Operation, Event)>>,
    pub(super) step: Cell<Duration>,
    pub(super) stall_finished: Cell<bool>,
    pub(super) corrupt_finished: Cell<bool>,
    pub(super) peer_complete: Cell<bool>,
    pub(super) client: RefCell<ClientConnection>,
    pub(super) qr: RefCell<Option<PairingQr>>,
    pub(super) inbound: RefCell<VecDeque<u8>>,
    saw_server: Cell<bool>,
}

impl<'a> Wire<'a> {
    pub(super) fn new(identity: &'a ReceiverIdentity, admission: &'a ReceiverAdmission) -> Self {
        let (_, qr) = identity.create_invitation().unwrap();
        let qr = PairingQr::decode(qr.as_bytes()).unwrap();
        Self::from_qr(identity, admission, qr)
    }

    pub(super) fn from_qr(
        identity: &'a ReceiverIdentity,
        admission: &'a ReceiverAdmission,
        qr: PairingQr,
    ) -> Self {
        let config = client_config(qr.spki_pin()).unwrap();
        let client = ClientConnection::new(config, "nuvyro.local".try_into().unwrap()).unwrap();
        Self {
            identity,
            admission,
            context: Cell::new(Some(
                ReceiverContext::new(1, 2, *identity.session().as_bytes()).unwrap(),
            )),
            time: Cell::new(Instant::now()),
            wall: Cell::new(SystemTime::now()),
            cancellation: ConnectCancellation::new(),
            drops: Rc::new(Cell::new(0)),
            reads: Cell::new(0),
            writes: Cell::new(0),
            windows: RefCell::default(),
            fragment: Cell::new(usize::MAX),
            read_fault: Cell::new(Fault::None),
            write_fault: Cell::new(Fault::None),
            poll_fault: Cell::new(Fault::None),
            hook: Cell::new(None),
            step: Cell::new(Duration::ZERO),
            stall_finished: Cell::new(false),
            corrupt_finished: Cell::new(false),
            peer_complete: Cell::new(false),
            client: RefCell::new(client),
            qr: RefCell::new(Some(qr)),
            inbound: RefCell::default(),
            saw_server: Cell::new(false),
        }
    }

    pub(super) fn set_alpn(&self, protocols: Vec<Vec<u8>>) {
        let (_, qr) = self.identity.create_invitation().unwrap();
        let qr = PairingQr::decode(qr.as_bytes()).unwrap();
        let mut config = client_config(qr.spki_pin()).unwrap();
        Arc::make_mut(&mut config).alpn_protocols = protocols;
        *self.client.borrow_mut() =
            ClientConnection::new(config, "nuvyro.local".try_into().unwrap()).unwrap();
    }

    pub(super) fn run(&self) -> Result<VerifiedIncoming<Socket>, IncomingTlsError> {
        let pending = self
            .admission
            .admit(self.socket(), self.cancellation.clone(), || self.time.get())
            .unwrap();
        self.run_pending(pending)
    }

    pub(super) fn run_pending(
        &self,
        pending: crate::transport::PendingIncoming<Socket>,
    ) -> Result<VerifiedIncoming<Socket>, IncomingTlsError> {
        handshake_incoming_with(pending, self, self.identity, || self.context.get())
    }

    pub(super) fn socket(&self) -> Socket {
        Socket(Rc::clone(&self.drops))
    }

    fn observe(&self, operation: Operation) {
        self.time.set(self.time.get() + self.step.get());
        if let Some((target, event)) = self.hook.get() {
            if target != operation {
                return;
            }
            self.hook.set(None);
            match event {
                Event::Cancel => self.cancellation.cancel(),
                Event::CloseIdentity => self.identity.close(),
                Event::CloseAdmission => self.admission.close(),
                Event::StaleNative => self.context.set(Some(
                    ReceiverContext::new(2, 2, *self.identity.session().as_bytes()).unwrap(),
                )),
                Event::StaleRoute => self.context.set(Some(
                    ReceiverContext::new(1, 3, *self.identity.session().as_bytes()).unwrap(),
                )),
                Event::MissingContext => self.context.set(None),
                Event::BackwardClock => self.time.set(self.time.get() - Duration::from_secs(1)),
                Event::Deadline => self.time.set(self.time.get() + Duration::from_secs(10)),
                Event::ExpiredWall => self.wall.set(self.wall.get() + Duration::from_secs(86_401)),
                Event::EarlierWall => self.wall.set(self.wall.get() - Duration::from_secs(301)),
            }
        }
    }

    fn fault(fault: &Cell<Fault>) -> Option<io::Result<usize>> {
        match fault.get() {
            Fault::None => None,
            Fault::InterruptedOnce => {
                fault.set(Fault::None);
                Some(Err(io::ErrorKind::Interrupted.into()))
            }
            Fault::AlwaysInterrupted => Some(Err(io::ErrorKind::Interrupted.into())),
            Fault::WouldBlockOnce => {
                fault.set(Fault::None);
                Some(Err(io::ErrorKind::WouldBlock.into()))
            }
            Fault::AlwaysBlock => Some(Err(io::ErrorKind::WouldBlock.into())),
            Fault::Error => Some(Err(io::ErrorKind::ConnectionReset.into())),
            Fault::Zero => Some(Ok(0)),
        }
    }
}

impl ReceiverTlsIo<Socket> for Wire<'_> {
    fn wall_now(&self) -> SystemTime {
        self.wall.get()
    }
}

impl TlsIo<Socket> for Wire<'_> {
    fn now(&self) -> Instant {
        self.time.get()
    }

    fn read(&self, _socket: &Socket, bytes: &mut [u8]) -> io::Result<usize> {
        self.reads.set(self.reads.get() + 1);
        self.observe(Operation::Read);
        if let Some(result) = Self::fault(&self.read_fault) {
            return result;
        }
        if self.inbound.borrow().is_empty() {
            if self.stall_finished.get() && self.saw_server.get() {
                return Err(io::ErrorKind::WouldBlock.into());
            }
            let mut client = self.client.borrow_mut();
            while client.wants_write() {
                let mut flight = Vec::new();
                client.write_tls(&mut flight)?;
                if self.corrupt_finished.get() && self.saw_server.get() {
                    *flight.last_mut().unwrap() ^= 1;
                }
                self.inbound.borrow_mut().extend(flight);
            }
        }
        let mut inbound = self.inbound.borrow_mut();
        if inbound.is_empty() {
            return Err(io::ErrorKind::WouldBlock.into());
        }
        let count = bytes.len().min(inbound.len()).min(self.fragment.get());
        for target in &mut bytes[..count] {
            *target = inbound.pop_front().unwrap();
        }
        Ok(count)
    }

    fn write(&self, _socket: &Socket, bytes: &[u8]) -> io::Result<usize> {
        self.writes.set(self.writes.get() + 1);
        self.observe(Operation::Write);
        if let Some(result) = Self::fault(&self.write_fault) {
            return result;
        }
        let mut client = self.client.borrow_mut();
        let count = bytes.len().min(self.fragment.get());
        let consumed = client.read_tls(&mut Cursor::new(&bytes[..count]))?;
        client
            .process_new_packets()
            .map_err(|_| io::Error::from(io::ErrorKind::InvalidData))?;
        self.peer_complete.set(!client.is_handshaking());
        self.saw_server.set(true);
        Ok(consumed)
    }

    fn poll(
        &self,
        _socket: &Socket,
        interest: TlsInterest,
        timeout_ms: i32,
    ) -> io::Result<PollProgress> {
        assert!((1..=50).contains(&timeout_ms));
        self.windows.borrow_mut().push((interest, timeout_ms));
        self.observe(Operation::Poll);
        self.time
            .set(self.time.get() + Duration::from_millis(timeout_ms as u64));
        if let Some(result) = Self::fault(&self.poll_fault) {
            result?;
        }
        Ok(PollProgress::Ready)
    }
}
