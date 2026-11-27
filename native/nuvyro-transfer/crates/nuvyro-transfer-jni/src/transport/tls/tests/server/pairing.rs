use std::time::{Duration, Instant};

use nuvyro_transfer_core::pairing::PairingQr;
use nuvyro_transfer_core::protocol::{ErrorCode, MessageKind};

use super::wire::{Event, Fault, Operation};
use crate::pairing::ReceiverIdentity;
use crate::transport::tls::{IncomingPairError, IncomingTlsError};
use crate::transport::{ConnectCancellation, ReceiverAdmission, ReceiverContext, ReceiverError};

mod peer;
use peer::{Behavior, Peer};

fn fixture() -> (ReceiverIdentity, ReceiverAdmission) {
    let identity = ReceiverIdentity::create().unwrap();
    let context = ReceiverContext::new(1, 2, *identity.session().as_bytes()).unwrap();
    let admission = ReceiverAdmission::new(context, Instant::now());
    (identity, admission)
}

fn timeout(error: IncomingPairError) -> bool {
    matches!(
        error,
        IncomingPairError::Protocol(ErrorCode::DeadlineExceeded)
            | IncomingPairError::Transport(IncomingTlsError::Context(
                ReceiverError::DeadlineExceeded
            ))
    )
}

#[test]
fn receiver_pairs_on_the_same_tls_connection_and_retains_only_one_paired_slot() {
    let (identity, admission) = fixture();
    let peer = Peer::new(&identity, &admission);
    let paired = peer.run().unwrap();
    assert_eq!(Some(paired.pair()), peer.established.get());
    assert_eq!(paired.pair().secure_session_id, identity.session());
    assert_eq!(paired.claimed_name(), "Sender");
    assert_eq!(format!("{paired:?}"), "PairedIncoming { .. }");
    assert_eq!(
        *peer.seen.borrow(),
        [MessageKind::SessionHelloAck, MessageKind::PairResult]
    );
    assert_eq!(peer.tls.drops.get(), 0);
    let first = admission
        .admit((), ConnectCancellation::new(), || peer.tls.time.get())
        .unwrap();
    let second = admission
        .admit((), ConnectCancellation::new(), || peer.tls.time.get())
        .unwrap();
    assert!(matches!(
        admission.admit((), ConnectCancellation::new(), || peer.tls.time.get()),
        Err(ReceiverError::Busy)
    ));
    drop((first, second, paired));
    assert_eq!(peer.tls.drops.get(), 1);
    drop(Peer::new(&identity, &admission).run().unwrap());
}

#[test]
fn hello_auth_and_both_responses_work_with_one_byte_socket_io() {
    let (identity, admission) = fixture();
    let peer = Peer::new(&identity, &admission);
    peer.tls.fragment.set(1);
    let paired = peer.run().unwrap();
    assert_eq!(Some(paired.pair()), peer.established.get());
    assert!(peer.tls.reads.get() > 100 && peer.tls.writes.get() > 100);
    drop(paired);
    assert_eq!(peer.tls.drops.get(), 1);
}

#[test]
fn the_largest_valid_utf8_name_fits_the_fixed_request_buffer() {
    let (identity, admission) = fixture();
    let peer = Peer::new(&identity, &admission);
    *peer.claimed_name.borrow_mut() = "\u{1f600}".repeat(32);
    peer.tls.fragment.set(1);
    let paired = peer.run().unwrap();
    assert_eq!(paired.claimed_name().len(), 128);
    assert_eq!(paired.claimed_name().chars().count(), 32);
    assert_eq!(paired.claimed_name(), &*peer.claimed_name.borrow());
}

#[test]
fn eof_in_the_middle_of_auth_discards_partial_secrets_without_consuming_the_invitation() {
    let (identity, admission) = fixture();
    let (_, qr) = identity.create_invitation().unwrap();
    let peer = Peer::from_qr(
        &identity,
        &admission,
        PairingQr::decode(qr.as_bytes()).unwrap(),
    );
    peer.behavior.set(Behavior::TruncatedAuth);
    assert!(matches!(
        peer.run(),
        Err(IncomingPairError::Transport(IncomingTlsError::PeerClosed))
    ));
    assert_eq!(peer.tls.drops.get(), 1);
    let retry = Peer::from_qr(
        &identity,
        &admission,
        PairingQr::decode(qr.as_bytes()).unwrap(),
    );
    drop(retry.run().unwrap());
}

#[test]
fn wrong_session_direction_and_unapproved_headers_fail_before_ack() {
    for behavior in [
        Behavior::WrongHello,
        Behavior::AuthFirst,
        Behavior::ReverseAck,
        Behavior::DataHeader,
        Behavior::OversizedHeader,
        Behavior::RemoteError,
    ] {
        let (identity, admission) = fixture();
        let peer = Peer::new(&identity, &admission);
        peer.behavior.set(behavior);
        assert!(matches!(peer.run(), Err(IncomingPairError::Protocol(_))));
        assert!(peer.seen.borrow().is_empty());
        assert_eq!(peer.tls.drops.get(), 1);
    }
}

#[test]
fn wrong_invitation_or_token_never_produces_pair_result() {
    for behavior in [Behavior::WrongToken, Behavior::WrongInvitation] {
        let (identity, admission) = fixture();
        let peer = Peer::new(&identity, &admission);
        peer.behavior.set(behavior);
        assert!(matches!(
            peer.run(),
            Err(IncomingPairError::Protocol(ErrorCode::AuthenticationFailed))
        ));
        assert_eq!(*peer.seen.borrow(), [MessageKind::SessionHelloAck]);
        assert_eq!(peer.tls.drops.get(), 1);
    }
}

#[test]
fn invitation_expiry_uses_receiver_monotonic_issuance_time() {
    let (identity, admission) = fixture();
    let peer = Peer::new(&identity, &admission);
    peer.tls
        .time
        .set(peer.tls.time.get() + Duration::from_secs(121));
    assert!(matches!(
        peer.run(),
        Err(IncomingPairError::Protocol(ErrorCode::AuthenticationFailed))
    ));
    assert_eq!(*peer.seen.borrow(), [MessageKind::SessionHelloAck]);
    assert_eq!(peer.tls.drops.get(), 1);
}

#[test]
fn consumed_invitation_cannot_pair_a_new_connection_after_successful_disconnect() {
    let (identity, admission) = fixture();
    let (_, qr) = identity.create_invitation().unwrap();
    let first = Peer::from_qr(
        &identity,
        &admission,
        PairingQr::decode(qr.as_bytes()).unwrap(),
    );
    drop(first.run().unwrap());
    let replay = Peer::from_qr(
        &identity,
        &admission,
        PairingQr::decode(qr.as_bytes()).unwrap(),
    );
    assert!(matches!(
        replay.run(),
        Err(IncomingPairError::Protocol(ErrorCode::AuthenticationFailed))
    ));
    assert_eq!(replay.tls.drops.get(), 1);
}

#[test]
fn cancel_after_result_flush_releases_both_permits_but_does_not_restore_the_token() {
    let (identity, admission) = fixture();
    let (_, qr) = identity.create_invitation().unwrap();
    let peer = Peer::from_qr(
        &identity,
        &admission,
        PairingQr::decode(qr.as_bytes()).unwrap(),
    );
    peer.cancel_after_result.set(true);
    assert!(matches!(
        peer.run(),
        Err(IncomingPairError::Transport(IncomingTlsError::Context(
            ReceiverError::Cancelled
        )))
    ));
    assert!(peer.established.get().is_some());
    assert_eq!(peer.tls.drops.get(), 1);
    let replay = Peer::from_qr(
        &identity,
        &admission,
        PairingQr::decode(qr.as_bytes()).unwrap(),
    );
    assert!(matches!(
        replay.run(),
        Err(IncomingPairError::Protocol(ErrorCode::AuthenticationFailed))
    ));
    drop(Peer::new(&identity, &admission).run().unwrap());
}

#[test]
fn failure_to_write_pair_result_consumes_the_token_and_releases_the_reservation() {
    for fault in [Fault::Error, Fault::Zero, Fault::AlwaysBlock] {
        let (identity, admission) = fixture();
        let (_, qr) = identity.create_invitation().unwrap();
        let peer = Peer::from_qr(
            &identity,
            &admission,
            PairingQr::decode(qr.as_bytes()).unwrap(),
        );
        peer.fault_after_ack.set(Some((Operation::Write, fault)));
        assert!(peer.run().is_err());
        assert!(peer.established.get().is_none());
        assert_eq!(peer.tls.drops.get(), 1);
        let replay = Peer::from_qr(
            &identity,
            &admission,
            PairingQr::decode(qr.as_bytes()).unwrap(),
        );
        assert!(matches!(
            replay.run(),
            Err(IncomingPairError::Protocol(ErrorCode::AuthenticationFailed))
        ));
        drop(Peer::new(&identity, &admission).run().unwrap());
    }
}

#[test]
fn hello_and_auth_silence_have_separate_absolute_deadlines() {
    for behavior in [Behavior::SilentHello, Behavior::SilentAuth] {
        let (identity, admission) = fixture();
        let peer = Peer::new(&identity, &admission);
        peer.behavior.set(behavior);
        let start = peer.tls.time.get();
        assert!(timeout(peer.run().unwrap_err()));
        assert_eq!(peer.tls.time.get() - start, Duration::from_secs(10));
        assert_eq!(peer.tls.drops.get(), 1);
    }
}

#[test]
fn auth_gets_a_fresh_budget_only_after_ack_is_flushed_within_hello_time() {
    let (identity, admission) = fixture();
    let peer = Peer::new(&identity, &admission);
    peer.delay_ack.set(Duration::from_secs(9));
    peer.delay_auth.set(Duration::from_secs(2));
    let start = peer.tls.time.get();
    drop(peer.run().unwrap());
    assert_eq!(peer.tls.time.get() - start, Duration::from_secs(11));
    let (identity, admission) = fixture();
    let peer = Peer::new(&identity, &admission);
    peer.delay_ack.set(Duration::from_secs(10));
    assert!(timeout(peer.run().unwrap_err()));
    assert_eq!(peer.tls.drops.get(), 1);
}

#[test]
fn buffered_pipelining_and_extra_bytes_are_rejected_at_control_boundaries() {
    for behavior in [
        Behavior::PipelinedAuth,
        Behavior::TrailingHello,
        Behavior::TrailingAuth,
        Behavior::DuplicateHello,
    ] {
        let (identity, admission) = fixture();
        let peer = Peer::new(&identity, &admission);
        peer.behavior.set(behavior);
        assert!(matches!(
            peer.run(),
            Err(IncomingPairError::Protocol(ErrorCode::UnexpectedState))
        ));
        assert!(peer.established.get().is_none());
        assert_eq!(peer.tls.drops.get(), 1);
    }
}

#[test]
fn malformed_auth_and_trailing_bytes_are_not_treated_as_valid_authentication() {
    for behavior in [Behavior::BadName, Behavior::TrailingAuth] {
        let (identity, admission) = fixture();
        let (_, qr) = identity.create_invitation().unwrap();
        let peer = Peer::from_qr(
            &identity,
            &admission,
            PairingQr::decode(qr.as_bytes()).unwrap(),
        );
        peer.behavior.set(behavior);
        assert!(peer.run().is_err());
        let clean = Peer::from_qr(
            &identity,
            &admission,
            PairingQr::decode(qr.as_bytes()).unwrap(),
        );
        drop(clean.run().unwrap());
    }
}

#[test]
fn reads_writes_and_interrupted_polls_retry_inside_the_control_budget() {
    for operation in [Operation::Read, Operation::Write, Operation::Poll] {
        for fault in [Fault::InterruptedOnce, Fault::WouldBlockOnce] {
            if operation == Operation::Poll && matches!(fault, Fault::WouldBlockOnce) {
                continue;
            }
            let (identity, admission) = fixture();
            let peer = Peer::new(&identity, &admission);
            peer.fault_after_ack.set(Some((operation, fault)));
            drop(peer.run().unwrap());
            assert_eq!(peer.tls.drops.get(), 1);
        }
    }
}

#[test]
fn lifecycle_checks_still_apply_during_control_reads_writes_and_polls() {
    for operation in [Operation::Read, Operation::Write, Operation::Poll] {
        for event in [
            Event::Cancel,
            Event::CloseIdentity,
            Event::CloseAdmission,
            Event::StaleNative,
            Event::StaleRoute,
            Event::MissingContext,
            Event::BackwardClock,
            Event::ExpiredWall,
            Event::EarlierWall,
            Event::Deadline,
        ] {
            let (identity, admission) = fixture();
            let peer = Peer::new(&identity, &admission);
            peer.after_ack.set(Some((operation, event)));
            if operation == Operation::Poll {
                peer.behavior.set(Behavior::SilentAuth);
            }
            assert!(peer.run().is_err());
            assert_eq!(peer.tls.drops.get(), 1);
        }
    }
}

#[test]
fn eof_tls_close_and_corrupt_auth_records_cannot_establish_pairing() {
    for behavior in [
        Behavior::CloseHello,
        Behavior::CloseAuth,
        Behavior::CorruptAuth,
    ] {
        let (identity, admission) = fixture();
        let peer = Peer::new(&identity, &admission);
        peer.behavior.set(behavior);
        assert!(peer.run().is_err());
        assert!(peer.established.get().is_none());
        assert_eq!(peer.tls.drops.get(), 1);
    }
    let (identity, admission) = fixture();
    let peer = Peer::new(&identity, &admission);
    peer.fault_after_ack
        .set(Some((Operation::Read, Fault::Zero)));
    assert!(matches!(
        peer.run(),
        Err(IncomingPairError::Transport(IncomingTlsError::PeerClosed))
    ));
}

#[test]
fn an_existing_paired_connection_blocks_a_second_and_the_second_token_stays_consumed() {
    let (identity, admission) = fixture();
    let first_peer = Peer::new(&identity, &admission);
    let first = first_peer.run().unwrap();
    let (_, qr) = identity.create_invitation().unwrap();
    let second = Peer::from_qr(
        &identity,
        &admission,
        PairingQr::decode(qr.as_bytes()).unwrap(),
    );
    assert!(matches!(
        second.run(),
        Err(IncomingPairError::Transport(IncomingTlsError::Context(
            ReceiverError::Busy
        )))
    ));
    assert_eq!(*second.seen.borrow(), [MessageKind::SessionHelloAck]);
    drop(first);
    let replay = Peer::from_qr(
        &identity,
        &admission,
        PairingQr::decode(qr.as_bytes()).unwrap(),
    );
    assert!(matches!(
        replay.run(),
        Err(IncomingPairError::Protocol(ErrorCode::AuthenticationFailed))
    ));
    drop(Peer::new(&identity, &admission).run().unwrap());
}

#[test]
fn failed_control_attempts_still_count_toward_the_original_receiver_tls_rate_limit() {
    let (identity, admission) = fixture();
    for _ in 0..8 {
        let peer = Peer::new(&identity, &admission);
        peer.behavior.set(Behavior::WrongToken);
        assert!(peer.run().is_err());
    }
    assert!(matches!(
        admission.admit((), ConnectCancellation::new(), Instant::now),
        Err(ReceiverError::RateLimited)
    ));
}
