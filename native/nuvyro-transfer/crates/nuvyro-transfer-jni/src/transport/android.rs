use std::io;
use std::net::SocketAddr;
use std::os::fd::AsRawFd;
use std::sync::OnceLock;
use std::time::{Duration, Instant};

use socket2::{Domain, Protocol, SockAddr, Socket, Type};

use super::budget::ConnectionBudget;
use super::connector::{ConnectProgress, PollProgress, SocketOps, connect_with, poll_timeout};
use super::{ConnectAttempt, ConnectError, PendingTlsSocket};

#[link(name = "android")]
unsafe extern "C" {
    fn android_setsocknetwork(network: u64, fd: libc::c_int) -> libc::c_int;
}

static CONNECTIONS: OnceLock<ConnectionBudget> = OnceLock::new();

/// Runs on the owning I/O worker; no thread is spawned and no TLS state is granted.
pub(crate) fn connect(attempt: ConnectAttempt) -> Result<PendingTlsSocket, ConnectError> {
    connect_with(
        &AndroidSockets,
        CONNECTIONS.get_or_init(ConnectionBudget::new),
        attempt,
    )
}

struct AndroidSockets;

impl SocketOps for AndroidSockets {
    type Socket = Socket;

    fn now(&self) -> Instant {
        Instant::now()
    }

    fn open(&self, address: SocketAddr) -> io::Result<Socket> {
        let domain = if address.is_ipv6() {
            Domain::IPV6
        } else {
            Domain::IPV4
        };
        let socket = Socket::new(domain, Type::STREAM, Some(Protocol::TCP))?;
        socket.set_nonblocking(true)?;
        if address.is_ipv6() {
            socket.set_only_v6(true)?;
        }
        Ok(socket)
    }

    fn bind_network(&self, socket: &Socket, network: u64) -> io::Result<()> {
        // SAFETY: socket owns a live FD for the duration of the call; the nonzero
        // opaque handle is passed with the exact NDK uint64_t ABI. No ownership is transferred.
        if unsafe { android_setsocknetwork(network, socket.as_raw_fd()) } == 0 {
            Ok(())
        } else {
            Err(io::Error::last_os_error())
        }
    }

    fn begin_connect(&self, socket: &Socket, address: SocketAddr) -> io::Result<ConnectProgress> {
        match socket.connect(&SockAddr::from(address)) {
            Ok(()) => Ok(ConnectProgress::Connected),
            Err(error)
                if matches!(
                    error.raw_os_error(),
                    Some(libc::EINPROGRESS | libc::EALREADY | libc::EINTR)
                ) || error.kind() == io::ErrorKind::WouldBlock =>
            {
                Ok(ConnectProgress::Pending)
            }
            Err(error) => Err(error),
        }
    }

    fn poll(&self, socket: &Socket, window: Duration) -> io::Result<PollProgress> {
        let timeout =
            poll_timeout(window).map_err(|_| io::Error::from(io::ErrorKind::InvalidInput))?;
        let mut descriptor = libc::pollfd {
            fd: socket.as_raw_fd(),
            events: libc::POLLOUT,
            revents: 0,
        };
        // SAFETY: descriptor is one initialized, writable pollfd. Its borrowed
        // socket remains owned throughout this bounded call; poll retains no pointer.
        let result = unsafe { libc::poll(&mut descriptor, 1, timeout) };
        if result < 0 {
            let error = io::Error::last_os_error();
            return if error.kind() == io::ErrorKind::Interrupted {
                Ok(PollProgress::Pending)
            } else {
                Err(error)
            };
        }
        if result == 0 {
            return Ok(PollProgress::Pending);
        }
        if descriptor.revents & (libc::POLLNVAL | libc::POLLERR | libc::POLLHUP) != 0 {
            return Err(io::Error::from(io::ErrorKind::ConnectionAborted));
        }
        Ok(if descriptor.revents & libc::POLLOUT != 0 {
            PollProgress::Ready
        } else {
            PollProgress::Pending
        })
    }

    fn verify_peer(&self, socket: &Socket, expected: SocketAddr) -> io::Result<()> {
        if let Some(error) = socket.take_error()? {
            return Err(error);
        }
        if socket.peer_addr()?.as_socket() != Some(expected) {
            return Err(io::Error::from(io::ErrorKind::NotConnected));
        }
        Ok(())
    }
}
