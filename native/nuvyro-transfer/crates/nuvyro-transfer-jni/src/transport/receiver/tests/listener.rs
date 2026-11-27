use std::cell::{Cell, RefCell};
use std::io;
use std::net::SocketAddr;
use std::rc::Rc;
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime};

use crate::pairing::ReceiverIdentity;
use crate::transport::connector::PollProgress;
use crate::transport::receiver::listener::ListenerIo;
use crate::transport::receiver::{
    AcceptOutcome, AcceptedReceiver, ListenerError, ReceiverListener, ReceiverRoute,
    ReceiverRoutes, RejectReason,
};
use crate::transport::tls::{ReceiverTlsIo, TlsInterest, TlsIo};
use crate::transport::{ConnectCancellation, ReceiverContext, ReceiverError};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Step {
    Open,
    Network,
    Bind,
    Listen,
    Local,
    Poll,
    Accept,
    Configure,
    Addresses,
}
#[derive(Clone, Copy)]
enum Event {
    Cancel,
    Stale,
    Backward,
    Expire,
    Timeout,
}
struct Bound {
    local: Cell<SocketAddr>,
    drops: Rc<Cell<usize>>,
}
impl Drop for Bound {
    fn drop(&mut self) {
        self.drops.set(self.drops.get() + 1);
    }
}
struct Socket {
    drops: Rc<Cell<usize>>,
}
impl Drop for Socket {
    fn drop(&mut self) {
        self.drops.set(self.drops.get() + 1);
    }
}

struct Io {
    fail_second_bind: Cell<bool>,
    identity: Arc<ReceiverIdentity>,
    routes: RefCell<Option<ReceiverRoutes>>,
    cancel: ConnectCancellation,
    time: Cell<Instant>,
    wall: Cell<SystemTime>,
    trace: RefCell<Vec<Step>>,
    networks: RefCell<Vec<u64>>,
    fail: Cell<Option<(Step, io::ErrorKind)>>,
    hook: Cell<Option<(Step, Event)>>,
    bound_drops: Rc<Cell<usize>>,
    socket_drops: Rc<Cell<usize>>,
    pending: Cell<bool>,
    local: Cell<SocketAddr>,
    peer: Cell<SocketAddr>,
    reported: Cell<SocketAddr>,
    wrong_bound: Cell<bool>,
    tls_calls: Cell<usize>,
}

impl Io {
    fn new(hotspot: bool, dual: bool) -> Self {
        let identity = Arc::new(ReceiverIdentity::create().unwrap());
        let ctx = ReceiverContext::new(1, 2, *identity.session().as_bytes()).unwrap();
        let mut locals = vec![ReceiverRoute::new(&[192, 168, 42, 1], 0, 24).unwrap()];
        if dual {
            locals.push(
                ReceiverRoute::new(
                    &"2001:db8::1"
                        .parse::<std::net::Ipv6Addr>()
                        .unwrap()
                        .octets(),
                    0,
                    64,
                )
                .unwrap(),
            );
        }
        let routes = if hotspot {
            ReceiverRoutes::hotspot(ctx, 7, &locals, &[], &[])
        } else {
            ReceiverRoutes::wifi(ctx, u64::MAX, 7, &locals)
        }
        .unwrap();
        Self {
            fail_second_bind: Cell::new(false),
            identity,
            routes: RefCell::new(Some(routes)),
            cancel: ConnectCancellation::new(),
            time: Cell::new(Instant::now()),
            wall: Cell::new(SystemTime::now()),
            trace: RefCell::default(),
            networks: RefCell::default(),
            fail: Cell::new(None),
            hook: Cell::new(None),
            bound_drops: Rc::new(Cell::new(0)),
            socket_drops: Rc::new(Cell::new(0)),
            pending: Cell::new(false),
            local: Cell::new("192.168.42.1:47739".parse().unwrap()),
            peer: Cell::new("192.168.42.3:12345".parse().unwrap()),
            reported: Cell::new("192.168.42.3:12345".parse().unwrap()),
            wrong_bound: Cell::new(false),
            tls_calls: Cell::new(0),
        }
    }
    fn current(&self) -> Option<ReceiverRoutes> {
        self.routes.borrow().clone()
    }
    fn open_owner(&self) -> Result<ReceiverListener<Bound>, ListenerError> {
        ReceiverListener::open_with(
            self,
            Arc::clone(&self.identity),
            self.current().unwrap(),
            self.cancel.clone(),
            || self.current(),
        )
    }
    fn accepted(&self, owner: &mut ReceiverListener<Bound>) -> AcceptedReceiver<Socket> {
        match owner.accept_with(self, || self.current()).unwrap() {
            AcceptOutcome::Accepted(value) => value,
            _ => panic!("accepted connection required"),
        }
    }
    fn hit(&self, step: Step) -> io::Result<()> {
        self.trace.borrow_mut().push(step);
        if step == Step::Bind
            && self.fail_second_bind.get()
            && self
                .trace
                .borrow()
                .iter()
                .filter(|call| **call == Step::Bind)
                .count()
                == 2
        {
            return Err(io::ErrorKind::AddrInUse.into());
        }
        if let Some((target, event)) = self.hook.get() {
            if target == step {
                self.hook.set(None);
                match event {
                    Event::Cancel => self.cancel.cancel(),
                    Event::Stale => *self.routes.borrow_mut() = None,
                    Event::Backward => self.time.set(self.time.get() - Duration::from_secs(1)),
                    Event::Expire => self.wall.set(self.wall.get() + Duration::from_secs(86401)),
                    Event::Timeout => self.time.set(self.time.get() + Duration::from_secs(10)),
                }
            }
        }
        if let Some((target, kind)) = self.fail.get() {
            if target == step {
                return Err(kind.into());
            }
        }
        Ok(())
    }
}

impl ListenerIo for Io {
    type Listener = Bound;
    type Socket = Socket;
    fn now(&self) -> Instant {
        self.time.get()
    }
    fn wall_now(&self) -> SystemTime {
        self.wall.get()
    }
    fn open(&self, address: SocketAddr) -> io::Result<Bound> {
        self.hit(Step::Open)?;
        Ok(Bound {
            local: Cell::new(address),
            drops: Rc::clone(&self.bound_drops),
        })
    }
    fn bind_network(&self, _: &Bound, network: u64) -> io::Result<()> {
        self.networks.borrow_mut().push(network);
        self.hit(Step::Network)
    }
    fn bind(&self, bound: &Bound, address: SocketAddr) -> io::Result<()> {
        self.hit(Step::Bind)?;
        assert_eq!(address.port(), 47739);
        bound.local.set(address);
        Ok(())
    }
    fn listen(&self, _: &Bound, backlog: i32) -> io::Result<()> {
        assert_eq!(backlog, 2);
        self.hit(Step::Listen)
    }
    fn local_addr(&self, bound: &Bound) -> io::Result<SocketAddr> {
        self.hit(Step::Local)?;
        let mut address = bound.local.get();
        if self.wrong_bound.get() {
            address.set_port(47738);
        }
        Ok(address)
    }
    fn poll(&self, _: &Bound, timeout: i32) -> io::Result<PollProgress> {
        assert_eq!(timeout, 50);
        self.hit(Step::Poll)?;
        Ok(if self.pending.get() {
            PollProgress::Pending
        } else {
            PollProgress::Ready
        })
    }
    fn accept(&self, _: &Bound) -> io::Result<(Socket, SocketAddr)> {
        self.hit(Step::Accept)?;
        Ok((
            Socket {
                drops: Rc::clone(&self.socket_drops),
            },
            self.reported.get(),
        ))
    }
    fn configure(&self, _: &Socket) -> io::Result<()> {
        self.hit(Step::Configure)
    }
    fn addresses(&self, _: &Socket) -> io::Result<(SocketAddr, SocketAddr)> {
        self.hit(Step::Addresses)?;
        Ok((self.local.get(), self.peer.get()))
    }
}

impl ReceiverTlsIo<Socket> for Io {
    fn wall_now(&self) -> SystemTime {
        self.wall.get()
    }
}
impl TlsIo<Socket> for Io {
    fn now(&self) -> Instant {
        self.time.get()
    }
    fn read(&self, _: &Socket, _: &mut [u8]) -> io::Result<usize> {
        self.tls_calls.set(self.tls_calls.get() + 1);
        panic!("revoked lease reached TLS")
    }
    fn write(&self, _: &Socket, _: &[u8]) -> io::Result<usize> {
        self.tls_calls.set(self.tls_calls.get() + 1);
        panic!("revoked lease reached TLS")
    }
    fn poll(&self, _: &Socket, _: TlsInterest, _: i32) -> io::Result<PollProgress> {
        panic!("revoked lease reached TLS")
    }
}

#[test]
fn wifi_binds_before_listening_and_preserves_the_unsigned_network_handle() {
    let io = Io::new(false, false);
    let owner = io.open_owner().unwrap();
    assert_eq!(
        *io.trace.borrow(),
        [
            Step::Open,
            Step::Network,
            Step::Bind,
            Step::Listen,
            Step::Local
        ]
    );
    assert_eq!(*io.networks.borrow(), [u64::MAX]);
    assert_eq!(format!("{owner:?}"), "ReceiverListener { .. }");
    drop(owner);
    assert_eq!(io.bound_drops.get(), 1);
    assert!(io.identity.check_live().is_err());
}

#[test]
fn hotspot_never_fabricates_a_client_network_and_dual_family_polling_is_round_robin() {
    let io = Io::new(true, true);
    let mut owner = io.open_owner().unwrap();
    assert!(io.networks.borrow().is_empty());
    let first = io.accepted(&mut owner);
    assert_eq!(format!("{first:?}"), "AcceptedReceiver { .. }");
    io.local.set("[2001:db8::1]:47739".parse().unwrap());
    io.peer.set("[2001:db8::3]:12345".parse().unwrap());
    io.reported.set(io.peer.get());
    let second = io.accepted(&mut owner);
    drop((first, second, owner));
    assert_eq!(io.bound_drops.get(), 2);
    assert_eq!(io.socket_drops.get(), 2);
}

#[test]
fn every_startup_failure_releases_bound_sockets_without_an_address_or_port_fallback() {
    for step in [
        Step::Open,
        Step::Network,
        Step::Bind,
        Step::Listen,
        Step::Local,
    ] {
        let io = Io::new(false, false);
        io.fail.set(Some((step, io::ErrorKind::AddrInUse)));
        assert!(io.open_owner().is_err());
        assert!(io.identity.check_live().is_err());
        assert_eq!(
            io.trace
                .borrow()
                .iter()
                .filter(|call| **call == Step::Open)
                .count(),
            1
        );
        assert_eq!(io.bound_drops.get(), usize::from(step != Step::Open));
    }
    let io = Io::new(false, false);
    io.wrong_bound.set(true);
    assert!(matches!(io.open_owner(), Err(ListenerError::LocalAddress)));
    assert_eq!(io.bound_drops.get(), 1);
}

#[test]
fn lifecycle_checks_surround_startup_syscalls_and_precede_error_handling() {
    for step in [
        Step::Open,
        Step::Network,
        Step::Bind,
        Step::Listen,
        Step::Local,
    ] {
        for event in [
            Event::Cancel,
            Event::Stale,
            Event::Backward,
            Event::Expire,
            Event::Timeout,
        ] {
            let io = Io::new(false, false);
            io.hook.set(Some((step, event)));
            io.fail.set(Some((step, io::ErrorKind::Other)));
            let result = io.open_owner();
            assert!(matches!(
                result,
                Err(ListenerError::Cancelled
                    | ListenerError::StaleRoute
                    | ListenerError::ClockInvalid
                    | ListenerError::Identity(_)
                    | ListenerError::StartupTimeout)
            ));
            assert!(io.identity.check_live().is_err());
        }
    }
}

#[test]
fn an_unrelated_identity_or_cancelled_owner_never_opens_a_descriptor() {
    let io = Io::new(false, false);
    io.cancel.cancel();
    assert!(matches!(io.open_owner(), Err(ListenerError::Cancelled)));
    assert!(io.trace.borrow().is_empty());
    let io = Io::new(false, false);
    let other = ReceiverIdentity::create().unwrap();
    let ctx = ReceiverContext::new(1, 2, *other.session().as_bytes()).unwrap();
    *io.routes.borrow_mut() = Some(
        ReceiverRoutes::wifi(
            ctx,
            1,
            7,
            &[ReceiverRoute::new(&[192, 168, 42, 1], 0, 24).unwrap()],
        )
        .unwrap(),
    );
    assert!(matches!(io.open_owner(), Err(ListenerError::StaleRoute)));
    assert!(io.trace.borrow().is_empty());
}

#[test]
fn poll_and_accept_retry_events_do_no_more_than_one_attempt_per_call() {
    let io = Io::new(false, false);
    let mut owner = io.open_owner().unwrap();
    io.pending.set(true);
    assert!(matches!(
        owner.accept_with(&io, || io.current()).unwrap(),
        AcceptOutcome::Pending
    ));
    assert!(!io.trace.borrow().contains(&Step::Accept));
    io.pending.set(false);
    for (step, kind) in [
        (Step::Poll, io::ErrorKind::Interrupted),
        (Step::Accept, io::ErrorKind::WouldBlock),
        (Step::Accept, io::ErrorKind::Interrupted),
        (Step::Accept, io::ErrorKind::ConnectionAborted),
    ] {
        io.fail.set(Some((step, kind)));
        let before = io.trace.borrow().len();
        assert!(matches!(
            owner.accept_with(&io, || io.current()).unwrap(),
            AcceptOutcome::Pending
        ));
        assert!(
            io.trace.borrow()[before..]
                .iter()
                .filter(|call| **call == Step::Accept)
                .count()
                <= 1
        );
    }
}

#[test]
fn poll_or_accept_fatal_errors_close_the_listener_and_its_session() {
    for step in [Step::Poll, Step::Accept] {
        let io = Io::new(false, false);
        let mut owner = io.open_owner().unwrap();
        io.fail.set(Some((step, io::ErrorKind::PermissionDenied)));
        assert!(owner.accept_with(&io, || io.current()).is_err());
        assert_eq!(io.bound_drops.get(), 1);
        assert!(io.identity.check_live().is_err());
    }
}

#[test]
fn rejected_configuration_queries_and_peers_never_start_tls_or_stop_a_live_listener() {
    for step in [Step::Configure, Step::Addresses] {
        let io = Io::new(false, false);
        let mut owner = io.open_owner().unwrap();
        io.fail.set(Some((step, io::ErrorKind::PermissionDenied)));
        assert!(matches!(
            owner.accept_with(&io, || io.current()).unwrap(),
            AcceptOutcome::Rejected(_)
        ));
        assert_eq!(io.socket_drops.get(), 1);
        assert_eq!(io.tls_calls.get(), 0);
        assert!(io.identity.check_live().is_ok());
        io.fail.set(None);
        drop(io.accepted(&mut owner));
    }
    for (local, peer, reported) in [
        ("0.0.0.0:47739", "192.168.42.3:9", "192.168.42.3:9"),
        ("192.168.42.1:47739", "192.168.43.3:9", "192.168.43.3:9"),
        ("192.168.42.1:47739", "192.168.42.3:9", "192.168.42.4:9"),
    ] {
        let io = Io::new(false, false);
        let mut owner = io.open_owner().unwrap();
        io.local.set(local.parse().unwrap());
        io.peer.set(peer.parse().unwrap());
        io.reported.set(reported.parse().unwrap());
        assert!(matches!(
            owner.accept_with(&io, || io.current()).unwrap(),
            AcceptOutcome::Rejected(RejectReason::PeerPolicy)
        ));
        assert_eq!(io.socket_drops.get(), 1);
        assert_eq!(io.tls_calls.get(), 0);
    }
}

#[test]
fn two_unpaired_and_eight_attempt_limits_apply_to_the_whole_listener_owner() {
    let io = Io::new(false, false);
    let mut owner = io.open_owner().unwrap();
    let first = io.accepted(&mut owner);
    let second = io.accepted(&mut owner);
    assert!(matches!(
        owner.accept_with(&io, || io.current()).unwrap(),
        AcceptOutcome::Rejected(RejectReason::Admission(ReceiverError::Busy))
    ));
    drop((first, second));
    for _ in 0..6 {
        drop(io.accepted(&mut owner));
    }
    assert!(matches!(
        owner.accept_with(&io, || io.current()).unwrap(),
        AcceptOutcome::Rejected(RejectReason::Admission(ReceiverError::RateLimited))
    ));
}

#[test]
fn lifecycle_changes_after_accept_drop_the_temporary_socket_and_close_the_owner() {
    for step in [Step::Poll, Step::Accept, Step::Configure, Step::Addresses] {
        for event in [Event::Cancel, Event::Stale, Event::Backward, Event::Expire] {
            let io = Io::new(false, false);
            let mut owner = io.open_owner().unwrap();
            io.hook.set(Some((step, event)));
            assert!(owner.accept_with(&io, || io.current()).is_err());
            assert_eq!(io.bound_drops.get(), 1);
            assert_eq!(io.socket_drops.get(), usize::from(step != Step::Poll));
            assert_eq!(io.tls_calls.get(), 0);
        }
    }
}

#[test]
fn dropping_listener_or_changing_the_child_route_revokes_accepted_sockets_before_tls() {
    for close_parent in [false, true] {
        let io = Io::new(false, false);
        let mut owner = io.open_owner().unwrap();
        let accepted = io.accepted(&mut owner);
        if close_parent {
            owner.close();
        } else {
            *io.routes.borrow_mut() = None;
        }
        assert!(accepted.pair_with(&io, || io.current()).is_err());
        assert_eq!(io.socket_drops.get(), 1);
        assert_eq!(io.tls_calls.get(), 0);
        assert!(io.identity.check_live().is_err());
        assert!(owner.accept_with(&io, || io.current()).is_err());
        assert_eq!(io.bound_drops.get(), 1);
    }
}

#[test]
fn time_spent_validating_an_accepted_socket_is_not_refunded_to_its_tls_budget() {
    let io = Io::new(false, false);
    let mut owner = io.open_owner().unwrap();
    io.hook.set(Some((Step::Configure, Event::Timeout)));
    assert!(matches!(
        owner.accept_with(&io, || io.current()).unwrap(),
        AcceptOutcome::Rejected(RejectReason::Admission(ReceiverError::DeadlineExceeded))
    ));
    assert_eq!(io.socket_drops.get(), 1);
    assert!(io.identity.check_live().is_ok());
    drop(io.accepted(&mut owner));
}

#[test]
fn failure_on_a_second_binding_closes_the_first_instead_of_falling_back_to_one_family() {
    let io = Io::new(false, true);
    io.fail_second_bind.set(true);
    assert!(matches!(io.open_owner(), Err(ListenerError::BindFailed)));
    assert_eq!(io.bound_drops.get(), 2);
    assert!(io.identity.check_live().is_err());
    assert_eq!(
        io.trace
            .borrow()
            .iter()
            .filter(|step| **step == Step::Open)
            .count(),
        2
    );
    assert_eq!(
        io.trace
            .borrow()
            .iter()
            .filter(|step| **step == Step::Listen)
            .count(),
        1
    );
}

#[test]
fn a_second_listener_owner_cannot_reset_admission_or_revoke_the_existing_session() {
    let io = Io::new(false, false);
    let first = io.open_owner().unwrap();
    let before = io.trace.borrow().len();
    assert!(matches!(io.open_owner(), Err(ListenerError::AlreadyOwned)));
    assert_eq!(io.trace.borrow().len(), before);
    assert!(io.identity.check_live().is_ok());
    drop(first);
    assert!(io.identity.check_live().is_err());
    assert!(matches!(io.open_owner(), Err(ListenerError::AlreadyOwned)));
}

#[test]
fn a_changed_route_snapshot_is_rejected_even_when_the_context_generation_was_not_bumped() {
    for change in 0..4 {
        let io = Io::new(false, false);
        let mut owner = io.open_owner().unwrap();
        let accepted = io.accepted(&mut owner);
        let context = io.current().unwrap().context();
        let local =
            ReceiverRoute::new(&[192, 168, 42, 1], 0, if change == 3 { 16 } else { 24 }).unwrap();
        let changed = if change == 2 {
            ReceiverRoutes::hotspot(context, 7, &[local], &[], &[])
        } else {
            ReceiverRoutes::wifi(
                context,
                if change == 0 { 7 } else { u64::MAX },
                if change == 1 { 8 } else { 7 },
                &[local],
            )
        }
        .unwrap();
        *io.routes.borrow_mut() = Some(changed);
        assert!(accepted.pair_with(&io, || io.current()).is_err());
        assert_eq!(io.tls_calls.get(), 0);
        assert!(owner.accept_with(&io, || io.current()).is_err());
        assert_eq!(io.bound_drops.get(), 1);
        assert_eq!(io.socket_drops.get(), 1);
    }
}
