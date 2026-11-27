/// Local policy failures never retain certificates, pins, or parser messages.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum TlsPolicyError {
    InvalidConfiguration,
    HandshakeIncomplete,
    ProtocolMismatch,
    AlpnMismatch,
    CertificateChain,
    CertificateSize,
    MalformedCertificate,
    CertificateProfile,
    CertificateExtensions,
    CertificateName,
    CertificateKey,
    CertificateValidity,
    ClockInvalid,
    NotYetValid,
    Expired,
    PinMismatch,
    CertificateSignature,
    HandshakeSignatureScheme,
}

impl TlsPolicyError {
    pub(super) fn into_rustls(self) -> rustls::Error {
        use rustls::CertificateError;

        let error = match self {
            Self::MalformedCertificate | Self::CertificateSize => CertificateError::BadEncoding,
            Self::CertificateName => CertificateError::NotValidForName,
            Self::NotYetValid => CertificateError::NotValidYet,
            Self::Expired => CertificateError::Expired,
            _ => CertificateError::ApplicationVerificationFailure,
        };
        rustls::Error::InvalidCertificate(error)
    }
}
