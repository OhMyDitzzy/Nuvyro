use std::collections::VecDeque;
use std::fmt;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use nuvyro_transfer_core::pairing::PairingQr;
use nuvyro_transfer_core::protocol::limits::{
    INVITATION_TTL_SECS, MAX_INVITATION_FAILURES_PER_MINUTE, MAX_PAIRING_QR_TEXT_BYTES,
    PAIRING_QR_PAYLOAD_BYTES,
};
use nuvyro_transfer_core::protocol::{
    ErrorCode, Id, PAIRING_QR_PREFIX, TRANSFER_PORT, WIRE_MAJOR, WIRE_MINOR,
};
use subtle::ConstantTimeEq;
use zeroize::Zeroizing;

use super::random_id;

const FAILURE_WINDOW: Duration = Duration::from_secs(60);
const QR_ENCODED_BYTES: usize = 140;
// Bound collision tracking as well as secret storage; exhaustion requires a new session.
const MAX_SESSION_IDS: usize = 1024;

pub(crate) struct SensitiveQr(Zeroizing<Vec<u8>>);

impl SensitiveQr {
    pub(crate) fn as_bytes(&self) -> &[u8] {
        &self.0
    }
}

impl fmt::Debug for SensitiveQr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("SensitiveQr([REDACTED])")
    }
}

struct Invitation {
    id: Id,
    token: Zeroizing<[u8; 32]>,
    issued: Instant,
    expires: Instant,
    failures: VecDeque<Instant>,
}

#[derive(Default)]
struct State {
    invitation: Option<Invitation>,
    used_ids: Vec<Id>,
    closed: bool,
}

/// One outstanding invitation bound to the session's immutable SPKI pin.
/// Checking and consuming is one mutex transaction, shared by all acceptors.
pub(crate) struct InvitationStore {
    session: Id,
    pin: [u8; 32],
    state: Mutex<State>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum PairingError {
    InvalidState,
    AuthenticationFailed,
    ResourceFailure,
}

impl PairingError {
    pub(crate) const fn remote_code(self) -> ErrorCode {
        match self {
            Self::InvalidState => ErrorCode::UnexpectedState,
            Self::AuthenticationFailed => ErrorCode::AuthenticationFailed,
            Self::ResourceFailure => ErrorCode::InternalFailure,
        }
    }
}

impl InvitationStore {
    pub(super) fn new(session: Id, pin: [u8; 32]) -> Self {
        Self {
            session,
            pin,
            state: Mutex::new(State::default()),
        }
    }

    pub(super) fn session(&self) -> Id {
        self.session
    }

    pub(super) fn is_open(&self) -> bool {
        self.state.lock().is_ok_and(|state| !state.closed)
    }

    pub(super) fn issue(&self, now: Instant) -> Result<(Id, SensitiveQr), PairingError> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| PairingError::ResourceFailure)?;
        if state.closed {
            return Err(PairingError::InvalidState);
        }
        let id = self.fresh_id(&mut state)?;
        let mut token = Zeroizing::new([0u8; 32]);
        for _ in 0..4 {
            getrandom::fill(&mut *token).map_err(|_| PairingError::ResourceFailure)?;
            if *token != [0; 32] {
                break;
            }
        }
        if *token == [0; 32] {
            return Err(PairingError::ResourceFailure);
        }
        let expires = now
            .checked_add(Duration::from_secs(INVITATION_TTL_SECS))
            .ok_or(PairingError::ResourceFailure)?;
        let qr = make_qr(self.session, id, &self.pin, &token)?;
        state.invitation = Some(Invitation {
            id,
            token,
            issued: now,
            expires,
            failures: VecDeque::new(),
        });
        state.used_ids.push(id);
        Ok((id, qr))
    }

    /// Valid authentication consumes the invitation even if allocating a pair ID fails.
    pub(super) fn authenticate(
        &self,
        invitation_id: Id,
        token: &[u8; 32],
        now: Instant,
    ) -> Result<Id, PairingError> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| PairingError::ResourceFailure)?;
        if state.closed {
            return Err(PairingError::AuthenticationFailed);
        }
        let Some(invitation) = state.invitation.as_mut() else {
            return Err(PairingError::AuthenticationFailed);
        };
        if now < invitation.issued || now >= invitation.expires {
            state.invitation = None;
            return Err(PairingError::AuthenticationFailed);
        }
        while invitation
            .failures
            .front()
            .is_some_and(|failed| now.saturating_duration_since(*failed) >= FAILURE_WINDOW)
        {
            invitation.failures.pop_front();
        }
        let id_matches = bool::from(invitation.id.as_bytes().ct_eq(invitation_id.as_bytes()));
        let token_matches = bool::from(invitation.token.as_slice().ct_eq(token.as_slice()));
        if !(id_matches & token_matches) {
            invitation
                .failures
                .try_reserve(1)
                .map_err(|_| PairingError::ResourceFailure)?;
            invitation.failures.push_back(now);
            if invitation.failures.len() >= MAX_INVITATION_FAILURES_PER_MINUTE {
                state.invitation = None;
            }
            return Err(PairingError::AuthenticationFailed);
        }
        state.invitation = None;
        let pair_id = self.fresh_id(&mut state)?;
        state.used_ids.push(pair_id);
        Ok(pair_id)
    }

    fn fresh_id(&self, state: &mut State) -> Result<Id, PairingError> {
        if state.used_ids.len() >= MAX_SESSION_IDS {
            return Err(PairingError::ResourceFailure);
        }
        state
            .used_ids
            .try_reserve(1)
            .map_err(|_| PairingError::ResourceFailure)?;
        for _ in 0..4 {
            let id = random_id().map_err(|_| PairingError::ResourceFailure)?;
            if id != self.session && !state.used_ids.contains(&id) {
                return Ok(id);
            }
        }
        Err(PairingError::ResourceFailure)
    }

    pub(super) fn close(&self) {
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        state.closed = true;
        state.invitation = None;
    }

    pub(super) fn revoke(&self) {
        if let Ok(mut state) = self.state.lock() {
            state.invitation = None;
        }
    }

    pub(super) fn expire(&self, now: Instant) {
        if let Ok(mut state) = self.state.lock()
            && state
                .invitation
                .as_ref()
                .is_some_and(|invitation| now < invitation.issued || now >= invitation.expires)
        {
            state.invitation = None;
        }
    }
}

fn make_qr(
    session: Id,
    invitation: Id,
    pin: &[u8; 32],
    token: &[u8; 32],
) -> Result<SensitiveQr, PairingError> {
    let mut payload = Zeroizing::new([0u8; PAIRING_QR_PAYLOAD_BYTES]);
    payload[0..2].copy_from_slice(&WIRE_MAJOR.to_be_bytes());
    payload[2..4].copy_from_slice(&WIRE_MINOR.to_be_bytes());
    payload[4..20].copy_from_slice(session.as_bytes());
    payload[20..36].copy_from_slice(invitation.as_bytes());
    payload[36..68].copy_from_slice(pin);
    payload[68..100].copy_from_slice(token);
    payload[100..102].copy_from_slice(&TRANSFER_PORT.to_be_bytes());
    payload[102..104].copy_from_slice(&(INVITATION_TTL_SECS as u16).to_be_bytes());
    let mut encoded = Zeroizing::new([0u8; QR_ENCODED_BYTES]);
    let encoded_len = URL_SAFE_NO_PAD
        .encode_slice(payload.as_slice(), &mut encoded[..])
        .map_err(|_| PairingError::ResourceFailure)?;
    let mut text = Zeroizing::new(Vec::new());
    text.try_reserve_exact(PAIRING_QR_PREFIX.len() + encoded_len)
        .map_err(|_| PairingError::ResourceFailure)?;
    text.extend_from_slice(PAIRING_QR_PREFIX.as_bytes());
    text.extend_from_slice(&encoded[..encoded_len]);
    if text.len() > MAX_PAIRING_QR_TEXT_BYTES {
        return Err(PairingError::ResourceFailure);
    }
    PairingQr::decode(&text).map_err(|_| PairingError::ResourceFailure)?;
    Ok(SensitiveQr(text))
}

#[cfg(test)]
mod tests;
