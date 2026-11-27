use std::fmt;

use super::cursor::Cursor;
use super::limits::{
    MAX_FILE_BYTES, MAX_FILES, MAX_MANIFEST_BYTES, MAX_MIME_BYTES, MAX_NAME_BYTES, MAX_TOTAL_BYTES,
};
use super::{BodyError, Id, MANIFEST_DOMAIN};

#[derive(Clone, Copy, Eq, PartialEq)]
pub struct ManifestEntry<'a> {
    pub file_id: Id,
    pub size: u64,
    pub name: &'a str,
    pub mime: &'a str,
}

/// Validated entries borrow the exact bytes that consent must bind to.
pub struct Manifest<'a> {
    encoded: &'a [u8],
    files: Vec<ManifestEntry<'a>>,
    total_bytes: u64,
    digest: [u8; 32],
}

impl<'a> Manifest<'a> {
    pub fn decode(encoded: &'a [u8]) -> Result<Self, BodyError> {
        if encoded.len() > MAX_MANIFEST_BYTES as usize {
            return Err(BodyError::LimitExceeded);
        }
        let mut cursor = Cursor::new(encoded);
        if cursor.u16()? != 1 {
            return Err(BodyError::MalformedFrame);
        }
        let count = cursor.u16()?;
        let total_bytes = cursor.u64()?;
        if count == 0 {
            return Err(BodyError::MalformedFrame);
        }
        if count > MAX_FILES || total_bytes > MAX_TOTAL_BYTES {
            return Err(BodyError::LimitExceeded);
        }
        let mut files: Vec<ManifestEntry<'a>> = Vec::new();
        files
            .try_reserve_exact(usize::from(count))
            .map_err(|_| BodyError::AllocationFailed)?;
        let mut sum = 0_u64;
        for _ in 0..count {
            let file_id = cursor.id()?;
            if files.iter().any(|file| file.file_id == file_id) {
                return Err(BodyError::DuplicateId);
            }
            let size = cursor.u64()?;
            if size > MAX_FILE_BYTES {
                return Err(BodyError::LimitExceeded);
            }
            sum = sum.checked_add(size).ok_or(BodyError::LimitExceeded)?;
            if sum > MAX_TOTAL_BYTES {
                return Err(BodyError::LimitExceeded);
            }
            let name = cursor.text(MAX_NAME_BYTES as usize)?;
            validate_filename(name)?;
            let mime = cursor.text(MAX_MIME_BYTES as usize)?;
            validate_mime(mime)?;
            files.push(ManifestEntry {
                file_id,
                size,
                name,
                mime,
            });
        }
        cursor.finish()?;
        if sum != total_bytes {
            return Err(BodyError::MalformedFrame);
        }
        let mut hasher = blake3::Hasher::new();
        hasher.update(MANIFEST_DOMAIN);
        hasher.update(encoded);
        Ok(Self {
            encoded,
            files,
            total_bytes,
            digest: *hasher.finalize().as_bytes(),
        })
    }

    pub fn encoded(&self) -> &'a [u8] {
        self.encoded
    }

    pub fn files(&self) -> &[ManifestEntry<'a>] {
        &self.files
    }

    pub const fn total_bytes(&self) -> u64 {
        self.total_bytes
    }

    pub const fn digest(&self) -> &[u8; 32] {
        &self.digest
    }
}

impl fmt::Debug for Manifest<'_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Manifest")
            .field("file_count", &self.files.len())
            .field("total_bytes", &self.total_bytes)
            .finish_non_exhaustive()
    }
}

pub(super) fn validate_filename(value: &str) -> Result<(), BodyError> {
    if value.is_empty() || value.len() > MAX_NAME_BYTES as usize {
        return Err(BodyError::LimitExceeded);
    }
    if matches!(value, "." | "..")
        || value.trim().is_empty()
        || value.chars().any(|ch| {
            ch.is_control()
                || matches!(ch, '/' | '\\' | '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}')
        })
    {
        return Err(BodyError::UnsafeName);
    }
    Ok(())
}

fn validate_mime(value: &str) -> Result<(), BodyError> {
    let Some((kind, subtype)) = value.split_once('/') else {
        return Err(BodyError::MalformedFrame);
    };
    let valid = |part: &str| {
        !part.is_empty()
            && part.bytes().all(|ch| {
                ch.is_ascii_alphanumeric()
                    || matches!(
                        ch,
                        b'!' | b'#' | b'$' | b'&' | b'^' | b'_' | b'.' | b'+' | b'-'
                    )
            })
    };
    if valid(kind) && valid(subtype) {
        Ok(())
    } else {
        Err(BodyError::MalformedFrame)
    }
}
