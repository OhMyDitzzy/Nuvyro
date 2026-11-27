use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use nuvyro_transfer_core::pairing::{PairingQr, PairingQrError};
use nuvyro_transfer_core::protocol::PAIRING_QR_PREFIX;
use nuvyro_transfer_core::protocol::limits::{MAX_PAIRING_QR_TEXT_BYTES, PAIRING_QR_PAYLOAD_BYTES};
use serde_json::Value;

fn payload() -> [u8; PAIRING_QR_PAYLOAD_BYTES] {
    let mut bytes = [0; PAIRING_QR_PAYLOAD_BYTES];
    bytes[..4].copy_from_slice(&[0, 1, 0, 0]);
    bytes[4..20].fill(0x11);
    bytes[20..36].fill(0x22);
    bytes[36..68].fill(0x33);
    bytes[68..100].fill(0x44);
    bytes[100..102].copy_from_slice(&47739_u16.to_be_bytes());
    bytes[102..104].copy_from_slice(&120_u16.to_be_bytes());
    bytes
}

fn qr(payload: &[u8]) -> Vec<u8> {
    let mut encoded = vec![0; payload.len().div_ceil(3) * 4];
    let len = URL_SAFE_NO_PAD.encode_slice(payload, &mut encoded).unwrap();
    let mut result = PAIRING_QR_PREFIX.as_bytes().to_vec();
    result.extend_from_slice(&encoded[..len]);
    result
}

fn fixture_outcome(result: Result<PairingQr, PairingQrError>) -> &'static str {
    match result {
        Ok(_) => "ok",
        Err(PairingQrError::LimitExceeded) => "limit",
        Err(PairingQrError::InvalidPrefix) => "prefix",
        Err(PairingQrError::InvalidEncoding) => "encoding",
        Err(PairingQrError::InvalidLength) => "length",
        Err(PairingQrError::UnsupportedVersion) => "version",
        Err(PairingQrError::InvalidPort) => "port",
        Err(PairingQrError::InvalidTtl) => "ttl",
        Err(PairingQrError::ZeroValue) => "zero",
    }
}

#[test]
fn public_qr_contract_vectors_match_the_decoder() {
    let vectors: Value =
        serde_json::from_str(include_str!("fixtures/contract-vectors.json")).unwrap();
    let cases = vectors["qr_cases"].as_array().unwrap();
    assert_eq!(cases.len(), 14);
    for case in cases {
        let result = PairingQr::decode(case["payload"].as_str().unwrap().as_bytes());
        assert_eq!(
            fixture_outcome(result),
            case["expect"].as_str().unwrap(),
            "{}",
            case["name"]
        );
    }
}

#[test]
fn valid_scan_exposes_only_validated_fields_and_fixed_routing_hints() {
    let decoded = PairingQr::decode(&qr(&payload())).unwrap();
    assert_eq!(decoded.secure_session_id().as_bytes(), &[0x11; 16]);
    assert_eq!(decoded.invitation_id().as_bytes(), &[0x22; 16]);
    assert_eq!(decoded.spki_pin().as_bytes(), &[0x33; 32]);
    assert_eq!(decoded.token().expose_secret(), &[0x44; 32]);
    assert_eq!(decoded.transfer_port(), 47739);
    assert_eq!(decoded.advertised_ttl_seconds(), 120);
}

#[test]
fn every_truncated_scan_is_rejected() {
    let input = qr(&payload());
    for end in 0..input.len() {
        assert!(PairingQr::decode(&input[..end]).is_err(), "end {end}");
    }
}

#[test]
fn canonical_payloads_of_any_other_bounded_length_are_rejected() {
    for len in 0..=MAX_PAIRING_QR_TEXT_BYTES {
        if len == PAIRING_QR_PAYLOAD_BYTES {
            continue;
        }
        let input = qr(&vec![1; len]);
        let expected = if input.len() > MAX_PAIRING_QR_TEXT_BYTES {
            PairingQrError::LimitExceeded
        } else if len == 0 {
            PairingQrError::InvalidEncoding
        } else {
            PairingQrError::InvalidLength
        };
        assert_eq!(
            PairingQr::decode(&input).unwrap_err(),
            expected,
            "len {len}"
        );
    }
}

#[test]
fn the_text_ceiling_is_checked_before_prefix_or_encoding() {
    let input = vec![0xff; MAX_PAIRING_QR_TEXT_BYTES + 1];
    assert_eq!(
        PairingQr::decode(&input).unwrap_err(),
        PairingQrError::LimitExceeded
    );
    let mut at_limit = PAIRING_QR_PREFIX.as_bytes().to_vec();
    at_limit.resize(MAX_PAIRING_QR_TEXT_BYTES, b'A');
    assert_eq!(
        PairingQr::decode(&at_limit).unwrap_err(),
        PairingQrError::InvalidLength
    );
    at_limit.push(b'A');
    assert_eq!(
        PairingQr::decode(&at_limit).unwrap_err(),
        PairingQrError::LimitExceeded
    );
}

#[test]
fn the_text_ceiling_counts_bytes_instead_of_unicode_characters() {
    let input = format!("{PAIRING_QR_PREFIX}{}", "é".repeat(250));
    assert!(input.chars().count() < MAX_PAIRING_QR_TEXT_BYTES);
    assert!(input.len() > MAX_PAIRING_QR_TEXT_BYTES);
    assert_eq!(
        PairingQr::decode(input.as_bytes()).unwrap_err(),
        PairingQrError::LimitExceeded
    );
}

#[test]
fn prefix_matching_is_exact_and_case_sensitive() {
    let input = qr(&payload());
    for index in 0..PAIRING_QR_PREFIX.len() {
        let mut changed = input.clone();
        changed[index] ^= 1;
        assert_eq!(
            PairingQr::decode(&changed).unwrap_err(),
            PairingQrError::InvalidPrefix,
            "prefix byte {index}"
        );
    }
    for input in [
        b"WIFI:T:WPA;S:example;P:example;;".as_slice(),
        b"https://nuvyro.local/NUVYRO:PAIR:1:",
        b"NUVYRO:PAIR:2:AAAA",
        b"\xef\xbb\xbfNUVYRO:PAIR:1:AAAA",
    ] {
        assert_eq!(
            PairingQr::decode(input).unwrap_err(),
            PairingQrError::InvalidPrefix
        );
    }
}

#[test]
fn whitespace_padding_alternate_alphabets_and_non_ascii_are_not_ignored() {
    let input = qr(&payload());
    for index in PAIRING_QR_PREFIX.len()..input.len() {
        for byte in [b' ', b'\t', b'\r', b'\n', b'=', b'+', b'/', b'%', 0, 0xff] {
            let mut changed = input.clone();
            changed[index] = byte;
            assert_eq!(
                PairingQr::decode(&changed).unwrap_err(),
                PairingQrError::InvalidEncoding,
                "byte {index}, invalid symbol {byte}"
            );
        }
    }
}

#[test]
fn surrounding_whitespace_and_nul_are_not_trimmed() {
    let input = qr(&payload());
    for byte in [b' ', b'\r', b'\n', b'\t', 0] {
        let mut leading = vec![byte];
        leading.extend_from_slice(&input);
        assert_eq!(
            PairingQr::decode(&leading).unwrap_err(),
            PairingQrError::InvalidPrefix
        );
        let mut trailing = input.clone();
        trailing.push(byte);
        assert!(PairingQr::decode(&trailing).is_err());
    }
}

#[test]
fn padding_and_extra_encoded_suffixes_are_rejected() {
    let input = qr(&payload());
    for suffix in [b"=".as_slice(), b"==", b"A", b"AA", b"AAA", b"AAAA"] {
        let mut changed = input.clone();
        changed.extend_from_slice(suffix);
        assert!(PairingQr::decode(&changed).is_err());
    }
}

#[test]
fn noncanonical_trailing_bits_are_rejected() {
    let alphabet = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
    let input = qr(&payload());
    let last = input.len() - 1;
    let index = alphabet
        .iter()
        .position(|byte| *byte == input[last])
        .unwrap();
    assert_eq!(index & 3, 0);
    for trailing_bits in 1..4 {
        let mut changed = input.clone();
        changed[last] = alphabet[index | trailing_bits];
        assert_eq!(
            PairingQr::decode(&changed).unwrap_err(),
            PairingQrError::InvalidEncoding
        );
    }
}

#[test]
fn the_url_safe_alphabet_is_accepted() {
    let mut bytes = payload();
    bytes[36..68].fill(0xff);
    bytes[36] = 0xfb;
    let input = qr(&bytes);
    assert!(input.contains(&b'-') && input.contains(&b'_'));
    let decoded = PairingQr::decode(&input).unwrap();
    assert_eq!(decoded.spki_pin().as_bytes().as_slice(), &bytes[36..68]);
}

#[test]
fn both_wire_version_words_are_exact() {
    for (offset, allowed) in [(0, 1), (2, 0)] {
        for version in [0_u16, 1, 2, 256, u16::MAX] {
            if version == allowed {
                continue;
            }
            let mut bytes = payload();
            bytes[offset..offset + 2].copy_from_slice(&version.to_be_bytes());
            assert_eq!(
                PairingQr::decode(&qr(&bytes)).unwrap_err(),
                PairingQrError::UnsupportedVersion
            );
        }
    }
}

#[test]
fn profile_arbitrary_and_byte_swapped_ports_are_rejected() {
    for port in [0_u16, 1, 47738, 47740, 47739_u16.swap_bytes(), u16::MAX] {
        let mut bytes = payload();
        bytes[100..102].copy_from_slice(&port.to_be_bytes());
        assert_eq!(
            PairingQr::decode(&qr(&bytes)).unwrap_err(),
            PairingQrError::InvalidPort
        );
    }
}

#[test]
fn ttl_cannot_choose_an_arbitrary_invitation_lifetime() {
    for ttl in [0_u16, 1, 119, 121, 120_u16.swap_bytes(), u16::MAX] {
        let mut bytes = payload();
        bytes[102..104].copy_from_slice(&ttl.to_be_bytes());
        assert_eq!(
            PairingQr::decode(&qr(&bytes)).unwrap_err(),
            PairingQrError::InvalidTtl
        );
    }
}

#[test]
fn zero_ids_pin_and_token_are_rejected_independently() {
    for (start, end) in [(4, 20), (20, 36), (36, 68), (68, 100)] {
        let mut bytes = payload();
        bytes[start..end].fill(0);
        assert_eq!(
            PairingQr::decode(&qr(&bytes)).unwrap_err(),
            PairingQrError::ZeroValue,
            "field {start}"
        );
    }
}

#[test]
fn nonzero_does_not_invent_an_entropy_or_identity_check() {
    for (start, end) in [(4, 20), (20, 36), (36, 68), (68, 100)] {
        for nonzero in [start, end - 1] {
            let mut bytes = payload();
            bytes[start..end].fill(0);
            bytes[nonzero] = 1;
            assert!(PairingQr::decode(&qr(&bytes)).is_ok());
        }
    }
}

#[test]
fn decoded_input_owns_its_data_without_retaining_the_scan_buffer() {
    let mut input = qr(&payload());
    let decoded = PairingQr::decode(&input).unwrap();
    input.fill(0);
    drop(input);
    assert_eq!(decoded.secure_session_id().as_bytes(), &[0x11; 16]);
    assert_eq!(decoded.token().expose_secret(), &[0x44; 32]);
}

#[test]
fn ordinary_and_pretty_debug_are_redacted_for_all_sensitive_types() {
    let decoded = PairingQr::decode(&qr(&payload())).unwrap();
    assert_eq!(format!("{decoded:?}"), "PairingQr([REDACTED])");
    assert_eq!(format!("{decoded:#?}"), "PairingQr([REDACTED])");
    assert_eq!(format!("{:?}", decoded.spki_pin()), "SpkiPin([REDACTED])");
    assert_eq!(format!("{:#?}", decoded.spki_pin()), "SpkiPin([REDACTED])");
    assert_eq!(
        format!("{:?}", decoded.token()),
        "InvitationToken([REDACTED])"
    );
    assert_eq!(
        format!("{:#?}", decoded.token()),
        "InvitationToken([REDACTED])"
    );
}

#[test]
fn local_error_formatting_does_not_echo_scanner_input() {
    let input = b"private-scanner-input-must-not-be-logged";
    let error = PairingQr::decode(input).unwrap_err();
    assert_eq!(format!("{error:?}"), "InvalidPrefix");
    assert_eq!(error.to_string(), "Pairing QR prefix is unsupported");
    assert!(std::error::Error::source(&error).is_none());
}
