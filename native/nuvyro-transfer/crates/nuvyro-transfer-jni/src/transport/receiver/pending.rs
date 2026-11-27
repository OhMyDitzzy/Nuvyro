use std::time::{Duration, Instant};

use super::admission::ReceiverPermit;
use super::{ReceiverContext, ReceiverError};
use crate::transport::ConnectCancellation;
use crate::transport::attempt::POLL_SLICE;

/// Owns the unpaired permit through TLS and the subsequent invitation exchange.
/// No raw descriptor, socket getter, cloning, or application I/O is exposed to callers.
pub(crate) struct PendingIncoming<S> {
    socket: Option<S>,
    context: ReceiverContext,
    cancellation: ConnectCancellation,
    last_observed: Instant,
    deadline: Instant,
    permit: Option<ReceiverPermit>,
}

impl<S> PendingIncoming<S> {
    pub(super) fn new(
        socket: S,
        context: ReceiverContext,
        cancellation: ConnectCancellation,
        now: Instant,
        deadline: Instant,
        permit: ReceiverPermit,
    ) -> Self {
        Self {
            socket: Some(socket),
            context,
            cancellation,
            last_observed: now,
            deadline,
            permit: Some(permit),
        }
    }

    pub(crate) const fn context(&self) -> ReceiverContext {
        self.context
    }

    pub(in crate::transport) fn check_context_at(
        &mut self,
        current: Option<ReceiverContext>,
        now: Instant,
    ) -> Result<(), ReceiverError> {
        let result = if self.cancellation.is_cancelled() || self.socket.is_none() {
            Err(ReceiverError::Cancelled)
        } else if now < self.last_observed {
            Err(ReceiverError::ClockInvalid)
        } else if now >= self.deadline {
            Err(ReceiverError::DeadlineExceeded)
        } else if current != Some(self.context) {
            Err(ReceiverError::StaleContext)
        } else {
            self.permit
                .as_ref()
                .ok_or(ReceiverError::Closed)
                .and_then(ReceiverPermit::check_open)
        };
        if result.is_err() {
            self.cancel();
        } else {
            self.last_observed = now;
        }
        result
    }

    pub(in crate::transport) fn socket_mut(
        &mut self,
        current: Option<ReceiverContext>,
        now: Instant,
    ) -> Result<&mut S, ReceiverError> {
        self.check_context_at(current, now)?;
        self.socket.as_mut().ok_or(ReceiverError::Cancelled)
    }

    pub(in crate::transport) fn poll_window(
        &mut self,
        current: Option<ReceiverContext>,
        now: Instant,
    ) -> Result<Duration, ReceiverError> {
        self.check_context_at(current, now)?;
        Ok((self.deadline - now).min(POLL_SLICE))
    }

    fn cancel(&mut self) {
        self.cancellation.cancel();
        self.socket.take();
        self.permit.take();
    }
}

impl<S> Drop for PendingIncoming<S> {
    fn drop(&mut self) {
        self.cancel();
    }
}

impl<S> std::fmt::Debug for PendingIncoming<S> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PendingIncoming").finish_non_exhaustive()
    }
}
