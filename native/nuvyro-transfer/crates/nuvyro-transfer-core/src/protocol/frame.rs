use super::HeaderError;
use super::limits::{
    DATA_CHUNK_FIXED_BODY_LEN, HEADER_BYTES, MAX_CHUNK_BYTES, MAX_CONTROL_FRAME_LEN, MAX_FRAME_LEN,
    MAX_MANIFEST_BYTES,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u16)]
pub enum MessageKind {
    SessionHello = 0x0001,
    SessionHelloAck = 0x0002,
    PairAuthenticate = 0x0003,
    PairResult = 0x0004,
    TransferOffer = 0x0100,
    TransferAccept = 0x0101,
    TransferDecline = 0x0102,
    FileBegin = 0x0200,
    FileReady = 0x0201,
    DataChunk = 0x0202,
    FileEnd = 0x0203,
    FileVerified = 0x0204,
    FileCommitted = 0x0205,
    TransferComplete = 0x0300,
    CompleteAck = 0x0301,
    TransferStatus = 0x0302,
    Cancel = 0x7f00,
    Error = 0x7fff,
}

impl TryFrom<u16> for MessageKind {
    type Error = HeaderError;

    fn try_from(value: u16) -> Result<Self, HeaderError> {
        match value {
            0x0001 => Ok(Self::SessionHello),
            0x0002 => Ok(Self::SessionHelloAck),
            0x0003 => Ok(Self::PairAuthenticate),
            0x0004 => Ok(Self::PairResult),
            0x0100 => Ok(Self::TransferOffer),
            0x0101 => Ok(Self::TransferAccept),
            0x0102 => Ok(Self::TransferDecline),
            0x0200 => Ok(Self::FileBegin),
            0x0201 => Ok(Self::FileReady),
            0x0202 => Ok(Self::DataChunk),
            0x0203 => Ok(Self::FileEnd),
            0x0204 => Ok(Self::FileVerified),
            0x0205 => Ok(Self::FileCommitted),
            0x0300 => Ok(Self::TransferComplete),
            0x0301 => Ok(Self::CompleteAck),
            0x0302 => Ok(Self::TransferStatus),
            0x7f00 => Ok(Self::Cancel),
            0x7fff => Ok(Self::Error),
            other => Err(HeaderError::UnknownMessageType(other)),
        }
    }
}

impl MessageKind {
    fn body_bounds(self) -> (u32, u32) {
        match self {
            Self::SessionHello | Self::SessionHelloAck => (24, 24),
            Self::PairAuthenticate => (51, 178),
            Self::PairResult => (32, 32),
            Self::TransferOffer => (80, 68 + MAX_MANIFEST_BYTES),
            Self::TransferAccept => (64, 64),
            Self::TransferDecline => (66, 66),
            Self::FileBegin | Self::FileReady => (56, 56),
            Self::DataChunk => (
                DATA_CHUNK_FIXED_BODY_LEN + 1,
                DATA_CHUNK_FIXED_BODY_LEN + MAX_CHUNK_BYTES,
            ),
            Self::FileEnd | Self::FileVerified => (88, 88),
            Self::FileCommitted => (107, 361),
            Self::TransferComplete | Self::CompleteAck => (74, 74),
            Self::TransferStatus => (57, 57),
            Self::Cancel => (34, 34),
            Self::Error => (35, 35),
        }
    }
}

/// Validated allocation bounds only; no body, state, or authentication checks.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FrameHeader {
    kind: MessageKind,
    body_len: u32,
}

impl FrameHeader {
    pub fn decode(bytes: &[u8; HEADER_BYTES]) -> Result<Self, HeaderError> {
        let frame_len = u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
        if !(2..=MAX_FRAME_LEN).contains(&frame_len) {
            return Err(HeaderError::InvalidLength);
        }
        let kind = MessageKind::try_from(u16::from_be_bytes([bytes[4], bytes[5]]))?;
        if kind != MessageKind::DataChunk && frame_len > MAX_CONTROL_FRAME_LEN {
            return Err(HeaderError::InvalidLength);
        }
        let body_len = frame_len - 2;
        let (minimum, maximum) = kind.body_bounds();
        if !(minimum..=maximum).contains(&body_len) {
            return Err(HeaderError::InvalidLength);
        }
        Ok(Self { kind, body_len })
    }

    pub const fn kind(self) -> MessageKind {
        self.kind
    }

    pub const fn body_len(self) -> u32 {
        self.body_len
    }
}
