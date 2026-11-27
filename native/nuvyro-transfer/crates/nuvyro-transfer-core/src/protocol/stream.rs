use std::fmt;

use super::limits::HEADER_BYTES;
use super::{BodyError, FrameHeader, HeaderError, Message};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StreamError {
    Header(HeaderError),
    Body(BodyError),
    AllocationFailed,
    Truncated,
    Closed,
}

/// One owned body, with no implicit queue and no secret-bearing Debug output.
pub struct Frame {
    header: FrameHeader,
    body: Vec<u8>,
}

impl Frame {
    pub const fn header(&self) -> FrameHeader {
        self.header
    }

    pub fn decode(&self) -> Result<Message<'_>, BodyError> {
        Message::decode(self.header, &self.body)
    }
}

impl fmt::Debug for Frame {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Frame")
            .field("header", &self.header)
            .finish_non_exhaustive()
    }
}

#[derive(Debug)]
pub struct Decoded {
    pub consumed: usize,
    pub frame: Option<Frame>,
}

/// Feed bytes from verified TLS and process each returned frame before feeding
/// the unconsumed suffix. At most one frame is returned per call, so a transport
/// can stop reading when its bounded downstream queue is full.
#[derive(Default)]
pub struct StreamDecoder {
    prefix: [u8; HEADER_BYTES],
    prefix_used: usize,
    header: Option<FrameHeader>,
    body: Vec<u8>,
    closed: bool,
}

impl StreamDecoder {
    pub fn push(&mut self, input: &[u8]) -> Result<Decoded, StreamError> {
        if self.closed {
            return Err(StreamError::Closed);
        }
        let mut consumed = 0;
        if self.header.is_none() {
            let count = (HEADER_BYTES - self.prefix_used).min(input.len());
            self.prefix[self.prefix_used..self.prefix_used + count]
                .copy_from_slice(&input[..count]);
            self.prefix_used += count;
            consumed += count;
            if self.prefix_used != HEADER_BYTES {
                return Ok(Decoded {
                    consumed,
                    frame: None,
                });
            }
            let header = match FrameHeader::decode(&self.prefix) {
                Ok(header) => header,
                Err(error) => {
                    self.cancel();
                    return Err(StreamError::Header(error));
                }
            };
            if self
                .body
                .try_reserve_exact(header.body_len() as usize)
                .is_err()
            {
                self.cancel();
                return Err(StreamError::AllocationFailed);
            }
            self.header = Some(header);
        }
        let header = self.header.ok_or(StreamError::Closed)?;
        let count = (header.body_len() as usize - self.body.len()).min(input.len() - consumed);
        self.body
            .extend_from_slice(&input[consumed..consumed + count]);
        consumed += count;
        if self.body.len() != header.body_len() as usize {
            return Ok(Decoded {
                consumed,
                frame: None,
            });
        }
        let frame = Frame {
            header,
            body: std::mem::take(&mut self.body),
        };
        self.header = None;
        self.prefix_used = 0;
        // Reject a malformed body before another coalesced frame can be read.
        // The returned frame owns bytes so its message can borrow them later.
        if let Err(error) = frame.decode() {
            self.cancel();
            return Err(StreamError::Body(error));
        }
        Ok(Decoded {
            consumed,
            frame: Some(frame),
        })
    }

    /// EOF closes the decoder even when it is on a frame boundary.
    pub fn finish(&mut self) -> Result<(), StreamError> {
        if self.closed {
            return Err(StreamError::Closed);
        }
        let truncated = self.prefix_used != 0 || self.header.is_some();
        self.cancel();
        if truncated {
            Err(StreamError::Truncated)
        } else {
            Ok(())
        }
    }

    /// Idempotently release any partially received frame.
    pub fn cancel(&mut self) {
        self.body = Vec::new();
        self.prefix = [0; HEADER_BYTES];
        self.prefix_used = 0;
        self.header = None;
        self.closed = true;
    }
}
