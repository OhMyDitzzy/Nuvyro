use std::cell::{Cell, RefCell};
use std::collections::VecDeque;
use std::io;
use std::net::{Ipv6Addr, SocketAddr};
use std::rc::Rc;
use std::time::{Duration, Instant};

use super::attempt::{MAX_CONNECT_TIME, POLL_SLICE};
use super::budget::ConnectionBudget;
use super::connector::{ConnectProgress, PollProgress, SocketOps, connect_with, poll_timeout};
use super::{
    ConnectAttempt, ConnectCancellation, ConnectError, ConnectionContext, NumericEndpoint,
};

struct FakeSocket {
    log: Rc<RefCell<Vec<&'static str>>>,
    drops: Rc<Cell<usize>>,
}

impl Drop for FakeSocket {
    fn drop(&mut self) {
        self.log.borrow_mut().push("drop");
        self.drops.set(self.drops.get() + 1);
    }
}

struct FakeSockets {
    time: Cell<Instant>,
    log: Rc<RefCell<Vec<&'static str>>>,
    drops: Rc<Cell<usize>>,
    fail: Option<&'static str>,
    cancel_on: Option<&'static str>,
    advance_on: Option<(&'static str, Duration)>,
    token: RefCell<Option<ConnectCancellation>>,
    immediate: bool,
    polls: RefCell<VecDeque<(Duration, PollProgress)>>,
    windows: RefCell<Vec<Duration>>,
    network: Cell<u64>,
    address: Cell<Option<SocketAddr>>,
}

impl FakeSockets {
    fn new() -> Self {
        Self {
            time: Cell::new(Instant::now()),
            log: Rc::default(),
            drops: Rc::default(),
            fail: None,
            cancel_on: None,
            advance_on: None,
            token: RefCell::default(),
            immediate: true,
            polls: RefCell::default(),
            windows: RefCell::default(),
            network: Cell::new(0),
            address: Cell::new(None),
        }
    }

    fn attempt(&self, timeout: Duration) -> ConnectAttempt {
        let attempt = ConnectAttempt::at(context(), endpoint(), timeout, self.now()).unwrap();
        *self.token.borrow_mut() = Some(attempt.cancellation());
        attempt
    }

    fn stage(&self, name: &'static str) -> io::Result<()> {
        self.log.borrow_mut().push(name);
        if self.cancel_on == Some(name) {
            self.token.borrow().as_ref().unwrap().cancel();
        }
        if let Some((phase, duration)) = self.advance_on {
            if phase == name {
                self.time.set(self.time.get() + duration);
            }
        }
        if self.fail == Some(name) {
            Err(io::Error::from(io::ErrorKind::PermissionDenied))
        } else {
            Ok(())
        }
    }
}

impl SocketOps for FakeSockets {
    type Socket = FakeSocket;

    fn now(&self) -> Instant {
        self.time.get()
    }

    fn open(&self, _address: SocketAddr) -> io::Result<FakeSocket> {
        self.stage("open")?;
        Ok(FakeSocket {
            log: Rc::clone(&self.log),
            drops: Rc::clone(&self.drops),
        })
    }

    fn bind_network(&self, _socket: &FakeSocket, network: u64) -> io::Result<()> {
        self.network.set(network);
        self.stage("bind")
    }

    fn begin_connect(
        &self,
        _socket: &FakeSocket,
        address: SocketAddr,
    ) -> io::Result<ConnectProgress> {
        self.address.set(Some(address));
        self.stage("connect")?;
        Ok(if self.immediate {
            ConnectProgress::Connected
        } else {
            ConnectProgress::Pending
        })
    }

    fn poll(&self, _socket: &FakeSocket, window: Duration) -> io::Result<PollProgress> {
        assert!(!window.is_zero() && window <= POLL_SLICE);
        self.windows.borrow_mut().push(window);
        self.stage("poll")?;
        let (elapsed, progress) = self
            .polls
            .borrow_mut()
            .pop_front()
            .unwrap_or((window, PollProgress::Pending));
        self.time.set(self.time.get() + elapsed);
        Ok(progress)
    }

    fn verify_peer(&self, _socket: &FakeSocket, address: SocketAddr) -> io::Result<()> {
        assert_eq!(self.address.get(), Some(address));
        self.stage("verify")
    }
}

fn context() -> ConnectionContext {
    ConnectionContext::new(1, 2, (1_u64 << 63) | 5, [0x11; 16]).unwrap()
}

fn endpoint() -> NumericEndpoint {
    NumericEndpoint::new(&[192, 168, 42, 3], 0, 47739).unwrap()
}

#[test]
fn native_context_keeps_full_network_bits_and_distinct_generation_roles() {
    let value = context();
    assert_eq!(value.network_handle(), (1_u64 << 63) | 5);
    assert_eq!(value.expected_session().as_bytes(), &[0x11; 16]);
    for (native, route, network, session) in [
        (0, 2, 5, [1; 16]),
        (1, 0, 5, [1; 16]),
        (1, 2, 0, [1; 16]),
        (1, 2, 5, [0; 16]),
        (1_u64 << 63, 2, 5, [1; 16]),
        (1, 1_u64 << 63, 5, [1; 16]),
    ] {
        assert_eq!(
            ConnectionContext::new(native, route, network, session),
            Err(ConnectError::InvalidContext)
        );
    }
}

#[test]
fn endpoint_rejects_bad_lengths_ports_scopes_and_non_unicast_ipv4() {
    for bytes in [vec![], vec![1; 5], vec![1; 15], vec![1; 17]] {
        assert_eq!(
            NumericEndpoint::new(&bytes, 0, 47739),
            Err(ConnectError::InvalidEndpoint)
        );
    }
    for bytes in [
        [0, 0, 0, 0],
        [127, 0, 0, 1],
        [224, 0, 0, 1],
        [255, 255, 255, 255],
    ] {
        assert_eq!(
            NumericEndpoint::new(&bytes, 0, 47739),
            Err(ConnectError::InvalidEndpoint)
        );
    }
    assert_eq!(
        NumericEndpoint::new(&[192, 168, 42, 3], 0, 47738),
        Err(ConnectError::InvalidEndpoint)
    );
    assert_eq!(
        NumericEndpoint::new(&[192, 168, 42, 3], 7, 47739),
        Err(ConnectError::InvalidEndpoint)
    );
}

#[test]
fn ipv6_requires_exact_link_scope_and_rejects_mapped_compatible_and_multicast() {
    let ip = Ipv6Addr::new(0xfe80, 0, 0, 0, 0, 0, 0, 3);
    assert_eq!(
        NumericEndpoint::new(&ip.octets(), 0, 47739),
        Err(ConnectError::InvalidEndpoint)
    );
    assert_eq!(
        NumericEndpoint::new(&ip.octets(), u32::MAX, 47739),
        Err(ConnectError::InvalidEndpoint)
    );
    let endpoint = NumericEndpoint::new(&ip.octets(), 7, 47739).unwrap();
    let SocketAddr::V6(address) = endpoint.address() else {
        panic!("expected IPv6")
    };
    assert_eq!(address.scope_id(), 7);
    assert_eq!(address.flowinfo(), 0);
    for ip in [
        Ipv6Addr::UNSPECIFIED,
        Ipv6Addr::LOCALHOST,
        Ipv6Addr::new(0, 0, 0, 0, 0, 0xffff, 0xc0a8, 0x2a03),
        Ipv6Addr::new(0, 0, 0, 0, 0, 0, 0xc0a8, 0x2a03),
        Ipv6Addr::new(0xff02, 0, 0, 0, 0, 0, 0, 1),
    ] {
        assert_eq!(
            NumericEndpoint::new(&ip.octets(), 0, 47739),
            Err(ConnectError::InvalidEndpoint)
        );
    }
    let global = Ipv6Addr::new(0x2001, 0xdb8, 1, 0, 0, 0, 0, 3);
    assert!(NumericEndpoint::new(&global.octets(), 0, 47739).is_ok());
    assert_eq!(
        NumericEndpoint::new(&global.octets(), 7, 47739),
        Err(ConnectError::InvalidEndpoint)
    );
}

#[test]
fn timeout_and_poll_rounding_never_create_an_infinite_or_zero_wait() {
    let now = Instant::now();
    for timeout in [Duration::ZERO, MAX_CONNECT_TIME + Duration::from_nanos(1)] {
        assert!(matches!(
            ConnectAttempt::at(context(), endpoint(), timeout, now),
            Err(ConnectError::InvalidTimeout)
        ));
    }
    assert_eq!(poll_timeout(Duration::from_nanos(1)), Ok(1));
    assert_eq!(poll_timeout(Duration::from_nanos(1_000_001)), Ok(2));
    assert_eq!(poll_timeout(POLL_SLICE), Ok(50));
    assert_eq!(
        poll_timeout(Duration::ZERO),
        Err(ConnectError::InvalidTimeout)
    );
    assert_eq!(
        poll_timeout(POLL_SLICE + Duration::from_nanos(1)),
        Err(ConnectError::InvalidTimeout)
    );
}

#[test]
fn creation_binding_connect_and_peer_verification_are_in_that_order() {
    let ops = FakeSockets::new();
    let budget = ConnectionBudget::new();
    let mut socket = connect_with(&ops, &budget, ops.attempt(MAX_CONNECT_TIME)).unwrap();
    assert_eq!(*ops.log.borrow(), ["open", "bind", "connect", "verify"]);
    assert_eq!(ops.network.get(), context().network_handle());
    assert_eq!(socket.context(), context());
    assert_eq!(ops.drops.get(), 0);
    socket.cancel();
    socket.cancel();
    assert_eq!(ops.drops.get(), 1);
    drop(socket);
    assert_eq!(ops.drops.get(), 1);
}

#[test]
fn binding_failure_closes_once_without_connecting_or_retrying() {
    let mut ops = FakeSockets::new();
    ops.fail = Some("bind");
    let budget = ConnectionBudget::new();
    assert_eq!(
        connect_with(&ops, &budget, ops.attempt(MAX_CONNECT_TIME)).unwrap_err(),
        ConnectError::NetworkBinding
    );
    assert_eq!(*ops.log.borrow(), ["open", "bind", "drop"]);
    let permits: Vec<_> = (0..4).map(|_| budget.acquire().unwrap()).collect();
    assert!(matches!(budget.acquire(), Err(ConnectError::ResourceLimit)));
    drop(permits);
}

#[test]
fn failure_at_each_socket_stage_releases_owned_resources_without_fallback() {
    for (stage, expected, drops) in [
        ("open", ConnectError::SocketCreation, 0),
        ("connect", ConnectError::ConnectFailed, 1),
        ("poll", ConnectError::PollFailed, 1),
        ("verify", ConnectError::ConnectFailed, 1),
    ] {
        let mut ops = FakeSockets::new();
        ops.fail = Some(stage);
        ops.immediate = stage != "poll";
        let budget = ConnectionBudget::new();
        assert_eq!(
            connect_with(&ops, &budget, ops.attempt(MAX_CONNECT_TIME)).unwrap_err(),
            expected
        );
        assert_eq!(ops.drops.get(), drops, "{stage}");
        assert_eq!(
            ops.log
                .borrow()
                .iter()
                .filter(|value| **value == "open")
                .count(),
            1
        );
        let permits: Vec<_> = (0..4).map(|_| budget.acquire().unwrap()).collect();
        drop(permits);
    }
}

#[test]
fn cancellation_before_creation_opens_no_socket() {
    let ops = FakeSockets::new();
    let attempt = ops.attempt(MAX_CONNECT_TIME);
    attempt.cancellation().cancel();
    assert_eq!(
        connect_with(&ops, &ConnectionBudget::new(), attempt).unwrap_err(),
        ConnectError::Cancelled
    );
    assert!(ops.log.borrow().is_empty());
}

#[test]
fn cancellation_between_stages_never_advances_to_a_later_stage() {
    for (stage, forbidden) in [
        ("open", "bind"),
        ("bind", "connect"),
        ("connect", "verify"),
        ("verify", "poll"),
    ] {
        let mut ops = FakeSockets::new();
        ops.cancel_on = Some(stage);
        let result = connect_with(
            &ops,
            &ConnectionBudget::new(),
            ops.attempt(MAX_CONNECT_TIME),
        );
        assert_eq!(result.unwrap_err(), ConnectError::Cancelled, "{stage}");
        assert!(!ops.log.borrow().contains(&forbidden), "{stage}");
        assert_eq!(ops.drops.get(), 1);
    }
}

#[test]
fn pending_connect_checks_cancellation_before_accepting_writability() {
    let mut ops = FakeSockets::new();
    ops.immediate = false;
    ops.cancel_on = Some("poll");
    ops.polls
        .borrow_mut()
        .push_back((Duration::from_millis(1), PollProgress::Ready));
    assert_eq!(
        connect_with(
            &ops,
            &ConnectionBudget::new(),
            ops.attempt(MAX_CONNECT_TIME)
        )
        .unwrap_err(),
        ConnectError::Cancelled
    );
    assert!(!ops.log.borrow().contains(&"verify"));
    assert_eq!(ops.drops.get(), 1);
}

#[test]
fn absolute_deadline_covers_creation_bind_connect_and_peer_checks() {
    let timeout = Duration::from_millis(100);
    for stage in ["open", "bind", "connect", "verify"] {
        let mut ops = FakeSockets::new();
        ops.advance_on = Some((stage, timeout));
        assert_eq!(
            connect_with(&ops, &ConnectionBudget::new(), ops.attempt(timeout)).unwrap_err(),
            ConnectError::DeadlineExceeded,
            "{stage}"
        );
        assert_eq!(ops.drops.get(), 1);
    }
}

#[test]
fn pending_poll_loops_do_not_extend_the_deadline() {
    let mut ops = FakeSockets::new();
    ops.immediate = false;
    assert_eq!(
        connect_with(
            &ops,
            &ConnectionBudget::new(),
            ops.attempt(Duration::from_millis(125))
        )
        .unwrap_err(),
        ConnectError::DeadlineExceeded
    );
    assert_eq!(
        *ops.windows.borrow(),
        [
            Duration::from_millis(50),
            Duration::from_millis(50),
            Duration::from_millis(25),
        ]
    );
    assert_eq!(ops.drops.get(), 1);
}

#[test]
fn readiness_at_the_deadline_is_rejected_even_when_peer_would_be_valid() {
    let mut ops = FakeSockets::new();
    ops.immediate = false;
    ops.polls
        .borrow_mut()
        .push_back((Duration::from_millis(50), PollProgress::Ready));
    assert_eq!(
        connect_with(
            &ops,
            &ConnectionBudget::new(),
            ops.attempt(Duration::from_millis(50))
        )
        .unwrap_err(),
        ConnectError::DeadlineExceeded
    );
    assert!(!ops.log.borrow().contains(&"verify"));
}

#[test]
fn a_clock_before_attempt_start_cannot_create_a_socket() {
    let ops = FakeSockets::new();
    let attempt = ops.attempt(MAX_CONNECT_TIME);
    ops.time.set(ops.time.get() - Duration::from_millis(1));
    assert_eq!(
        connect_with(&ops, &ConnectionBudget::new(), attempt).unwrap_err(),
        ConnectError::ClockInvalid
    );
    assert!(ops.log.borrow().is_empty());
}

#[test]
fn pending_connect_verifies_peer_after_readiness_and_preserves_ipv6_scope() {
    let mut ops = FakeSockets::new();
    ops.immediate = false;
    ops.polls.borrow_mut().extend([
        (Duration::from_millis(10), PollProgress::Pending),
        (Duration::from_millis(5), PollProgress::Ready),
    ]);
    let ip = Ipv6Addr::new(0xfe80, 0, 0, 0, 0, 0, 0, 3);
    let endpoint = NumericEndpoint::new(&ip.octets(), 7, 47739).unwrap();
    let attempt = ConnectAttempt::at(context(), endpoint, MAX_CONNECT_TIME, ops.now()).unwrap();
    let socket = connect_with(&ops, &ConnectionBudget::new(), attempt).unwrap();
    assert_eq!(
        *ops.log.borrow(),
        ["open", "bind", "connect", "poll", "poll", "verify"]
    );
    assert_eq!(ops.address.get(), Some(endpoint.address()));
    drop(socket);
    assert_eq!(ops.drops.get(), 1);
}

#[test]
fn stale_native_route_network_or_session_context_closes_the_pending_socket() {
    for expected in [
        ConnectionContext::new(2, 2, context().network_handle(), [0x11; 16]).unwrap(),
        ConnectionContext::new(1, 3, context().network_handle(), [0x11; 16]).unwrap(),
        ConnectionContext::new(1, 2, 9, [0x11; 16]).unwrap(),
        ConnectionContext::new(1, 2, context().network_handle(), [0x22; 16]).unwrap(),
    ] {
        let ops = FakeSockets::new();
        let mut socket = connect_with(
            &ops,
            &ConnectionBudget::new(),
            ops.attempt(MAX_CONNECT_TIME),
        )
        .unwrap();
        assert_eq!(socket.check_context(context()), Ok(()));
        assert_eq!(
            socket.check_context(expected),
            Err(ConnectError::StaleContext)
        );
        assert_eq!(ops.drops.get(), 1);
        assert_eq!(
            socket.check_context(context()),
            Err(ConnectError::Cancelled)
        );
    }
}

#[test]
fn a_previous_cancellation_handle_cannot_cancel_a_new_attempt() {
    let ops = FakeSockets::new();
    let first = ops.attempt(MAX_CONNECT_TIME);
    let old = first.cancellation();
    let second = ops.attempt(MAX_CONNECT_TIME);
    old.cancel();
    old.cancel();
    assert!(old.is_cancelled());
    assert!(!second.cancellation().is_cancelled());
    assert_eq!(
        connect_with(&ops, &ConnectionBudget::new(), first).unwrap_err(),
        ConnectError::Cancelled
    );
    let socket = connect_with(&ops, &ConnectionBudget::new(), second).unwrap();
    drop(socket);
}

#[test]
fn connected_cancellation_is_observed_by_the_owner_without_sharing_its_fd() {
    let ops = FakeSockets::new();
    let attempt = ops.attempt(MAX_CONNECT_TIME);
    let token = attempt.cancellation();
    let mut socket = connect_with(&ops, &ConnectionBudget::new(), attempt).unwrap();
    token.cancel();
    assert_eq!(ops.drops.get(), 0);
    assert_eq!(
        socket.check_context(context()),
        Err(ConnectError::Cancelled)
    );
    assert_eq!(ops.drops.get(), 1);
}

#[test]
fn resource_limit_holds_until_socket_close_and_does_not_allocate_a_fifth_socket() {
    let ops = FakeSockets::new();
    let budget = ConnectionBudget::new();
    let mut sockets: Vec<_> = (0..4)
        .map(|_| connect_with(&ops, &budget, ops.attempt(MAX_CONNECT_TIME)).unwrap())
        .collect();
    assert_eq!(
        connect_with(&ops, &budget, ops.attempt(MAX_CONNECT_TIME)).unwrap_err(),
        ConnectError::ResourceLimit
    );
    assert_eq!(
        ops.log
            .borrow()
            .iter()
            .filter(|value| **value == "open")
            .count(),
        4
    );
    sockets[0].cancel();
    sockets[0].cancel();
    let replacement = connect_with(&ops, &budget, ops.attempt(MAX_CONNECT_TIME)).unwrap();
    assert_eq!(ops.drops.get(), 1);
    drop(sockets);
    drop(replacement);
    assert_eq!(ops.drops.get(), 5);
}
