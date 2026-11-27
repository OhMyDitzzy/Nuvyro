use std::time::{Duration, Instant};

use nuvyro_transfer_core::protocol::{ErrorCode, TLS_ALPN};
use rustls::{HandshakeKind, ProtocolVersion};

use crate::pairing::ReceiverIdentity;
use crate::transport::receiver::ReceiverError;
use crate::transport::tls::{IncomingTlsError, TlsInterest, TlsPolicyError};
use crate::transport::{ConnectCancellation, ReceiverAdmission, ReceiverContext};

mod pairing;
mod wire;
use wire::{Event, Fault, Operation, Wire};

fn context(identity: &ReceiverIdentity) -> ReceiverContext {
    ReceiverContext::new(1, 2, *identity.session().as_bytes()).unwrap()
}

#[test]
fn production_identity_and_pinned_client_complete_full_server_tls_without_releasing_admission() {
    let identity = ReceiverIdentity::create().unwrap();
    let admission = ReceiverAdmission::new(context(&identity), Instant::now());
    let io = Wire::new(&identity, &admission);
    let verified = io.run().unwrap();
    assert!(io.peer_complete.get());
    assert!(!verified.server.is_handshaking());
    assert!(!verified.server.wants_write());
    assert_eq!(
        verified.server.protocol_version(),
        Some(ProtocolVersion::TLSv1_3)
    );
    assert_eq!(verified.server.alpn_protocol(), Some(TLS_ALPN));
    assert_eq!(verified.server.handshake_kind(), Some(HandshakeKind::Full));
    assert_eq!(verified.pending.context(), context(&identity));
    assert_eq!(format!("{verified:?}"), "VerifiedIncoming { .. }");
    assert_eq!(io.drops.get(), 0);
    let second = admission
        .admit((), ConnectCancellation::new(), || io.time.get())
        .unwrap();
    assert!(matches!(
        admission.admit((), ConnectCancellation::new(), || io.time.get()),
        Err(ReceiverError::Busy)
    ));
    drop(verified);
    assert_eq!(io.drops.get(), 1);
    drop(
        admission
            .admit((), ConnectCancellation::new(), || io.time.get())
            .unwrap(),
    );
    drop(second);
}

#[test]
fn one_byte_tls_reads_and_writes_preserve_the_handshake() {
    let identity = ReceiverIdentity::create().unwrap();
    let admission = ReceiverAdmission::new(context(&identity), Instant::now());
    let io = Wire::new(&identity, &admission);
    io.fragment.set(1);
    let verified = io.run().unwrap();
    assert!(io.reads.get() > 100 && io.writes.get() > 100);
    drop(verified);
    assert_eq!(io.drops.get(), 1);
}

#[test]
fn missing_and_wrong_alpn_fail_closed() {
    for protocols in [vec![], vec![b"other/1".to_vec()]] {
        let identity = ReceiverIdentity::create().unwrap();
        let admission = ReceiverAdmission::new(context(&identity), Instant::now());
        let io = Wire::new(&identity, &admission);
        io.set_alpn(protocols.clone());
        let result = io.run();
        if protocols.is_empty() {
            assert!(matches!(
                result,
                Err(IncomingTlsError::Policy(TlsPolicyError::AlpnMismatch))
            ));
        } else {
            assert!(matches!(result, Err(IncomingTlsError::ProtocolFailure)));
        }
        assert_eq!(io.drops.get(), 1);
    }
}

#[test]
fn an_unrelated_identity_is_rejected_before_any_tls_io() {
    let identity = ReceiverIdentity::create().unwrap();
    let other = ReceiverIdentity::create().unwrap();
    let admission = ReceiverAdmission::new(context(&other), Instant::now());
    let io = Wire::new(&identity, &admission);
    io.context.set(Some(context(&other)));
    assert!(matches!(
        io.run(),
        Err(IncomingTlsError::Context(ReceiverError::StaleContext))
    ));
    assert_eq!((io.reads.get(), io.writes.get(), io.drops.get()), (0, 0, 1));
}

#[test]
fn identity_closed_before_tls_drops_the_socket_without_io() {
    let identity = ReceiverIdentity::create().unwrap();
    let admission = ReceiverAdmission::new(context(&identity), Instant::now());
    let io = Wire::new(&identity, &admission);
    identity.close();
    assert!(matches!(
        io.run(),
        Err(IncomingTlsError::Identity(ErrorCode::SessionExpired))
    ));
    assert_eq!((io.reads.get(), io.writes.get(), io.drops.get()), (0, 0, 1));
}

#[test]
fn cancellation_is_checked_after_reads_writes_and_interrupted_polls() {
    for operation in [Operation::Read, Operation::Write, Operation::Poll] {
        let identity = ReceiverIdentity::create().unwrap();
        let admission = ReceiverAdmission::new(context(&identity), Instant::now());
        let io = Wire::new(&identity, &admission);
        io.hook.set(Some((operation, Event::Cancel)));
        if operation == Operation::Poll {
            io.read_fault.set(Fault::AlwaysBlock);
            io.poll_fault.set(Fault::InterruptedOnce);
        }
        assert!(matches!(
            io.run(),
            Err(IncomingTlsError::Context(ReceiverError::Cancelled))
        ));
        assert_eq!(io.drops.get(), 1);
        let first = admission
            .admit((), ConnectCancellation::new(), || io.time.get())
            .unwrap();
        let second = admission
            .admit((), ConnectCancellation::new(), || io.time.get())
            .unwrap();
        drop((first, second));
    }
}

#[test]
fn closed_identity_admission_and_stale_context_are_checked_after_io_and_poll_errors() {
    for operation in [Operation::Read, Operation::Write, Operation::Poll] {
        for event in [
            Event::CloseIdentity,
            Event::CloseAdmission,
            Event::StaleNative,
            Event::StaleRoute,
            Event::MissingContext,
        ] {
            let identity = ReceiverIdentity::create().unwrap();
            let admission = ReceiverAdmission::new(context(&identity), Instant::now());
            let io = Wire::new(&identity, &admission);
            io.hook.set(Some((operation, event)));
            if operation == Operation::Poll {
                io.read_fault.set(Fault::AlwaysBlock);
                io.poll_fault.set(Fault::Error);
            }
            let result = io.run();
            match event {
                Event::CloseIdentity => assert!(matches!(
                    result,
                    Err(IncomingTlsError::Identity(ErrorCode::SessionExpired))
                )),
                Event::CloseAdmission => assert!(matches!(
                    result,
                    Err(IncomingTlsError::Context(ReceiverError::Closed))
                )),
                _ => assert!(matches!(
                    result,
                    Err(IncomingTlsError::Context(ReceiverError::StaleContext))
                )),
            }
            assert_eq!(io.drops.get(), 1);
        }
    }
}

#[test]
fn expiry_and_wall_clock_reversal_are_checked_on_existing_connections() {
    for operation in [Operation::Read, Operation::Write, Operation::Poll] {
        for event in [Event::ExpiredWall, Event::EarlierWall] {
            let identity = ReceiverIdentity::create().unwrap();
            let admission = ReceiverAdmission::new(context(&identity), Instant::now());
            let io = Wire::new(&identity, &admission);
            io.hook.set(Some((operation, event)));
            if operation == Operation::Poll {
                io.read_fault.set(Fault::AlwaysBlock);
            }
            assert!(matches!(
                io.run(),
                Err(IncomingTlsError::Identity(ErrorCode::SessionExpired))
            ));
            assert_eq!(io.drops.get(), 1);
            assert!(identity.check_live().is_err());
        }
    }
}

#[test]
fn backward_monotonic_clock_or_deadline_during_io_closes_the_socket() {
    for event in [Event::BackwardClock, Event::Deadline] {
        let identity = ReceiverIdentity::create().unwrap();
        let admission = ReceiverAdmission::new(context(&identity), Instant::now());
        let io = Wire::new(&identity, &admission);
        io.hook.set(Some((Operation::Read, event)));
        let expected = match event {
            Event::BackwardClock => ReceiverError::ClockInvalid,
            _ => ReceiverError::DeadlineExceeded,
        };
        assert!(matches!(io.run(), Err(IncomingTlsError::Context(e)) if e == expected));
        assert_eq!(io.drops.get(), 1);
    }
}

#[test]
fn queued_time_counts_toward_tls_and_the_last_poll_uses_only_remaining_time() {
    let identity = ReceiverIdentity::create().unwrap();
    let admission = ReceiverAdmission::new(context(&identity), Instant::now());
    let io = Wire::new(&identity, &admission);
    let pending = admission
        .admit(io.socket(), io.cancellation.clone(), || io.time.get())
        .unwrap();
    io.time.set(io.time.get() + Duration::from_millis(9_975));
    io.read_fault.set(Fault::AlwaysBlock);
    assert!(matches!(
        io.run_pending(pending),
        Err(IncomingTlsError::Context(ReceiverError::DeadlineExceeded))
    ));
    assert_eq!(*io.windows.borrow(), [(TlsInterest::Read, 25)]);
    assert_eq!(io.drops.get(), 1);
}

#[test]
fn a_client_hello_without_client_finished_never_counts_as_tls_complete() {
    let identity = ReceiverIdentity::create().unwrap();
    let admission = ReceiverAdmission::new(context(&identity), Instant::now());
    let io = Wire::new(&identity, &admission);
    io.stall_finished.set(true);
    assert!(matches!(
        io.run(),
        Err(IncomingTlsError::Context(ReceiverError::DeadlineExceeded))
    ));
    assert!(io.peer_complete.get());
    assert_eq!(io.windows.borrow().len(), 200);
    assert!(
        io.windows
            .borrow()
            .iter()
            .all(|(i, ms)| *i == TlsInterest::Read && *ms == 50)
    );
    assert_eq!(io.drops.get(), 1);
}

#[test]
fn interrupted_and_blocked_reads_and_writes_retry_with_the_original_budget() {
    for read in [false, true] {
        for fault in [Fault::InterruptedOnce, Fault::WouldBlockOnce] {
            let identity = ReceiverIdentity::create().unwrap();
            let admission = ReceiverAdmission::new(context(&identity), Instant::now());
            let io = Wire::new(&identity, &admission);
            if read {
                io.read_fault.set(fault);
            } else {
                io.write_fault.set(fault);
            }
            drop(io.run().unwrap());
            assert_eq!(io.drops.get(), 1);
        }
    }
}

#[test]
fn repeated_interrupted_calls_cannot_extend_tls_forever() {
    for read in [false, true] {
        let identity = ReceiverIdentity::create().unwrap();
        let admission = ReceiverAdmission::new(context(&identity), Instant::now());
        let io = Wire::new(&identity, &admission);
        if read {
            io.read_fault.set(Fault::AlwaysInterrupted);
        } else {
            io.write_fault.set(Fault::AlwaysInterrupted);
        }
        io.step.set(Duration::from_millis(10));
        assert!(matches!(
            io.run(),
            Err(IncomingTlsError::Context(ReceiverError::DeadlineExceeded))
        ));
        assert_eq!(io.drops.get(), 1);
    }
}

#[test]
fn permanent_write_backpressure_times_out_on_write_interest() {
    let identity = ReceiverIdentity::create().unwrap();
    let admission = ReceiverAdmission::new(context(&identity), Instant::now());
    let io = Wire::new(&identity, &admission);
    io.write_fault.set(Fault::AlwaysBlock);
    assert!(matches!(
        io.run(),
        Err(IncomingTlsError::Context(ReceiverError::DeadlineExceeded))
    ));
    assert!(
        io.windows
            .borrow()
            .iter()
            .all(|(i, _)| *i == TlsInterest::Write)
    );
    assert_eq!(io.drops.get(), 1);
}

#[test]
fn zero_progress_eof_socket_errors_and_corrupted_records_release_the_socket() {
    for read in [false, true] {
        for fault in [Fault::Zero, Fault::Error] {
            let identity = ReceiverIdentity::create().unwrap();
            let admission = ReceiverAdmission::new(context(&identity), Instant::now());
            let io = Wire::new(&identity, &admission);
            if read {
                io.read_fault.set(fault);
            } else {
                io.write_fault.set(fault);
            }
            let result = io.run();
            assert!(matches!(
                result,
                Err(IncomingTlsError::PeerClosed
                    | IncomingTlsError::InvalidProgress
                    | IncomingTlsError::SocketIo)
            ));
            assert_eq!(io.drops.get(), 1);
        }
    }
    let identity = ReceiverIdentity::create().unwrap();
    let admission = ReceiverAdmission::new(context(&identity), Instant::now());
    let io = Wire::new(&identity, &admission);
    io.inbound.borrow_mut().extend([0xff; 6]);
    assert!(io.run().is_err());
    assert_eq!(io.drops.get(), 1);
}

#[test]
fn corrupt_client_finished_is_not_accepted() {
    let identity = ReceiverIdentity::create().unwrap();
    let admission = ReceiverAdmission::new(context(&identity), Instant::now());
    let io = Wire::new(&identity, &admission);
    io.corrupt_finished.set(true);
    assert!(matches!(io.run(), Err(IncomingTlsError::ProtocolFailure)));
    assert_eq!(io.drops.get(), 1);
}

#[test]
fn tls12_only_client_hello_is_rejected_before_the_server_flight() {
    let identity = ReceiverIdentity::create().unwrap();
    let admission = ReceiverAdmission::new(context(&identity), Instant::now());
    let io = Wire::new(&identity, &admission);
    let mut hello = Vec::new();
    io.client.borrow_mut().write_tls(&mut hello).unwrap();
    let versions = [0, 43, 0, 3, 2, 3, 4];
    let offsets: Vec<_> = hello
        .windows(versions.len())
        .enumerate()
        .filter_map(|(i, w)| (w == versions).then_some(i))
        .collect();
    assert_eq!(offsets.len(), 1);
    hello[offsets[0] + 6] = 3;
    io.inbound.borrow_mut().extend(hello);
    assert!(matches!(io.run(), Err(IncomingTlsError::ProtocolFailure)));
    assert_eq!(io.writes.get(), 0);
    assert_eq!(io.drops.get(), 1);
}

#[test]
fn failed_tls_and_invitation_rotation_cannot_reset_the_receiver_attempt_limit() {
    let identity = ReceiverIdentity::create().unwrap();
    let admission = ReceiverAdmission::new(context(&identity), Instant::now());
    for _ in 0..8 {
        let io = Wire::new(&identity, &admission);
        io.read_fault.set(Fault::Zero);
        assert!(matches!(io.run(), Err(IncomingTlsError::PeerClosed)));
        assert_eq!(io.drops.get(), 1);
    }
    assert!(matches!(
        admission.admit((), ConnectCancellation::new(), Instant::now),
        Err(ReceiverError::RateLimited)
    ));
}
