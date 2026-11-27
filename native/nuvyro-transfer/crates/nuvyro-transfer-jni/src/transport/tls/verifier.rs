use std::fmt;
use std::sync::Arc;

use nuvyro_transfer_core::pairing::SpkiPin;
use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::crypto::{CryptoProvider, verify_tls13_signature};
use rustls::pki_types::{CertificateDer, ServerName, UnixTime};
use rustls::{DigitallySignedStruct, Error, SignatureScheme};
use zeroize::Zeroizing;

use super::certificate::validate_leaf;
use super::{SERVER_NAME, TlsPolicyError};

pub(super) struct PinnedServerVerifier {
    pin: Zeroizing<[u8; 32]>,
    provider: Arc<CryptoProvider>,
}

impl PinnedServerVerifier {
    pub(super) fn new(pin: &SpkiPin, provider: Arc<CryptoProvider>) -> Self {
        let mut owned = Zeroizing::new([0; 32]);
        owned.copy_from_slice(pin.as_bytes());
        Self {
            pin: owned,
            provider,
        }
    }
}

impl fmt::Debug for PinnedServerVerifier {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("PinnedServerVerifier([REDACTED])")
    }
}

impl ServerCertVerifier for PinnedServerVerifier {
    fn verify_server_cert(
        &self,
        end_entity: &CertificateDer<'_>,
        intermediates: &[CertificateDer<'_>],
        server_name: &ServerName<'_>,
        _ocsp_response: &[u8],
        now: UnixTime,
    ) -> Result<ServerCertVerified, Error> {
        if !intermediates.is_empty() {
            return Err(TlsPolicyError::CertificateChain.into_rustls());
        }
        if !matches!(server_name, ServerName::DnsName(name) if name.as_ref() == SERVER_NAME) {
            return Err(TlsPolicyError::CertificateName.into_rustls());
        }
        validate_leaf(end_entity.as_ref(), &self.pin, now).map_err(TlsPolicyError::into_rustls)?;
        Ok(ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        _message: &[u8],
        _certificate: &CertificateDer<'_>,
        _signature: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, Error> {
        Err(TlsPolicyError::ProtocolMismatch.into_rustls())
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        certificate: &CertificateDer<'_>,
        signature: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, Error> {
        if signature.scheme != SignatureScheme::ECDSA_NISTP256_SHA256 {
            return Err(TlsPolicyError::HandshakeSignatureScheme.into_rustls());
        }
        verify_tls13_signature(
            message,
            certificate,
            signature,
            &self.provider.signature_verification_algorithms,
        )
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        vec![SignatureScheme::ECDSA_NISTP256_SHA256]
    }
}
