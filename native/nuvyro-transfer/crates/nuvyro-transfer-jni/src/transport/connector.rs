use std::fmt;
use std::io;
use std::net::SocketAddr;
use std::time::{Duration, Instant};

use super::attempt::POLL_SLICE;
use super::budget::{ConnectionBudget, ConnectionPermit};
use super::{ConnectAttempt, ConnectCancellation, ConnectError, ConnectionContext};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum ConnectProgress {
    Connected,
    Pending,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum PollProgress {
    Ready,
    Pending,
}

pub(super) trait SocketOps {
    type Socket;

    fn now(&self) -> Instant;
    fn open(&self, address: SocketAddr) -> io::Result<Self::Socket>;
    fn bind_network(&self, socket: &Self::Socket, network: u64) -> io::Result<()>;
    fn begin_connect(
        &self,
        socket: &Self::Socket,
        address: SocketAddr,
    ) -> io::Result<ConnectProgress>;
    fn poll(&self, socket: &Self::Socket, window: Duration) -> io::Result<PollProgress>;
    fn verify_peer(&self, socket: &Self::Socket, address: SocketAddr) -> io::Result<()>;
}

/// No Read/Write, raw FD, socket cloning, or plaintext stream escapes this module.
/// A TLS adapter inside transport must verify the pinned session before exposing application I/O.
pub(crate) struct PendingConnection<S> {
    socket: Option<S>,
    context: ConnectionContext,
    cancellation: ConnectCancellation,
    started: Instant,
    last_observed: Instant,
    deadline: Instant,
    permit: Option<ConnectionPermit>,
}

impl<S> PendingConnection<S> {
    pub(crate) const fn context(&self) -> ConnectionContext {
        self.context
    }

    pub(crate) fn check_context(
        &mut self,
        expected: ConnectionContext,
    ) -> Result<(), ConnectError> {
        self.check_context_at(Some(expected), Instant::now())
    }

    pub(super) fn check_context_at(
        &mut self,
        current: Option<ConnectionContext>,
        now: Instant,
    ) -> Result<(), ConnectError> {
        let error = if self.cancellation.is_cancelled() || self.socket.is_none() {
            Some(ConnectError::Cancelled)
        } else if now < self.started || now < self.last_observed {
            Some(ConnectError::ClockInvalid)
        } else if now >= self.deadline {
            Some(ConnectError::DeadlineExceeded)
        } else if current != Some(self.context) {
            Some(ConnectError::StaleContext)
        } else {
            None
        };
        if let Some(error) = error {
            self.cancel();
            Err(error)
        } else {
            self.last_observed = now;
            Ok(())
        }
    }

    /// Only protocol phase transitions may change the deadline, never peer traffic or retries.
    pub(super) fn enter_control_phase(
        &mut self,
        current: Option<ConnectionContext>,
        now: Instant,
        deadline: Instant,
    ) -> Result<(), ConnectError> {
        self.check_context_at(current, now)?;
        if deadline <= now || deadline - now > super::attempt::MAX_CONNECT_TIME {
            self.cancel();
            return Err(ConnectError::InvalidTimeout);
        }
        self.deadline = deadline;
        Ok(())
    }

    pub(super) fn socket_mut(
        &mut self,
        current: Option<ConnectionContext>,
        now: Instant,
    ) -> Result<&mut S, ConnectError> {
        self.check_context_at(current, now)?;
        self.socket.as_mut().ok_or(ConnectError::Cancelled)
    }

    pub(super) fn poll_window(
        &mut self,
        current: Option<ConnectionContext>,
        now: Instant,
    ) -> Result<Duration, ConnectError> {
        self.check_context_at(current, now)?;
        Ok((self.deadline - now).min(POLL_SLICE))
    }

    pub(crate) fn cancel(&mut self) {
        self.cancellation.cancel();
        self.socket.take();
        self.permit.take();
    }
}

impl<S> Drop for PendingConnection<S> {
    fn drop(&mut self) {
        self.cancel();
    }
}

impl<S> fmt::Debug for PendingConnection<S> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PendingConnection")
            .field("open", &self.socket.is_some())
            .finish_non_exhaustive()
    }
}

pub(super) fn connect_with<O: SocketOps>(
    ops: &O,
    budget: &ConnectionBudget,
    attempt: ConnectAttempt,
) -> Result<PendingConnection<O::Socket>, ConnectError> {
    attempt.check(ops.now())?;
    let permit = budget.acquire()?;
    attempt.check(ops.now())?;
    let address = attempt.endpoint.address();
    let socket = ops
        .open(address)
        .map_err(|_| ConnectError::SocketCreation)?;
    attempt.check(ops.now())?;
    ops.bind_network(&socket, attempt.context.network_handle())
        .map_err(|_| ConnectError::NetworkBinding)?;
    attempt.check(ops.now())?;
    let progress = ops
        .begin_connect(&socket, address)
        .map_err(|_| ConnectError::ConnectFailed)?;
    attempt.check(ops.now())?;
    if progress == ConnectProgress::Pending {
        loop {
            let window = attempt.poll_window(ops.now())?;
            let progress = ops
                .poll(&socket, window)
                .map_err(|_| ConnectError::PollFailed)?;
            attempt.check(ops.now())?;
            if progress == PollProgress::Ready {
                break;
            }
        }
    }
    ops.verify_peer(&socket, address)
        .map_err(|_| ConnectError::ConnectFailed)?;
    let observed = ops.now();
    attempt.check(observed)?;
    let started = attempt.started_at();
    let deadline = attempt.deadline();
    Ok(PendingConnection {
        socket: Some(socket),
        context: attempt.context,
        cancellation: attempt.cancellation,
        started,
        last_observed: observed,
        deadline,
        permit: Some(permit),
    })
}

pub(super) fn poll_timeout(window: Duration) -> Result<i32, ConnectError> {
    if window.is_zero() || window > POLL_SLICE {
        return Err(ConnectError::InvalidTimeout);
    }
    i32::try_from(window.as_nanos().div_ceil(1_000_000)).map_err(|_| ConnectError::InvalidTimeout)
}
