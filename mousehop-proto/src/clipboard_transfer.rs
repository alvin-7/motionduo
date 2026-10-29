//! Standalone clipboard-transfer wire codec; not connected to peer transport.
//!
//! All integers are little-endian. Frames start with `MHCB`, version, and kind
//! (Permission=0, Offer=1, Chunk=2, Complete=3, Cancel=4). Transfer IDs are
//! nonzero and scoped to an authenticated session by the eventual caller.
//! Permission is an explicit text/PNG bit mask, not inherited input consent.
//! This codec bounds individual messages only: consent, transfer ordering,
//! matching chunks to offers, cumulative buffering and PNG content validation
//! belong to a future transfer driver. PNG bytes are opaque, not decoded here.

use thiserror::Error;

pub const VERSION: u8 = 1;
pub const MAX_TEXT_SIZE: usize = 1024 * 1024;
pub const MAX_PNG_SIZE: usize = 16 * 1024 * 1024;
/// Codec-level payload limit, NOT the current KCP core's message budget.
/// The existing input KCP core accepts only about 1 KiB per message: callers
/// must use smaller chunks or add bounded framing/reassembly before sending
/// these frames there. This module does not implement that transport layer.
pub const MAX_CHUNK_SIZE: usize = 64 * 1024;
/// Header + transfer ID + offset + payload length + largest chunk.
pub const MAX_MESSAGE_SIZE: usize = 6 + 8 + 4 + 4 + MAX_CHUNK_SIZE;
const MAGIC: &[u8; 4] = b"MHCB";
const HEADER_SIZE: usize = 6;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum Format {
    Text = 0,
    Png = 1,
}

impl Format {
    pub fn max_size(self) -> usize {
        match self {
            Self::Text => MAX_TEXT_SIZE,
            Self::Png => MAX_PNG_SIZE,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum CancelReason {
    PermissionDenied = 0,
    Superseded = 1,
    InvalidData = 2,
    TooLarge = 3,
    Aborted = 4,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Message {
    Permission {
        text: bool,
        png: bool,
    },
    Offer {
        id: u64,
        format: Format,
        size: u32,
    },
    Chunk {
        id: u64,
        offset: u32,
        payload: Vec<u8>,
    },
    Complete {
        id: u64,
    },
    Cancel {
        id: u64,
        reason: CancelReason,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum CodecError {
    #[error("clipboard-transfer frame has invalid length")]
    InvalidLength,
    #[error("clipboard-transfer frame exceeds the message limit")]
    MessageTooLarge,
    #[error("invalid clipboard-transfer magic")]
    InvalidMagic,
    #[error("unsupported clipboard-transfer version: {0}")]
    UnsupportedVersion(u8),
    #[error("unknown clipboard-transfer message kind: {0}")]
    InvalidKind(u8),
    #[error("invalid clipboard-transfer permission mask: {0}")]
    InvalidPermission(u8),
    #[error("clipboard-transfer ID must be nonzero")]
    InvalidId,
    #[error("unknown clipboard-transfer format: {0}")]
    InvalidFormat(u8),
    #[error("clipboard offer size is outside the format limit")]
    InvalidOfferSize,
    #[error("clipboard chunk is empty or exceeds the chunk limit")]
    InvalidChunkSize,
    #[error("clipboard chunk range exceeds the transfer limit")]
    InvalidChunkRange,
    #[error("unknown clipboard-transfer cancellation reason: {0}")]
    InvalidCancelReason(u8),
}

impl Message {
    /// Validate before allocating or narrowing any payload length.
    pub fn encode(&self) -> Result<Vec<u8>, CodecError> {
        self.validate()?;
        let (kind, length) = match self {
            Self::Permission { .. } => (0, 7),
            Self::Offer { .. } => (1, 19),
            Self::Chunk { payload, .. } => (2, 22 + payload.len()),
            Self::Complete { .. } => (3, 14),
            Self::Cancel { .. } => (4, 15),
        };
        let mut bytes = Vec::with_capacity(length);
        bytes.extend_from_slice(MAGIC);
        bytes.extend_from_slice(&[VERSION, kind]);
        match self {
            Self::Permission { text, png } => bytes.push(u8::from(*text) | (u8::from(*png) << 1)),
            Self::Offer { id, format, size } => {
                bytes.extend_from_slice(&id.to_le_bytes());
                bytes.push(*format as u8);
                bytes.extend_from_slice(&size.to_le_bytes());
            }
            Self::Chunk {
                id,
                offset,
                payload,
            } => {
                bytes.extend_from_slice(&id.to_le_bytes());
                bytes.extend_from_slice(&offset.to_le_bytes());
                bytes.extend_from_slice(&(payload.len() as u32).to_le_bytes());
                bytes.extend_from_slice(payload);
            }
            Self::Complete { id } => bytes.extend_from_slice(&id.to_le_bytes()),
            Self::Cancel { id, reason } => {
                bytes.extend_from_slice(&id.to_le_bytes());
                bytes.push(*reason as u8);
            }
        }
        Ok(bytes)
    }

    /// Decode exactly one complete frame. Reject oversized input before copying.
    pub fn decode(bytes: &[u8]) -> Result<Self, CodecError> {
        if bytes.len() > MAX_MESSAGE_SIZE {
            return Err(CodecError::MessageTooLarge);
        }
        if bytes.len() < HEADER_SIZE {
            return Err(CodecError::InvalidLength);
        }
        if &bytes[..4] != MAGIC {
            return Err(CodecError::InvalidMagic);
        }
        if bytes[4] != VERSION {
            return Err(CodecError::UnsupportedVersion(bytes[4]));
        }
        let body = &bytes[HEADER_SIZE..];
        let message = match bytes[5] {
            0 => {
                exact_length(body, 1)?;
                if body[0] & !3 != 0 {
                    return Err(CodecError::InvalidPermission(body[0]));
                }
                Self::Permission {
                    text: body[0] & 1 != 0,
                    png: body[0] & 2 != 0,
                }
            }
            1 => {
                exact_length(body, 13)?;
                let format = match body[8] {
                    0 => Format::Text,
                    1 => Format::Png,
                    value => return Err(CodecError::InvalidFormat(value)),
                };
                Self::Offer {
                    id: read_id(body)?,
                    format,
                    size: read_u32(&body[9..13])?,
                }
            }
            2 => {
                if body.len() < 16 {
                    return Err(CodecError::InvalidLength);
                }
                let id = read_id(body)?;
                let offset = read_u32(&body[8..12])?;
                let length = read_u32(&body[12..16])?;
                // Check the untrusted length and range before allocating payload.
                validate_chunk(offset, length)?;
                exact_length(body, 16 + length as usize)?;
                Self::Chunk {
                    id,
                    offset,
                    payload: body[16..].to_vec(),
                }
            }
            3 => {
                exact_length(body, 8)?;
                Self::Complete { id: read_id(body)? }
            }
            4 => {
                exact_length(body, 9)?;
                let reason = match body[8] {
                    0 => CancelReason::PermissionDenied,
                    1 => CancelReason::Superseded,
                    2 => CancelReason::InvalidData,
                    3 => CancelReason::TooLarge,
                    4 => CancelReason::Aborted,
                    value => return Err(CodecError::InvalidCancelReason(value)),
                };
                Self::Cancel {
                    id: read_id(body)?,
                    reason,
                }
            }
            kind => return Err(CodecError::InvalidKind(kind)),
        };
        message.validate()?;
        Ok(message)
    }

    fn validate(&self) -> Result<(), CodecError> {
        let id = match self {
            Self::Permission { .. } => return Ok(()),
            Self::Offer { id, format, size } => {
                if *size == 0 || u64::from(*size) > format.max_size() as u64 {
                    return Err(CodecError::InvalidOfferSize);
                }
                id
            }
            Self::Chunk {
                id,
                offset,
                payload,
            } => {
                if payload.is_empty() || payload.len() > MAX_CHUNK_SIZE {
                    return Err(CodecError::InvalidChunkSize);
                }
                validate_chunk(*offset, payload.len() as u32)?;
                id
            }
            Self::Complete { id } | Self::Cancel { id, .. } => id,
        };
        if *id == 0 {
            Err(CodecError::InvalidId)
        } else {
            Ok(())
        }
    }
}

fn exact_length(bytes: &[u8], expected: usize) -> Result<(), CodecError> {
    if bytes.len() == expected {
        Ok(())
    } else {
        Err(CodecError::InvalidLength)
    }
}

fn read_id(bytes: &[u8]) -> Result<u64, CodecError> {
    let id = u64::from_le_bytes(
        bytes
            .get(..8)
            .ok_or(CodecError::InvalidLength)?
            .try_into()
            .map_err(|_| CodecError::InvalidLength)?,
    );
    if id == 0 {
        Err(CodecError::InvalidId)
    } else {
        Ok(id)
    }
}

fn read_u32(bytes: &[u8]) -> Result<u32, CodecError> {
    Ok(u32::from_le_bytes(
        bytes.try_into().map_err(|_| CodecError::InvalidLength)?,
    ))
}

fn validate_chunk(offset: u32, length: u32) -> Result<(), CodecError> {
    if length == 0 || length > MAX_CHUNK_SIZE as u32 {
        return Err(CodecError::InvalidChunkSize);
    }
    if offset
        .checked_add(length)
        .is_none_or(|end| end > MAX_PNG_SIZE as u32)
    {
        return Err(CodecError::InvalidChunkRange);
    }
    Ok(())
}

/// Session envelope version 2 is deliberately incompatible with bare MHCB v1.
/// Each endpoint announces its own nonzero monotonic receive-generation token.
/// Data echoes the recipient's token; tokens never identify or authorize a peer.
/// Transport must preserve message order. Old-token data is ignored, future-token
/// data is rejected; neither is rebound to the recipient's current generation.
/// A new authenticated lane starts both token spaces at 1. Never reuse a lane.
pub const SESSION_VERSION: u8 = 2;
pub const SESSION_OVERHEAD: usize = 22;
pub const SESSION_CHUNK_SIZE: usize = MAX_CHUNK_SIZE - SESSION_OVERHEAD;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SessionFrame {
    Generation {
        token: u64,
    },
    Data {
        token: u64,
        source: u64,
        message: Message,
    },
}

impl SessionFrame {
    pub fn encode(&self) -> Result<Vec<u8>, CodecError> {
        let (kind, token) = match self {
            Self::Generation { token } => (0, *token),
            Self::Data { token, .. } => (1, *token),
        };
        if token == 0 {
            return Err(CodecError::InvalidId);
        }
        let mut bytes = Vec::from(&b"MHCS"[..]);
        bytes.extend_from_slice(&[SESSION_VERSION, kind]);
        bytes.extend_from_slice(&token.to_le_bytes());
        if let Self::Data { source, .. } = self {
            if *source == 0 {
                return Err(CodecError::InvalidId);
            }
            bytes.extend_from_slice(&source.to_le_bytes());
        } else {
            bytes.extend_from_slice(&token.to_le_bytes());
        }
        if let Self::Data { message, .. } = self {
            bytes.extend_from_slice(&message.encode()?);
        }
        if bytes.len() > MAX_MESSAGE_SIZE {
            return Err(CodecError::MessageTooLarge);
        }
        Ok(bytes)
    }

    pub fn decode(bytes: &[u8]) -> Result<Self, CodecError> {
        if bytes.len() > MAX_MESSAGE_SIZE {
            return Err(CodecError::MessageTooLarge);
        }
        if bytes.len() < SESSION_OVERHEAD {
            return Err(CodecError::InvalidLength);
        }
        if &bytes[..4] != b"MHCS" {
            return Err(CodecError::InvalidMagic);
        }
        if bytes[4] != SESSION_VERSION {
            return Err(CodecError::UnsupportedVersion(bytes[4]));
        }
        let token = read_id(&bytes[6..14])?;
        match bytes[5] {
            0 => {
                exact_length(bytes, SESSION_OVERHEAD)?;
                if read_id(&bytes[14..22])? != token {
                    return Err(CodecError::InvalidId);
                }
                Ok(Self::Generation { token })
            }
            1 => Ok(Self::Data {
                token,
                source: read_id(&bytes[14..22])?,
                message: Message::decode(&bytes[22..])?,
            }),
            kind => Err(CodecError::InvalidKind(kind)),
        }
    }
}

#[cfg(test)]
mod tests;
