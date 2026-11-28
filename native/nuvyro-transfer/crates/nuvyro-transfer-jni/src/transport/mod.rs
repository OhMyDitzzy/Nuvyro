//! Single-owner socket establishment. Connected sockets remain private until TLS owns them.

#[cfg(target_os = "android")]
mod android;
mod attempt;
mod budget;
mod connector;
mod endpoint;
mod receiver;
mod tls;

#[cfg(test)]
mod tests;

pub(crate) use attempt::{ConnectAttempt, ConnectCancellation, ConnectionContext};
pub(crate) use endpoint::NumericEndpoint;
#[cfg(target_os = "android")]
pub(crate) use receiver::receiver_runtime;
pub(crate) use receiver::{
    AcceptOutcome, AcceptedReceiver, ListenerError, ReceiverListener, ReceiverRoute, ReceiverRoutes,
};
pub(crate) use receiver::{PendingIncoming, ReceiverAdmission, ReceiverContext, ReceiverError};
#[cfg(target_os = "android")]
pub(crate) use receiver::{accept_receiver, open_receiver_listener, pair_accepted_receiver};
#[cfg(test)]
pub(crate) use tls::{
    check_negotiated_profile as test_check_tls_profile, new_client as test_tls_client,
};

#[cfg(target_os = "android")]
pub(crate) use android::{
    connect, perform_pairing, perform_receiver_handshake, perform_receiver_pairing,
    perform_tls_handshake,
};
pub(crate) use tls::{IncomingPairError, PairConnectionError, PairedConnection, PairedIncoming};
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
