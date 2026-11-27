use nuvyro_transfer_core::integrity::{FileStreamPhase, FileStreamStep, FileStreamVerifier};
use nuvyro_transfer_core::protocol::limits::{MAX_CHUNK_BYTES, MAX_FILE_BYTES};
use nuvyro_transfer_core::protocol::{
    ErrorCode, FileContext, FileDigest, Id, Message, TransferContext,
};

fn id(value: u8) -> Id {
    Id::from_bytes([value; 16]).unwrap()
}

fn context() -> FileContext {
    FileContext {
        transfer: TransferContext {
            pair_id: id(1),
            transfer_id: id(2),
        },
        file_id: id(3),
    }
}

fn begin(size: u64) -> Message<'static> {
    Message::FileBegin {
        context: context(),
        size,
    }
}

fn chunk(offset: u64, bytes: &[u8]) -> Message<'_> {
    Message::DataChunk {
        context: context(),
        offset,
        bytes,
    }
}

fn end(bytes: &[u8]) -> Message<'static> {
    Message::FileEnd(FileDigest {
        context: context(),
        size: bytes.len() as u64,
        digest: *blake3::hash(bytes).as_bytes(),
    })
}

fn ready(size: u64) -> FileStreamVerifier {
    let mut verifier = FileStreamVerifier::new(context(), size).unwrap();
    assert!(matches!(
        verifier.observe(&begin(size)).unwrap(),
        FileStreamStep::PrepareDestination
    ));
    assert_eq!(verifier.phase(), FileStreamPhase::AwaitingDestination);
    verifier.destination_ready().unwrap();
    verifier
}

#[test]
fn exact_sequential_chunks_and_hash_complete_only_the_integrity_check() {
    let mut verifier = ready(6);
    assert!(matches!(
        verifier.observe(&chunk(0, b"abc")).unwrap(),
        FileStreamStep::Data(b"abc")
    ));
    assert_eq!(verifier.received_bytes(), 3);
    verifier.observe(&chunk(3, b"def")).unwrap();
    let FileStreamStep::IntegrityChecked(file) = verifier.observe(&end(b"abcdef")).unwrap() else {
        panic!("expected integrity result");
    };
    assert_eq!(file.context, context());
    assert_eq!(file.size, 6);
    assert_eq!(verifier.phase(), FileStreamPhase::IntegrityChecked);
    assert_eq!(
        verifier.observe(&begin(6)).err(),
        Some(ErrorCode::UnexpectedState)
    );
    assert_eq!(verifier.phase(), FileStreamPhase::IntegrityChecked);
}

#[test]
fn an_empty_file_still_requires_begin_local_ready_and_matching_end() {
    let mut verifier = ready(0);
    assert_eq!(verifier.received_bytes(), 0);
    assert!(matches!(
        verifier.observe(&end(b"")).unwrap(),
        FileStreamStep::IntegrityChecked(FileDigest { size: 0, .. })
    ));
    let mut verifier = ready(0);
    assert_eq!(
        verifier.observe(&chunk(0, b"x")).err(),
        Some(ErrorCode::LimitExceeded)
    );
}

#[test]
fn peer_frames_cannot_replace_local_destination_readiness() {
    let mut verifier = FileStreamVerifier::new(context(), 1).unwrap();
    assert_eq!(
        verifier.observe(&chunk(0, b"x")).err(),
        Some(ErrorCode::UnexpectedState)
    );
    assert_eq!(verifier.phase(), FileStreamPhase::Failed);
    assert_eq!(
        verifier.observe(&begin(1)).err(),
        Some(ErrorCode::UnexpectedState)
    );
    let mut verifier = FileStreamVerifier::new(context(), 1).unwrap();
    verifier.observe(&begin(1)).unwrap();
    assert_eq!(
        verifier
            .observe(&Message::FileReady {
                context: context(),
                next_offset: 0,
            })
            .err(),
        Some(ErrorCode::UnexpectedState)
    );
    let mut verifier = FileStreamVerifier::new(context(), 1).unwrap();
    verifier.observe(&begin(1)).unwrap();
    assert_eq!(
        verifier.observe(&chunk(0, b"x")).err(),
        Some(ErrorCode::UnexpectedState)
    );
}

#[test]
fn invalid_local_ready_and_repeated_begin_fail_closed() {
    let mut verifier = FileStreamVerifier::new(context(), 1).unwrap();
    assert_eq!(
        verifier.destination_ready(),
        Err(ErrorCode::UnexpectedState)
    );
    assert_eq!(verifier.phase(), FileStreamPhase::Failed);
    let mut verifier = ready(1);
    assert_eq!(
        verifier.destination_ready(),
        Err(ErrorCode::UnexpectedState)
    );
    let mut verifier = ready(1);
    assert_eq!(
        verifier.observe(&begin(1)).err(),
        Some(ErrorCode::UnexpectedState)
    );
    let mut verifier = FileStreamVerifier::new(context(), 1).unwrap();
    assert_eq!(
        verifier.observe(&begin(2)).err(),
        Some(ErrorCode::UnexpectedState)
    );
}

#[test]
fn each_context_identifier_must_match_the_expected_file() {
    for field in 0..3 {
        let mut wrong = context();
        match field {
            0 => wrong.transfer.pair_id = id(4),
            1 => wrong.transfer.transfer_id = id(4),
            _ => wrong.file_id = id(4),
        }
        let mut verifier = ready(1);
        assert_eq!(
            verifier
                .observe(&Message::DataChunk {
                    context: wrong,
                    offset: 0,
                    bytes: b"x",
                })
                .err(),
            Some(ErrorCode::UnexpectedState)
        );
        assert_eq!(verifier.received_bytes(), 0);
        assert_eq!(verifier.phase(), FileStreamPhase::Failed);
    }
}

#[test]
fn duplicates_gaps_empty_and_excess_chunks_are_rejected_without_counting_them() {
    for offset in [0, 2, u64::MAX] {
        let mut verifier = ready(2);
        verifier.observe(&chunk(0, b"x")).unwrap();
        assert_eq!(
            verifier.observe(&chunk(offset, b"x")).err(),
            Some(ErrorCode::UnexpectedState)
        );
        assert_eq!(verifier.received_bytes(), 1);
    }
    for bytes in [b"".as_slice(), b"xx".as_slice()] {
        let mut verifier = ready(1);
        assert_eq!(
            verifier.observe(&chunk(0, bytes)).err(),
            Some(ErrorCode::LimitExceeded)
        );
        assert_eq!(verifier.received_bytes(), 0);
    }
    let bytes = vec![1; MAX_CHUNK_BYTES as usize + 1];
    let mut verifier = ready(bytes.len() as u64);
    assert_eq!(
        verifier.observe(&chunk(0, &bytes)).err(),
        Some(ErrorCode::LimitExceeded)
    );
}

#[test]
fn early_end_wrong_size_and_corrupt_hash_fail_without_verified_result() {
    let mut verifier = ready(2);
    verifier.observe(&chunk(0, b"x")).unwrap();
    assert_eq!(
        verifier.observe(&end(b"x")).err(),
        Some(ErrorCode::IntegrityMismatch)
    );
    let mut verifier = ready(1);
    verifier.observe(&chunk(0, b"x")).unwrap();
    assert_eq!(
        verifier.observe(&end(b"y")).err(),
        Some(ErrorCode::IntegrityMismatch)
    );
    assert_eq!(verifier.phase(), FileStreamPhase::Failed);
    assert_eq!(
        verifier.observe(&end(b"x")).err(),
        Some(ErrorCode::UnexpectedState)
    );
    let mut verifier = ready(1);
    verifier.observe(&chunk(0, b"x")).unwrap();
    let wrong_size = Message::FileEnd(FileDigest {
        context: context(),
        size: 2,
        digest: *blake3::hash(b"x").as_bytes(),
    });
    assert_eq!(
        verifier.observe(&wrong_size).err(),
        Some(ErrorCode::IntegrityMismatch)
    );
}

#[test]
fn cancellation_is_terminal_and_never_restarts_a_partial_stream() {
    let mut verifier = ready(2);
    verifier.observe(&chunk(0, b"x")).unwrap();
    verifier.cancel();
    verifier.cancel();
    assert_eq!(verifier.phase(), FileStreamPhase::Cancelled);
    assert_eq!(verifier.received_bytes(), 1);
    assert_eq!(
        verifier.destination_ready(),
        Err(ErrorCode::UnexpectedState)
    );
    assert_eq!(
        verifier.observe(&chunk(1, b"y")).err(),
        Some(ErrorCode::UnexpectedState)
    );
    assert_eq!(verifier.phase(), FileStreamPhase::Cancelled);
}

#[test]
fn size_limits_are_checked_without_truncation_or_file_allocation() {
    assert!(FileStreamVerifier::new(context(), (1_u64 << 32) + 1).is_ok());
    assert!(FileStreamVerifier::new(context(), MAX_FILE_BYTES).is_ok());
    assert_eq!(
        FileStreamVerifier::new(context(), MAX_FILE_BYTES + 1).err(),
        Some(ErrorCode::LimitExceeded)
    );
}
