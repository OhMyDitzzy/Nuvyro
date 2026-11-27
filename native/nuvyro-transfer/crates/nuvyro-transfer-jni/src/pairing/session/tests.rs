use super::*;
use crate::transport::{test_check_tls_profile, test_tls_client};
use nuvyro_transfer_core::pairing::PairingQr;
use rustls::{ClientConnection, HandshakeKind, ProtocolVersion};

fn handshake(client: &mut ClientConnection, server: &mut ServerConnection) -> Result<(), ()> {
    for _ in 0..64 {
        if client.wants_write() {
            let mut bytes = Vec::new();
            client.write_tls(&mut bytes).map_err(|_| ())?;
            server
                .read_tls(&mut std::io::Cursor::new(bytes))
                .map_err(|_| ())?;
            server.process_new_packets().map_err(|_| ())?;
        }
        if server.wants_write() {
            let mut bytes = Vec::new();
            server.write_tls(&mut bytes).map_err(|_| ())?;
            client
                .read_tls(&mut std::io::Cursor::new(bytes))
                .map_err(|_| ())?;
            client.process_new_packets().map_err(|_| ())?;
        }
        if !client.is_handshaking()
            && !server.is_handshaking()
            && !client.wants_write()
            && !server.wants_write()
        {
            return Ok(());
        }
    }
    Err(())
}

#[test]
fn session_identity_and_qr_interoperate_with_the_existing_pinned_tls_client() {
    let identity = ReceiverIdentity::create().unwrap();
    let (_, qr) = identity.create_invitation().unwrap();
    let scanned = PairingQr::decode(qr.as_bytes()).unwrap();
    assert_eq!(identity.session(), scanned.secure_session_id());
    assert_eq!(identity.pin(), scanned.spki_pin().as_bytes());
    assert_eq!(format!("{identity:?}"), "ReceiverIdentity([REDACTED])");
    for _ in 0..2 {
        let mut client = test_tls_client(scanned.spki_pin()).unwrap();
        let mut server = identity.new_server().unwrap();
        assert_eq!(handshake(&mut client, &mut server), Ok(()));
        assert_eq!(test_check_tls_profile(&client), Ok(()));
        assert_eq!(client.protocol_version(), Some(ProtocolVersion::TLSv1_3));
        assert_eq!(client.alpn_protocol(), Some(TLS_ALPN));
        assert_eq!(client.handshake_kind(), Some(HandshakeKind::Full));
        assert_eq!(server.handshake_kind(), Some(HandshakeKind::Full));
    }
    assert_eq!(identity.config.send_tls13_tickets, 0);
    assert!(!identity.config.ticketer.enabled());
    assert_eq!(identity.config.max_early_data_size, 0);
    assert!(!identity.config.send_half_rtt_data);
    assert!(!identity.config.enable_secret_extraction);
    assert!(!identity.config.key_log.will_log("CLIENT_RANDOM"));
}

#[test]
fn different_sessions_have_fresh_keys_ids_and_reject_old_pins() {
    let first = ReceiverIdentity::create().unwrap();
    let second = ReceiverIdentity::create().unwrap();
    assert_ne!(first.session(), second.session());
    assert_ne!(first.pin(), second.pin());
    let (_, qr) = first.create_invitation().unwrap();
    let scanned = PairingQr::decode(qr.as_bytes()).unwrap();
    let mut client = test_tls_client(scanned.spki_pin()).unwrap();
    let mut server = second.new_server().unwrap();
    assert!(handshake(&mut client, &mut server).is_err());
}

#[test]
fn invalid_wall_time_and_expiry_fail_closed_without_reopening_identity() {
    let now = Instant::now();
    assert!(matches!(
        ReceiverIdentity::at(UNIX_EPOCH - Duration::from_secs(1), now),
        Err(ErrorCode::SessionExpired)
    ));
    for outside in [Duration::ZERO, LIFETIME] {
        let wall = SystemTime::now();
        let identity = ReceiverIdentity::at(wall, now).unwrap();
        let observed = if outside.is_zero() {
            now - Duration::from_secs(1)
        } else {
            now + outside
        };
        assert_eq!(
            identity.validate_at(wall, observed),
            Err(ErrorCode::SessionExpired)
        );
        assert!(matches!(
            identity.new_server(),
            Err(ErrorCode::SessionExpired)
        ));
        assert!(matches!(
            identity.create_invitation(),
            Err(ErrorCode::SessionExpired)
        ));
    }
    let identity = ReceiverIdentity::create().unwrap();
    assert_eq!(
        identity.validate_at(identity.not_after + Duration::from_secs(1), Instant::now()),
        Err(ErrorCode::SessionExpired)
    );
    let identity = ReceiverIdentity::create().unwrap();
    assert_eq!(
        identity.validate_at(identity.not_before - Duration::from_secs(1), Instant::now()),
        Err(ErrorCode::SessionExpired)
    );
}

#[test]
fn closing_or_dropping_identity_revokes_invites_held_by_existing_gates() {
    for explicitly_close in [false, true] {
        let identity = ReceiverIdentity::create().unwrap();
        let mut gate = identity.incoming_gate().unwrap();
        let invitations = Arc::clone(&identity.invitations);
        let (invitation, qr) = identity.create_invitation().unwrap();
        let scanned = PairingQr::decode(qr.as_bytes()).unwrap();
        if explicitly_close {
            identity.close();
        }
        drop(identity);
        assert_eq!(
            invitations.authenticate(invitation, scanned.token().expose_secret(), Instant::now()),
            Err(super::super::PairingError::AuthenticationFailed)
        );
        assert_eq!(
            gate.check_time(Instant::now()),
            Err(ErrorCode::AuthenticationFailed)
        );
    }
}
