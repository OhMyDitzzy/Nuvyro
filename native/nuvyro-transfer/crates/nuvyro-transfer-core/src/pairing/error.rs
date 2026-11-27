use std::fmt;

/// Local scan errors contain no input text, bytes, or remote authentication detail.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PairingQrError {
    LimitExceeded,
    InvalidPrefix,
    InvalidEncoding,
    InvalidLength,
    UnsupportedVersion,
    InvalidPort,
    InvalidTtl,
    ZeroValue,
}

impl fmt::Display for PairingQrError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::LimitExceeded => "Pairing QR exceeds the text limit",
            Self::InvalidPrefix => "Pairing QR prefix is unsupported",
            Self::InvalidEncoding => "Pairing QR encoding is invalid",
            Self::InvalidLength => "Pairing QR payload length is invalid",
            Self::UnsupportedVersion => "Pairing QR wire version is unsupported",
            Self::InvalidPort => "Pairing QR transfer port is invalid",
            Self::InvalidTtl => "Pairing QR advertised lifetime is invalid",
            Self::ZeroValue => "Pairing QR contains an empty identifier, pin, or token",
        })
    }
}

impl std::error::Error for PairingQrError {}
