use super::*;
use nuvyro_transfer_core::protocol::{FileContext, FrameHeader, TransferContext};

fn id(byte: u8) -> Id {
    Id::from_bytes([byte; 16]).unwrap()
}

fn setup(
    now: Instant,
) -> (
    HandshakeGate,
    HandshakeGate,
    Arc<InvitationStore>,
    PairingQr,
) {
    let invitations = Arc::new(InvitationStore::new(id(0x11), [0x22; 32]));
    let (_, qr) = invitations.issue(now).unwrap();
    let outgoing = HandshakeGate::outgoing(
        PairingQr::decode(qr.as_bytes()).unwrap(),
        "Claimed name".to_owned(),
        now,
    )
    .unwrap();
    let incoming = HandshakeGate::incoming(Arc::clone(&invitations), now).unwrap();
    (
        outgoing,
        incoming,
        invitations,
        PairingQr::decode(qr.as_bytes()).unwrap(),
    )
}

fn frame(action: HandshakeAction) -> EncodedFrame {
    let HandshakeAction::Send(frame) = action else {
        panic!("expected control response")
    };
    frame
}

fn decode(frame: &EncodedFrame) -> Message<'_> {
    let bytes = frame.as_slice();
    let header = FrameHeader::decode(bytes[..6].try_into().unwrap()).unwrap();
    Message::decode(header, &bytes[6..]).unwrap()
}

fn through_hello(
    outgoing: &mut HandshakeGate,
    incoming: &mut HandshakeGate,
    now: Instant,
) -> EncodedFrame {
    let hello = frame(outgoing.initial_message(now).unwrap());
    assert_eq!(outgoing.sent(now).unwrap(), None);
    let ack = frame(incoming.receive(decode(&hello), now).unwrap());
    assert_eq!(incoming.sent(now).unwrap(), None);
    let auth = frame(outgoing.receive(decode(&ack), now).unwrap());
    assert_eq!(outgoing.sent(now).unwrap(), None);
    auth
}

#[test]
fn both_roles_establish_the_same_pair_without_creating_a_transfer() {
    let now = Instant::now();
    let (mut outgoing, mut incoming, _, _) = setup(now);
    let auth = through_hello(&mut outgoing, &mut incoming, now);
    assert!(outgoing.credentials.is_none());
    let result = frame(incoming.receive(decode(&auth), now).unwrap());
    assert!(matches!(incoming.state, State::SendingPairResult(_)));
    let receiver_pair = incoming.sent(now).unwrap().unwrap();
    let HandshakeAction::Established(sender_pair) = outgoing.receive(decode(&result), now).unwrap()
    else {
        panic!("expected established pair")
    };
    assert_eq!(receiver_pair, sender_pair);
    assert_eq!(sender_pair.secure_session_id, id(0x11));
    assert!(matches!(outgoing.state, State::Paired));
    assert!(matches!(incoming.state, State::Paired));
}

#[test]
fn sending_control_frames_is_a_gate_and_cannot_be_repeated() {
    let now = Instant::now();
    let (mut outgoing, mut incoming, _, _) = setup(now);
    let hello = frame(outgoing.initial_message(now).unwrap());
    assert!(matches!(
        outgoing.initial_message(now),
        Err(ErrorCode::UnexpectedState)
    ));
    assert!(outgoing.credentials.is_none());
    let _ack = incoming.receive(decode(&hello), now).unwrap();
    assert!(matches!(
        incoming.receive(decode(&hello), now),
        Err(ErrorCode::UnexpectedState)
    ));
    assert_eq!(incoming.sent(now), Err(ErrorCode::UnexpectedState));
}

#[test]
fn wrong_session_hello_and_ack_fail_before_the_token_is_sent() {
    let now = Instant::now();
    let (mut outgoing, mut incoming, _, _) = setup(now);
    assert!(matches!(
        incoming.receive(
            Message::SessionHello(Hello {
                secure_session_id: id(0x55)
            }),
            now
        ),
        Err(ErrorCode::SessionExpired)
    ));
    outgoing.initial_message(now).unwrap();
    outgoing.sent(now).unwrap();
    assert!(matches!(
        outgoing.receive(
            Message::SessionHelloAck(Hello {
                secure_session_id: id(0x55)
            }),
            now
        ),
        Err(ErrorCode::SessionExpired)
    ));
    assert!(outgoing.credentials.is_none());
}

#[test]
fn wrong_result_session_and_wrong_direction_are_terminal() {
    let now = Instant::now();
    let (mut outgoing, mut incoming, _, _) = setup(now);
    through_hello(&mut outgoing, &mut incoming, now);
    assert!(matches!(
        outgoing.receive(
            Message::PairResult {
                pair_id: id(0x66),
                secure_session_id: id(0x55),
            },
            now
        ),
        Err(ErrorCode::SessionExpired)
    ));
    assert!(matches!(
        outgoing.receive(
            Message::PairResult {
                pair_id: id(0x66),
                secure_session_id: id(0x11),
            },
            now
        ),
        Err(ErrorCode::UnexpectedState)
    ));
    let (mut outgoing, _, _, _) = setup(now);
    assert!(matches!(
        outgoing.receive(
            Message::SessionHello(Hello {
                secure_session_id: id(0x11)
            }),
            now
        ),
        Err(ErrorCode::UnexpectedState)
    ));
}

#[test]
fn invalid_auth_is_terminal_and_does_not_leave_a_retrying_gate() {
    let now = Instant::now();
    let (mut outgoing, mut incoming, _, qr) = setup(now);
    let auth = through_hello(&mut outgoing, &mut incoming, now);
    assert!(matches!(
        incoming.receive(
            Message::PairAuthenticate {
                invitation_id: qr.invitation_id(),
                token: &[0x55; 32],
                claimed_sender_name: "Sender",
            },
            now
        ),
        Err(ErrorCode::AuthenticationFailed)
    ));
    assert!(incoming.credentials.is_none());
    assert!(matches!(
        incoming.receive(decode(&auth), now),
        Err(ErrorCode::UnexpectedState)
    ));
}

#[test]
fn losing_the_connection_after_valid_auth_consumes_the_invitation() {
    let now = Instant::now();
    let (mut outgoing, mut incoming, invitations, qr) = setup(now);
    let auth = through_hello(&mut outgoing, &mut incoming, now);
    incoming.receive(decode(&auth), now).unwrap();
    incoming.abort();
    assert_eq!(
        invitations.authenticate(qr.invitation_id(), qr.token().expose_secret(), now),
        Err(PairingError::AuthenticationFailed)
    );
    assert_eq!(incoming.sent(now), Err(ErrorCode::UnexpectedState));
}

#[test]
fn a_closed_receiver_session_revokes_existing_gates() {
    let now = Instant::now();
    let (mut outgoing, mut incoming, invitations, _) = setup(now);
    let auth = through_hello(&mut outgoing, &mut incoming, now);
    invitations.close();
    assert!(matches!(
        incoming.receive(decode(&auth), now),
        Err(ErrorCode::AuthenticationFailed)
    ));
    assert!(incoming.credentials.is_none());
}

#[test]
fn hello_and_auth_have_absolute_deadlines_and_reject_clock_reversal() {
    let now = Instant::now();
    let (mut outgoing, _, _, _) = setup(now);
    outgoing.initial_message(now).unwrap();
    outgoing.sent(now + Duration::from_secs(5)).unwrap();
    assert!(matches!(
        outgoing.receive(
            Message::SessionHelloAck(Hello {
                secure_session_id: id(0x11)
            }),
            now + Duration::from_secs(HELLO_TIMEOUT_SECS)
        ),
        Err(ErrorCode::DeadlineExceeded)
    ));
    let (mut outgoing, mut incoming, _, _) = setup(now);
    let auth = through_hello(&mut outgoing, &mut incoming, now);
    assert!(matches!(
        incoming.receive(decode(&auth), now + Duration::from_secs(AUTH_TIMEOUT_SECS)),
        Err(ErrorCode::DeadlineExceeded)
    ));
    let (_, mut incoming, _, _) = setup(now);
    incoming.check_time(now + Duration::from_secs(1)).unwrap();
    assert_eq!(incoming.check_time(now), Err(ErrorCode::DeadlineExceeded));
    assert!(incoming.credentials.is_none());
}

#[test]
fn file_bytes_and_scoped_errors_are_rejected_before_pairing() {
    let now = Instant::now();
    let (_, mut incoming, _, _) = setup(now);
    let context = FileContext {
        transfer: TransferContext {
            pair_id: id(0x66),
            transfer_id: id(0x77),
        },
        file_id: id(0x55),
    };
    assert!(matches!(
        incoming.receive(
            Message::DataChunk {
                context,
                offset: 0,
                bytes: b"file"
            },
            now
        ),
        Err(ErrorCode::UnexpectedState)
    ));
    let (_, mut incoming, _, _) = setup(now);
    assert!(matches!(
        incoming.receive(
            Message::Error {
                pair_id: Some(id(0x66)),
                transfer_id: None,
                code: ErrorCode::Busy,
                scope: ErrorScope::Connection
            },
            now
        ),
        Err(ErrorCode::UnexpectedState)
    ));
    let (_, mut incoming, _, _) = setup(now);
    assert!(matches!(
        incoming.receive(
            Message::Error {
                pair_id: None,
                transfer_id: None,
                code: ErrorCode::Busy,
                scope: ErrorScope::Connection
            },
            now
        ),
        Err(ErrorCode::Busy)
    ));
}

#[test]
fn invalid_names_and_debug_output_cannot_bypass_or_expose_credentials() {
    let now = Instant::now();
    for name in [
        "".to_owned(),
        "x".repeat(33),
        "\u{85}".to_owned(),
        "🙂".repeat(33),
    ] {
        let (_, _, _, qr) = setup(now);
        assert!(matches!(
            HandshakeGate::outgoing(qr, name, now),
            Err(ErrorCode::MalformedFrame)
        ));
    }
    let (mut outgoing, mut incoming, _, _) = setup(now);
    assert!(!format!("{outgoing:?}").contains("Claimed name"));
    let auth = through_hello(&mut outgoing, &mut incoming, now);
    assert_eq!(format!("{auth:?}"), "EncodedFrame([REDACTED])");
}

#[test]
fn revocation_while_pair_result_is_pending_prevents_establishment() {
    let now = Instant::now();
    let (mut outgoing, mut incoming, invitations, _) = setup(now);
    let auth = through_hello(&mut outgoing, &mut incoming, now);
    incoming.receive(decode(&auth), now).unwrap();
    invitations.close();
    assert_eq!(incoming.sent(now), Err(ErrorCode::AuthenticationFailed));
}
