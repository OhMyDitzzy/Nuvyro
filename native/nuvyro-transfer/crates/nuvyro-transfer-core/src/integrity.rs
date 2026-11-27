//! Per-file order and byte integrity, with no network or storage side effects.

use crate::protocol::limits::{MAX_CHUNK_BYTES, MAX_FILE_BYTES};
use crate::protocol::{ErrorCode, FileContext, FileDigest, Message};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FileStreamPhase {
    AwaitingBegin,
    AwaitingDestination,
    Streaming,
    IntegrityChecked,
    Cancelled,
    Failed,
}

pub enum FileStreamStep<'a> {
    PrepareDestination,
    Data(&'a [u8]),
    IntegrityChecked(FileDigest),
}

/// An integrity checker is not permission to open a source or destination.
/// The coordinator must bind the approved manifest, authenticated connection,
/// generation, and storage preflight before using it. IntegrityChecked covers
/// received bytes only; staging readback, export, and receipts remain mandatory.
pub struct FileStreamVerifier {
    context: FileContext,
    offered_size: u64,
    received: u64,
    hasher: blake3::Hasher,
    phase: FileStreamPhase,
}

impl FileStreamVerifier {
    pub fn new(context: FileContext, offered_size: u64) -> Result<Self, ErrorCode> {
        if offered_size > MAX_FILE_BYTES {
            return Err(ErrorCode::LimitExceeded);
        }
        Ok(Self {
            context,
            offered_size,
            received: 0,
            hasher: blake3::Hasher::new(),
            phase: FileStreamPhase::AwaitingBegin,
        })
    }

    pub const fn phase(&self) -> FileStreamPhase {
        self.phase
    }

    pub const fn received_bytes(&self) -> u64 {
        self.received
    }

    pub fn observe<'a>(&mut self, message: &Message<'a>) -> Result<FileStreamStep<'a>, ErrorCode> {
        if self.is_terminal() {
            return Err(ErrorCode::UnexpectedState);
        }
        match message {
            Message::FileBegin { context, size }
                if self.phase == FileStreamPhase::AwaitingBegin
                    && *context == self.context
                    && *size == self.offered_size =>
            {
                self.phase = FileStreamPhase::AwaitingDestination;
                Ok(FileStreamStep::PrepareDestination)
            }
            Message::DataChunk {
                context,
                offset,
                bytes,
            } if self.phase == FileStreamPhase::Streaming && *context == self.context => {
                if *offset != self.received {
                    return self.reject(ErrorCode::UnexpectedState);
                }
                if bytes.is_empty() || bytes.len() > MAX_CHUNK_BYTES as usize {
                    return self.reject(ErrorCode::LimitExceeded);
                }
                let Some(end) = self.received.checked_add(bytes.len() as u64) else {
                    return self.reject(ErrorCode::LimitExceeded);
                };
                if end > self.offered_size {
                    return self.reject(ErrorCode::LimitExceeded);
                }
                self.hasher.update(bytes);
                self.received = end;
                Ok(FileStreamStep::Data(*bytes))
            }
            Message::FileEnd(file)
                if self.phase == FileStreamPhase::Streaming && file.context == self.context =>
            {
                if self.received != self.offered_size || file.size != self.offered_size {
                    return self.reject(ErrorCode::IntegrityMismatch);
                }
                if self.hasher.finalize().as_bytes() != &file.digest {
                    return self.reject(ErrorCode::IntegrityMismatch);
                }
                self.phase = FileStreamPhase::IntegrityChecked;
                Ok(FileStreamStep::IntegrityChecked(*file))
            }
            _ => self.reject(ErrorCode::UnexpectedState),
        }
    }

    /// Called locally after the staging destination is open and ready for data.
    /// A peer-sent FileReady never invokes this transition.
    pub fn destination_ready(&mut self) -> Result<(), ErrorCode> {
        if self.is_terminal() {
            return Err(ErrorCode::UnexpectedState);
        }
        if self.phase != FileStreamPhase::AwaitingDestination {
            return self.reject(ErrorCode::UnexpectedState);
        }
        self.phase = FileStreamPhase::Streaming;
        Ok(())
    }

    pub fn cancel(&mut self) {
        if !self.is_terminal() {
            self.phase = FileStreamPhase::Cancelled;
            self.hasher = blake3::Hasher::new();
        }
    }

    fn is_terminal(&self) -> bool {
        matches!(
            self.phase,
            FileStreamPhase::IntegrityChecked
                | FileStreamPhase::Cancelled
                | FileStreamPhase::Failed
        )
    }

    fn reject<T>(&mut self, error: ErrorCode) -> Result<T, ErrorCode> {
        self.phase = FileStreamPhase::Failed;
        self.hasher = blake3::Hasher::new();
        Err(error)
    }
}
