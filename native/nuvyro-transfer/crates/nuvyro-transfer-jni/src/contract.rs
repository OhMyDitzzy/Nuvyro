//! ABI 1 contract values; JNI entry points are implemented in a later milestone.

pub const ABI_VERSION: i32 = 1;
pub const MAX_COMMAND_BYTES: usize = 65_536;
pub const MAX_EVENT_BATCH_BYTES: usize = 65_536;
pub const MAX_EVENT_BYTES: usize = 16_384;
pub const MAX_EVENTS_PER_POLL: usize = 32;
pub const MAX_POLL_TIMEOUT_MS: i32 = 1_000;
pub const COMMAND_QUEUE_CAPACITY: usize = 32;
pub const TERMINAL_EVENT_CAPACITY: usize = 16;
pub const PROGRESS_EVENT_CAPACITY: usize = 32;
pub const MAX_PROVIDER_IO_WORKERS: usize = 2;
pub const MAX_JSON_DEPTH: usize = 6;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(i32)]
pub enum NativeStatus {
    Accepted = 0,
    InvalidArgument = 1,
    InvalidHandle = 2,
    AbiMismatch = 3,
    InvalidState = 4,
    QueueFull = 5,
    ResourceLimit = 6,
    InternalFailure = 7,
}
