use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use super::ConnectError;

pub(super) const MAX_CONNECTIONS: usize = 4;

pub(super) struct ConnectionBudget(Arc<AtomicUsize>);

impl ConnectionBudget {
    pub(super) fn new() -> Self {
        Self(Arc::new(AtomicUsize::new(0)))
    }

    pub(super) fn acquire(&self) -> Result<ConnectionPermit, ConnectError> {
        self.0
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |active| {
                (active < MAX_CONNECTIONS).then_some(active + 1)
            })
            .map_err(|_| ConnectError::ResourceLimit)?;
        Ok(ConnectionPermit(Arc::clone(&self.0)))
    }
}

pub(super) struct ConnectionPermit(Arc<AtomicUsize>);

impl Drop for ConnectionPermit {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::AcqRel);
    }
}
