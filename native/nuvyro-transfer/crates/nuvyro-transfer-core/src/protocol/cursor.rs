use super::{BodyError, Id};

pub(super) struct Cursor<'a> {
    bytes: &'a [u8],
    position: usize,
}

impl<'a> Cursor<'a> {
    pub(super) fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, position: 0 }
    }

    pub(super) fn take(&mut self, len: usize) -> Result<&'a [u8], BodyError> {
        let end = self
            .position
            .checked_add(len)
            .ok_or(BodyError::MalformedFrame)?;
        let value = self
            .bytes
            .get(self.position..end)
            .ok_or(BodyError::MalformedFrame)?;
        self.position = end;
        Ok(value)
    }

    pub(super) fn array<const N: usize>(&mut self) -> Result<&'a [u8; N], BodyError> {
        self.take(N)?
            .try_into()
            .map_err(|_| BodyError::MalformedFrame)
    }

    pub(super) fn u8(&mut self) -> Result<u8, BodyError> {
        Ok(self.array::<1>()?[0])
    }

    pub(super) fn u16(&mut self) -> Result<u16, BodyError> {
        Ok(u16::from_be_bytes(*self.array()?))
    }

    pub(super) fn u32(&mut self) -> Result<u32, BodyError> {
        Ok(u32::from_be_bytes(*self.array()?))
    }

    pub(super) fn u64(&mut self) -> Result<u64, BodyError> {
        Ok(u64::from_be_bytes(*self.array()?))
    }

    pub(super) fn id(&mut self) -> Result<Id, BodyError> {
        Id::from_bytes(*self.array::<16>()?)
    }

    pub(super) fn optional_id(&mut self) -> Result<Option<Id>, BodyError> {
        let bytes = *self.array::<16>()?;
        if bytes == [0; 16] {
            Ok(None)
        } else {
            Id::from_bytes(bytes).map(Some)
        }
    }

    pub(super) fn text(&mut self, maximum: usize) -> Result<&'a str, BodyError> {
        let len = usize::from(self.u16()?);
        if len == 0 || len > maximum {
            return Err(BodyError::LimitExceeded);
        }
        std::str::from_utf8(self.take(len)?).map_err(|_| BodyError::MalformedFrame)
    }

    pub(super) fn remaining(&mut self) -> &'a [u8] {
        let result = &self.bytes[self.position..];
        self.position = self.bytes.len();
        result
    }

    pub(super) fn finish(self) -> Result<(), BodyError> {
        if self.position == self.bytes.len() {
            Ok(())
        } else {
            Err(BodyError::MalformedFrame)
        }
    }
}
