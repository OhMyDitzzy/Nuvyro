use std::cell::{Cell, RefCell};
use std::collections::VecDeque;
use std::io::{self, Cursor};
use std::net::SocketAddr;
use std::rc::Rc;
use std::sync::Arc;
use std::time::{Duration, Instant};

use rustls::ServerConnection;

use super::super::{TlsHandshakeError, TlsInterest, TlsIo};
use super::fixtures::{client, client_configuration, identity, params, server_configuration};
use crate::transport::connector::{ConnectProgress, PollProgress, SocketOps, connect_with};
use crate::transport::{
    ConnectAttempt, ConnectCancellation, ConnectError, ConnectionContext, NumericEndpoint,
    budget::ConnectionBudget, tls,
};

struct FakeSocket {
    server: RefCell<ServerConnection>,
    inbound: RefCell<VecDeque<u8>>,
    server_complete: Rc<Cell<bool>>,
    drops: Rc<Cell<usize>>,
}

impl Drop for FakeSocket {
    fn drop(&mut self) {
        self.drops.set(self.drops.get() + 1);
    }
}

struct FakeIo {
    server_config: Arc<rustls::ServerConfig>,
    time: Cell<Instant>,
    server_complete: Rc<Cell<bool>>,
    drops: Rc<Cell<usize>>,
    cancellation: RefCell<Option<ConnectCancellation>>,
    cancel_on_write: Cell<bool>,
    cancel_on_poll: Cell<bool>,
    advance_on_verify: Cell<Duration>,
    advance_on_poll: Cell<bool>,
    withhold_server_flight: Cell<bool>,
    poll_windows: RefCell<Vec<i32>>,
}

impl FakeIo {
    fn new(server_config: rustls::ServerConfig) -> Self {
        Self {
            server_config: Arc::new(server_config),
            time: Cell::new(Instant::now()),
            server_complete: Rc::new(Cell::new(false)),
            drops: Rc::new(Cell::new(0)),
            cancellation: RefCell::new(None),
            cancel_on_write: Cell::new(false),
            cancel_on_poll: Cell::new(false),
            advance_on_verify: Cell::new(Duration::ZERO),
            advance_on_poll: Cell::new(false),
            withhold_server_flight: Cell::new(false),
            poll_windows: RefCell::new(Vec::new()),
        }
    }

    fn pending(
        &self,
        timeout: Duration,
    ) -> crate::transport::connector::PendingConnection<FakeSocket> {
        let attempt = ConnectAttempt::at(context(), endpoint(), timeout, self.time.get()).unwrap();
        *self.cancellation.borrow_mut() = Some(attempt.cancellation());
        connect_with(self, &ConnectionBudget::new(), attempt).unwrap()
    }
}

impl SocketOps for FakeIo {
    type Socket = FakeSocket;

    fn now(&self) -> Instant {
        self.time.get()
    }

    fn open(&self, _address: SocketAddr) -> io::Result<FakeSocket> {
        Ok(FakeSocket {
            server: RefCell::new(ServerConnection::new(Arc::clone(&self.server_config)).unwrap()),
            inbound: RefCell::default(),
            server_complete: Rc::clone(&self.server_complete),
            drops: Rc::clone(&self.drops),
        })
    }

    fn bind_network(&self, _socket: &FakeSocket, _network: u64) -> io::Result<()> {
        Ok(())
    }

    fn begin_connect(
        &self,
        _socket: &FakeSocket,
        _address: SocketAddr,
    ) -> io::Result<ConnectProgress> {
        Ok(ConnectProgress::Connected)
    }

    fn poll(&self, _socket: &FakeSocket, _window: Duration) -> io::Result<PollProgress> {
        Ok(PollProgress::Ready)
    }

    fn verify_peer(&self, _socket: &FakeSocket, _address: SocketAddr) -> io::Result<()> {
        self.time
            .set(self.time.get() + self.advance_on_verify.get());
        Ok(())
    }
}

impl TlsIo<FakeSocket> for FakeIo {
    fn now(&self) -> Instant {
        self.time.get()
    }

    fn read(&self, socket: &FakeSocket, bytes: &mut [u8]) -> io::Result<usize> {
        let mut inbound = socket.inbound.borrow_mut();
        if inbound.is_empty() {
            return Err(io::Error::from(io::ErrorKind::WouldBlock));
        }
        let count = bytes.len().min(inbound.len());
        for target in &mut bytes[..count] {
            *target = inbound
                .pop_front()
                .expect("count is bounded by queued bytes");
        }
        Ok(count)
    }

    fn write(&self, socket: &FakeSocket, bytes: &[u8]) -> io::Result<usize> {
        if self.cancel_on_write.get() {
            if let Some(cancellation) = self.cancellation.borrow().as_ref() {
                cancellation.cancel();
            }
        }
        let mut server = socket.server.borrow_mut();
        server.read_tls(&mut Cursor::new(bytes))?;
        server
            .process_new_packets()
            .map_err(|_| io::Error::from(io::ErrorKind::InvalidData))?;
        socket.server_complete.set(!server.is_handshaking());
        if !self.withhold_server_flight.get() {
            while server.wants_write() {
                let mut flight = Vec::new();
                server.write_tls(&mut flight)?;
                socket.inbound.borrow_mut().extend(flight);
            }
        }
        Ok(bytes.len())
    }

    fn poll(
        &self,
        socket: &FakeSocket,
        interest: TlsInterest,
        timeout_ms: i32,
    ) -> io::Result<PollProgress> {
        self.poll_windows.borrow_mut().push(timeout_ms);
        if self.cancel_on_poll.get() {
            if let Some(cancellation) = self.cancellation.borrow().as_ref() {
                cancellation.cancel();
            }
        }
        if self.advance_on_poll.get() {
            self.time
                .set(self.time.get() + Duration::from_millis(timeout_ms as u64));
        }
        Ok(match interest {
            TlsInterest::Write => PollProgress::Ready,
            TlsInterest::Read if !socket.inbound.borrow().is_empty() => PollProgress::Ready,
            TlsInterest::Read => PollProgress::Pending,
        })
    }
}

fn context() -> ConnectionContext {
    ConnectionContext::new(1, 2, 7, [0x11; 16]).unwrap()
}

fn stale_context() -> ConnectionContext {
    ConnectionContext::new(2, 2, 7, [0x11; 16]).unwrap()
}

fn endpoint() -> NumericEndpoint {
    NumericEndpoint::new(&[192, 168, 42, 3], 0, 47739).unwrap()
}

fn run(
    io: &FakeIo,
    pin: &[u8; 32],
    timeout: Duration,
) -> Result<tls::VerifiedTlsConnection<FakeSocket>, TlsHandshakeError> {
    let pending = io.pending(timeout);
    let client = client(client_configuration(pin));
    tls::test_handshake_with_client(pending, io, client, || Some(context()))
}

fn context_handshake(
    io: &FakeIo,
    pin: &[u8; 32],
    timeout: Duration,
    current_context: impl FnMut() -> Option<ConnectionContext>,
) -> Result<tls::VerifiedTlsConnection<FakeSocket>, TlsHandshakeError> {
    let pending = io.pending(timeout);
    let client = client(client_configuration(pin));
    tls::test_handshake_with_client(pending, io, client, current_context)
}

#[test]
fn driver_completes_only_the_pinned_tls_profile_and_keeps_the_socket_owned() {
    let identity = identity(params());
    let io = FakeIo::new(server_configuration(
        &identity,
        vec![nuvyro_transfer_core::protocol::TLS_ALPN.to_vec()],
    ));
    let verified = run(&io, &identity.pin, Duration::from_secs(1)).unwrap();
    assert!(io.server_complete.get());
    assert_eq!(io.drops.get(), 0);
    drop(verified);
    assert_eq!(io.drops.get(), 1);
}

#[test]
fn wrong_pin_fails_closed_and_drops_the_private_socket() {
    let identity = identity(params());
    let io = FakeIo::new(server_configuration(
        &identity,
        vec![nuvyro_transfer_core::protocol::TLS_ALPN.to_vec()],
    ));
    let mut wrong_pin = identity.pin;
    wrong_pin[0] ^= 1;
    assert!(matches!(
        run(&io, &wrong_pin, Duration::from_secs(1)),
        Err(TlsHandshakeError::ProtocolFailure)
    ));
    assert_eq!(io.drops.get(), 1);
}

#[test]
fn missing_alpn_fails_the_driver_after_tls_and_drops_the_socket() {
    let identity = identity(params());
    let io = FakeIo::new(server_configuration(&identity, vec![]));
    assert!(matches!(
        run(&io, &identity.pin, Duration::from_secs(1)),
        Err(TlsHandshakeError::Policy(
            super::super::TlsPolicyError::AlpnMismatch
        ))
    ));
    assert_eq!(io.drops.get(), 1);
}

#[test]
fn cancellation_during_socket_io_closes_without_accepting_the_handshake() {
    let identity = identity(params());
    let io = FakeIo::new(server_configuration(
        &identity,
        vec![nuvyro_transfer_core::protocol::TLS_ALPN.to_vec()],
    ));
    io.cancel_on_write.set(true);
    assert!(matches!(
        run(&io, &identity.pin, Duration::from_secs(1)),
        Err(TlsHandshakeError::Context(ConnectError::Cancelled))
    ));
    assert_eq!(io.drops.get(), 1);
}

#[test]
fn a_generation_change_during_socket_io_closes_the_attempt() {
    let identity = identity(params());
    let io = FakeIo::new(server_configuration(
        &identity,
        vec![nuvyro_transfer_core::protocol::TLS_ALPN.to_vec()],
    ));
    let checks = Cell::new(0);
    let result = context_handshake(&io, &identity.pin, Duration::from_secs(1), || {
        let count = checks.get() + 1;
        checks.set(count);
        Some(if count < 4 {
            context()
        } else {
            stale_context()
        })
    });
    assert!(matches!(
        result,
        Err(TlsHandshakeError::Context(ConnectError::StaleContext))
    ));
    assert_eq!(checks.get(), 4);
    assert_eq!(io.drops.get(), 1);
}

#[test]
fn the_connect_deadline_also_bounds_tls_and_poll_windows_shrink() {
    let identity = identity(params());
    let io = FakeIo::new(server_configuration(
        &identity,
        vec![nuvyro_transfer_core::protocol::TLS_ALPN.to_vec()],
    ));
    io.withhold_server_flight.set(true);
    io.advance_on_poll.set(true);
    io.advance_on_verify.set(Duration::from_millis(75));
    assert!(matches!(
        run(&io, &identity.pin, Duration::from_millis(125)),
        Err(TlsHandshakeError::Context(ConnectError::DeadlineExceeded))
    ));
    assert_eq!(*io.poll_windows.borrow(), [50]);
    assert_eq!(io.drops.get(), 1);
}

#[test]
fn cancellation_after_a_poll_wakes_the_owner_within_the_poll_slice() {
    let identity = identity(params());
    let io = FakeIo::new(server_configuration(
        &identity,
        vec![nuvyro_transfer_core::protocol::TLS_ALPN.to_vec()],
    ));
    io.withhold_server_flight.set(true);
    io.cancel_on_poll.set(true);
    let pending = io.pending(Duration::from_secs(1));
    let cancellation = io.cancellation.borrow().as_ref().unwrap().clone();
    let result = tls::test_handshake_with_client(
        pending,
        &io,
        client(client_configuration(&identity.pin)),
        || Some(context()),
    );
    assert!(matches!(
        result,
        Err(TlsHandshakeError::Context(ConnectError::Cancelled))
    ));
    assert_eq!(io.poll_windows.borrow().len(), 1);
    assert_eq!(io.drops.get(), 1);
    assert!(cancellation.is_cancelled());
}
