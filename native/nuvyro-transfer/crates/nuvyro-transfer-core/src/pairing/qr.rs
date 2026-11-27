use base64::{DecodeSliceError, Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use zeroize::Zeroizing;

use super::{PairingQr, PairingQrError};
use crate::protocol::PAIRING_QR_PREFIX;
use crate::protocol::limits::{MAX_PAIRING_QR_TEXT_BYTES, PAIRING_QR_PAYLOAD_BYTES};

impl PairingQr {
    /// Decode borrowed scanner bytes with a fixed buffer and no heap allocation.
    /// The caller must clear its own scan buffer when the UI session ends.
    pub fn decode(input: &[u8]) -> Result<Self, PairingQrError> {
        if input.len() > MAX_PAIRING_QR_TEXT_BYTES {
            return Err(PairingQrError::LimitExceeded);
        }
        let encoded = input
            .strip_prefix(PAIRING_QR_PREFIX.as_bytes())
            .ok_or(PairingQrError::InvalidPrefix)?;
        if encoded.is_empty() {
            return Err(PairingQrError::InvalidEncoding);
        }
        let mut payload = Zeroizing::new([0; PAIRING_QR_PAYLOAD_BYTES]);
        let len = URL_SAFE_NO_PAD
            .decode_slice(encoded, &mut payload[..])
            .map_err(|error| match error {
                DecodeSliceError::OutputSliceTooSmall => PairingQrError::InvalidLength,
                DecodeSliceError::DecodeError(_) => PairingQrError::InvalidEncoding,
            })?;
        if len != PAIRING_QR_PAYLOAD_BYTES {
            return Err(PairingQrError::InvalidLength);
        }
        Self::from_payload(&payload)
    }
}
