use ring::digest::{SHA256, digest};
use rustls::pki_types::UnixTime;
use subtle::ConstantTimeEq;
use x509_parser::extensions::{GeneralName, ParsedExtension};
use x509_parser::oid_registry::{
    OID_EC_P256, OID_KEY_TYPE_EC_PUBLIC_KEY, OID_SIG_ECDSA_WITH_SHA256,
};
use x509_parser::prelude::{FromDer, X509Certificate, X509Version};

use super::{MAX_CERTIFICATE_BYTES, MAX_CERTIFICATE_VALIDITY_SECONDS, SERVER_NAME, TlsPolicyError};

pub(super) fn validate_leaf(
    der: &[u8],
    expected_pin: &[u8; 32],
    now: UnixTime,
) -> Result<(), TlsPolicyError> {
    if der.is_empty() || der.len() > MAX_CERTIFICATE_BYTES {
        return Err(TlsPolicyError::CertificateSize);
    }
    let (remainder, certificate) =
        X509Certificate::from_der(der).map_err(|_| TlsPolicyError::MalformedCertificate)?;
    if !remainder.is_empty() {
        return Err(TlsPolicyError::MalformedCertificate);
    }
    if certificate.version() != X509Version::V3
        || certificate.issuer() != certificate.subject()
        || certificate.signature_algorithm != certificate.tbs_certificate.signature
        || certificate.signature_algorithm.algorithm != OID_SIG_ECDSA_WITH_SHA256
        || certificate.signature_algorithm.parameters.is_some()
        || certificate.signature_value.unused_bits != 0
    {
        return Err(TlsPolicyError::CertificateProfile);
    }
    let key = certificate.public_key();
    if key.algorithm.algorithm != OID_KEY_TYPE_EC_PUBLIC_KEY
        || key
            .algorithm
            .parameters
            .as_ref()
            .and_then(|value| value.as_oid().ok())
            .as_ref()
            != Some(&OID_EC_P256)
        || key.subject_public_key.unused_bits != 0
        || key.subject_public_key.data.len() != 65
        || key.subject_public_key.data.first() != Some(&4)
    {
        return Err(TlsPolicyError::CertificateKey);
    }
    validate_extensions(&certificate)?;
    let validity = certificate.validity();
    let before = validity.not_before.timestamp();
    let after = validity.not_after.timestamp();
    let span = after
        .checked_sub(before)
        .ok_or(TlsPolicyError::CertificateValidity)?;
    if span <= 0 || span > MAX_CERTIFICATE_VALIDITY_SECONDS {
        return Err(TlsPolicyError::CertificateValidity);
    }
    let now = i64::try_from(now.as_secs()).map_err(|_| TlsPolicyError::ClockInvalid)?;
    if now < before {
        return Err(TlsPolicyError::NotYetValid);
    }
    if now > after {
        return Err(TlsPolicyError::Expired);
    }
    let actual = digest(&SHA256, key.raw);
    if !bool::from(actual.as_ref().ct_eq(expected_pin.as_slice())) {
        return Err(TlsPolicyError::PinMismatch);
    }
    // This checks the certificate's self-signature, not TLS CertificateVerify.
    certificate
        .verify_signature(None)
        .map_err(|_| TlsPolicyError::CertificateSignature)
}

fn validate_extensions(certificate: &X509Certificate<'_>) -> Result<(), TlsPolicyError> {
    certificate
        .extensions_map()
        .map_err(|_| TlsPolicyError::CertificateExtensions)?;
    for extension in certificate.extensions() {
        let parsed = extension.parsed_extension();
        if matches!(parsed, ParsedExtension::ParseError { .. })
            || (extension.critical
                && !matches!(
                    parsed,
                    ParsedExtension::BasicConstraints(_)
                        | ParsedExtension::KeyUsage(_)
                        | ParsedExtension::ExtendedKeyUsage(_)
                        | ParsedExtension::SubjectAlternativeName(_)
                ))
        {
            return Err(TlsPolicyError::CertificateExtensions);
        }
    }
    let basic = certificate
        .basic_constraints()
        .map_err(|_| TlsPolicyError::CertificateExtensions)?
        .ok_or(TlsPolicyError::CertificateExtensions)?;
    if basic.value.ca || basic.value.path_len_constraint.is_some() {
        return Err(TlsPolicyError::CertificateProfile);
    }
    let usage = certificate
        .key_usage()
        .map_err(|_| TlsPolicyError::CertificateExtensions)?
        .ok_or(TlsPolicyError::CertificateExtensions)?;
    if !usage.value.digital_signature() || usage.value.key_cert_sign() || usage.value.crl_sign() {
        return Err(TlsPolicyError::CertificateProfile);
    }
    let extended = certificate
        .extended_key_usage()
        .map_err(|_| TlsPolicyError::CertificateExtensions)?
        .ok_or(TlsPolicyError::CertificateExtensions)?;
    if !extended.value.server_auth || extended.value.any {
        return Err(TlsPolicyError::CertificateProfile);
    }
    let names = certificate
        .subject_alternative_name()
        .map_err(|_| TlsPolicyError::CertificateExtensions)?
        .ok_or(TlsPolicyError::CertificateName)?;
    if !names
        .value
        .general_names
        .iter()
        .any(|name| matches!(name, GeneralName::DNSName(SERVER_NAME)))
    {
        return Err(TlsPolicyError::CertificateName);
    }
    Ok(())
}
