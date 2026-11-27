use std::fmt;

use super::cursor::Cursor;
use super::limits::{
    MAX_FILE_BYTES, MAX_FILES, MAX_MANIFEST_BYTES, MAX_NAME_BYTES, MAX_TOTAL_BYTES,
};
use super::manifest::validate_filename;
use super::{BodyError, ErrorCode, FrameHeader, Manifest, MessageKind, WIRE_MAJOR, WIRE_MINOR};

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct Id([u8; 16]);

impl Id {
    pub fn from_bytes(bytes: [u8; 16]) -> Result<Self, BodyError> {
        if bytes == [0; 16] {
            Err(BodyError::MalformedFrame)
        } else {
            Ok(Self(bytes))
        }
    }

    pub const fn as_bytes(&self) -> &[u8; 16] {
        &self.0
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TransferContext {
    pub pair_id: Id,
    pub transfer_id: Id,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FileContext {
    pub transfer: TransferContext,
    pub file_id: Id,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Hello {
    pub secure_session_id: Id,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FileDigest {
    pub context: FileContext,
    pub size: u64,
    pub digest: [u8; 32],
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TransferSummary {
    pub context: TransferContext,
    pub manifest_digest: [u8; 32],
    pub committed_count: u16,
    pub total_bytes: u64,
}

macro_rules! wire_enum {
    ($name:ident, $repr:ident, { $($variant:ident = $value:literal),+ $(,)? }) => {
        #[derive(Clone, Copy, Debug, Eq, PartialEq)]
        #[repr($repr)]
        pub enum $name { $($variant = $value),+ }

        impl TryFrom<$repr> for $name {
            type Error = BodyError;

            fn try_from(value: $repr) -> Result<Self, BodyError> {
                match value {
                    $($value => Ok(Self::$variant),)+
                    _ => Err(BodyError::MalformedFrame),
                }
            }
        }
    };
}

wire_enum!(DeclineReason, u16, {
    UserDeclined = 1, Busy = 2, UnsupportedFile = 3, StorageUnavailable = 4,
});
wire_enum!(CancelReason, u16, {
    UserCancelled = 1, SessionEnded = 2, NetworkLost = 3, DeadlineExceeded = 4,
});
wire_enum!(StatusPhase, u8, {
    AwaitingApproval = 1, PreparingStorage = 2, Verifying = 3, Saving = 4,
    AwaitingCompletion = 5,
});
wire_enum!(ErrorScope, u8, { Connection = 0, Transfer = 1 });

/// Decoding checks wire shape, never authentication, consent, or active IDs.
/// Tokens, names, and file data are deliberately excluded from Debug output.
pub enum Message<'a> {
    SessionHello(Hello),
    SessionHelloAck(Hello),
    PairAuthenticate {
        invitation_id: Id,
        token: &'a [u8; 32],
        claimed_sender_name: &'a str,
    },
    PairResult {
        pair_id: Id,
        secure_session_id: Id,
    },
    TransferOffer {
        context: TransferContext,
        manifest: Manifest<'a>,
    },
    TransferAccept {
        context: TransferContext,
        manifest_digest: [u8; 32],
    },
    TransferDecline {
        context: TransferContext,
        manifest_digest: [u8; 32],
        reason: DeclineReason,
    },
    FileBegin {
        context: FileContext,
        size: u64,
    },
    FileReady {
        context: FileContext,
        next_offset: u64,
    },
    DataChunk {
        context: FileContext,
        offset: u64,
        bytes: &'a [u8],
    },
    FileEnd(FileDigest),
    FileVerified(FileDigest),
    FileCommitted {
        file: FileDigest,
        receipt_id: Id,
        actual_name: &'a str,
    },
    TransferComplete(TransferSummary),
    CompleteAck(TransferSummary),
    TransferStatus {
        context: TransferContext,
        file_id: Option<Id>,
        phase: StatusPhase,
        phase_bytes: u64,
    },
    Cancel {
        context: TransferContext,
        reason: CancelReason,
    },
    Error {
        pair_id: Option<Id>,
        transfer_id: Option<Id>,
        code: ErrorCode,
        scope: ErrorScope,
    },
}

impl<'a> Message<'a> {
    pub fn decode(header: FrameHeader, body: &'a [u8]) -> Result<Self, BodyError> {
        if body.len() != header.body_len() as usize {
            return Err(BodyError::MalformedFrame);
        }
        let mut cursor = Cursor::new(body);
        let message = match header.kind() {
            MessageKind::SessionHello | MessageKind::SessionHelloAck => {
                let major = cursor.u16()?;
                let minor = cursor.u16()?;
                let secure_session_id = cursor.id()?;
                let capabilities = cursor.u32()?;
                if major != WIRE_MAJOR || minor != WIRE_MINOR || capabilities != 0 {
                    return Err(BodyError::UnsupportedVersion);
                }
                let hello = Hello { secure_session_id };
                if header.kind() == MessageKind::SessionHello {
                    Self::SessionHello(hello)
                } else {
                    Self::SessionHelloAck(hello)
                }
            }
            MessageKind::PairAuthenticate => {
                let invitation_id = cursor.id()?;
                let token = cursor.array::<32>()?;
                let claimed_sender_name = cursor.text(128)?;
                if token == &[0; 32]
                    || claimed_sender_name.chars().count() > 32
                    || claimed_sender_name.chars().any(char::is_control)
                {
                    return Err(BodyError::MalformedFrame);
                }
                Self::PairAuthenticate {
                    invitation_id,
                    token,
                    claimed_sender_name,
                }
            }
            MessageKind::PairResult => Self::PairResult {
                pair_id: cursor.id()?,
                secure_session_id: cursor.id()?,
            },
            MessageKind::TransferOffer => {
                let context = transfer_context(&mut cursor)?;
                let digest = *cursor.array::<32>()?;
                let len = cursor.u32()?;
                if len > MAX_MANIFEST_BYTES {
                    return Err(BodyError::LimitExceeded);
                }
                let manifest = Manifest::decode(cursor.take(len as usize)?)?;
                if manifest.digest() != &digest {
                    return Err(BodyError::IntegrityMismatch);
                }
                Self::TransferOffer { context, manifest }
            }
            MessageKind::TransferAccept => Self::TransferAccept {
                context: transfer_context(&mut cursor)?,
                manifest_digest: *cursor.array()?,
            },
            MessageKind::TransferDecline => Self::TransferDecline {
                context: transfer_context(&mut cursor)?,
                manifest_digest: *cursor.array()?,
                reason: DeclineReason::try_from(cursor.u16()?)?,
            },
            MessageKind::FileBegin => Self::FileBegin {
                context: file_context(&mut cursor)?,
                size: file_size(&mut cursor)?,
            },
            MessageKind::FileReady => {
                let context = file_context(&mut cursor)?;
                let next_offset = cursor.u64()?;
                if next_offset != 0 {
                    return Err(BodyError::MalformedFrame);
                }
                Self::FileReady {
                    context,
                    next_offset,
                }
            }
            MessageKind::DataChunk => {
                let context = file_context(&mut cursor)?;
                let offset = cursor.u64()?;
                let bytes = cursor.remaining();
                let end = offset
                    .checked_add(bytes.len() as u64)
                    .ok_or(BodyError::LimitExceeded)?;
                if end > MAX_FILE_BYTES {
                    return Err(BodyError::LimitExceeded);
                }
                Self::DataChunk {
                    context,
                    offset,
                    bytes,
                }
            }
            MessageKind::FileEnd => Self::FileEnd(file_digest(&mut cursor)?),
            MessageKind::FileVerified => Self::FileVerified(file_digest(&mut cursor)?),
            MessageKind::FileCommitted => {
                let file = file_digest(&mut cursor)?;
                let receipt_id = cursor.id()?;
                let actual_name = cursor.text(MAX_NAME_BYTES as usize)?;
                validate_filename(actual_name)?;
                Self::FileCommitted {
                    file,
                    receipt_id,
                    actual_name,
                }
            }
            MessageKind::TransferComplete => Self::TransferComplete(summary(&mut cursor)?),
            MessageKind::CompleteAck => Self::CompleteAck(summary(&mut cursor)?),
            MessageKind::TransferStatus => {
                let context = transfer_context(&mut cursor)?;
                let file_id = cursor.optional_id()?;
                let phase = StatusPhase::try_from(cursor.u8()?)?;
                let phase_bytes = cursor.u64()?;
                let maximum = if file_id.is_some() {
                    MAX_FILE_BYTES
                } else {
                    MAX_TOTAL_BYTES
                };
                if phase_bytes > maximum {
                    return Err(BodyError::LimitExceeded);
                }
                Self::TransferStatus {
                    context,
                    file_id,
                    phase,
                    phase_bytes,
                }
            }
            MessageKind::Cancel => Self::Cancel {
                context: transfer_context(&mut cursor)?,
                reason: CancelReason::try_from(cursor.u16()?)?,
            },
            MessageKind::Error => {
                let pair_id = cursor.optional_id()?;
                let transfer_id = cursor.optional_id()?;
                let code = ErrorCode::try_from(cursor.u16()?)?;
                let scope = ErrorScope::try_from(cursor.u8()?)?;
                match scope {
                    ErrorScope::Connection if transfer_id.is_some() => {
                        return Err(BodyError::MalformedFrame);
                    }
                    ErrorScope::Transfer if pair_id.is_none() || transfer_id.is_none() => {
                        return Err(BodyError::MalformedFrame);
                    }
                    _ => {}
                }
                Self::Error {
                    pair_id,
                    transfer_id,
                    code,
                    scope,
                }
            }
        };
        cursor.finish()?;
        Ok(message)
    }

    pub const fn kind(&self) -> MessageKind {
        match self {
            Self::SessionHello(_) => MessageKind::SessionHello,
            Self::SessionHelloAck(_) => MessageKind::SessionHelloAck,
            Self::PairAuthenticate { .. } => MessageKind::PairAuthenticate,
            Self::PairResult { .. } => MessageKind::PairResult,
            Self::TransferOffer { .. } => MessageKind::TransferOffer,
            Self::TransferAccept { .. } => MessageKind::TransferAccept,
            Self::TransferDecline { .. } => MessageKind::TransferDecline,
            Self::FileBegin { .. } => MessageKind::FileBegin,
            Self::FileReady { .. } => MessageKind::FileReady,
            Self::DataChunk { .. } => MessageKind::DataChunk,
            Self::FileEnd(_) => MessageKind::FileEnd,
            Self::FileVerified(_) => MessageKind::FileVerified,
            Self::FileCommitted { .. } => MessageKind::FileCommitted,
            Self::TransferComplete(_) => MessageKind::TransferComplete,
            Self::CompleteAck(_) => MessageKind::CompleteAck,
            Self::TransferStatus { .. } => MessageKind::TransferStatus,
            Self::Cancel { .. } => MessageKind::Cancel,
            Self::Error { .. } => MessageKind::Error,
        }
    }
}

impl fmt::Debug for Message<'_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Message")
            .field("kind", &self.kind())
            .finish_non_exhaustive()
    }
}

fn transfer_context(cursor: &mut Cursor<'_>) -> Result<TransferContext, BodyError> {
    Ok(TransferContext {
        pair_id: cursor.id()?,
        transfer_id: cursor.id()?,
    })
}

fn file_context(cursor: &mut Cursor<'_>) -> Result<FileContext, BodyError> {
    Ok(FileContext {
        transfer: transfer_context(cursor)?,
        file_id: cursor.id()?,
    })
}

fn file_size(cursor: &mut Cursor<'_>) -> Result<u64, BodyError> {
    let size = cursor.u64()?;
    if size > MAX_FILE_BYTES {
        Err(BodyError::LimitExceeded)
    } else {
        Ok(size)
    }
}

fn file_digest(cursor: &mut Cursor<'_>) -> Result<FileDigest, BodyError> {
    Ok(FileDigest {
        context: file_context(cursor)?,
        size: file_size(cursor)?,
        digest: *cursor.array()?,
    })
}

fn summary(cursor: &mut Cursor<'_>) -> Result<TransferSummary, BodyError> {
    let context = transfer_context(cursor)?;
    let manifest_digest = *cursor.array()?;
    let committed_count = cursor.u16()?;
    let total_bytes = cursor.u64()?;
    if committed_count == 0 {
        return Err(BodyError::MalformedFrame);
    }
    if committed_count > MAX_FILES || total_bytes > MAX_TOTAL_BYTES {
        return Err(BodyError::LimitExceeded);
    }
    Ok(TransferSummary {
        context,
        manifest_digest,
        committed_count,
        total_bytes,
    })
}
