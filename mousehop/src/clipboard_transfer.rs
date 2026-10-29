//! Isolated clipboard receive foundation; no transport, OS clipboard or consent side effects.
//!
//! One receiver belongs to one authenticated peer and holds at most one transfer.
//! Callers must supply authoritative context on every operation. Session and authorization
//! epochs must change on *every* replacement/revocation (even if later restored), and the
//! local generation must change on local clipboard edits. This module cannot discover
//! lifecycle changes that callers hide by reusing an old epoch. Completion only returns
//! bytes; a future clipboard writer must serialize its final context check with the write.

use thiserror::Error;

// Share codec limits only; this state machine never parses or dispatches wire frames.
use mousehop_proto::clipboard_transfer::{Format, MAX_CHUNK_SIZE, MAX_PNG_SIZE, Message};

pub const MAX_TRANSFER_BYTES: usize = MAX_PNG_SIZE;
pub const MAX_CHUNK_BYTES: usize = MAX_CHUNK_SIZE;

/// A snapshot supplied by the caller, never inferred from input-control permissions.
/// Default denies receipt and represents no live authenticated session.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ReceiveContext {
    pub session_epoch: u64,
    pub session_valid: bool,
    pub authorization_epoch: u64,
    pub receive_authorized: bool,
    pub local_generation: u64,
}

#[derive(Clone, Copy, Debug, Error, PartialEq, Eq)]
pub enum TransferError {
    #[error("no live authenticated session")]
    SessionInvalid,
    #[error("clipboard receipt is not explicitly authorized")]
    Unauthorized,
    #[error("authenticated session changed")]
    SessionChanged,
    #[error("clipboard authorization changed")]
    AuthorizationChanged,
    #[error("local clipboard generation changed")]
    LocalGenerationChanged,
    #[error("a transfer is already active")]
    Busy,
    #[error("no transfer is active")]
    NoTransfer,
    #[error("transfer identifier does not match")]
    WrongTransfer,
    #[error("transfer identifier must be nonzero")]
    InvalidId,
    #[error("transfer exceeds the 16 MiB limit")]
    TooLarge,
    #[error("clipboard offer size is outside the format limit")]
    InvalidOfferSize,
    #[error("chunk must contain between 1 and 65536 bytes")]
    InvalidChunkSize,
    #[error("chunk duplicates or overlaps already received bytes")]
    DuplicateOrOverlap,
    #[error("chunk is out of order")]
    OutOfOrder,
    #[error("chunk exceeds declared transfer bounds")]
    OutOfBounds,
    #[error("transfer is incomplete")]
    Incomplete,
    #[error("cannot allocate receive buffer")]
    AllocationFailed,
}

/// A bounded sender which produces one protocol frame per call. The caller must
/// not advance it until the preceding frame was accepted by the transport.
pub struct ClipboardSender {
    id: u64,
    format: Format,
    chunk_size: usize,
    bytes: Vec<u8>,
    offset: usize,
    phase: SendPhase,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum SendPhase {
    Offer,
    Chunks,
    Complete,
    Done,
}

impl ClipboardSender {
    pub fn new(id: u64, format: Format, bytes: Vec<u8>) -> Result<Self, TransferError> {
        if id == 0 {
            return Err(TransferError::InvalidId);
        }
        if bytes.is_empty() || bytes.len() > format.max_size() {
            return Err(TransferError::InvalidOfferSize);
        }
        Ok(Self {
            id,
            format,
            chunk_size: mousehop_proto::clipboard_transfer::SESSION_CHUNK_SIZE,
            bytes,
            offset: 0,
            phase: SendPhase::Offer,
        })
    }

    /// Repeating this method without `advance` returns the same frame, allowing
    /// transport backpressure to be retried without skipping bytes.
    pub fn frame(&self) -> Option<Message> {
        match self.phase {
            SendPhase::Offer => Some(Message::Offer {
                id: self.id,
                format: self.format,
                size: self.bytes.len() as u32,
            }),
            SendPhase::Chunks => {
                let end = (self.offset + self.chunk_size).min(self.bytes.len());
                Some(Message::Chunk {
                    id: self.id,
                    offset: self.offset as u32,
                    payload: self.bytes[self.offset..end].to_vec(),
                })
            }
            SendPhase::Complete => Some(Message::Complete { id: self.id }),
            SendPhase::Done => None,
        }
    }

    pub(crate) fn is_unstarted(&self) -> bool {
        self.phase == SendPhase::Offer
    }

    pub fn advance(&mut self) {
        self.phase = match self.phase {
            SendPhase::Offer => SendPhase::Chunks,
            SendPhase::Chunks => {
                self.offset = (self.offset + self.chunk_size).min(self.bytes.len());
                if self.offset == self.bytes.len() {
                    SendPhase::Complete
                } else {
                    SendPhase::Chunks
                }
            }
            SendPhase::Complete | SendPhase::Done => SendPhase::Done,
        };
    }
}

#[derive(Debug)]
struct Pending {
    id: u64,
    context: ReceiveContext,
    total: usize,
    bytes: Vec<u8>,
}

#[derive(Debug, Default)]
pub struct ClipboardReceiver {
    pending: Option<Pending>,
}

impl ClipboardReceiver {
    /// Empty payloads are allowed and complete without any chunks. Allocation is
    /// incremental: an untrusted length announcement never reserves 16 MiB up front.
    pub fn begin(
        &mut self,
        id: u64,
        total: u64,
        context: ReceiveContext,
    ) -> Result<(), TransferError> {
        self.validate(context)?;
        if id == 0 {
            return Err(TransferError::InvalidId);
        }
        if self.pending.is_some() {
            return Err(TransferError::Busy);
        }
        if total > MAX_TRANSFER_BYTES as u64 {
            return Err(TransferError::TooLarge);
        }
        self.pending = Some(Pending {
            id,
            context,
            total: total as usize,
            bytes: Vec::new(),
        });
        Ok(())
    }

    /// Begin a wire offer, enforcing its format-specific size limit before buffering.
    /// Unlike the lower-level receiver, wire offers cannot be empty.
    pub fn begin_offer(
        &mut self,
        id: u64,
        format: Format,
        total: u64,
        context: ReceiveContext,
    ) -> Result<(), TransferError> {
        self.validate(context)?;
        if total == 0 || total > format.max_size() as u64 {
            return Err(TransferError::InvalidOfferSize);
        }
        self.begin(id, total, context)
    }

    /// Chunks must be contiguous and nonempty. A malformed chunk for the active
    /// transfer aborts it and releases its buffer; an unrelated ID cannot cancel it.
    pub fn chunk(
        &mut self,
        id: u64,
        offset: u64,
        bytes: &[u8],
        context: ReceiveContext,
    ) -> Result<(), TransferError> {
        self.validate(context)?;
        let pending = self.matching(id)?;
        let result = (|| {
            if bytes.is_empty() || bytes.len() > MAX_CHUNK_BYTES {
                return Err(TransferError::InvalidChunkSize);
            }
            let end = offset
                .checked_add(bytes.len() as u64)
                .ok_or(TransferError::OutOfBounds)?;
            if end > pending.total as u64 {
                return Err(TransferError::OutOfBounds);
            }
            if offset < pending.bytes.len() as u64 {
                return Err(TransferError::DuplicateOrOverlap);
            }
            if offset > pending.bytes.len() as u64 {
                return Err(TransferError::OutOfOrder);
            }
            // Geometric growth keeps tiny chunks amortized-linear, capped at
            // the declared size rather than Vec's potentially larger growth.
            let needed = end as usize;
            if needed > pending.bytes.capacity() {
                let capacity = pending
                    .bytes
                    .capacity()
                    .saturating_mul(2)
                    .max(needed)
                    .min(pending.total);
                pending
                    .bytes
                    .try_reserve_exact(capacity - pending.bytes.len())
                    .map_err(|_| TransferError::AllocationFailed)?;
            }
            pending.bytes.extend_from_slice(bytes);
            Ok(())
        })();
        if result.is_err() {
            self.pending = None;
        }
        result
    }

    /// Rechecks all lifecycle fences even after the last chunk. Incomplete finish
    /// is retryable; a successful finish consumes the transfer exactly once.
    pub fn finish(&mut self, id: u64, context: ReceiveContext) -> Result<Vec<u8>, TransferError> {
        self.validate(context)?;
        let pending = self.matching(id)?;
        if pending.bytes.len() != pending.total {
            return Err(TransferError::Incomplete);
        }
        Ok(self.pending.take().expect("matching transfer exists").bytes)
    }

    /// A cancellation is scoped to the active ID; repeated cancellation is an error.
    pub fn cancel(&mut self, id: u64) -> Result<(), TransferError> {
        self.matching(id)?;
        self.pending = None;
        Ok(())
    }

    /// Apply lifecycle notifications even when no chunks arrive. Any invalidation
    /// permanently discards the pending bytes, so restoring consent cannot revive them.
    pub fn update_context(&mut self, context: ReceiveContext) -> Result<(), TransferError> {
        self.validate(context)
    }

    pub fn buffered_bytes(&self) -> usize {
        self.pending.as_ref().map_or(0, |p| p.bytes.len())
    }

    fn matching(&mut self, id: u64) -> Result<&mut Pending, TransferError> {
        let pending = self.pending.as_mut().ok_or(TransferError::NoTransfer)?;
        if pending.id != id {
            return Err(TransferError::WrongTransfer);
        }
        Ok(pending)
    }

    fn validate(&mut self, context: ReceiveContext) -> Result<(), TransferError> {
        let error = if !context.session_valid {
            Some(TransferError::SessionInvalid)
        } else if !context.receive_authorized {
            Some(TransferError::Unauthorized)
        } else if let Some(pending) = &self.pending {
            if context.session_epoch != pending.context.session_epoch {
                Some(TransferError::SessionChanged)
            } else if context.authorization_epoch != pending.context.authorization_epoch {
                Some(TransferError::AuthorizationChanged)
            } else if context.local_generation != pending.context.local_generation {
                Some(TransferError::LocalGenerationChanged)
            } else {
                None
            }
        } else {
            None
        };
        if let Some(error) = error {
            self.pending = None;
            return Err(error);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn context() -> ReceiveContext {
        ReceiveContext {
            session_epoch: 7,
            session_valid: true,
            authorization_epoch: 11,
            receive_authorized: true,
            local_generation: 19,
        }
    }

    fn started(total: u64) -> ClipboardReceiver {
        let mut receiver = ClipboardReceiver::default();
        receiver.begin(42, total, context()).unwrap();
        receiver
    }

    #[test]
    fn reassembles_and_finishes_exactly_once() {
        let mut receiver = started(5);
        receiver.chunk(42, 0, b"ab", context()).unwrap();
        assert_eq!(receiver.buffered_bytes(), 2);
        receiver.chunk(42, 2, b"cde", context()).unwrap();
        assert_eq!(receiver.finish(42, context()).unwrap(), b"abcde");
        assert_eq!(receiver.buffered_bytes(), 0);
        assert_eq!(
            receiver.finish(42, context()),
            Err(TransferError::NoTransfer)
        );
    }

    #[test]
    fn zero_id_is_rejected_like_the_codec() {
        let mut receiver = ClipboardReceiver::default();
        assert_eq!(
            receiver.begin(0, 1, context()),
            Err(TransferError::InvalidId)
        );
        assert!(receiver.pending.is_none());
    }

    #[test]
    fn empty_clipboard_needs_no_chunks() {
        assert!(started(0).finish(42, context()).unwrap().is_empty());
    }

    #[test]
    fn incomplete_finish_is_retryable() {
        let mut receiver = started(2);
        receiver.chunk(42, 0, b"a", context()).unwrap();
        assert_eq!(
            receiver.finish(42, context()),
            Err(TransferError::Incomplete)
        );
        receiver.chunk(42, 1, b"b", context()).unwrap();
        assert_eq!(receiver.finish(42, context()).unwrap(), b"ab");
    }

    #[test]
    fn no_implicit_authorization() {
        let mut receiver = ClipboardReceiver::default();
        assert_eq!(
            receiver.begin(1, 1, ReceiveContext::default()),
            Err(TransferError::SessionInvalid)
        );
        let denied = ReceiveContext {
            receive_authorized: false,
            ..context()
        };
        assert_eq!(
            receiver.begin(1, 1, denied),
            Err(TransferError::Unauthorized)
        );
        assert_eq!(receiver.buffered_bytes(), 0);
    }

    #[test]
    fn announcement_is_bounded_without_eager_allocation() {
        let mut receiver = ClipboardReceiver::default();
        for total in [MAX_TRANSFER_BYTES as u64 + 1, u64::MAX] {
            assert_eq!(
                receiver.begin(42, total, context()),
                Err(TransferError::TooLarge)
            );
        }
        receiver
            .begin(42, MAX_TRANSFER_BYTES as u64, context())
            .unwrap();
        assert_eq!(receiver.pending.as_ref().unwrap().bytes.capacity(), 0);
    }

    #[test]
    fn sender_retries_and_reassembles_multiple_chunks() {
        let bytes = vec![7; MAX_CHUNK_BYTES + 13];
        let mut sender = ClipboardSender::new(42, Format::Text, bytes.clone()).unwrap();
        let mut receiver = ClipboardReceiver::default();
        while let Some(frame) = sender.frame() {
            assert_eq!(sender.frame(), Some(frame.clone()));
            match frame {
                Message::Offer { id, format, size } => receiver
                    .begin_offer(id, format, size.into(), context())
                    .unwrap(),
                Message::Chunk {
                    id,
                    offset,
                    payload,
                } => receiver
                    .chunk(id, offset.into(), &payload, context())
                    .unwrap(),
                Message::Complete { id } => {
                    assert_eq!(receiver.finish(id, context()).unwrap(), bytes)
                }
                _ => unreachable!(),
            }
            sender.advance();
        }
        sender.advance();
        assert_eq!(sender.frame(), None);
        assert!(matches!(
            ClipboardSender::new(0, Format::Text, vec![1]),
            Err(TransferError::InvalidId)
        ));
        assert!(matches!(
            ClipboardSender::new(1, Format::Text, vec![]),
            Err(TransferError::InvalidOfferSize)
        ));
        assert!(matches!(
            ClipboardSender::new(1, Format::Text, vec![0; Format::Text.max_size() + 1]),
            Err(TransferError::InvalidOfferSize)
        ));
    }

    #[test]
    fn wire_offers_enforce_format_limits_and_lifecycle() {
        let mut receiver = ClipboardReceiver::default();
        for format in [Format::Text, Format::Png] {
            for size in [0, format.max_size() as u64 + 1] {
                assert_eq!(
                    receiver.begin_offer(42, format, size, context()),
                    Err(TransferError::InvalidOfferSize)
                );
                assert!(receiver.pending.is_none());
            }
            receiver
                .begin_offer(42, format, format.max_size() as u64, context())
                .unwrap();
            receiver.cancel(42).unwrap();
        }
        receiver
            .begin_offer(42, Format::Text, 1, context())
            .unwrap();
        let changed = ReceiveContext {
            session_epoch: 8,
            ..context()
        };
        assert_eq!(
            receiver.begin_offer(43, Format::Text, 0, changed),
            Err(TransferError::SessionChanged)
        );
        assert!(receiver.pending.is_none());
    }

    #[test]
    fn exact_sixteen_mib_limit() {
        let mut receiver = started(MAX_TRANSFER_BYTES as u64);
        let chunk = vec![0xa5; MAX_CHUNK_BYTES];
        for offset in (0..MAX_TRANSFER_BYTES).step_by(MAX_CHUNK_BYTES) {
            receiver
                .chunk(42, offset as u64, &chunk, context())
                .unwrap();
            assert!(receiver.buffered_bytes() <= MAX_TRANSFER_BYTES);
        }
        let bytes = receiver.finish(42, context()).unwrap();
        assert_eq!(bytes.len(), MAX_TRANSFER_BYTES);
        assert!(bytes.iter().all(|byte| *byte == 0xa5));
    }

    #[test]
    fn malformed_chunks_abort_and_release_buffer() {
        let cases = [
            (0, vec![1], TransferError::DuplicateOrOverlap),
            (1, vec![1], TransferError::DuplicateOrOverlap),
            (3, vec![1], TransferError::OutOfOrder),
            (4, vec![1], TransferError::OutOfBounds),
            (u64::MAX, vec![1], TransferError::OutOfBounds),
            (2, vec![], TransferError::InvalidChunkSize),
            (
                2,
                vec![1; MAX_CHUNK_BYTES + 1],
                TransferError::InvalidChunkSize,
            ),
        ];
        for (offset, bytes, error) in cases {
            let mut receiver = started(4);
            receiver.chunk(42, 0, b"ab", context()).unwrap();
            assert_eq!(receiver.chunk(42, offset, &bytes, context()), Err(error));
            assert_eq!(receiver.buffered_bytes(), 0);
            assert_eq!(
                receiver.finish(42, context()),
                Err(TransferError::NoTransfer)
            );
        }
    }

    #[test]
    fn extra_chunk_after_full_payload_aborts() {
        let mut receiver = started(1);
        receiver.chunk(42, 0, b"a", context()).unwrap();
        assert_eq!(
            receiver.chunk(42, 1, b"b", context()),
            Err(TransferError::OutOfBounds)
        );
        assert_eq!(
            receiver.finish(42, context()),
            Err(TransferError::NoTransfer)
        );
    }

    #[test]
    fn busy_and_unrelated_ids_do_not_replace_or_cancel() {
        let mut receiver = started(2);
        assert_eq!(receiver.begin(43, 1, context()), Err(TransferError::Busy));
        assert_eq!(
            receiver.chunk(43, 0, b"x", context()),
            Err(TransferError::WrongTransfer)
        );
        assert_eq!(
            receiver.finish(43, context()),
            Err(TransferError::WrongTransfer)
        );
        assert_eq!(receiver.cancel(43), Err(TransferError::WrongTransfer));
        receiver.chunk(42, 0, b"ab", context()).unwrap();
        assert_eq!(receiver.finish(42, context()).unwrap(), b"ab");
    }

    #[test]
    fn cancellation_releases_bytes_and_allows_new_transfer() {
        let mut receiver = started(2);
        receiver.chunk(42, 0, b"a", context()).unwrap();
        receiver.cancel(42).unwrap();
        assert_eq!(receiver.buffered_bytes(), 0);
        assert_eq!(receiver.cancel(42), Err(TransferError::NoTransfer));
        assert_eq!(
            receiver.chunk(42, 1, b"b", context()),
            Err(TransferError::NoTransfer)
        );
        assert_eq!(
            receiver.finish(42, context()),
            Err(TransferError::NoTransfer)
        );
        receiver.begin(43, 0, context()).unwrap();
        assert!(receiver.finish(43, context()).unwrap().is_empty());
    }

    fn invalid_contexts() -> Vec<(ReceiveContext, TransferError)> {
        vec![
            (
                ReceiveContext {
                    session_valid: false,
                    ..context()
                },
                TransferError::SessionInvalid,
            ),
            (
                ReceiveContext {
                    receive_authorized: false,
                    ..context()
                },
                TransferError::Unauthorized,
            ),
            (
                ReceiveContext {
                    session_epoch: 8,
                    ..context()
                },
                TransferError::SessionChanged,
            ),
            (
                ReceiveContext {
                    authorization_epoch: 12,
                    ..context()
                },
                TransferError::AuthorizationChanged,
            ),
            (
                ReceiveContext {
                    local_generation: 20,
                    ..context()
                },
                TransferError::LocalGenerationChanged,
            ),
        ]
    }

    #[test]
    fn finish_rechecks_all_fences_after_last_chunk() {
        for (changed, error) in invalid_contexts() {
            let mut receiver = started(1);
            receiver.chunk(42, 0, b"a", context()).unwrap();
            assert_eq!(receiver.finish(42, changed), Err(error));
            assert_eq!(receiver.buffered_bytes(), 0);
            assert_eq!(
                receiver.finish(42, context()),
                Err(TransferError::NoTransfer)
            );
        }
    }

    #[test]
    fn chunks_and_notifications_recheck_all_fences() {
        for (changed, error) in invalid_contexts() {
            let mut receiver = started(2);
            receiver.chunk(42, 0, b"a", context()).unwrap();
            assert_eq!(receiver.chunk(42, 1, b"b", changed), Err(error));
            assert_eq!(
                receiver.finish(42, context()),
                Err(TransferError::NoTransfer)
            );

            let mut receiver = started(1);
            receiver.chunk(42, 0, b"a", context()).unwrap();
            assert_eq!(receiver.update_context(changed), Err(error));
            assert_eq!(
                receiver.finish(42, context()),
                Err(TransferError::NoTransfer)
            );
        }
    }

    #[test]
    fn changed_context_cannot_replace_pending_implicitly() {
        let mut receiver = started(1);
        let changed = ReceiveContext {
            session_epoch: 8,
            ..context()
        };
        assert_eq!(
            receiver.begin(43, 1, changed),
            Err(TransferError::SessionChanged)
        );
        assert_eq!(
            receiver.finish(42, context()),
            Err(TransferError::NoTransfer)
        );
        receiver.begin(43, 0, changed).unwrap();
        assert!(receiver.finish(43, changed).unwrap().is_empty());
    }
}
