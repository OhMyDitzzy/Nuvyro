//! Protocol 1.0 definitions. See docs/transfer-protocol-v1.md in the repository.

mod cursor;
mod encode;
mod error;
mod frame;
pub mod limits;
mod manifest;
mod message;
mod stream;

pub use encode::{EncodedFrame, encode_manifest, encode_message};
pub use error::{BodyError, ErrorCode, HeaderError};
pub use frame::{FrameHeader, MessageKind};
pub use manifest::{Manifest, ManifestEntry};
pub use message::{
    CancelReason, DeclineReason, ErrorScope, FileContext, FileDigest, Hello, Id, Message,
    StatusPhase, TransferContext, TransferSummary,
};
pub use stream::{Decoded, Frame, StreamDecoder, StreamError};

pub const WIRE_MAJOR: u16 = 1;
pub const WIRE_MINOR: u16 = 0;
pub const TRANSFER_PORT: u16 = 47739;
pub const TRANSFER_SERVICE_TYPE: &str = "_nuvyro-xfer._tcp.";
pub const TLS_ALPN: &[u8] = b"nuvyro-transfer/1";
pub const PAIRING_QR_PREFIX: &str = "NUVYRO:PAIR:1:";
pub const MANIFEST_DOMAIN: &[u8] = b"NuvyroManifestV1\0";
