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
    paired: bool,
    next_connection_id: u64,
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
                paired: false,
                next_connection_id: 1,
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
        self.admit_since(socket, cancellation, None, now)
    }

    pub(in crate::transport) fn admit_since<S>(
        &self,
        socket: S,
        cancellation: ConnectCancellation,
        accepted_at: Option<Instant>,
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
        let started = accepted_at.unwrap_or(now);
        if now < started {
            return Err(ReceiverError::ClockInvalid);
        }
        let deadline = started
            .checked_add(super::super::attempt::MAX_CONNECT_TIME)
            .ok_or(ReceiverError::DeadlineExceeded)?;
        if now >= deadline {
            return Err(ReceiverError::DeadlineExceeded);
        }
        if state.active >= MAX_UNPAIRED {
            return Err(ReceiverError::Busy);
        }
        for attempt in &mut state.attempts {
            if attempt.is_some_and(|at| now.duration_since(at) >= WINDOW) {
                *attempt = None;
            }
        }
        let connection_id = state.next_connection_id;
        let Some(next_connection_id) = connection_id.checked_add(1) else {
            state.closed = true;
            return Err(ReceiverError::Closed);
        };
        let entry = state
            .attempts
            .iter_mut()
            .find(|entry| entry.is_none())
            .ok_or(ReceiverError::RateLimited)?;
        *entry = Some(now);
        state.active += 1;
        state.next_connection_id = next_connection_id;
        let permit = ReceiverPermit {
            shared: Arc::clone(&self.shared),
            connection_id,
        };
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

pub(super) struct ReceiverPermit {
    shared: Arc<Mutex<State>>,
    connection_id: u64,
}

impl ReceiverPermit {
    pub(super) fn reserve_pair(&self) -> Result<PairedPermit, ReceiverError> {
        let mut state = self.shared.lock().map_err(|_| ReceiverError::Closed)?;
        if state.closed {
            return Err(ReceiverError::Closed);
        }
        if state.paired {
            return Err(ReceiverError::Busy);
        }
        state.paired = true;
        Ok(PairedPermit {
            shared: Arc::clone(&self.shared),
            connection_id: self.connection_id,
        })
    }

    pub(super) fn owns_reservation(&self, reservation: &PairedPermit) -> bool {
        self.connection_id == reservation.connection_id
            && Arc::ptr_eq(&self.shared, &reservation.shared)
    }

    pub(super) fn check_open(&self) -> Result<(), ReceiverError> {
        let state = self.shared.lock().map_err(|_| ReceiverError::Closed)?;
        if state.closed {
            Err(ReceiverError::Closed)
        } else {
            Ok(())
        }
    }
}

impl Drop for ReceiverPermit {
    fn drop(&mut self) {
        if let Ok(mut state) = self.shared.lock() {
            state.active -= 1;
        }
    }
}

/// Reserved before PairResult is sent; ownership survives only a successful promotion.
pub(in crate::transport) struct PairedPermit {
    shared: Arc<Mutex<State>>,
    connection_id: u64,
}

impl PairedPermit {
    pub(super) fn check_open(&self) -> Result<(), ReceiverError> {
        let state = self.shared.lock().map_err(|_| ReceiverError::Closed)?;
        if state.closed {
            Err(ReceiverError::Closed)
        } else {
            Ok(())
        }
    }
}

impl Drop for PairedPermit {
    fn drop(&mut self) {
        if let Ok(mut state) = self.shared.lock() {
            state.paired = false;
        }
    }
}
