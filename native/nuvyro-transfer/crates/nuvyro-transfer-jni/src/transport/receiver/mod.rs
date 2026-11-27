//! Accepted sockets remain private and unpaired until invitation authentication completes.

mod admission;
mod context;
mod pending;

#[cfg(test)]
mod tests;

pub(crate) use admission::ReceiverAdmission;
pub(crate) use context::ReceiverContext;
pub(crate) use pending::PendingIncoming;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ReceiverError {
    InvalidContext,
    Closed,
    Busy,
    RateLimited,
    Cancelled,
    ClockInvalid,
    DeadlineExceeded,
    StaleContext,
}
