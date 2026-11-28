//! Accepted sockets remain private and unpaired until invitation authentication completes.

mod admission;
#[cfg(target_os = "android")]
mod android;
mod context;
mod listener;
pub(in crate::transport) use listener::ListenerIo;
mod pending;
mod route;

#[cfg(test)]
mod tests;

pub(in crate::transport) use admission::PairedPermit;
pub(crate) use admission::ReceiverAdmission;
#[cfg(target_os = "android")]
pub(crate) use android::{
    accept_receiver, open_receiver_listener, pair_accepted_receiver, receiver_runtime,
};
pub(crate) use context::ReceiverContext;
pub(crate) use listener::{
    AcceptOutcome, AcceptedReceiver, ListenerError, ReceiverListener, RejectReason,
};
pub(crate) use pending::PendingIncoming;
pub(crate) use route::{ReceiverRoute, ReceiverRoutes};

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
