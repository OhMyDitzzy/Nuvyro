use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use super::{PendingIncoming, ReceiverContext, ReceiverError};
use crate::transport::ConnectCancellation;

const MAX_UNPAIRED: usize = 2;
const ATTEMPTS_PER_MINUTE: usize = 8;
const WINDOW: Duration = Duration::from_secs(60);

struct State {
    closed: bool,
    active: usize,
    last_observed: Instant,
    attempts: [Option<Instant>; ATTEMPTS_PER_MINUTE],
}

/// One owner per Receive session; retain it across invitation replacement and socket failures.
pub(crate) struct ReceiverAdmission {
    context: ReceiverContext,
    shared: Arc<Mutex<State>>,
}

impl ReceiverAdmission {
    pub(crate) fn new(context: ReceiverContext, now: Instant) -> Self {
        Self {
            context,
            shared: Arc::new(Mutex::new(State {
                closed: false,
                active: 0,
                last_observed: now,
                attempts: [None; ATTEMPTS_PER_MINUTE],
            })),
        }
    }

    /// The future listener supplies only a nonblocking, route-validated accepted socket.
    /// Sample time under the lock so concurrent callers cannot reorder clock observations.
    pub(in crate::transport) fn admit<S>(
        &self,
        socket: S,
        cancellation: ConnectCancellation,
        now: impl FnOnce() -> Instant,
    ) -> Result<PendingIncoming<S>, ReceiverError> {
        let mut state = self.shared.lock().map_err(|_| ReceiverError::Closed)?;
        if state.closed {
            return Err(ReceiverError::Closed);
        }
        if cancellation.is_cancelled() {
            return Err(ReceiverError::Cancelled);
        }
        let now = now();
        if now < state.last_observed {
            state.closed = true;
            return Err(ReceiverError::ClockInvalid);
        }
        state.last_observed = now;
        if state.active >= MAX_UNPAIRED {
            return Err(ReceiverError::Busy);
        }
        for attempt in &mut state.attempts {
            if attempt.is_some_and(|at| now.duration_since(at) >= WINDOW) {
                *attempt = None;
            }
        }
        let entry = state
            .attempts
            .iter_mut()
            .find(|entry| entry.is_none())
            .ok_or(ReceiverError::RateLimited)?;
        let deadline = now
            .checked_add(super::super::attempt::MAX_CONNECT_TIME)
            .ok_or(ReceiverError::DeadlineExceeded)?;
        *entry = Some(now);
        state.active += 1;
        let permit = ReceiverPermit(Arc::clone(&self.shared));
        Ok(PendingIncoming::new(
            socket,
            self.context,
            cancellation,
            now,
            deadline,
            permit,
        ))
    }

    pub(crate) fn close(&self) {
        if let Ok(mut state) = self.shared.lock() {
            state.closed = true;
        }
    }
}

impl Drop for ReceiverAdmission {
    fn drop(&mut self) {
        self.close();
    }
}

pub(super) struct ReceiverPermit(Arc<Mutex<State>>);

impl ReceiverPermit {
    pub(super) fn check_open(&self) -> Result<(), ReceiverError> {
        let state = self.0.lock().map_err(|_| ReceiverError::Closed)?;
        if state.closed {
            Err(ReceiverError::Closed)
        } else {
            Ok(())
        }
    }
}

impl Drop for ReceiverPermit {
    fn drop(&mut self) {
        if let Ok(mut state) = self.0.lock() {
            state.active -= 1;
        }
    }
}
