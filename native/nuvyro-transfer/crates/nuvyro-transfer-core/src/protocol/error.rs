/// Stable remote error codes; never include local paths, secrets, or panic text.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u16)]
pub enum ErrorCode {
    UnsupportedVersion = 0x0001,
    MalformedFrame = 0x0002,
    LimitExceeded = 0x0003,
    UnexpectedState = 0x0004,
    AuthenticationFailed = 0x0005,
    SessionExpired = 0x0006,
    Busy = 0x0007,
    UnsupportedFile = 0x0008,
    SourceUnavailable = 0x0009,
    StorageUnavailable = 0x000a,
    IntegrityMismatch = 0x000b,
    DeadlineExceeded = 0x000c,
    Cancelled = 0x000d,
    InternalFailure = 0x000e,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HeaderError {
    InvalidLength,
    UnknownMessageType(u16),
}

/// Local decoding failures contain no peer text or secret bytes.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BodyError {
    MalformedFrame,
    UnsupportedVersion,
    LimitExceeded,
    DuplicateId,
    UnsafeName,
    IntegrityMismatch,
    AllocationFailed,
}

impl BodyError {
    pub const fn remote_code(self) -> ErrorCode {
        match self {
            Self::MalformedFrame | Self::DuplicateId | Self::UnsafeName => {
                ErrorCode::MalformedFrame
            }
            Self::UnsupportedVersion => ErrorCode::UnsupportedVersion,
            Self::LimitExceeded => ErrorCode::LimitExceeded,
            Self::IntegrityMismatch => ErrorCode::IntegrityMismatch,
            Self::AllocationFailed => ErrorCode::InternalFailure,
        }
    }
}

impl TryFrom<u16> for ErrorCode {
    type Error = BodyError;

    fn try_from(value: u16) -> Result<Self, BodyError> {
        Ok(match value {
            1 => Self::UnsupportedVersion,
            2 => Self::MalformedFrame,
            3 => Self::LimitExceeded,
            4 => Self::UnexpectedState,
            5 => Self::AuthenticationFailed,
            6 => Self::SessionExpired,
            7 => Self::Busy,
            8 => Self::UnsupportedFile,
            9 => Self::SourceUnavailable,
            10 => Self::StorageUnavailable,
            11 => Self::IntegrityMismatch,
            12 => Self::DeadlineExceeded,
            13 => Self::Cancelled,
            14 => Self::InternalFailure,
            _ => return Err(BodyError::MalformedFrame),
        })
    }
}
