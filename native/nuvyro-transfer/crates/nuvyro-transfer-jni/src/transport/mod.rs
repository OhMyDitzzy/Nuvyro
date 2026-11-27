//! Single-owner socket establishment. Connected sockets remain private until TLS owns them.

#[cfg(target_os = "android")]
mod android;
mod attempt;
mod budget;
mod connector;
mod endpoint;
mod tls;

#[cfg(test)]
mod tests;

pub(crate) use attempt::{ConnectAttempt, ConnectCancellation, ConnectionContext};
pub(crate) use endpoint::NumericEndpoint;

#[cfg(target_os = "android")]
pub(crate) use android::connect;
#[cfg(target_os = "android")]
pub(crate) type PendingTlsSocket = connector::PendingConnection<socket2::Socket>;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ConnectError {
    InvalidContext,
    InvalidEndpoint,
    InvalidTimeout,
    Cancelled,
    StaleContext,
    DeadlineExceeded,
    ClockInvalid,
    ResourceLimit,
    SocketCreation,
    NetworkBinding,
    ConnectFailed,
    PollFailed,
}
