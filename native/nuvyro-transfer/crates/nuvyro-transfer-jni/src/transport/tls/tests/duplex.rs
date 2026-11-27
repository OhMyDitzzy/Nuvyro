use std::cell::RefCell;
use std::collections::VecDeque;
use std::io;
use std::net::SocketAddr;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::{Receiver, SyncSender, TryRecvError, TrySendError, sync_channel};
use std::time::{Duration, Instant};

use nuvyro_transfer_core::pairing::PairingQr;

use crate::pairing::ReceiverIdentity;
use crate::transport::budget::ConnectionBudget;
use crate::transport::connector::{ConnectProgress, PollProgress, SocketOps, connect_with};
use crate::transport::tls::incoming_pairing::pair_incoming_with;
use crate::transport::tls::pairing::pair_outgoing_with;
use crate::transport::tls::{ReceiverTlsIo, TlsInterest, TlsIo};
use crate::transport::{
    ConnectAttempt, ConnectCancellation, ConnectionContext, NumericEndpoint, ReceiverAdmission,
    ReceiverContext,
};

struct MemorySocket {
    outgoing: SyncSender<Vec<u8>>,
    incoming: Receiver<Vec<u8>>,
    buffered: RefCell<VecDeque<u8>>,
    drops: Arc<AtomicUsize>,
}

impl Drop for MemorySocket {
    fn drop(&mut self) {
        self.drops.fetch_add(1, Ordering::Relaxed);
    }
}

impl MemorySocket {
    fn pair(drops: &Arc<AtomicUsize>) -> (Self, Self) {
        let (a_tx, a_rx) = sync_channel(4);
        let (b_tx, b_rx) = sync_channel(4);
        (
            Self {
                outgoing: a_tx,
                incoming: b_rx,
                buffered: RefCell::default(),
                drops: Arc::clone(drops),
            },
            Self {
                outgoing: b_tx,
                incoming: a_rx,
                buffered: RefCell::default(),
                drops: Arc::clone(drops),
            },
        )
    }
}

struct Io;

impl ReceiverTlsIo<MemorySocket> for Io {}

impl TlsIo<MemorySocket> for Io {
    fn now(&self) -> Instant {
        Instant::now()
    }

    fn read(&self, socket: &MemorySocket, bytes: &mut [u8]) -> io::Result<usize> {
        let mut buffered = socket.buffered.borrow_mut();
        if buffered.is_empty() {
            match socket.incoming.try_recv() {
                Ok(record) => buffered.extend(record),
                Err(TryRecvError::Empty) => return Err(io::ErrorKind::WouldBlock.into()),
                Err(TryRecvError::Disconnected) => return Ok(0),
            }
        }
        let count = bytes.len().min(buffered.len());
        for target in &mut bytes[..count] {
            *target = buffered.pop_front().unwrap();
        }
        Ok(count)
    }

    fn write(&self, socket: &MemorySocket, bytes: &[u8]) -> io::Result<usize> {
        let count = bytes.len().min(512);
        match socket.outgoing.try_send(bytes[..count].to_vec()) {
            Ok(()) => Ok(count),
            Err(TrySendError::Full(_)) => Err(io::ErrorKind::WouldBlock.into()),
            Err(TrySendError::Disconnected(_)) => Err(io::ErrorKind::BrokenPipe.into()),
        }
    }

    fn poll(&self, _: &MemorySocket, _: TlsInterest, timeout: i32) -> io::Result<PollProgress> {
        assert!((1..=50).contains(&timeout));
        std::thread::sleep(Duration::from_millis(1));
        Ok(PollProgress::Ready)
    }
}

struct Connector(RefCell<Option<MemorySocket>>);

impl SocketOps for Connector {
    type Socket = MemorySocket;
    fn now(&self) -> Instant {
        Instant::now()
    }
    fn open(&self, _: SocketAddr) -> io::Result<MemorySocket> {
        Ok(self.0.borrow_mut().take().unwrap())
    }
    fn bind_network(&self, _: &MemorySocket, network: u64) -> io::Result<()> {
        assert_eq!(network, 7);
        Ok(())
    }
    fn begin_connect(&self, _: &MemorySocket, _: SocketAddr) -> io::Result<ConnectProgress> {
        Ok(ConnectProgress::Connected)
    }
    fn poll(&self, _: &MemorySocket, _: Duration) -> io::Result<PollProgress> {
        Ok(PollProgress::Ready)
    }
    fn verify_peer(&self, _: &MemorySocket, _: SocketAddr) -> io::Result<()> {
        Ok(())
    }
}

#[test]
fn production_outgoing_and_incoming_operations_agree_on_one_pair_over_bounded_duplex_tls() {
    let identity = ReceiverIdentity::create().unwrap();
    let (_, qr) = identity.create_invitation().unwrap();
    let qr = PairingQr::decode(qr.as_bytes()).unwrap();
    let outgoing_context = ConnectionContext::new(1, 2, 7, *identity.session().as_bytes()).unwrap();
    let incoming_context = ReceiverContext::new(3, 4, *identity.session().as_bytes()).unwrap();
    let admission = ReceiverAdmission::new(incoming_context, Instant::now());
    let drops = Arc::new(AtomicUsize::new(0));
    let (client_socket, server_socket) = MemorySocket::pair(&drops);
    let connector = Connector(RefCell::new(Some(client_socket)));
    let budget = ConnectionBudget::new();
    let attempt = ConnectAttempt::new(
        outgoing_context,
        NumericEndpoint::new(&[192, 168, 42, 3], 0, 47739).unwrap(),
        Duration::from_secs(10),
    )
    .unwrap();
    let outgoing_cancellation = attempt.cancellation();
    let outgoing = connect_with(&connector, &budget, attempt).unwrap();
    let incoming_cancellation = ConnectCancellation::new();
    let incoming = admission
        .admit(server_socket, incoming_cancellation.clone(), Instant::now)
        .unwrap();
    let (outgoing, incoming) = std::thread::scope(|scope| {
        let sender = scope.spawn(move || {
            pair_outgoing_with(outgoing, &Io, qr, "Sender".to_owned(), || {
                Some(outgoing_context)
            })
        });
        let receiver =
            scope.spawn(|| pair_incoming_with(incoming, &Io, &identity, || Some(incoming_context)));
        (
            sender.join().unwrap().unwrap(),
            receiver.join().unwrap().unwrap(),
        )
    });
    assert_eq!(outgoing.pair(), incoming.pair());
    assert_eq!(incoming.pair().secure_session_id, identity.session());
    assert_eq!(incoming.claimed_name(), "Sender");
    assert_eq!(drops.load(Ordering::Relaxed), 0);
    drop((outgoing, incoming));
    assert_eq!(drops.load(Ordering::Relaxed), 2);
    assert!(outgoing_cancellation.is_cancelled());
    assert!(incoming_cancellation.is_cancelled());
}
