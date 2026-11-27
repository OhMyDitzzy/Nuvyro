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
