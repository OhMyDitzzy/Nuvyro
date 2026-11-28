use std::io;
use std::net::SocketAddr;
use std::os::fd::AsRawFd;
use std::sync::Arc;
use std::time::Instant;

use socket2::{Domain, Protocol, SockAddr, Socket, Type};

use super::listener::ListenerIo;
use super::{AcceptOutcome, AcceptedReceiver, ListenerError, ReceiverListener, ReceiverRoutes};
use crate::engine::receiver_worker::{ReceiverPairer, ReceiverRuntime, WorkerError};
use crate::pairing::ReceiverIdentity;
use crate::transport::ConnectCancellation;
use crate::transport::android::AndroidTlsIo;
use crate::transport::connector::PollProgress;
use crate::transport::tls::{IncomingPairError, PairedIncoming, TlsInterest, TlsIo};

#[link(name = "android")]
unsafe extern "C" {
    fn android_setsocknetwork(network: u64, fd: libc::c_int) -> libc::c_int;
}

pub(crate) fn open_receiver_listener(
    identity: Arc<ReceiverIdentity>,
    routes: ReceiverRoutes,
    cancellation: ConnectCancellation,
    current: impl FnMut() -> Option<ReceiverRoutes>,
) -> Result<ReceiverListener<Socket>, ListenerError> {
    ReceiverListener::open_with(&AndroidListenerIo, identity, routes, cancellation, current)
}

pub(crate) fn accept_receiver(
    listener: &mut ReceiverListener<Socket>,
    current: impl FnMut() -> Option<ReceiverRoutes>,
) -> Result<AcceptOutcome<Socket>, ListenerError> {
    listener.accept_with(&AndroidListenerIo, current)
}

pub(crate) fn pair_accepted_receiver(
    accepted: AcceptedReceiver<Socket>,
    current: impl FnMut() -> Option<ReceiverRoutes>,
) -> Result<PairedIncoming<Socket>, IncomingPairError> {
    accepted.pair_with(&AndroidTlsIo, current)
}

pub(crate) fn receiver_runtime(
    identity: Arc<ReceiverIdentity>,
    validity: Arc<crate::engine::receiver_worker::SessionValidity>,
    cancellation: ConnectCancellation,
) -> Result<impl ReceiverRuntime, WorkerError> {
    if cancellation.is_cancelled() {
        return Err(WorkerError::ListenerStopped);
    }
    let routes = validity.current_routes().ok_or(WorkerError::StartFailed)?;
    let bound_route_generation = routes.context().route_generation();
    let current_validity = Arc::clone(&validity);
    let listener = open_receiver_listener(Arc::clone(&identity), routes, cancellation, move || {
        current_validity.current_routes()
    })
    .map_err(|_| WorkerError::StartFailed)?;
    Ok(AndroidReceiverRuntime {
        listener,
        validity,
        identity,
        bound_route_generation,
    })
}

struct AndroidReceiverRuntime {
    listener: ReceiverListener<Socket>,
    validity: Arc<crate::engine::receiver_worker::SessionValidity>,
    identity: Arc<ReceiverIdentity>,
    bound_route_generation: u64,
}

#[derive(Clone)]
struct AndroidReceiverPairer {
    validity: Arc<crate::engine::receiver_worker::SessionValidity>,
}

struct RouteAccepted {
    accepted: AcceptedReceiver<Socket>,
    route_generation: u64,
}

impl std::fmt::Debug for RouteAccepted {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("RouteAccepted")
            .finish_non_exhaustive()
    }
}

impl ReceiverRuntime for AndroidReceiverRuntime {
    type Accepted = RouteAccepted;
    type Paired = PairedIncoming<Socket>;
    type Pairer = AndroidReceiverPairer;

    fn accept(
        &mut self,
        cancellation: &ConnectCancellation,
    ) -> Result<Option<Self::Accepted>, WorkerError> {
        let current_validity = Arc::clone(&self.validity);
        match accept_receiver(&mut self.listener, move || {
            current_validity.current_routes()
        })
        .map_err(|_| WorkerError::ListenerStopped)?
        {
            AcceptOutcome::Accepted(accepted) => Ok(Some(RouteAccepted {
                route_generation: accepted.context().route_generation(),
                accepted,
            })),
            AcceptOutcome::Pending | AcceptOutcome::Rejected(_) => {
                if cancellation.is_cancelled() {
                    Err(WorkerError::ListenerStopped)
                } else {
                    Ok(None)
                }
            }
        }
    }

    fn pairer(&self) -> Self::Pairer {
        AndroidReceiverPairer {
            validity: Arc::clone(&self.validity),
        }
    }

    fn bound_route_generation(&self) -> u64 {
        self.bound_route_generation
    }

    fn close(&mut self) {
        self.listener.close();
        self.identity.clear_invitation();
    }

    fn maintenance(&mut self) {
        self.identity.expire_invitation(Instant::now());
    }
}

impl ReceiverPairer<RouteAccepted, PairedIncoming<Socket>> for AndroidReceiverPairer {
    fn pair(
        &self,
        accepted: RouteAccepted,
        cancellation: &ConnectCancellation,
    ) -> Option<(u64, PairedIncoming<Socket>)> {
        if cancellation.is_cancelled() {
            return None;
        }
        let route_generation = accepted.route_generation;
        let current_validity = Arc::clone(&self.validity);
        pair_accepted_receiver(accepted.accepted, move || {
            if cancellation.is_cancelled() {
                None
            } else {
                current_validity.current_routes()
            }
        })
        .ok()
        .map(|paired| (route_generation, paired))
    }
}

struct AndroidListenerIo;

#[cfg(test)]
mod tests;

impl ListenerIo for AndroidListenerIo {
    type Listener = Socket;
    type Socket = Socket;
    fn now(&self) -> Instant {
        Instant::now()
    }
    fn open(&self, address: SocketAddr) -> io::Result<Socket> {
        let socket = Socket::new(
            if address.is_ipv4() {
                Domain::IPV4
            } else {
                Domain::IPV6
            },
            Type::STREAM,
            Some(Protocol::TCP),
        )?;
        socket.set_nonblocking(true)?;
        if address.is_ipv6() {
            socket.set_only_v6(true)?;
        }
        Ok(socket)
    }
    fn bind_network(&self, listener: &Socket, network: u64) -> io::Result<()> {
        // SAFETY: listener owns a live descriptor; the NDK retains no pointer or ownership.
        if unsafe { android_setsocknetwork(network, listener.as_raw_fd()) } == 0 {
            Ok(())
        } else {
            Err(io::Error::last_os_error())
        }
    }
    fn bind(&self, listener: &Socket, address: SocketAddr) -> io::Result<()> {
        listener.bind(&SockAddr::from(address))
    }
    fn listen(&self, listener: &Socket, backlog: i32) -> io::Result<()> {
        listener.listen(backlog)
    }
    fn local_addr(&self, listener: &Socket) -> io::Result<SocketAddr> {
        listener
            .local_addr()?
            .as_socket()
            .ok_or_else(|| io::ErrorKind::InvalidInput.into())
    }
    fn poll(&self, listener: &Socket, timeout: i32) -> io::Result<PollProgress> {
        AndroidTlsIo.poll(listener, TlsInterest::Read, timeout)
    }
    fn accept(&self, listener: &Socket) -> io::Result<(Socket, SocketAddr)> {
        // Socket2 accept uses accept4(SOCK_CLOEXEC) on Android; nonblocking is set before admission.
        let (socket, peer) = listener.accept()?;
        Ok((socket, peer.as_socket().ok_or(io::ErrorKind::InvalidInput)?))
    }
    fn configure(&self, socket: &Socket) -> io::Result<()> {
        socket.set_nonblocking(true)?;
        // SAFETY: fcntl operates on this owned socket and transfers no descriptor ownership.
        let flags = unsafe { libc::fcntl(socket.as_raw_fd(), libc::F_GETFD) };
        if flags < 0 {
            return Err(io::Error::last_os_error());
        }
        if flags & libc::FD_CLOEXEC == 0 {
            return Err(io::ErrorKind::InvalidInput.into());
        }
        Ok(())
    }
    fn addresses(&self, socket: &Socket) -> io::Result<(SocketAddr, SocketAddr)> {
        Ok((
            socket
                .local_addr()?
                .as_socket()
                .ok_or(io::ErrorKind::InvalidInput)?,
            socket
                .peer_addr()?
                .as_socket()
                .ok_or(io::ErrorKind::InvalidInput)?,
        ))
    }
}
