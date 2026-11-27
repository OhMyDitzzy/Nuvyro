use super::*;
use std::sync::{Arc, Barrier};

fn id(byte: u8) -> Id {
    Id::from_bytes([byte; 16]).unwrap()
}
fn store() -> InvitationStore {
    InvitationStore::new(id(0x11), [0x22; 32])
}

#[test]
fn qr_is_canonical_bound_to_identity_and_redacted() {
    let store = store();
    let (invitation, qr) = store.issue(Instant::now()).unwrap();
    let decoded = PairingQr::decode(qr.as_bytes()).unwrap();
    assert_eq!(decoded.secure_session_id(), id(0x11));
    assert_eq!(decoded.invitation_id(), invitation);
    assert_eq!(decoded.spki_pin().as_bytes(), &[0x22; 32]);
    assert_eq!(decoded.advertised_ttl_seconds(), INVITATION_TTL_SECS);
    assert!(qr.as_bytes().len() <= MAX_PAIRING_QR_TEXT_BYTES);
    assert_eq!(format!("{qr:?}"), "SensitiveQr([REDACTED])");
}

#[test]
fn replacement_replay_and_wrong_invitation_fail_with_the_same_remote_reason() {
    let now = Instant::now();
    let store = store();
    let (first, qr1) = store.issue(now).unwrap();
    let decoded1 = PairingQr::decode(qr1.as_bytes()).unwrap();
    let (second, qr2) = store.issue(now).unwrap();
    let decoded2 = PairingQr::decode(qr2.as_bytes()).unwrap();
    assert_ne!(first, second);
    assert_eq!(
        store.authenticate(first, decoded1.token().expose_secret(), now),
        Err(PairingError::AuthenticationFailed)
    );
    assert_eq!(
        store.authenticate(first, decoded2.token().expose_secret(), now),
        Err(PairingError::AuthenticationFailed)
    );
    let pair = store
        .authenticate(second, decoded2.token().expose_secret(), now)
        .unwrap();
    assert_ne!(pair, first);
    assert_ne!(pair, second);
    assert_ne!(pair, store.session());
    assert_eq!(
        store.authenticate(second, decoded2.token().expose_secret(), now),
        Err(PairingError::AuthenticationFailed)
    );
}

#[test]
fn simultaneous_token_holders_have_exactly_one_winner() {
    let now = Instant::now();
    let store = Arc::new(store());
    let (invitation, qr) = store.issue(now).unwrap();
    let parsed = PairingQr::decode(qr.as_bytes()).unwrap();
    let token = Zeroizing::new(*parsed.token().expose_secret());
    let barrier = Arc::new(Barrier::new(8));
    let workers: Vec<_> = (0..8)
        .map(|_| {
            let store = Arc::clone(&store);
            let barrier = Arc::clone(&barrier);
            let token = Zeroizing::new(*token);
            std::thread::spawn(move || {
                barrier.wait();
                store.authenticate(invitation, &token, Instant::now())
            })
        })
        .collect();
    assert_eq!(
        workers
            .into_iter()
            .map(|worker| worker.join().unwrap())
            .filter(Result::is_ok)
            .count(),
        1
    );
}

#[test]
fn fifth_failure_invalidates_even_the_correct_token() {
    let now = Instant::now();
    let store = store();
    let (invitation, qr) = store.issue(now).unwrap();
    let parsed = PairingQr::decode(qr.as_bytes()).unwrap();
    for _ in 0..MAX_INVITATION_FAILURES_PER_MINUTE {
        assert_eq!(
            store.authenticate(invitation, &[0x33; 32], now),
            Err(PairingError::AuthenticationFailed)
        );
    }
    assert_eq!(
        store.authenticate(invitation, parsed.token().expose_secret(), now),
        Err(PairingError::AuthenticationFailed)
    );
}

#[test]
fn failures_outside_the_rolling_window_do_not_disable_a_valid_invitation() {
    let now = Instant::now();
    let store = store();
    let (invitation, qr) = store.issue(now).unwrap();
    let parsed = PairingQr::decode(qr.as_bytes()).unwrap();
    for _ in 0..MAX_INVITATION_FAILURES_PER_MINUTE - 1 {
        assert_eq!(
            store.authenticate(invitation, &[0x33; 32], now),
            Err(PairingError::AuthenticationFailed)
        );
    }
    let later = now + FAILURE_WINDOW;
    assert_eq!(
        store.authenticate(invitation, &[0x33; 32], later),
        Err(PairingError::AuthenticationFailed)
    );
    assert!(
        store
            .authenticate(invitation, parsed.token().expose_secret(), later)
            .is_ok()
    );
}

#[test]
fn expiry_and_time_before_issuance_destroy_the_invitation() {
    let now = Instant::now();
    for observed in [
        now - Duration::from_secs(1),
        now + Duration::from_secs(INVITATION_TTL_SECS),
    ] {
        let store = store();
        let (invitation, qr) = store.issue(now).unwrap();
        let parsed = PairingQr::decode(qr.as_bytes()).unwrap();
        assert_eq!(
            store.authenticate(invitation, parsed.token().expose_secret(), observed),
            Err(PairingError::AuthenticationFailed)
        );
        assert_eq!(
            store.authenticate(invitation, parsed.token().expose_secret(), now),
            Err(PairingError::AuthenticationFailed)
        );
    }
}

#[test]
fn closed_store_cannot_issue_or_authenticate() {
    let now = Instant::now();
    let store = store();
    let (invitation, qr) = store.issue(now).unwrap();
    let parsed = PairingQr::decode(qr.as_bytes()).unwrap();
    store.close();
    store.close();
    assert!(matches!(store.issue(now), Err(PairingError::InvalidState)));
    assert_eq!(
        store.authenticate(invitation, parsed.token().expose_secret(), now),
        Err(PairingError::AuthenticationFailed)
    );
}

#[test]
fn collision_tracking_exhaustion_is_bounded_and_consumes_a_valid_attempt() {
    let now = Instant::now();
    let store = store();
    let (invitation, qr) = store.issue(now).unwrap();
    let parsed = PairingQr::decode(qr.as_bytes()).unwrap();
    store
        .state
        .lock()
        .unwrap()
        .used_ids
        .resize(MAX_SESSION_IDS, id(0x55));
    assert!(matches!(
        store.issue(now),
        Err(PairingError::ResourceFailure)
    ));
    assert_eq!(
        store.authenticate(invitation, parsed.token().expose_secret(), now),
        Err(PairingError::ResourceFailure)
    );
    assert_eq!(
        store.authenticate(invitation, parsed.token().expose_secret(), now),
        Err(PairingError::AuthenticationFailed)
    );
}
