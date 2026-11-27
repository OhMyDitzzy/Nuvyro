use std::sync::Arc;
use std::time::Duration;

use rcgen::{
    BasicConstraints, CustomExtension, ExtendedKeyUsagePurpose, IsCa, KeyPair, KeyUsagePurpose,
    SanType,
};
use ring::digest::{SHA256, digest};
use rustls::client::danger::ServerCertVerifier;
use rustls::pki_types::ServerName;

use super::super::certificate::validate_leaf;
use super::super::verifier::PinnedServerVerifier;
use super::super::{MAX_CERTIFICATE_BYTES, SERVER_NAME, TlsPolicyError};
use super::fixtures::{NOW_SECONDS, at, identity, identity_with_key, now, params, scan, spki_pin};

#[test]
fn a_pinned_self_signed_p256_server_leaf_is_accepted() {
    let identity = identity(params());
    assert_eq!(
        validate_leaf(identity.certificate.as_ref(), &identity.pin, now()),
        Ok(())
    );
}

#[test]
fn pinning_hashes_the_spki_instead_of_the_certificate() {
    let identity = identity(params());
    let certificate_hash: [u8; 32] = digest(&SHA256, identity.certificate.as_ref())
        .as_ref()
        .try_into()
        .unwrap();
    assert_ne!(certificate_hash, identity.pin);
    assert_eq!(
        validate_leaf(identity.certificate.as_ref(), &certificate_hash, now()),
        Err(TlsPolicyError::PinMismatch)
    );
}

#[test]
fn every_changed_pin_byte_is_rejected() {
    let identity = identity(params());
    for index in 0..32 {
        let mut wrong = identity.pin;
        wrong[index] ^= 1;
        assert_eq!(
            validate_leaf(identity.certificate.as_ref(), &wrong, now()),
            Err(TlsPolicyError::PinMismatch)
        );
    }
}

#[test]
fn reissued_certificates_can_retain_the_same_public_key_pin() {
    let key = KeyPair::generate_for(&rcgen::PKCS_ECDSA_P256_SHA256).unwrap();
    let original = params().self_signed(&key).unwrap();
    let mut changed = params();
    changed.serial_number = Some(42_u64.into());
    let reissued = changed.self_signed(&key).unwrap();
    assert_ne!(original.der(), reissued.der());
    let pin = spki_pin(original.der().as_ref());
    assert_eq!(spki_pin(reissued.der().as_ref()), pin);
    assert_eq!(validate_leaf(reissued.der().as_ref(), &pin, now()), Ok(()));
}

#[test]
fn empty_and_oversized_certificates_fail_before_parsing() {
    assert_eq!(
        validate_leaf(&[], &[1; 32], now()),
        Err(TlsPolicyError::CertificateSize)
    );
    assert_eq!(
        validate_leaf(&vec![0; MAX_CERTIFICATE_BYTES + 1], &[1; 32], now()),
        Err(TlsPolicyError::CertificateSize)
    );
    assert_eq!(
        validate_leaf(&vec![0; MAX_CERTIFICATE_BYTES], &[1; 32], now()),
        Err(TlsPolicyError::MalformedCertificate)
    );
}

#[test]
fn truncated_der_and_trailing_der_objects_are_rejected() {
    let identity = identity(params());
    let der = identity.certificate.as_ref();
    for end in 0..der.len() {
        assert!(validate_leaf(&der[..end], &identity.pin, now()).is_err());
    }
    let mut trailing = der.to_vec();
    trailing.extend_from_slice(&[5, 0]);
    assert_eq!(
        validate_leaf(&trailing, &identity.pin, now()),
        Err(TlsPolicyError::MalformedCertificate)
    );
}

#[test]
fn a_corrupted_self_signature_fails_even_with_the_correct_pin() {
    let identity = identity(params());
    let mut changed = identity.certificate.as_ref().to_vec();
    *changed.last_mut().unwrap() ^= 1;
    assert_eq!(
        validate_leaf(&changed, &identity.pin, now()),
        Err(TlsPolicyError::CertificateSignature)
    );
}

#[test]
fn a_same_name_certificate_signed_by_a_different_key_is_not_self_signed() {
    let leaf_key = KeyPair::generate_for(&rcgen::PKCS_ECDSA_P256_SHA256).unwrap();
    let signer_key = KeyPair::generate_for(&rcgen::PKCS_ECDSA_P256_SHA256).unwrap();
    let params = params();
    let issuer = rcgen::Issuer::from_params(&params, &signer_key);
    let certificate = params.signed_by(&leaf_key, &issuer).unwrap();
    let pin = spki_pin(certificate.der().as_ref());
    assert_eq!(
        validate_leaf(certificate.der().as_ref(), &pin, now()),
        Err(TlsPolicyError::CertificateSignature)
    );
}

#[test]
fn non_p256_keys_and_other_signature_algorithms_are_rejected() {
    for algorithm in [&rcgen::PKCS_ECDSA_P384_SHA384, &rcgen::PKCS_ED25519] {
        let key = KeyPair::generate_for(algorithm).unwrap();
        let identity = identity_with_key(params(), key);
        assert!(validate_leaf(identity.certificate.as_ref(), &identity.pin, now()).is_err());
    }
}

#[test]
fn a_p384_subject_is_rejected_even_when_signed_with_p256_sha256() {
    let leaf_key = KeyPair::generate_for(&rcgen::PKCS_ECDSA_P384_SHA384).unwrap();
    let signer_key = KeyPair::generate_for(&rcgen::PKCS_ECDSA_P256_SHA256).unwrap();
    let params = params();
    let issuer = rcgen::Issuer::from_params(&params, &signer_key);
    let certificate = params.signed_by(&leaf_key, &issuer).unwrap();
    let pin = spki_pin(certificate.der().as_ref());
    assert_eq!(
        validate_leaf(certificate.der().as_ref(), &pin, now()),
        Err(TlsPolicyError::CertificateKey)
    );
}

#[test]
fn a_leaf_must_explicitly_have_basic_constraints_ca_false() {
    let mut absent = params();
    absent.is_ca = IsCa::NoCa;
    let identity = identity(absent);
    assert_eq!(
        validate_leaf(identity.certificate.as_ref(), &identity.pin, now()),
        Err(TlsPolicyError::CertificateExtensions)
    );
    let mut ca = params();
    ca.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);
    let identity = super::fixtures::identity(ca);
    assert_eq!(
        validate_leaf(identity.certificate.as_ref(), &identity.pin, now()),
        Err(TlsPolicyError::CertificateProfile)
    );
}

#[test]
fn a_path_length_constraint_is_invalid_for_a_non_ca_leaf() {
    let mut params = params();
    params.is_ca = IsCa::NoCa;
    params
        .custom_extensions
        .push(CustomExtension::from_oid_content(
            &[2, 5, 29, 19],
            vec![0x30, 3, 2, 1, 0],
        ));
    let identity = identity(params);
    assert_eq!(
        validate_leaf(identity.certificate.as_ref(), &identity.pin, now()),
        Err(TlsPolicyError::CertificateProfile)
    );
}

#[test]
fn digital_signature_key_usage_is_mandatory() {
    for usages in [
        vec![],
        vec![KeyUsagePurpose::KeyEncipherment],
        vec![
            KeyUsagePurpose::DigitalSignature,
            KeyUsagePurpose::KeyCertSign,
        ],
    ] {
        let mut params = params();
        params.key_usages = usages;
        let identity = identity(params);
        assert!(validate_leaf(identity.certificate.as_ref(), &identity.pin, now()).is_err());
    }
}

#[test]
fn server_auth_eku_is_mandatory_and_any_eku_is_not_a_substitute() {
    for usages in [
        vec![],
        vec![ExtendedKeyUsagePurpose::ClientAuth],
        vec![ExtendedKeyUsagePurpose::Any],
    ] {
        let mut params = params();
        params.extended_key_usages = usages;
        let identity = identity(params);
        assert!(validate_leaf(identity.certificate.as_ref(), &identity.pin, now()).is_err());
    }
}

#[test]
fn the_exact_san_dns_name_is_required_without_common_name_fallback() {
    for names in [
        vec![],
        vec![SanType::DnsName("other.local".try_into().unwrap())],
        vec![SanType::DnsName("*.local".try_into().unwrap())],
        vec![SanType::IpAddress([127, 0, 0, 1].into())],
    ] {
        let mut params = params();
        params.subject_alt_names = names;
        let identity = identity(params);
        assert_eq!(
            validate_leaf(identity.certificate.as_ref(), &identity.pin, now()),
            Err(TlsPolicyError::CertificateName)
        );
    }
}

#[test]
fn unknown_critical_extensions_are_not_ignored() {
    let mut params = params();
    let mut extension = CustomExtension::from_oid_content(&[1, 2, 3, 4], vec![5, 0]);
    extension.set_criticality(true);
    params.custom_extensions.push(extension);
    let identity = identity(params);
    assert_eq!(
        validate_leaf(identity.certificate.as_ref(), &identity.pin, now()),
        Err(TlsPolicyError::CertificateExtensions)
    );
}

#[test]
fn unknown_noncritical_extensions_do_not_change_pin_or_trust_policy() {
    let mut params = params();
    params
        .custom_extensions
        .push(CustomExtension::from_oid_content(&[1, 2, 3, 4], vec![5, 0]));
    let identity = identity(params);
    assert_eq!(
        validate_leaf(identity.certificate.as_ref(), &identity.pin, now()),
        Ok(())
    );
}

#[test]
fn duplicate_extensions_are_rejected() {
    let mut params = params();
    for _ in 0..2 {
        params
            .custom_extensions
            .push(CustomExtension::from_oid_content(&[1, 2, 3, 4], vec![5, 0]));
    }
    let identity = identity(params);
    assert_eq!(
        validate_leaf(identity.certificate.as_ref(), &identity.pin, now()),
        Err(TlsPolicyError::CertificateExtensions)
    );
}

#[test]
fn malformed_required_extension_contents_are_rejected() {
    let mut params = params();
    params.key_usages.clear();
    params
        .custom_extensions
        .push(CustomExtension::from_oid_content(
            &[2, 5, 29, 15],
            vec![1, 1, 0xff],
        ));
    let identity = identity(params);
    assert_eq!(
        validate_leaf(identity.certificate.as_ref(), &identity.pin, now()),
        Err(TlsPolicyError::CertificateExtensions)
    );
}

#[test]
fn certificate_validity_uses_the_supplied_wall_time_with_inclusive_bounds() {
    let identity = identity(params());
    let der = identity.certificate.as_ref();
    assert_eq!(
        validate_leaf(der, &identity.pin, at(NOW_SECONDS - 301)),
        Err(TlsPolicyError::NotYetValid)
    );
    assert_eq!(
        validate_leaf(der, &identity.pin, at(NOW_SECONDS - 300)),
        Ok(())
    );
    assert_eq!(
        validate_leaf(der, &identity.pin, at(NOW_SECONDS + 86_400)),
        Ok(())
    );
    assert_eq!(
        validate_leaf(der, &identity.pin, at(NOW_SECONDS + 86_401)),
        Err(TlsPolicyError::Expired)
    );
}

#[test]
fn zero_reversed_and_overlong_validity_periods_are_rejected() {
    for seconds in [0, 1, 86_701] {
        let mut params = params();
        params.not_after = if seconds == 1 {
            params.not_before - Duration::from_secs(1)
        } else {
            params.not_before + Duration::from_secs(seconds)
        };
        let identity = identity(params);
        assert_eq!(
            validate_leaf(identity.certificate.as_ref(), &identity.pin, now()),
            Err(TlsPolicyError::CertificateValidity)
        );
    }
}

#[test]
fn unrepresentable_wall_time_is_rejected_without_fallback() {
    let identity = identity(params());
    assert_eq!(
        validate_leaf(identity.certificate.as_ref(), &identity.pin, at(u64::MAX)),
        Err(TlsPolicyError::ClockInvalid)
    );
}

#[test]
fn additional_chain_certificates_are_rejected_before_parsing() {
    let identity = identity(params());
    let qr = scan(&identity.pin);
    let verifier = PinnedServerVerifier::new(
        qr.spki_pin(),
        Arc::new(rustls::crypto::ring::default_provider()),
    );
    let name = SERVER_NAME.try_into().unwrap();
    assert!(
        verifier
            .verify_server_cert(
                &identity.certificate,
                &[identity.certificate.clone()],
                &name,
                &[],
                now()
            )
            .is_err()
    );
}

#[test]
fn requested_names_cannot_replace_the_fixed_tls_name() {
    let identity = identity(params());
    let qr = scan(&identity.pin);
    let verifier = PinnedServerVerifier::new(
        qr.spki_pin(),
        Arc::new(rustls::crypto::ring::default_provider()),
    );
    for name in [
        ServerName::try_from("other.local").unwrap(),
        ServerName::from(std::net::IpAddr::from([127, 0, 0, 1])),
    ] {
        assert!(
            verifier
                .verify_server_cert(&identity.certificate, &[], &name, &[], now())
                .is_err()
        );
    }
}

#[test]
fn verifier_debug_is_redacted_and_only_p256_sha256_is_advertised() {
    let identity = identity(params());
    let qr = scan(&identity.pin);
    let verifier = PinnedServerVerifier::new(
        qr.spki_pin(),
        Arc::new(rustls::crypto::ring::default_provider()),
    );
    assert_eq!(format!("{verifier:?}"), "PinnedServerVerifier([REDACTED])");
    assert_eq!(format!("{verifier:#?}"), "PinnedServerVerifier([REDACTED])");
    assert_eq!(
        verifier.supported_verify_schemes(),
        vec![rustls::SignatureScheme::ECDSA_NISTP256_SHA256]
    );
}
