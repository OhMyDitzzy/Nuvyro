use std::fmt;
use std::num::NonZeroU64;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use nuvyro_transfer_core::protocol::Id;

use super::{ConnectError, NumericEndpoint};

pub(super) const MAX_CONNECT_TIME: Duration = Duration::from_secs(10);
pub(super) const POLL_SLICE: Duration = Duration::from_millis(50);

/// Owner-supplied generations are not authentication, consent, or Network liveness proofs.
#[derive(Clone, Copy, Eq, PartialEq)]
pub(crate) struct ConnectionContext {
    native_generation: NonZeroU64,
    route_generation: NonZeroU64,
    network_handle: NonZeroU64,
    expected_session: Id,
}

impl ConnectionContext {
    pub(crate) fn new(
        native_generation: u64,
        route_generation: u64,
        network_handle: u64,
        expected_session: [u8; 16],
    ) -> Result<Self, ConnectError> {
        if native_generation > i64::MAX as u64 || route_generation > i64::MAX as u64 {
            return Err(ConnectError::InvalidContext);
        }
        Ok(Self {
            native_generation: NonZeroU64::new(native_generation)
                .ok_or(ConnectError::InvalidContext)?,
            route_generation: NonZeroU64::new(route_generation)
                .ok_or(ConnectError::InvalidContext)?,
            network_handle: NonZeroU64::new(network_handle).ok_or(ConnectError::InvalidContext)?,
            expected_session: Id::from_bytes(expected_session)
                .map_err(|_| ConnectError::InvalidContext)?,
        })
    }

    pub(crate) const fn network_handle(self) -> u64 {
        self.network_handle.get()
    }

    pub(crate) const fn expected_session(self) -> Id {
        self.expected_session
    }
}

impl fmt::Debug for ConnectionContext {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ConnectionContext")
            .finish_non_exhaustive()
    }
}

/// Cancellation changes no FD from another thread; the owning worker drops its socket.
#[derive(Clone)]
pub(crate) struct ConnectCancellation(Arc<AtomicBool>);

impl ConnectCancellation {
    pub(in crate::transport) fn new() -> Self {
        Self(Arc::new(AtomicBool::new(false)))
    }

    pub(crate) fn cancel(&self) {
        self.0.store(true, Ordering::Release);
    }

    pub(crate) fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::Acquire)
    }
}

/// Consumed by exactly one connect operation. A new attempt has a distinct cancellation token.
/// Checks surround platform calls and short polls; a blocked platform call is not preempted.
pub(crate) struct ConnectAttempt {
    pub(super) context: ConnectionContext,
    pub(super) endpoint: NumericEndpoint,
    pub(super) cancellation: ConnectCancellation,
    started: Instant,
    deadline: Instant,
}

impl ConnectAttempt {
    pub(crate) fn new(
        context: ConnectionContext,
        endpoint: NumericEndpoint,
        timeout: Duration,
    ) -> Result<Self, ConnectError> {
        Self::at(context, endpoint, timeout, Instant::now())
    }

    pub(super) fn at(
        context: ConnectionContext,
        endpoint: NumericEndpoint,
        timeout: Duration,
        now: Instant,
    ) -> Result<Self, ConnectError> {
        if timeout.is_zero() || timeout > MAX_CONNECT_TIME {
            return Err(ConnectError::InvalidTimeout);
        }
        Ok(Self {
            context,
            endpoint,
            cancellation: ConnectCancellation::new(),
            started: now,
            deadline: now
                .checked_add(timeout)
                .ok_or(ConnectError::InvalidTimeout)?,
        })
    }

    pub(crate) fn cancellation(&self) -> ConnectCancellation {
        self.cancellation.clone()
    }

    pub(super) const fn started_at(&self) -> Instant {
        self.started
    }

    pub(super) const fn deadline(&self) -> Instant {
        self.deadline
    }

    pub(super) fn check(&self, now: Instant) -> Result<(), ConnectError> {
        if self.cancellation.is_cancelled() {
            Err(ConnectError::Cancelled)
        } else if now < self.started {
            Err(ConnectError::ClockInvalid)
        } else if now >= self.deadline {
            Err(ConnectError::DeadlineExceeded)
        } else {
            Ok(())
        }
    }

    pub(super) fn poll_window(&self, now: Instant) -> Result<Duration, ConnectError> {
        self.check(now)?;
        Ok((self.deadline - now).min(POLL_SLICE))
    }
}
