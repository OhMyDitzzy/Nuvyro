mod wire;

use std::time::Duration;

use nuvyro_transfer_core::pairing::PairingQr;
use nuvyro_transfer_core::protocol::{ErrorCode, MessageKind};

use super::super::TlsHandshakeError;
use super::super::pairing::{PairConnectionError, pair_outgoing_with};
use crate::transport::budget::MAX_CONNECTIONS;
use crate::transport::{ConnectError, ConnectionContext};
use wire::{Behavior, Wire};

fn closed(wire: &Wire) {
    assert_eq!(wire.drops.get(), 1);
    let permits: Vec<_> = (0..MAX_CONNECTIONS)
        .map(|_| wire.budget.acquire().unwrap())
        .collect();
    assert!(matches!(
        wire.budget.acquire(),
        Err(ConnectError::ResourceLimit)
    ));
    drop(permits);
}

#[test]
fn outgoing_owner_pairs_over_tls_and_retains_the_socket_without_file_capabilities() {
    let (wire, qr) = Wire::new();
    let paired = wire.run(qr).unwrap();
    assert_eq!(paired.pair().secure_session_id, wire.identity.session());
    assert_eq!(Some(paired.pair()), wire.established.get());
    assert_eq!(
        *wire.seen.borrow(),
        [MessageKind::SessionHello, MessageKind::PairAuthenticate]
    );
    assert_eq!(format!("{paired:?}"), "PairedConnection { .. }");
    assert_eq!(wire.drops.get(), 0);
    drop(paired);
    closed(&wire);
}

#[test]
fn every_tls_and_control_byte_can_arrive_in_a_separate_socket_operation() {
    let (wire, qr) = Wire::new();
    wire.max_io.set(1);
    drop(wire.run(qr).unwrap());
    assert_eq!(wire.seen.borrow().len(), 2);
    closed(&wire);
}

#[test]
fn target_session_mismatch_is_rejected_before_any_tls_or_application_write() {
    let (wire, qr) = Wire::new();
    let other = ConnectionContext::new(1, 2, 7, [0x55; 16]).unwrap();
    let result = pair_outgoing_with(wire.pending(other), &wire, qr, "Sender".to_owned(), || {
        Some(other)
    });
    assert!(matches!(
        result,
        Err(PairConnectionError::Protocol(ErrorCode::UnexpectedState))
    ));
    assert!(wire.seen.borrow().is_empty());
    closed(&wire);
}

#[test]
fn a_different_qr_pin_fails_before_sending_hello_or_invitation() {
    let (wire, _) = Wire::new();
    let (_, qr) = wire.identity.create_invitation().unwrap();
    let mut text = zeroize::Zeroizing::new(qr.as_bytes().to_vec());
    // Keep the real invitation and session while substituting its scanned pin.
    use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
    let prefix = nuvyro_transfer_core::protocol::PAIRING_QR_PREFIX;
    let mut payload = zeroize::Zeroizing::new([0; 104]);
    assert_eq!(
        URL_SAFE_NO_PAD
            .decode_slice(&text[prefix.len()..], &mut *payload)
            .unwrap(),
        104
    );
    payload[36] ^= 1;
    let mut encoded = zeroize::Zeroizing::new([0; 140]);
    let count = URL_SAFE_NO_PAD
        .encode_slice(payload.as_slice(), &mut *encoded)
        .unwrap();
    text.truncate(prefix.len());
    text.extend_from_slice(&encoded[..count]);
    assert!(wire.run(PairingQr::decode(&text).unwrap()).is_err());
    assert!(wire.seen.borrow().is_empty());
    closed(&wire);
}

#[test]
fn wrong_ack_session_never_releases_the_invitation() {
    let (wire, qr) = Wire::new();
    wire.behavior.set(Behavior::WrongAck);
    assert!(matches!(
        wire.run(qr),
        Err(PairConnectionError::Protocol(ErrorCode::SessionExpired))
    ));
    assert_eq!(*wire.seen.borrow(), [MessageKind::SessionHello]);
    closed(&wire);
}

#[test]
fn wrong_result_or_auth_refusal_cannot_establish_a_pair() {
    for behavior in [Behavior::WrongResult, Behavior::RefuseAuth] {
        let (wire, qr) = Wire::new();
        wire.behavior.set(behavior);
        assert!(matches!(
            wire.run(qr),
            Err(PairConnectionError::Protocol(
                ErrorCode::SessionExpired | ErrorCode::AuthenticationFailed
            ))
        ));
        closed(&wire);
    }
}

#[test]
fn unapproved_data_and_oversized_headers_are_rejected_before_reading_the_body() {
    for behavior in [Behavior::DataHeader, Behavior::OversizedHeader] {
        let (wire, qr) = Wire::new();
        wire.behavior.set(behavior);
        assert!(matches!(
            wire.run(qr),
            Err(PairConnectionError::Protocol(
                ErrorCode::UnexpectedState | ErrorCode::MalformedFrame
            ))
        ));
        assert_eq!(wire.seen.borrow().len(), 1);
        closed(&wire);
    }
}

#[test]
fn response_pipelining_and_extra_bytes_after_pair_result_fail_closed() {
    for behavior in [Behavior::PipelinedResult, Behavior::TrailingResult] {
        let (wire, qr) = Wire::new();
        wire.behavior.set(behavior);
        assert!(matches!(
            wire.run(qr),
            Err(PairConnectionError::Protocol(ErrorCode::UnexpectedState))
        ));
        if behavior == Behavior::PipelinedResult {
            assert_eq!(wire.seen.borrow().len(), 1);
        }
        closed(&wire);
    }
}

#[test]
fn tls_close_and_raw_eof_do_not_establish_pairing() {
    let (wire, qr) = Wire::new();
    wire.behavior.set(Behavior::ClosedResult);
    assert!(matches!(
        wire.run(qr),
        Err(PairConnectionError::Transport(
            TlsHandshakeError::PeerClosed
        ))
    ));
    closed(&wire);
    let (wire, qr) = Wire::new();
    wire.raw_eof.set(true);
    assert!(matches!(
        wire.run(qr),
        Err(PairConnectionError::Transport(
            TlsHandshakeError::PeerClosed
        ))
    ));
    closed(&wire);
}

#[test]
fn hello_and_auth_silence_hit_their_own_phase_deadlines() {
    for behavior in [Behavior::SilentHello, Behavior::SilentAuth] {
        let (wire, qr) = Wire::new();
        wire.behavior.set(behavior);
        let started = wire.time.get();
        assert!(matches!(
            wire.run(qr),
            Err(PairConnectionError::Transport(TlsHandshakeError::Context(
                ConnectError::DeadlineExceeded
            )))
        ));
        assert_eq!(
            wire.time.get().duration_since(started),
            Duration::from_secs(10)
        );
        assert_eq!(wire.polls.borrow().len(), 200);
        closed(&wire);
    }
}

#[test]
fn cancellation_after_authentication_drops_the_socket_and_consumes_the_invitation() {
    let (wire, _) = Wire::new();
    let (_, text) = wire.identity.create_invitation().unwrap();
    let qr = PairingQr::decode(text.as_bytes()).unwrap();
    let replay = PairingQr::decode(text.as_bytes()).unwrap();
    wire.cancel_after_frame.set(2);
    assert!(matches!(
        wire.run(qr),
        Err(PairConnectionError::Transport(TlsHandshakeError::Context(
            ConnectError::Cancelled
        )))
    ));
    closed(&wire);
    wire.cancel_after_frame.set(usize::MAX);
    assert!(matches!(
        wire.run(replay),
        Err(PairConnectionError::Protocol(
            ErrorCode::AuthenticationFailed
        ))
    ));
    assert_eq!(wire.drops.get(), 2);
}

#[test]
fn route_loss_and_clock_reversal_during_control_io_are_terminal() {
    let (wire, qr) = Wire::new();
    wire.invalidate_on_write.set(true);
    assert!(matches!(
        wire.run(qr),
        Err(PairConnectionError::Transport(TlsHandshakeError::Context(
            ConnectError::StaleContext
        )))
    ));
    closed(&wire);
    let (wire, qr) = Wire::new();
    wire.rewind_on_read.set(true);
    assert!(matches!(
        wire.run(qr),
        Err(PairConnectionError::Transport(TlsHandshakeError::Context(
            ConnectError::ClockInvalid
        )))
    ));
    closed(&wire);
}

#[test]
fn interrupted_reads_polls_and_a_blocked_write_retry_inside_the_same_budget() {
    let (wire, qr) = Wire::new();
    wire.interrupt_read.set(true);
    wire.interrupt_poll.set(true);
    wire.block_write.set(true);
    drop(wire.run(qr).unwrap());
    assert!(!wire.polls.borrow().is_empty());
    closed(&wire);
}

#[test]
fn socket_errors_zero_progress_and_expiry_after_a_write_release_ownership() {
    for fault in 0..3 {
        let (wire, qr) = Wire::new();
        match fault {
            0 => wire.fail_write.set(true),
            1 => wire.zero_write.set(true),
            _ => wire.advance_on_write.set(Duration::from_secs(10)),
        }
        assert!(wire.run(qr).is_err());
        closed(&wire);
    }
}

#[test]
fn successful_tls_and_hello_transitions_give_auth_its_own_deadline() {
    let (wire, qr) = Wire::new();
    let started = wire.time.get();
    wire.advance_on_write.set(Duration::from_secs(5));
    drop(wire.run(qr).unwrap());
    assert_eq!(
        wire.time.get().duration_since(started),
        Duration::from_secs(10)
    );
    closed(&wire);
}

#[test]
fn cancellation_during_a_readiness_poll_is_checked_before_retry() {
    let (wire, qr) = Wire::new();
    wire.behavior.set(Behavior::SilentHello);
    wire.cancel_on_poll.set(true);
    wire.interrupt_poll.set(true);
    assert!(matches!(
        wire.run(qr),
        Err(PairConnectionError::Transport(TlsHandshakeError::Context(
            ConnectError::Cancelled
        )))
    ));
    assert_eq!(wire.polls.borrow().len(), 1);
    closed(&wire);
}

#[test]
fn a_corrupt_tls_control_record_fails_before_authentication() {
    let (wire, qr) = Wire::new();
    wire.corrupt_response.set(true);
    assert!(matches!(
        wire.run(qr),
        Err(PairConnectionError::Transport(
            TlsHandshakeError::ProtocolFailure
        ))
    ));
    assert_eq!(wire.seen.borrow().len(), 1);
    closed(&wire);
}
