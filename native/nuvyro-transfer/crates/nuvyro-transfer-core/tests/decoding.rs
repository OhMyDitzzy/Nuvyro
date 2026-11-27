use nuvyro_transfer_core::protocol::limits::{
    MAX_CHUNK_BYTES, MAX_FILE_BYTES, MAX_MANIFEST_BYTES, MAX_TOTAL_BYTES,
};
use nuvyro_transfer_core::protocol::{
    BodyError, FrameHeader, HeaderError, MANIFEST_DOMAIN, Manifest, Message, MessageKind,
    StreamDecoder, StreamError,
};
use serde_json::Value;

fn vectors() -> Value {
    serde_json::from_str(include_str!("fixtures/contract-vectors.json")).unwrap()
}

fn hex(value: &str) -> Vec<u8> {
    assert_eq!(value.len() % 2, 0);
    value
        .as_bytes()
        .chunks_exact(2)
        .map(|pair| u8::from_str_radix(std::str::from_utf8(pair).unwrap(), 16).unwrap())
        .collect()
}

fn header(kind: MessageKind, body_len: usize) -> FrameHeader {
    let mut bytes = [0_u8; 6];
    bytes[..4].copy_from_slice(&u32::try_from(body_len + 2).unwrap().to_be_bytes());
    bytes[4..].copy_from_slice(&(kind as u16).to_be_bytes());
    FrameHeader::decode(&bytes).unwrap()
}

fn decode(kind: MessageKind, body: &[u8]) -> Result<Message<'_>, BodyError> {
    Message::decode(header(kind, body.len()), body)
}

fn frame(kind: MessageKind, body: &[u8]) -> Vec<u8> {
    let mut bytes = Vec::new();
    bytes.extend_from_slice(&u32::try_from(body.len() + 2).unwrap().to_be_bytes());
    bytes.extend_from_slice(&(kind as u16).to_be_bytes());
    bytes.extend_from_slice(body);
    bytes
}

fn transfer_context() -> Vec<u8> {
    [[0x66; 16], [0x77; 16]].concat()
}

fn file_context() -> Vec<u8> {
    [transfer_context(), vec![0x55; 16]].concat()
}

fn sized_file(size: u64) -> Vec<u8> {
    [file_context(), size.to_be_bytes().to_vec()].concat()
}

fn hello() -> Vec<u8> {
    [vec![0, 1, 0, 0], vec![0x11; 16], vec![0; 4]].concat()
}

fn text(value: &str) -> Vec<u8> {
    [
        u16::try_from(value.len()).unwrap().to_be_bytes().to_vec(),
        value.as_bytes().to_vec(),
    ]
    .concat()
}

fn manifest(entries: &[(u8, u64, &str, &str)]) -> Vec<u8> {
    let total: u64 = entries.iter().map(|entry| entry.1).sum();
    let mut bytes = vec![0, 1];
    bytes.extend_from_slice(&u16::try_from(entries.len()).unwrap().to_be_bytes());
    bytes.extend_from_slice(&total.to_be_bytes());
    for (id, size, name, mime) in entries {
        bytes.extend_from_slice(&[*id; 16]);
        bytes.extend_from_slice(&size.to_be_bytes());
        bytes.extend_from_slice(&text(name));
        bytes.extend_from_slice(&text(mime));
    }
    bytes
}

fn offer(bytes: &[u8]) -> Vec<u8> {
    let mut hasher = blake3::Hasher::new();
    hasher.update(MANIFEST_DOMAIN);
    hasher.update(bytes);
    [
        transfer_context(),
        hasher.finalize().as_bytes().to_vec(),
        u32::try_from(bytes.len()).unwrap().to_be_bytes().to_vec(),
        bytes.to_vec(),
    ]
    .concat()
}

#[test]
fn manifest_artifact_vectors_match_the_native_decoder() {
    for case in vectors()["manifest_cases"].as_array().unwrap() {
        let bytes = hex(case["hex"].as_str().unwrap());
        match Manifest::decode(&bytes) {
            Ok(manifest) => {
                assert_eq!(case["expect"], "ok", "{}", case["name"]);
                let sizes: Vec<_> = manifest
                    .files()
                    .iter()
                    .map(|file| file.size.to_string())
                    .collect();
                assert_eq!(serde_json::to_value(sizes).unwrap(), case["expected_sizes"]);
                assert_eq!(manifest.encoded(), bytes);
            }
            Err(error) => {
                assert_eq!(format!("{error:?}"), case["expect"].as_str().unwrap());
            }
        }
    }
}

#[test]
fn outbound_encoder_reproduces_every_canonical_full_frame_example() {
    for example in vectors()["frame_examples"].as_array().unwrap() {
        let bytes = hex(example["hex"].as_str().unwrap());
        let header: [u8; 6] = bytes[..6].try_into().unwrap();
        let header = FrameHeader::decode(&header).unwrap();
        let message = Message::decode(header, &bytes[6..]).unwrap();
        let encoded = nuvyro_transfer_core::protocol::encode_message(&message).unwrap();
        assert_eq!(encoded.as_slice(), bytes, "{}", example["name"]);
    }
}

#[test]
fn outbound_encoder_rejects_invalid_messages_and_bounds_chunks() {
    let context = nuvyro_transfer_core::protocol::TransferContext {
        pair_id: nuvyro_transfer_core::protocol::Id::from_bytes([1; 16]).unwrap(),
        transfer_id: nuvyro_transfer_core::protocol::Id::from_bytes([2; 16]).unwrap(),
    };
    let file = nuvyro_transfer_core::protocol::FileContext {
        transfer: context,
        file_id: nuvyro_transfer_core::protocol::Id::from_bytes([3; 16]).unwrap(),
    };
    let empty = Message::DataChunk {
        context: file,
        offset: 0,
        bytes: &[],
    };
    assert_eq!(
        nuvyro_transfer_core::protocol::encode_message(&empty).unwrap_err(),
        BodyError::LimitExceeded
    );
    let invalid_file_ready = Message::FileReady {
        context: file,
        next_offset: 1,
    };
    assert_eq!(
        nuvyro_transfer_core::protocol::encode_message(&invalid_file_ready).unwrap_err(),
        BodyError::MalformedFrame
    );
    let chunk = vec![0x5a; MAX_CHUNK_BYTES as usize];
    let largest = Message::DataChunk {
        context: file,
        offset: 0,
        bytes: &chunk,
    };
    let encoded = nuvyro_transfer_core::protocol::encode_message(&largest).unwrap();
    assert_eq!(encoded.as_slice().len(), MAX_CHUNK_BYTES as usize + 62);
    let above = vec![0x5a; MAX_CHUNK_BYTES as usize + 1];
    assert_eq!(
        nuvyro_transfer_core::protocol::encode_message(&Message::DataChunk {
            context: file,
            offset: 0,
            bytes: &above,
        })
        .unwrap_err(),
        BodyError::LimitExceeded
    );
}

#[test]
fn manifest_encoder_preserves_canonical_vectors_and_rejects_duplicate_or_unsafe_entries() {
    use nuvyro_transfer_core::protocol::{Id, ManifestEntry, encode_manifest};
    for case in vectors()["manifest_cases"].as_array().unwrap() {
        if case["expect"] != "ok" {
            continue;
        }
        let bytes = hex(case["hex"].as_str().unwrap());
        let decoded = Manifest::decode(&bytes).unwrap();
        assert_eq!(encode_manifest(decoded.files()).unwrap(), bytes);
    }
    let file = ManifestEntry {
        file_id: Id::from_bytes([0x55; 16]).unwrap(),
        size: 0,
        name: "safe.txt",
        mime: "text/plain",
    };
    assert_eq!(encode_manifest(&[file, file]), Err(BodyError::DuplicateId));
    assert_eq!(
        encode_manifest(&[ManifestEntry {
            name: "../escape",
            ..file
        }]),
        Err(BodyError::UnsafeName)
    );
    assert_eq!(
        encode_manifest(&[ManifestEntry {
            size: MAX_FILE_BYTES + 1,
            ..file
        }]),
        Err(BodyError::LimitExceeded)
    );
    assert_eq!(
        encode_manifest(&[ManifestEntry {
            mime: "bad",
            ..file
        }]),
        Err(BodyError::MalformedFrame)
    );
}

#[test]
fn outbound_authentication_is_redacted_and_rejects_invalid_tokens_and_names() {
    use nuvyro_transfer_core::protocol::{Id, encode_message};
    let invitation_id = Id::from_bytes([0x22; 16]).unwrap();
    let frame = encode_message(&Message::PairAuthenticate {
        invitation_id,
        token: &[0x44; 32],
        claimed_sender_name: "Claimed name",
    })
    .unwrap();
    assert_eq!(format!("{frame:?}"), "EncodedFrame([REDACTED])");
    assert!(
        encode_message(&Message::PairAuthenticate {
            invitation_id,
            token: &[0; 32],
            claimed_sender_name: "Sender",
        })
        .is_err()
    );
    assert!(
        encode_message(&Message::PairAuthenticate {
            invitation_id,
            token: &[0x44; 32],
            claimed_sender_name: "\u{85}",
        })
        .is_err()
    );
}

#[test]
fn manifest_rejects_every_truncated_encoding_and_unknown_schema() {
    let bytes = manifest(&[(1, 5, "hello.txt", "text/plain")]);
    for end in 0..bytes.len() {
        assert!(Manifest::decode(&bytes[..end]).is_err(), "prefix {end}");
    }
    let mut unknown = bytes;
    unknown[1] = 2;
    assert_eq!(
        Manifest::decode(&unknown).unwrap_err(),
        BodyError::MalformedFrame
    );
    let above_total = manifest(&[
        (1, MAX_FILE_BYTES, "a", "a/b"),
        (2, MAX_FILE_BYTES, "b", "a/b"),
        (3, MAX_FILE_BYTES, "c", "a/b"),
        (4, MAX_FILE_BYTES, "d", "a/b"),
        (5, 1, "e", "a/b"),
    ]);
    assert_eq!(
        Manifest::decode(&above_total).unwrap_err(),
        BodyError::LimitExceeded
    );
}

#[test]
fn official_empty_digest_and_manifest_domain_are_distinct() {
    let data = vectors();
    let expected = hex(data["hash_cases"][0]["digest_hex"].as_str().unwrap());
    assert_eq!(blake3::hash(b"").as_bytes().as_slice(), expected);
    assert_eq!(MANIFEST_DOMAIN, b"NuvyroManifestV1\0");
    let encoded = manifest(&[(1, 0, "empty.txt", "application/octet-stream")]);
    let parsed = Manifest::decode(&encoded).unwrap();
    let domain_bytes = [MANIFEST_DOMAIN, encoded.as_slice()].concat();
    assert_eq!(parsed.digest(), blake3::hash(&domain_bytes).as_bytes());
    assert_ne!(parsed.digest(), blake3::hash(&encoded).as_bytes());
}

#[test]
fn manifest_limits_preserve_large_sizes_and_duplicate_display_names() {
    let entries: Vec<_> = (1..=128)
        .map(|id| (id, 0, "same.txt", "text/plain"))
        .collect();
    let bytes = manifest(&entries);
    assert_eq!(Manifest::decode(&bytes).unwrap().files().len(), 128);
    let large = manifest(&[
        (1, MAX_FILE_BYTES, "a", "a/b"),
        (2, MAX_FILE_BYTES, "a", "a/b"),
        (3, MAX_FILE_BYTES, "a", "a/b"),
        (4, MAX_FILE_BYTES, "a", "a/b"),
    ]);
    let parsed = Manifest::decode(&large).unwrap();
    assert_eq!(parsed.total_bytes(), MAX_TOTAL_BYTES);
    assert_eq!(parsed.files()[0].size, MAX_FILE_BYTES);
    assert_eq!(
        Manifest::decode(&manifest(&[(1, MAX_FILE_BYTES + 1, "a", "a/b")])).unwrap_err(),
        BodyError::LimitExceeded
    );
    let mut excessive_count = bytes;
    excessive_count[2..4].copy_from_slice(&129_u16.to_be_bytes());
    assert_eq!(
        Manifest::decode(&excessive_count).unwrap_err(),
        BodyError::LimitExceeded
    );
    assert_eq!(
        Manifest::decode(&vec![0; MAX_MANIFEST_BYTES as usize + 1]).unwrap_err(),
        BodyError::LimitExceeded
    );
}

#[test]
fn filename_and_mime_rules_reject_unsafe_components() {
    for name in [
        ".",
        "..",
        " \t",
        "a/b",
        "a\\b",
        "a\0b",
        "a\u{85}b",
        "a\u{2066}b",
    ] {
        assert_eq!(
            Manifest::decode(&manifest(&[(1, 0, name, "text/plain")])).unwrap_err(),
            BodyError::UnsafeName
        );
    }
    for mime in [
        "text",
        "text/",
        "/plain",
        "a/b/c",
        "text/*",
        "text/plain;x=1",
        "téxt/plain",
    ] {
        assert_eq!(
            Manifest::decode(&manifest(&[(1, 0, "a", mime)])).unwrap_err(),
            BodyError::MalformedFrame
        );
    }
    let encoded = manifest(&[(1, 0, "\u{feff}é.txt", "application/vnd.test+json")]);
    assert_eq!(
        Manifest::decode(&encoded).unwrap().files()[0].name,
        "\u{feff}é.txt"
    );
    let composed = manifest(&[(1, 0, "é", "a/b")]);
    let decomposed = manifest(&[(1, 0, "e\u{301}", "a/b")]);
    assert_ne!(
        Manifest::decode(&composed).unwrap().digest(),
        Manifest::decode(&decomposed).unwrap().digest()
    );
}

#[test]
fn offer_binds_the_exact_manifest_length_and_digest() {
    let encoded = manifest(&[(1, (1 << 32) + 1, "big.bin", "application/octet-stream")]);
    let body = offer(&encoded);
    let Message::TransferOffer { manifest, .. } =
        decode(MessageKind::TransferOffer, &body).unwrap()
    else {
        panic!("expected offer");
    };
    assert_eq!(manifest.files()[0].size, (1 << 32) + 1);
    let mut wrong_digest = body.clone();
    wrong_digest[32] ^= 1;
    assert_eq!(
        decode(MessageKind::TransferOffer, &wrong_digest).unwrap_err(),
        BodyError::IntegrityMismatch
    );
    let mut shorter = body.clone();
    shorter[64..68].copy_from_slice(&u32::try_from(encoded.len() - 1).unwrap().to_be_bytes());
    assert_eq!(
        decode(MessageKind::TransferOffer, &shorter).unwrap_err(),
        BodyError::MalformedFrame
    );
    let mut extra = body;
    extra.push(0);
    assert_eq!(
        decode(MessageKind::TransferOffer, &extra).unwrap_err(),
        BodyError::MalformedFrame
    );
}

#[test]
fn all_registered_messages_have_a_valid_body_example() {
    let manifest_bytes = manifest(&[(1, 0, "empty", "a/b")]);
    let digest = blake3::hash(b"").as_bytes().to_vec();
    let decision = [transfer_context(), vec![0x44; 32]].concat();
    let summary = [decision.clone(), vec![0, 1], vec![0; 8]].concat();
    let file_digest = [sized_file(0), digest].concat();
    let cases = [
        (MessageKind::SessionHello, hello()),
        (MessageKind::SessionHelloAck, hello()),
        (
            MessageKind::PairAuthenticate,
            [vec![1; 16], vec![2; 32], text("Sender")].concat(),
        ),
        (MessageKind::PairResult, [vec![1; 16], vec![2; 16]].concat()),
        (MessageKind::TransferOffer, offer(&manifest_bytes)),
        (MessageKind::TransferAccept, decision.clone()),
        (
            MessageKind::TransferDecline,
            [decision, vec![0, 1]].concat(),
        ),
        (MessageKind::FileBegin, sized_file(0)),
        (MessageKind::FileReady, sized_file(0)),
        (MessageKind::DataChunk, [sized_file(0), vec![42]].concat()),
        (MessageKind::FileEnd, file_digest.clone()),
        (MessageKind::FileVerified, file_digest.clone()),
        (
            MessageKind::FileCommitted,
            [file_digest, vec![3; 16], text("saved")].concat(),
        ),
        (MessageKind::TransferComplete, summary.clone()),
        (MessageKind::CompleteAck, summary),
        (
            MessageKind::TransferStatus,
            [file_context(), vec![4], vec![0; 8]].concat(),
        ),
        (
            MessageKind::Cancel,
            [transfer_context(), vec![0, 1]].concat(),
        ),
        (MessageKind::Error, [vec![0; 32], vec![0, 2, 0]].concat()),
    ];
    for (kind, bytes) in cases {
        let message = decode(kind, &bytes).unwrap();
        assert_eq!(message.kind(), kind);
        let encoded = nuvyro_transfer_core::protocol::encode_message(&message).unwrap();
        assert_eq!(encoded.as_slice(), frame(kind, &bytes), "{kind:?}");
        let header = header(kind, bytes.len());
        for truncated in 0..bytes.len() {
            assert_eq!(
                Message::decode(header, &bytes[..truncated]).unwrap_err(),
                BodyError::MalformedFrame,
                "{kind:?} truncated to {truncated}"
            );
        }
    }
}

#[test]
fn hello_rejects_unknown_versions_capabilities_and_zero_session() {
    for offset in [1, 3, 23] {
        let mut bytes = hello();
        bytes[offset] = 9;
        assert_eq!(
            decode(MessageKind::SessionHello, &bytes).unwrap_err(),
            BodyError::UnsupportedVersion
        );
    }
    let mut bytes = hello();
    bytes[4..20].fill(0);
    assert_eq!(
        decode(MessageKind::SessionHello, &bytes).unwrap_err(),
        BodyError::MalformedFrame
    );
}

#[test]
fn claimed_names_are_strict_bounded_unicode_and_debug_is_redacted() {
    let name = "😀".repeat(32);
    let body = [vec![1; 16], vec![2; 32], text(&name)].concat();
    let message = decode(MessageKind::PairAuthenticate, &body).unwrap();
    assert_eq!(
        format!("{message:?}"),
        "Message { kind: PairAuthenticate, .. }"
    );
    for name in ["a".repeat(33), "name\n".to_string(), "\u{85}".to_string()] {
        let body = [vec![1; 16], vec![2; 32], text(&name)].concat();
        assert_eq!(
            decode(MessageKind::PairAuthenticate, &body).unwrap_err(),
            BodyError::MalformedFrame
        );
    }
    let invalid_utf8 = [vec![1; 16], vec![2; 32], vec![0, 1, 255]].concat();
    assert_eq!(
        decode(MessageKind::PairAuthenticate, &invalid_utf8).unwrap_err(),
        BodyError::MalformedFrame
    );
    let zero_token = [vec![1; 16], vec![0; 32], text("A")].concat();
    assert_eq!(
        decode(MessageKind::PairAuthenticate, &zero_token).unwrap_err(),
        BodyError::MalformedFrame
    );
}

#[test]
fn authentication_text_lengths_and_trailing_bytes_are_not_implicit() {
    for (length, expected) in [
        (0, BodyError::LimitExceeded),
        (u16::MAX, BodyError::LimitExceeded),
        (128, BodyError::MalformedFrame),
    ] {
        let bytes = [
            vec![1; 16],
            vec![2; 32],
            length.to_be_bytes().to_vec(),
            vec![b'A'],
        ]
        .concat();
        assert_eq!(
            decode(MessageKind::PairAuthenticate, &bytes).unwrap_err(),
            expected
        );
    }
    let mut extra = [vec![1; 16], vec![2; 32], text("A")].concat();
    extra.push(0);
    assert_eq!(
        decode(MessageKind::PairAuthenticate, &extra).unwrap_err(),
        BodyError::MalformedFrame
    );
}

#[test]
fn commit_receipt_and_summary_fields_have_their_own_bounds() {
    let file = [sized_file(0), blake3::hash(b"").as_bytes().to_vec()].concat();
    let missing_receipt = [file.clone(), vec![0; 16], text("saved.txt")].concat();
    assert_eq!(
        decode(MessageKind::FileCommitted, &missing_receipt).unwrap_err(),
        BodyError::MalformedFrame
    );
    let unsafe_name = [file, vec![3; 16], text("../saved")].concat();
    assert_eq!(
        decode(MessageKind::FileCommitted, &unsafe_name).unwrap_err(),
        BodyError::UnsafeName
    );
    for (count, total, expected) in [
        (0_u16, 0_u64, BodyError::MalformedFrame),
        (129, 0, BodyError::LimitExceeded),
        (1, MAX_TOTAL_BYTES + 1, BodyError::LimitExceeded),
    ] {
        let bytes = [
            transfer_context(),
            vec![0x44; 32],
            count.to_be_bytes().to_vec(),
            total.to_be_bytes().to_vec(),
        ]
        .concat();
        for kind in [MessageKind::TransferComplete, MessageKind::CompleteAck] {
            assert_eq!(decode(kind, &bytes).unwrap_err(), expected);
        }
    }
}

#[test]
fn ready_offsets_enums_and_error_scopes_are_checked() {
    assert_eq!(
        decode(MessageKind::FileReady, &sized_file(1)).unwrap_err(),
        BodyError::MalformedFrame
    );
    for kind in [MessageKind::TransferDecline, MessageKind::Cancel] {
        let mut body = transfer_context();
        if kind == MessageKind::TransferDecline {
            body.extend_from_slice(&[0x44; 32]);
        }
        body.extend_from_slice(&0_u16.to_be_bytes());
        assert_eq!(decode(kind, &body).unwrap_err(), BodyError::MalformedFrame);
    }
    let bad_status = [file_context(), vec![0], vec![0; 8]].concat();
    assert_eq!(
        decode(MessageKind::TransferStatus, &bad_status).unwrap_err(),
        BodyError::MalformedFrame
    );
    for body in [
        [vec![0; 32], vec![0, 15, 0]].concat(),
        [vec![0; 32], vec![0, 2, 2]].concat(),
        [vec![0; 32], vec![0, 2, 1]].concat(),
        [transfer_context(), vec![0, 2, 0]].concat(),
    ] {
        assert_eq!(
            decode(MessageKind::Error, &body).unwrap_err(),
            BodyError::MalformedFrame
        );
    }
    assert!(
        decode(
            MessageKind::Error,
            &[transfer_context(), vec![0, 2, 1]].concat()
        )
        .is_ok()
    );
}

#[test]
fn chunk_offsets_use_checked_u64_arithmetic() {
    for offset in [u64::MAX, MAX_FILE_BYTES] {
        let bytes = [sized_file(offset), vec![1]].concat();
        assert_eq!(
            decode(MessageKind::DataChunk, &bytes).unwrap_err(),
            BodyError::LimitExceeded
        );
    }
    let offset = (1_u64 << 32) + 5;
    let bytes = [sized_file(offset), vec![1]].concat();
    assert!(matches!(
        decode(MessageKind::DataChunk, &bytes).unwrap(),
        Message::DataChunk { offset: actual, bytes: [1], .. } if actual == offset
    ));
}

#[test]
fn stream_handles_every_split_of_the_public_frame_examples() {
    for case in vectors()["frame_examples"].as_array().unwrap() {
        let bytes = hex(case["hex"].as_str().unwrap());
        for split in 0..=bytes.len() {
            let mut decoder = StreamDecoder::default();
            let first = decoder.push(&bytes[..split]).unwrap();
            assert_eq!(first.consumed, split);
            let frame = if let Some(frame) = first.frame {
                assert_eq!(split, bytes.len());
                frame
            } else {
                let second = decoder.push(&bytes[split..]).unwrap();
                assert_eq!(second.consumed, bytes.len() - split);
                second.frame.unwrap()
            };
            assert_eq!(
                frame.decode().unwrap().kind() as u16,
                case["kind"].as_u64().unwrap() as u16
            );
            assert_eq!(decoder.finish(), Ok(()));
            assert_eq!(decoder.push(&[]).unwrap_err(), StreamError::Closed);
        }
    }
}

#[test]
fn coalesced_frames_return_one_frame_at_a_time_for_backpressure() {
    let one = frame(MessageKind::SessionHello, &hello());
    let bytes = [one.clone(), one.clone()].concat();
    let mut decoder = StreamDecoder::default();
    let first = decoder.push(&bytes).unwrap();
    assert_eq!(first.consumed, one.len());
    assert!(first.frame.is_some());
    let second = decoder.push(&bytes[first.consumed..]).unwrap();
    assert_eq!(second.consumed, one.len());
    assert!(second.frame.is_some());
    assert_eq!(decoder.finish(), Ok(()));
}

#[test]
fn truncated_eof_cancel_and_invalid_headers_close_the_decoder() {
    let bytes = frame(MessageKind::SessionHello, &hello());
    for end in 1..bytes.len() {
        let mut decoder = StreamDecoder::default();
        assert!(decoder.push(&bytes[..end]).unwrap().frame.is_none());
        assert_eq!(decoder.finish(), Err(StreamError::Truncated));
        assert_eq!(decoder.push(&bytes).unwrap_err(), StreamError::Closed);
    }
    let mut decoder = StreamDecoder::default();
    decoder.push(&bytes[..12]).unwrap();
    decoder.cancel();
    decoder.cancel();
    assert_eq!(decoder.push(&bytes).unwrap_err(), StreamError::Closed);
    for (prefix, error) in [
        ([255, 255, 255, 255, 0, 1], HeaderError::InvalidLength),
        ([0, 0, 0, 26, 1, 3], HeaderError::UnknownMessageType(0x0103)),
    ] {
        let mut decoder = StreamDecoder::default();
        assert_eq!(
            decoder.push(&prefix).unwrap_err(),
            StreamError::Header(error)
        );
        assert_eq!(decoder.push(&bytes).unwrap_err(), StreamError::Closed);
    }
}

#[test]
fn a_bad_body_cannot_be_followed_by_a_valid_coalesced_frame() {
    let mut invalid = hello();
    invalid[23] = 1;
    let bytes = [
        frame(MessageKind::SessionHello, &invalid),
        frame(MessageKind::SessionHello, &hello()),
    ]
    .concat();
    let mut decoder = StreamDecoder::default();
    assert_eq!(
        decoder.push(&bytes).unwrap_err(),
        StreamError::Body(BodyError::UnsupportedVersion)
    );
    assert_eq!(decoder.push(&bytes[30..]).unwrap_err(), StreamError::Closed);
}

#[test]
fn largest_chunk_is_accepted_at_the_wire_limit() {
    let bytes = frame(
        MessageKind::DataChunk,
        &[sized_file(0), vec![0x5a; MAX_CHUNK_BYTES as usize]].concat(),
    );
    let mut decoder = StreamDecoder::default();
    let decoded = decoder.push(&bytes).unwrap();
    assert_eq!(decoded.consumed, bytes.len());
    let frame = decoded.frame.unwrap();
    let Message::DataChunk { bytes, .. } = frame.decode().unwrap() else {
        panic!("expected data");
    };
    assert_eq!(bytes.len(), MAX_CHUNK_BYTES as usize);
    assert!(bytes.iter().all(|byte| *byte == 0x5a));
    assert_eq!(frame.header().body_len(), 56 + MAX_CHUNK_BYTES);
    assert!(!format!("{frame:?}").contains("90, 90"));
}
