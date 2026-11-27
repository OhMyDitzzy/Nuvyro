use std::fmt;

use super::limits::{HEADER_BYTES, MAX_MANIFEST_BYTES, MAX_MIME_BYTES, MAX_NAME_BYTES};
use super::{
    BodyError, FileContext, FileDigest, FrameHeader, Id, Manifest, ManifestEntry, Message,
    TransferContext, WIRE_MAJOR, WIRE_MINOR,
};
use zeroize::Zeroizing;

/// Bounded encoded frame; secret-bearing storage is cleared on drop.
pub struct EncodedFrame(Zeroizing<Vec<u8>>);

impl EncodedFrame {
    pub fn as_slice(&self) -> &[u8] {
        &self.0
    }
}

impl fmt::Debug for EncodedFrame {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("EncodedFrame([REDACTED])")
    }
}

pub fn encode_message(message: &Message<'_>) -> Result<EncodedFrame, BodyError> {
    let (_, max_body) = message.kind().body_bounds();
    let mut out = Encoder::new(HEADER_BYTES + max_body as usize)?;
    out.bytes(&[0; HEADER_BYTES])?;
    match message {
        Message::SessionHello(h) | Message::SessionHelloAck(h) => {
            out.bytes(&WIRE_MAJOR.to_be_bytes())?;
            out.bytes(&WIRE_MINOR.to_be_bytes())?;
            out.id(h.secure_session_id)?;
            out.bytes(&0u32.to_be_bytes())?;
        }
        Message::PairAuthenticate {
            invitation_id,
            token,
            claimed_sender_name,
        } => {
            out.id(*invitation_id)?;
            out.bytes(token.as_slice())?;
            out.text(claimed_sender_name, 128)?;
        }
        Message::PairResult {
            pair_id,
            secure_session_id,
        } => {
            out.id(*pair_id)?;
            out.id(*secure_session_id)?;
        }
        Message::TransferOffer { context, manifest } => {
            out.transfer(*context)?;
            out.bytes(manifest.digest())?;
            out.bytes(&(manifest.encoded().len() as u32).to_be_bytes())?;
            out.bytes(manifest.encoded())?;
        }
        Message::TransferAccept {
            context,
            manifest_digest,
        } => {
            out.transfer(*context)?;
            out.bytes(manifest_digest)?;
        }
        Message::TransferDecline {
            context,
            manifest_digest,
            reason,
        } => {
            out.transfer(*context)?;
            out.bytes(manifest_digest)?;
            out.bytes(&(*reason as u16).to_be_bytes())?;
        }
        Message::FileBegin { context, size } => {
            out.file(*context)?;
            out.bytes(&size.to_be_bytes())?;
        }
        Message::FileReady {
            context,
            next_offset,
        } => {
            out.file(*context)?;
            out.bytes(&next_offset.to_be_bytes())?;
        }
        Message::DataChunk {
            context,
            offset,
            bytes,
        } => {
            out.file(*context)?;
            out.bytes(&offset.to_be_bytes())?;
            out.bytes(bytes)?;
        }
        Message::FileEnd(file) | Message::FileVerified(file) => out.digest(*file)?,
        Message::FileCommitted {
            file,
            receipt_id,
            actual_name,
        } => {
            out.digest(*file)?;
            out.id(*receipt_id)?;
            out.text(actual_name, MAX_NAME_BYTES as usize)?;
        }
        Message::TransferComplete(s) | Message::CompleteAck(s) => {
            out.transfer(s.context)?;
            out.bytes(&s.manifest_digest)?;
            out.bytes(&s.committed_count.to_be_bytes())?;
            out.bytes(&s.total_bytes.to_be_bytes())?;
        }
        Message::TransferStatus {
            context,
            file_id,
            phase,
            phase_bytes,
        } => {
            out.transfer(*context)?;
            out.bytes(file_id.as_ref().map_or(&[0; 16], Id::as_bytes))?;
            out.bytes(&[*phase as u8])?;
            out.bytes(&phase_bytes.to_be_bytes())?;
        }
        Message::Cancel { context, reason } => {
            out.transfer(*context)?;
            out.bytes(&(*reason as u16).to_be_bytes())?;
        }
        Message::Error {
            pair_id,
            transfer_id,
            code,
            scope,
        } => {
            out.bytes(pair_id.as_ref().map_or(&[0; 16], Id::as_bytes))?;
            out.bytes(transfer_id.as_ref().map_or(&[0; 16], Id::as_bytes))?;
            out.bytes(&(*code as u16).to_be_bytes())?;
            out.bytes(&[*scope as u8])?;
        }
    }
    let len = u32::try_from(out.0.len() - 4).map_err(|_| BodyError::LimitExceeded)?;
    out.0[..4].copy_from_slice(&len.to_be_bytes());
    out.0[4..6].copy_from_slice(&(message.kind() as u16).to_be_bytes());
    let prefix = out.0[..HEADER_BYTES]
        .try_into()
        .map_err(|_| BodyError::MalformedFrame)?;
    let header = FrameHeader::decode(prefix).map_err(|_| BodyError::LimitExceeded)?;
    Message::decode(header, &out.0[HEADER_BYTES..])?;
    Ok(EncodedFrame(out.0))
}

/// Preserves file order and original strings; the decoder applies the full manifest policy.
pub fn encode_manifest(files: &[ManifestEntry<'_>]) -> Result<Vec<u8>, BodyError> {
    if files.is_empty() || files.len() > super::limits::MAX_FILES as usize {
        return Err(BodyError::LimitExceeded);
    }
    let total = files
        .iter()
        .try_fold(0u64, |total, f| total.checked_add(f.size))
        .ok_or(BodyError::LimitExceeded)?;
    let mut out = Encoder::new(MAX_MANIFEST_BYTES as usize)?;
    out.bytes(&1u16.to_be_bytes())?;
    out.bytes(&(files.len() as u16).to_be_bytes())?;
    out.bytes(&total.to_be_bytes())?;
    for f in files {
        out.id(f.file_id)?;
        out.bytes(&f.size.to_be_bytes())?;
        out.text(f.name, MAX_NAME_BYTES as usize)?;
        out.text(f.mime, MAX_MIME_BYTES as usize)?;
    }
    Manifest::decode(&out.0)?;
    Ok(std::mem::take(&mut *out.0))
}

struct Encoder(Zeroizing<Vec<u8>>, usize);
impl Encoder {
    fn new(limit: usize) -> Result<Self, BodyError> {
        let mut bytes = Zeroizing::new(Vec::new());
        // Reserve before copying any secret, so growth never leaves old token copies.
        bytes
            .try_reserve_exact(limit)
            .map_err(|_| BodyError::AllocationFailed)?;
        Ok(Self(bytes, limit))
    }
    fn bytes(&mut self, bytes: &[u8]) -> Result<(), BodyError> {
        let end = self
            .0
            .len()
            .checked_add(bytes.len())
            .ok_or(BodyError::LimitExceeded)?;
        if end > self.1 {
            return Err(BodyError::LimitExceeded);
        }
        self.0.extend_from_slice(bytes);
        Ok(())
    }
    fn id(&mut self, id: Id) -> Result<(), BodyError> {
        self.bytes(id.as_bytes())
    }
    fn transfer(&mut self, c: TransferContext) -> Result<(), BodyError> {
        self.id(c.pair_id)?;
        self.id(c.transfer_id)
    }
    fn file(&mut self, c: FileContext) -> Result<(), BodyError> {
        self.transfer(c.transfer)?;
        self.id(c.file_id)
    }
    fn digest(&mut self, d: FileDigest) -> Result<(), BodyError> {
        self.file(d.context)?;
        self.bytes(&d.size.to_be_bytes())?;
        self.bytes(&d.digest)
    }
    fn text(&mut self, text: &str, limit: usize) -> Result<(), BodyError> {
        if text.is_empty() || text.len() > limit {
            return Err(BodyError::LimitExceeded);
        }
        self.bytes(&(text.len() as u16).to_be_bytes())?;
        self.bytes(text.as_bytes())
    }
}
