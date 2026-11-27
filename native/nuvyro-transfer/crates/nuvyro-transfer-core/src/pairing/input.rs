use std::fmt;

use zeroize::Zeroizing;

use super::PairingQrError;
use crate::protocol::limits::{INVITATION_TTL_SECS, PAIRING_QR_PAYLOAD_BYTES};
use crate::protocol::{Id, TRANSFER_PORT, WIRE_MAJOR, WIRE_MINOR};

/// Unverified SHA-256 SPKI pin from a scan. TLS must verify the peer against it.
pub struct SpkiPin(Zeroizing<[u8; 32]>);

impl SpkiPin {
    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

impl fmt::Debug for SpkiPin {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("SpkiPin([REDACTED])")
    }
}

/// Borrow only for authentication inside pinned TLS; never log or persist it.
pub struct InvitationToken(Zeroizing<[u8; 32]>);

impl InvitationToken {
    pub fn expose_secret(&self) -> &[u8; 32] {
        &self.0
    }
}

impl fmt::Debug for InvitationToken {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("InvitationToken([REDACTED])")
    }
}

/// Structurally valid scan data, not a paired or authorized session.
///
/// Pin/token storage is zeroized on drop. Caller-owned QR input, explicit copies,
/// and copies left by moves or other libraries are outside that guarantee.
/// There is no implicit cloning, equality, serialization, or text encoding.
pub struct PairingQr {
    secure_session_id: Id,
    invitation_id: Id,
    spki_pin: SpkiPin,
    token: InvitationToken,
}

impl PairingQr {
    pub fn secure_session_id(&self) -> Id {
        self.secure_session_id
    }

    pub fn invitation_id(&self) -> Id {
        self.invitation_id
    }

    pub fn spki_pin(&self) -> &SpkiPin {
        &self.spki_pin
    }

    pub fn token(&self) -> &InvitationToken {
        &self.token
    }

    pub const fn transfer_port(&self) -> u16 {
        TRANSFER_PORT
    }

    /// This hint must not create or extend the receiver's monotonic expiry.
    pub const fn advertised_ttl_seconds(&self) -> u64 {
        INVITATION_TTL_SECS
    }

    pub(super) fn from_payload(
        payload: &[u8; PAIRING_QR_PAYLOAD_BYTES],
    ) -> Result<Self, PairingQrError> {
        if u16::from_be_bytes([payload[0], payload[1]]) != WIRE_MAJOR
            || u16::from_be_bytes([payload[2], payload[3]]) != WIRE_MINOR
        {
            return Err(PairingQrError::UnsupportedVersion);
        }
        if u16::from_be_bytes([payload[100], payload[101]]) != TRANSFER_PORT {
            return Err(PairingQrError::InvalidPort);
        }
        if u64::from(u16::from_be_bytes([payload[102], payload[103]])) != INVITATION_TTL_SECS {
            return Err(PairingQrError::InvalidTtl);
        }
        if [(4, 20), (20, 36), (36, 68), (68, 100)]
            .into_iter()
            .any(|(start, end)| payload[start..end].iter().all(|byte| *byte == 0))
        {
            return Err(PairingQrError::ZeroValue);
        }
        let secure_session_id = Id::from_bytes(
            payload[4..20]
                .try_into()
                .map_err(|_| PairingQrError::InvalidLength)?,
        )
        .map_err(|_| PairingQrError::ZeroValue)?;
        let invitation_id = Id::from_bytes(
            payload[20..36]
                .try_into()
                .map_err(|_| PairingQrError::InvalidLength)?,
        )
        .map_err(|_| PairingQrError::ZeroValue)?;
        let mut pin = Zeroizing::new([0; 32]);
        pin.copy_from_slice(&payload[36..68]);
        let mut token = Zeroizing::new([0; 32]);
        token.copy_from_slice(&payload[68..100]);
        Ok(Self {
            secure_session_id,
            invitation_id,
            spki_pin: SpkiPin(pin),
            token: InvitationToken(token),
        })
    }
}

impl fmt::Debug for PairingQr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("PairingQr([REDACTED])")
    }
}
