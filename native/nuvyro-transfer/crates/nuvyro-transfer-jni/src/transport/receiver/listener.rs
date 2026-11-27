use std::io;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{Instant, SystemTime};

use nuvyro_transfer_core::protocol::ErrorCode;

use super::{PendingIncoming, ReceiverAdmission, ReceiverError, ReceiverRoutes};
use crate::pairing::ReceiverIdentity;
use crate::transport::ConnectCancellation;
use crate::transport::attempt::{MAX_CONNECT_TIME, POLL_SLICE};
use crate::transport::connector::{PollProgress, poll_timeout};
use crate::transport::tls::{IncomingPairError, PairedIncoming, ReceiverTlsIo, pair_incoming_with};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ListenerError {
    AlreadyOwned,
    InvalidRoute,
    Cancelled,
    ClockInvalid,
    StaleRoute,
    StartupTimeout,
    Closed,
    ResourceLimit,
    OpenFailed,
    NetworkBinding,
    BindFailed,
    ListenFailed,
    LocalAddress,
    PollFailed,
    AcceptFailed,
    Identity(ErrorCode),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum RejectReason {
    PeerPolicy,
    Configuration,
    Addresses,
    Admission(ReceiverError),
}

pub(in crate::transport) trait ListenerIo {
    type Listener;
    type Socket;
    fn now(&self) -> Instant;
    fn wall_now(&self) -> SystemTime {
        SystemTime::now()
    }
    fn open(&self, address: SocketAddr) -> io::Result<Self::Listener>;
    fn bind_network(&self, listener: &Self::Listener, network: u64) -> io::Result<()>;
    fn bind(&self, listener: &Self::Listener, address: SocketAddr) -> io::Result<()>;
    fn listen(&self, listener: &Self::Listener, backlog: i32) -> io::Result<()>;
    fn local_addr(&self, listener: &Self::Listener) -> io::Result<SocketAddr>;
    fn poll(&self, listener: &Self::Listener, timeout_ms: i32) -> io::Result<PollProgress>;
    fn accept(&self, listener: &Self::Listener) -> io::Result<(Self::Socket, SocketAddr)>;
    fn configure(&self, socket: &Self::Socket) -> io::Result<()>;
    fn addresses(&self, socket: &Self::Socket) -> io::Result<(SocketAddr, SocketAddr)>;
}

pub(crate) enum AcceptOutcome<S> {
    Pending,
    Rejected(RejectReason),
    Accepted(AcceptedReceiver<S>),
}

/// One owner for both address families. No background thread, advertisement, or plaintext I/O.
pub(crate) struct ReceiverListener<L> {
    listeners: Vec<L>,
    routes: ReceiverRoutes,
    identity: Arc<ReceiverIdentity>,
    admission: Arc<ReceiverAdmission>,
    cancellation: ConnectCancellation,
    last_observed: Instant,
    startup_deadline: Option<Instant>,
    next: usize,
    closed: bool,
}

impl<L> ReceiverListener<L> {
    pub(in crate::transport) fn open_with<O: ListenerIo<Listener = L>>(
        io: &O,
        identity: Arc<ReceiverIdentity>,
        routes: ReceiverRoutes,
        cancellation: ConnectCancellation,
        mut current: impl FnMut() -> Option<ReceiverRoutes>,
    ) -> Result<Self, ListenerError> {
        let now = io.now();
        let deadline = now
            .checked_add(MAX_CONNECT_TIME)
            .ok_or(ListenerError::StartupTimeout)?;
        if !identity.claim_listener() {
            return Err(ListenerError::AlreadyOwned);
        }
        let mut owner = Self {
            listeners: Vec::new(),
            admission: Arc::new(ReceiverAdmission::new(routes.context(), now)),
            identity,
            routes,
            cancellation,
            last_observed: now,
            startup_deadline: Some(deadline),
            next: 0,
            closed: false,
        };
        owner.check(io, current())?;
        owner
            .listeners
            .try_reserve_exact(owner.routes.bindings().len())
            .map_err(|_| ListenerError::ResourceLimit)?;
        for index in 0..owner.routes.bindings().len() {
            let address = owner.routes.bindings()[index].local();
            owner.check(io, current())?;
            let socket = io.open(address);
            owner.check(io, current())?;
            let socket = socket.map_err(|_| ListenerError::OpenFailed)?;
            if let Some(network) = owner.routes.network() {
                let result = io.bind_network(&socket, network);
                owner.check(io, current())?;
                result.map_err(|_| ListenerError::NetworkBinding)?;
            }
            let result = io.bind(&socket, address);
            owner.check(io, current())?;
            result.map_err(|_| ListenerError::BindFailed)?;
            let result = io.listen(&socket, 2);
            owner.check(io, current())?;
            result.map_err(|_| ListenerError::ListenFailed)?;
            let bound = io.local_addr(&socket);
            owner.check(io, current())?;
            if bound.map_err(|_| ListenerError::LocalAddress)? != address {
                return Err(ListenerError::LocalAddress);
            }
            owner.listeners.push(socket);
        }
        owner.check(io, current())?;
        owner.startup_deadline = None;
        Ok(owner)
    }

    /// At most one poll and one accept per call; hostile peers cannot create a drain loop.
    pub(in crate::transport) fn accept_with<O: ListenerIo<Listener = L>>(
        &mut self,
        io: &O,
        mut current: impl FnMut() -> Option<ReceiverRoutes>,
    ) -> Result<AcceptOutcome<O::Socket>, ListenerError> {
        self.check(io, current())?;
        let index = self.next;
        self.next = (self.next + 1) % self.listeners.len();
        let result = io.poll(
            &self.listeners[index],
            poll_timeout(POLL_SLICE).map_err(|_| ListenerError::PollFailed)?,
        );
        self.check(io, current())?;
        match result {
            Ok(PollProgress::Pending) => return Ok(AcceptOutcome::Pending),
            Err(e) if e.kind() == io::ErrorKind::Interrupted => return Ok(AcceptOutcome::Pending),
            Err(_) => {
                self.close();
                return Err(ListenerError::PollFailed);
            }
            Ok(PollProgress::Ready) => {}
        }
        let accepted = io.accept(&self.listeners[index]);
        let accepted_at = io.now();
        self.check(io, current())?;
        let (socket, reported) = match accepted {
            Ok(value) => value,
            Err(e)
                if matches!(
                    e.kind(),
                    io::ErrorKind::Interrupted
                        | io::ErrorKind::WouldBlock
                        | io::ErrorKind::ConnectionAborted
                ) =>
            {
                return Ok(AcceptOutcome::Pending);
            }
            Err(_) => {
                self.close();
                return Err(ListenerError::AcceptFailed);
            }
        };
        let configured = io.configure(&socket);
        self.check(io, current())?;
        if configured.is_err() {
            return Ok(AcceptOutcome::Rejected(RejectReason::Configuration));
        }
        let addresses = io.addresses(&socket);
        self.check(io, current())?;
        let (local, peer) = match addresses {
            Ok(value) => value,
            Err(_) => return Ok(AcceptOutcome::Rejected(RejectReason::Addresses)),
        };
        if peer != reported
            || local != self.routes.bindings()[index].local()
            || !self.routes.allows(local, peer)
        {
            return Ok(AcceptOutcome::Rejected(RejectReason::PeerPolicy));
        }
        let pending = self.admission.admit_since(
            socket,
            ConnectCancellation::new(),
            Some(accepted_at),
            || io.now(),
        );
        self.check(io, current())?;
        match pending {
            Ok(pending) => Ok(AcceptOutcome::Accepted(AcceptedReceiver {
                pending,
                routes: self.routes.clone(),
                identity: Arc::clone(&self.identity),
                admission: Arc::clone(&self.admission),
                cancellation: self.cancellation.clone(),
            })),
            Err(error) => Ok(AcceptOutcome::Rejected(RejectReason::Admission(error))),
        }
    }

    fn check<O: ListenerIo<Listener = L>>(
        &mut self,
        io: &O,
        current: Option<ReceiverRoutes>,
    ) -> Result<(), ListenerError> {
        let now = io.now();
        let error = if self.closed {
            Some(ListenerError::Closed)
        } else if self.cancellation.is_cancelled() {
            Some(ListenerError::Cancelled)
        } else if now < self.last_observed {
            Some(ListenerError::ClockInvalid)
        } else if self
            .startup_deadline
            .is_some_and(|deadline| now >= deadline)
        {
            Some(ListenerError::StartupTimeout)
        } else if current.as_ref() != Some(&self.routes)
            || self.routes.context().session() != self.identity.session()
        {
            Some(ListenerError::StaleRoute)
        } else {
            self.identity
                .check_live_at(io.wall_now(), now)
                .err()
                .map(ListenerError::Identity)
        };
        if let Some(error) = error {
            self.close();
            Err(error)
        } else {
            self.last_observed = now;
            Ok(())
        }
    }

    pub(crate) fn close(&mut self) {
        self.closed = true;
        self.cancellation.cancel();
        self.admission.close();
        self.identity.close();
        self.listeners.clear();
    }
}

impl<L> Drop for ReceiverListener<L> {
    fn drop(&mut self) {
        self.close();
    }
}

/// Carries the listener's identity and lease into pairing without exposing a socket or FD.
pub(crate) struct AcceptedReceiver<S> {
    pending: PendingIncoming<S>,
    routes: ReceiverRoutes,
    identity: Arc<ReceiverIdentity>,
    admission: Arc<ReceiverAdmission>,
    cancellation: ConnectCancellation,
}

impl<S> AcceptedReceiver<S> {
    pub(in crate::transport) fn pair_with<O: ReceiverTlsIo<S>>(
        self,
        io: &O,
        mut current: impl FnMut() -> Option<ReceiverRoutes>,
    ) -> Result<PairedIncoming<S>, IncomingPairError> {
        let Self {
            pending,
            routes,
            identity,
            admission,
            cancellation,
        } = self;
        pair_incoming_with(pending, io, &identity, || {
            if cancellation.is_cancelled() || current().as_ref() != Some(&routes) {
                cancellation.cancel();
                admission.close();
                identity.close();
                None
            } else {
                Some(routes.context())
            }
        })
    }
}

impl<L> std::fmt::Debug for ReceiverListener<L> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ReceiverListener").finish_non_exhaustive()
    }
}
impl<S> std::fmt::Debug for AcceptedReceiver<S> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AcceptedReceiver").finish_non_exhaustive()
    }
}
