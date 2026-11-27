use super::*;
use std::net::TcpStream;
use std::time::Duration;

fn assert_flags(socket: &Socket) {
    // SAFETY: the socket is owned and remains live for this read-only fcntl call.
    let status = unsafe { libc::fcntl(socket.as_raw_fd(), libc::F_GETFL) };
    assert!(status >= 0 && status & libc::O_NONBLOCK != 0);
    // SAFETY: the socket is owned and remains live for this read-only fcntl call.
    let flags = unsafe { libc::fcntl(socket.as_raw_fd(), libc::F_GETFD) };
    assert!(flags >= 0 && flags & libc::FD_CLOEXEC != 0);
}

#[test]
fn native_listener_and_accepted_socket_are_nonblocking_and_close_on_exec() {
    // Exercise the low-level adapter on loopback only; production route policy rejects loopback.
    let io = AndroidListenerIo;
    let socket = io.open("127.0.0.1:0".parse().unwrap()).unwrap();
    assert_flags(&socket);
    io.bind(&socket, "127.0.0.1:0".parse().unwrap()).unwrap();
    io.listen(&socket, 2).unwrap();
    let address = io.local_addr(&socket).unwrap();
    assert_ne!(address.port(), 0);
    assert_eq!(io.poll(&socket, 1).unwrap(), PollProgress::Pending);
    let client = TcpStream::connect_timeout(&address, Duration::from_secs(1)).unwrap();
    assert_eq!(io.poll(&socket, 50).unwrap(), PollProgress::Ready);
    let (accepted, reported) = io.accept(&socket).unwrap();
    io.configure(&accepted).unwrap();
    assert_flags(&accepted);
    let (local, peer) = io.addresses(&accepted).unwrap();
    assert_eq!(local, address);
    assert_eq!(peer, reported);
    assert_eq!(peer, client.local_addr().unwrap());
    assert!(matches!(io.accept(&socket),Err(e) if e.kind()==io::ErrorKind::WouldBlock));
}

#[test]
fn native_ipv6_listener_is_v6_only_and_cannot_claim_ipv4_peers() {
    let io = AndroidListenerIo;
    let socket = io.open("[::1]:0".parse().unwrap()).unwrap();
    assert_flags(&socket);
    assert!(socket.only_v6().unwrap());
    io.bind(&socket, "[::1]:0".parse().unwrap()).unwrap();
    io.listen(&socket, 2).unwrap();
    let local = io.local_addr(&socket).unwrap();
    let client = TcpStream::connect_timeout(&local, Duration::from_secs(1)).unwrap();
    let (accepted, _) = io.accept(&socket).unwrap();
    io.configure(&accepted).unwrap();
    assert_flags(&accepted);
    assert!(io.addresses(&accepted).unwrap().1.is_ipv6());
    drop(client);
}

#[test]
fn native_bind_conflict_is_reported_without_reusing_or_replacing_the_listener() {
    let io = AndroidListenerIo;
    let first = io.open("127.0.0.1:0".parse().unwrap()).unwrap();
    io.bind(&first, "127.0.0.1:0".parse().unwrap()).unwrap();
    io.listen(&first, 2).unwrap();
    let address = io.local_addr(&first).unwrap();
    let other = io.open(address).unwrap();
    assert!(matches!(io.bind(&other,address),Err(e) if e.kind()==io::ErrorKind::AddrInUse));
    assert_eq!(io.local_addr(&first).unwrap(), address);
}
