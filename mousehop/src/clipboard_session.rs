//! Socket/OS-free, one-peer bidirectional clipboard session.
//!
//! The caller binds this instance to an authenticated identity; no payload supplies
//! identity or lifecycle epochs. Publish every revocation/replacement immediately,
//! using fresh session/authorization epochs (never reuse them after restoration).
//! Permission frames describe the peer's explicit format consent, not identity.
//! Both endpoints' consent is required in either direction. One transfer per
//! direction is allowed; IDs increase independently within each session.
//!
//! Output is NOT an OS-write receipt. Consumers must validate PNGs off the input
//! executor, then serialize `validate_completion` and any eventual OS write with
//! lifecycle/local-generation publication. External OS edits remain a caller concern.

use std::sync::Arc;

use mousehop_proto::clipboard_transfer::{Format, Message, SessionFrame};
use thiserror::Error;

use crate::clipboard_transfer::{
    ClipboardReceiver, ClipboardSender, ReceiveContext, TransferError,
};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Formats {
    pub text: bool,
    pub png: bool,
}

impl Formats {
    fn allows(self, format: Format) -> bool {
        match format {
            Format::Text => self.text,
            Format::Png => self.png,
        }
    }
}

/// Authoritative application state, never reconstructed from a wire message.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SessionContext {
    pub session_epoch: u64,
    pub session_valid: bool,
    pub authorization_epoch: u64,
    pub local_consent: Formats,
    pub local_generation: u64,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum SessionError {
    #[error(transparent)]
    Transfer(#[from] TransferError),
    #[error("both endpoints must explicitly allow this clipboard format")]
    PermissionDenied,
    #[error("transfer ID must increase within this direction and session")]
    ReusedId,
    #[error("completed text is not UTF-8")]
    InvalidUtf8,
    #[error("completion is stale")]
    StaleCompletion,
    #[error("permission revision exhausted; replace this session instance")]
    RevisionExhausted,
}

#[derive(Debug, PartialEq, Eq)]
pub enum Content {
    Text(String),
    /// Opaque, unvalidated PNG. Never pass directly to an OS writer.
    PngNeedsValidation(Vec<u8>),
}

/// Private fences prevent callers from manufacturing an accepted completion.
#[derive(Debug, PartialEq, Eq)]
pub struct Completion {
    pub content: Content,
    context: SessionContext,
    permission_revision: u64,
    owner: Arc<()>,
}

struct Sending {
    id: u64,
    sender: ClipboardSender,
}

#[derive(Default)]
pub struct ClipboardSession {
    context: SessionContext,
    peer_consent: Formats,
    permission_revision: u64,
    owner: Arc<()>,
    receiver: ClipboardReceiver,
    receiving: Option<(u64, Format)>,
    sending: Option<Sending>,
    last_received_id: u64,
    last_sent_id: u64,
    completed_send: u64,
}

impl ClipboardSession {
    /// Local consent is authoritative; incoming Permission can never enable it.
    /// Notifications discard buffers even if no subsequent network frame arrives.
    pub fn update_context(&mut self, context: SessionContext) -> Result<(), SessionError> {
        let session_changed = context.session_epoch != self.context.session_epoch
            || context.session_valid != self.context.session_valid;
        let authorization_changed = context.authorization_epoch != self.context.authorization_epoch;
        if context != self.context {
            self.discard();
        }
        if session_changed || authorization_changed || !context.session_valid {
            self.peer_consent = Formats::default();
        }
        // Do not reset replay fences on authorization changes or a mere validity
        // toggle: only a fresh authenticated session epoch scopes a fresh ID space.
        if context.session_epoch != self.context.session_epoch {
            self.last_received_id = 0;
            self.last_sent_id = 0;
        }
        if context != self.context {
            self.bump_revision()?;
        }
        self.context = context;
        Ok(())
    }

    /// Advertise only local consent, never the peer's claim. No transport effects.
    pub fn permission(&mut self, context: SessionContext) -> Result<Message, SessionError> {
        self.update_context(context)?;
        self.live()?;
        Ok(Message::Permission {
            text: context.local_consent.text,
            png: context.local_consent.png,
        })
    }

    pub fn start_send(
        &mut self,
        id: u64,
        format: Format,
        bytes: Vec<u8>,
        context: SessionContext,
    ) -> Result<(), SessionError> {
        self.update_context(context)?;
        self.allowed(format)?;
        if self.sending.is_some() {
            return Err(TransferError::Busy.into());
        }
        if id == 0 {
            return Err(TransferError::InvalidId.into());
        }
        if id <= self.last_sent_id {
            return Err(SessionError::ReusedId);
        }
        if format == Format::Text && std::str::from_utf8(&bytes).is_err() {
            return Err(SessionError::InvalidUtf8);
        }
        let sender = ClipboardSender::new(id, format, bytes)?;
        self.sending = Some(Sending { id, sender });
        self.last_sent_id = id;
        Ok(())
    }

    /// Retry without advancing on backpressure. Returned frames are snapshots:
    /// the caller must retire queued snapshots whenever context/permission changes.
    pub fn send_frame(&mut self, context: SessionContext) -> Result<Option<Message>, SessionError> {
        self.update_context(context)?;
        self.live()?;
        Ok(self.sending.as_ref().and_then(|s| s.sender.frame()))
    }

    /// Call exactly once after transport admission, with the current context.
    /// Never acknowledge a frame from a retired sender (including after ID reuse).
    pub fn advance_send(&mut self, id: u64, context: SessionContext) -> Result<(), SessionError> {
        self.update_context(context)?;
        self.live()?;
        let sending = self.sending.as_mut().ok_or(TransferError::NoTransfer)?;
        if sending.id != id {
            return Err(TransferError::WrongTransfer.into());
        }
        sending.sender.advance();
        if sending.sender.frame().is_none() {
            self.sending = None;
            self.completed_send = id;
        }
        Ok(())
    }

    /// Dispatch a decoded frame only from the authenticated session represented
    /// by context. No frame can change session identity or local consent.
    pub fn receive(
        &mut self,
        message: Message,
        context: SessionContext,
    ) -> Result<Option<Completion>, SessionError> {
        self.update_context(context)?;
        self.live()?;
        match message {
            Message::Permission { text, png } => {
                let consent = Formats { text, png };
                if consent != self.peer_consent {
                    self.discard();
                    self.bump_revision()?;
                    self.peer_consent = consent;
                }
            }
            Message::Offer { id, format, size } => {
                self.allowed(format)?;
                if id == 0 {
                    return Err(TransferError::InvalidId.into());
                }
                if self.receiving.is_some() {
                    return Err(TransferError::Busy.into());
                }
                if id <= self.last_received_id {
                    return Err(SessionError::ReusedId);
                }
                self.receiver
                    .begin_offer(id, format, size.into(), self.receive_context())?;
                self.receiving = Some((id, format));
                self.last_received_id = id;
            }
            Message::Chunk {
                id,
                offset,
                payload,
            } => {
                let result =
                    self.receiver
                        .chunk(id, offset.into(), &payload, self.receive_context());
                if let Err(error) = result {
                    if error != TransferError::WrongTransfer {
                        self.receiving = None;
                    }
                    return Err(error.into());
                }
            }
            Message::Complete { id } => {
                let bytes = self.receiver.finish(id, self.receive_context())?;
                let (_, format) = self
                    .receiving
                    .take()
                    .expect("receiver and format are paired");
                let content = match format {
                    Format::Text => Content::Text(
                        String::from_utf8(bytes).map_err(|_| SessionError::InvalidUtf8)?,
                    ),
                    Format::Png => Content::PngNeedsValidation(bytes),
                };
                return Ok(Some(Completion {
                    content,
                    context: self.context,
                    permission_revision: self.permission_revision,
                    owner: Arc::clone(&self.owner),
                }));
            }
            Message::Cancel { id, .. } => {
                // The wire has no direction field. Conservatively retire BOTH
                // matching directions on collision, never an unrelated transfer.
                let mut matched = false;
                if self.receiving.is_some_and(|(active, _)| active == id) {
                    self.receiver.cancel(id)?;
                    self.receiving = None;
                    matched = true;
                }
                if self.sending.as_ref().is_some_and(|s| s.id == id) {
                    self.sending = None;
                    matched = true;
                }
                if !matched {
                    return Err(TransferError::WrongTransfer.into());
                }
            }
        }
        Ok(None)
    }

    /// Recheck after asynchronous PNG validation, under the future writer's
    /// lifecycle lock. Validation alone does not authorize a later unlocked write.
    pub fn validate_completion(
        &mut self,
        completion: &Completion,
        context: SessionContext,
    ) -> Result<(), SessionError> {
        self.update_context(context)?;
        self.live()?;
        if !Arc::ptr_eq(&completion.owner, &self.owner)
            || completion.context != self.context
            || completion.permission_revision != self.permission_revision
        {
            return Err(SessionError::StaleCompletion);
        }
        Ok(())
    }

    fn live(&self) -> Result<(), SessionError> {
        if self.permission_revision == u64::MAX {
            return Err(SessionError::RevisionExhausted);
        }
        if !self.context.session_valid {
            return Err(TransferError::SessionInvalid.into());
        }
        Ok(())
    }

    fn allowed(&self, format: Format) -> Result<(), SessionError> {
        self.live()?;
        if !self.context.local_consent.allows(format) || !self.peer_consent.allows(format) {
            return Err(SessionError::PermissionDenied);
        }
        Ok(())
    }

    fn receive_context(&self) -> ReceiveContext {
        ReceiveContext {
            session_epoch: self.context.session_epoch,
            session_valid: self.context.session_valid,
            authorization_epoch: self.context.authorization_epoch,
            receive_authorized: true, // per-format bilateral permission checked at offer
            local_generation: self.context.local_generation,
        }
    }

    pub(crate) fn discard_receiver(&mut self) {
        self.receiver = ClipboardReceiver::default();
        self.receiving = None;
    }

    fn discard(&mut self) {
        self.discard_receiver();
        self.sending = None;
    }

    fn bump_revision(&mut self) -> Result<(), SessionError> {
        self.permission_revision = self.permission_revision.checked_add(1).ok_or_else(|| {
            self.peer_consent = Formats::default();
            SessionError::RevisionExhausted
        })?;
        Ok(())
    }
}

/// Small adapter seam keeps bridge tests independent of DTLS and the driver.
/// Production construction accepts only the transport-owned authenticated lane.
pub(crate) trait Lane {
    fn send(&self, bytes: &[u8]) -> Result<(), crate::transport::ClipboardError>;
    fn receive(&self) -> Result<Option<Vec<u8>>, crate::transport::ClipboardError>;
    fn close(&self);
    fn invalidated(&self) -> Option<Arc<std::sync::atomic::AtomicBool>> {
        None
    }
}

impl Lane for Arc<crate::transport::ClipboardLane> {
    fn invalidated(&self) -> Option<Arc<std::sync::atomic::AtomicBool>> {
        Some(crate::transport::ClipboardLane::invalidated(self))
    }
    fn send(&self, bytes: &[u8]) -> Result<(), crate::transport::ClipboardError> {
        self.try_send(bytes)
    }
    fn receive(&self) -> Result<Option<Vec<u8>>, crate::transport::ClipboardError> {
        self.try_receive()
    }
    fn close(&self) {
        crate::transport::ClipboardLane::close(self);
    }
}

#[derive(Debug, Error, PartialEq, Eq)]
pub(crate) enum BridgeError {
    #[error(transparent)]
    Session(#[from] SessionError),
    #[error(transparent)]
    Codec(#[from] mousehop_proto::clipboard_transfer::CodecError),
    #[error("clipboard lane failure: {0:?}")]
    Lane(crate::transport::ClipboardError),
    #[error("clipboard bridge retired; a fresh authenticated lane is required")]
    Retired,
    #[error("peer generation changed; retry interrupted transfer {id} with a fresh ID")]
    SendInterrupted { id: u64 },
}

/// Isolated lane consumer, never installed in Service. Operations are synchronous
/// and bounded (one admission/receipt per call), with no second socket reader.
/// Own exclusively: sharing this lane with another application consumer/sender
/// would bypass these fences. Authorization/consent/session changes terminally
/// retire the lane. Local edits instead increment a receive token and announce
/// an ordered source-generation barrier before any new data. Each data frame
/// echoes both endpoints' tokens; old frames are dropped, future frames fail
/// closed. Generation announcements may skip values (coalesced local edits),
/// duplicate/older announcements are harmless. Requires ordered lane delivery.
/// The caller must serialize all lifecycle publications and calls on this object.
/// Already-admitted data cannot be recalled; receiver tokens fence late packets.
pub(crate) struct LaneBridge<L: Lane> {
    lane: L,
    session: ClipboardSession,
    bound: SessionContext,
    permission_pending: bool,
    peer_permission: Option<Formats>,
    local_token: u64,
    peer_token: Option<u64>,
    generation_pending: bool,
    retired: bool,
    received_frame: bool,
}

pub(crate) struct ClipboardBridge(LaneBridge<Arc<crate::transport::ClipboardLane>>);

impl ClipboardBridge {
    pub(crate) fn new(
        lane: Arc<crate::transport::ClipboardLane>,
        context: SessionContext,
    ) -> Result<Self, BridgeError> {
        Ok(Self(LaneBridge::new(lane, context)?))
    }

    pub(crate) fn update_context(&mut self, context: SessionContext) -> Result<(), BridgeError> {
        self.0.check(context)
    }

    pub(crate) fn start_send(
        &mut self,
        id: u64,
        format: Format,
        bytes: Vec<u8>,
        context: SessionContext,
    ) -> Result<(), BridgeError> {
        self.0.start_send(id, format, bytes, context)
    }

    /// false means no frame or retryable backpressure; call again on driver
    /// progress, never spin. true means admission, not peer/OS acknowledgment.
    pub(crate) fn try_send(&mut self, context: SessionContext) -> Result<bool, BridgeError> {
        self.0.try_send(context)
    }

    pub(crate) fn try_receive(
        &mut self,
        context: SessionContext,
    ) -> Result<Option<Completion>, BridgeError> {
        self.0.try_receive(context)
    }

    pub(crate) fn validate_completion(
        &mut self,
        completion: &Completion,
        context: SessionContext,
    ) -> Result<(), BridgeError> {
        self.0.check(context)?;
        Ok(self.0.session.validate_completion(completion, context)?)
    }

    pub(crate) fn close(&mut self) {
        self.0.retire();
    }
}

impl<L: Lane> LaneBridge<L> {
    pub(crate) fn completed_send(&self) -> u64 {
        self.session.completed_send
    }

    pub(crate) fn is_sending(&self) -> bool {
        self.session.sending.is_some()
    }

    pub(crate) fn receiving_id(&self) -> Option<u64> {
        self.session.receiving.map(|(id, _)| id)
    }

    pub(crate) fn received_frame(&self) -> bool {
        self.received_frame
    }

    pub(crate) fn update_context(&mut self, context: SessionContext) -> Result<(), BridgeError> {
        self.check(context)
    }
    pub(crate) fn validate_completion(
        &mut self,
        completion: &Completion,
        context: SessionContext,
    ) -> Result<(), BridgeError> {
        self.check(context)?;
        Ok(self.session.validate_completion(completion, context)?)
    }
    pub(crate) fn close(&mut self) {
        self.retire();
    }
    pub(crate) fn new(lane: L, context: SessionContext) -> Result<Self, BridgeError> {
        let mut bridge = Self {
            lane,
            session: ClipboardSession::default(),
            bound: context,
            permission_pending: true,
            peer_permission: None,
            local_token: 1,
            peer_token: None,
            generation_pending: true,
            retired: false,
            received_frame: false,
        };
        if !context.session_valid {
            bridge.retire();
            return Err(BridgeError::Session(TransferError::SessionInvalid.into()));
        }
        bridge.session.update_context(context)?;
        Ok(bridge)
    }

    fn retire(&mut self) {
        self.retired = true;
        self.lane.close();
        self.session.discard();
        self.permission_pending = false;
    }

    fn check(&mut self, context: SessionContext) -> Result<(), BridgeError> {
        if self.retired {
            return Err(BridgeError::Retired);
        }
        if context.session_epoch != self.bound.session_epoch
            || context.authorization_epoch != self.bound.authorization_epoch
            || context.local_consent != self.bound.local_consent
            || !context.session_valid
        {
            self.retire();
            return Err(BridgeError::Retired);
        }
        if context.local_generation != self.bound.local_generation {
            let Some(token) = self.local_token.checked_add(1) else {
                self.retire();
                return Err(BridgeError::Retired);
            };
            self.session.update_context(context)?;
            self.local_token = token;
            self.bound = context;
            self.generation_pending = true;
            self.permission_pending = true;
        }
        Ok(())
    }

    pub(crate) fn start_send(
        &mut self,
        id: u64,
        format: Format,
        bytes: Vec<u8>,
        context: SessionContext,
    ) -> Result<(), BridgeError> {
        self.check(context)?;
        Ok(self.session.start_send(id, format, bytes, context)?)
    }

    pub(crate) fn try_send(&mut self, context: SessionContext) -> Result<bool, BridgeError> {
        self.check(context)?;
        let frame = if self.generation_pending {
            SessionFrame::Generation {
                token: self.local_token,
            }
        } else {
            let Some(token) = self.peer_token else {
                return Ok(false);
            };
            let message = if self.permission_pending {
                self.session.permission(context)?
            } else if let Some(frame) = self.session.send_frame(context)? {
                frame
            } else {
                return Ok(false);
            };
            SessionFrame::Data {
                token,
                source: self.local_token,
                message,
            }
        };
        let bytes = frame.encode()?;
        match self.lane.send(&bytes) {
            Ok(()) => {
                if self.generation_pending {
                    self.generation_pending = false;
                } else if self.permission_pending {
                    self.permission_pending = false;
                } else {
                    let id = self
                        .session
                        .sending
                        .as_ref()
                        .expect("frame has a sender")
                        .id;
                    self.session.advance_send(id, context)?;
                }
                Ok(true)
            }
            Err(
                crate::transport::ClipboardError::Backpressure
                | crate::transport::ClipboardError::NotNegotiated,
            ) => Ok(false),
            Err(error) => {
                self.retire();
                Err(BridgeError::Lane(error))
            }
        }
    }

    pub(crate) fn try_receive(
        &mut self,
        context: SessionContext,
    ) -> Result<Option<Completion>, BridgeError> {
        self.received_frame = false;
        self.check(context)?;
        let bytes = match self.lane.receive() {
            Ok(Some(bytes)) => bytes,
            Ok(None) | Err(crate::transport::ClipboardError::NotNegotiated) => return Ok(None),
            Err(error) => {
                self.retire();
                return Err(BridgeError::Lane(error));
            }
        };
        self.received_frame = true;
        let envelope = match SessionFrame::decode(&bytes) {
            Ok(message) => message,
            Err(error) => {
                self.retire();
                return Err(error.into());
            }
        };
        let message = match envelope {
            SessionFrame::Generation { token } => {
                if self.peer_token.is_some_and(|old| token < old) {
                    return Ok(None);
                }
                if self.peer_token != Some(token) {
                    // A peer generation barrier invalidates only transfers
                    // received from that peer. Local sends were admitted before
                    // the cross-barrier race and remain caller-owned retryable
                    // work; silently dropping them would lose clipboard data.
                    self.session.discard_receiver();
                    self.session.bump_revision()?;
                    self.peer_token = Some(token);
                    self.permission_pending = true;
                    if self
                        .session
                        .sending
                        .as_ref()
                        .is_some_and(|sending| !sending.sender.is_unstarted())
                    {
                        let id = self
                            .session
                            .sending
                            .take()
                            .expect("interrupted sender exists")
                            .id;
                        // Already-admitted old-target frames cannot be rebound.
                        // Report interruption, preserving the lane and requiring
                        // caller retry with a new ID rather than silent loss.
                        return Err(BridgeError::SendInterrupted { id });
                    }
                }
                return Ok(None);
            }
            SessionFrame::Data {
                token,
                source,
                message,
            } => {
                if token < self.local_token || self.peer_token.is_some_and(|old| source < old) {
                    return Ok(None);
                }
                if token != self.local_token || self.peer_token != Some(source) {
                    self.retire();
                    return Err(BridgeError::Retired);
                }
                message
            }
        };
        if let Message::Permission { text, png } = message {
            let permission = Formats { text, png };
            if self
                .peer_permission
                .is_some_and(|previous| previous != permission)
            {
                // No stale frames survive a revoke/restore race. The wire has
                // no permission epoch, so do not recycle this lane afterwards.
                self.retire();
                return Err(BridgeError::Retired);
            }
            self.peer_permission = Some(permission);
        }
        match self.session.receive(message, context) {
            Ok(completion) => Ok(completion),
            Err(error) => {
                // Fail closed on malformed ordering/data or unsolicited traffic;
                // do not keep an unbounded error/cancellation response queue.
                self.retire();
                Err(error.into())
            }
        }
    }
}

impl<L: Lane> Drop for LaneBridge<L> {
    fn drop(&mut self) {
        self.retire();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mousehop_proto::clipboard_transfer::{CancelReason, MAX_CHUNK_SIZE};

    fn context() -> SessionContext {
        SessionContext {
            session_epoch: 7,
            session_valid: true,
            authorization_epoch: 9,
            local_consent: Formats {
                text: true,
                png: true,
            },
            local_generation: 11,
        }
    }

    fn ready() -> ClipboardSession {
        let mut session = ClipboardSession::default();
        session
            .receive(
                Message::Permission {
                    text: true,
                    png: true,
                },
                context(),
            )
            .unwrap();
        session
    }

    fn offer(session: &mut ClipboardSession, id: u64, format: Format, size: u32) {
        session
            .receive(Message::Offer { id, format, size }, context())
            .unwrap();
    }

    fn complete(
        session: &mut ClipboardSession,
        id: u64,
        format: Format,
        bytes: &[u8],
    ) -> Result<Option<Completion>, SessionError> {
        offer(session, id, format, bytes.len() as u32);
        session.receive(
            Message::Chunk {
                id,
                offset: 0,
                payload: bytes.to_vec(),
            },
            context(),
        )?;
        session.receive(Message::Complete { id }, context())
    }

    #[derive(Default)]
    struct Pipe {
        queue: std::collections::VecDeque<Vec<u8>>,
        closed: bool,
        negotiating: bool,
    }

    #[derive(Clone)]
    struct TestLane {
        input: Arc<std::sync::Mutex<Pipe>>,
        output: Arc<std::sync::Mutex<Pipe>>,
    }

    impl Lane for TestLane {
        fn send(&self, bytes: &[u8]) -> Result<(), crate::transport::ClipboardError> {
            let mut output = self.output.lock().unwrap();
            if output.closed {
                return Err(crate::transport::ClipboardError::Closed);
            }
            if output.negotiating {
                return Err(crate::transport::ClipboardError::NotNegotiated);
            }
            if output.queue.len() == 2 {
                return Err(crate::transport::ClipboardError::Backpressure);
            }
            output.queue.push_back(bytes.to_vec());
            Ok(())
        }
        fn receive(&self) -> Result<Option<Vec<u8>>, crate::transport::ClipboardError> {
            let mut input = self.input.lock().unwrap();
            if input.closed {
                return Err(crate::transport::ClipboardError::Closed);
            }
            if input.negotiating {
                return Err(crate::transport::ClipboardError::NotNegotiated);
            }
            Ok(input.queue.pop_front())
        }
        fn close(&self) {
            for pipe in [&self.input, &self.output] {
                let mut pipe = pipe.lock().unwrap();
                pipe.closed = true;
                pipe.queue.clear();
            }
        }
    }

    fn bridges() -> (LaneBridge<TestLane>, LaneBridge<TestLane>) {
        let a = Arc::new(std::sync::Mutex::new(Pipe::default()));
        let b = Arc::new(std::sync::Mutex::new(Pipe::default()));
        let left = TestLane {
            input: a.clone(),
            output: b.clone(),
        };
        let right = TestLane {
            input: b,
            output: a,
        };
        (
            LaneBridge::new(left, context()).unwrap(),
            LaneBridge::new(right, context()).unwrap(),
        )
    }

    fn exchange_permissions(a: &mut LaneBridge<TestLane>, b: &mut LaneBridge<TestLane>) {
        for _ in 0..2 {
            assert!(a.try_send(context()).unwrap());
            assert!(b.try_send(context()).unwrap());
            assert!(a.try_receive(context()).unwrap().is_none());
            assert!(b.try_receive(context()).unwrap().is_none());
        }
    }

    #[test]
    fn bridge_bidirectional_codec_and_backpressure_never_skip_frames() {
        let (mut a, mut b) = bridges();
        assert_eq!(
            a.start_send(1, Format::Text, b"no".to_vec(), context()),
            Err(BridgeError::Session(SessionError::PermissionDenied))
        );
        exchange_permissions(&mut a, &mut b);
        let text = "双向".repeat(MAX_CHUNK_SIZE);
        a.start_send(1, Format::Text, text.as_bytes().to_vec(), context())
            .unwrap();
        b.start_send(1, Format::Png, vec![0xff; MAX_CHUNK_SIZE + 1], context())
            .unwrap();
        assert!(a.try_send(context()).unwrap());
        assert!(a.try_send(context()).unwrap());
        let retry = a.session.send_frame(context()).unwrap();
        assert!(!a.try_send(context()).unwrap());
        assert_eq!(a.session.send_frame(context()).unwrap(), retry);
        let mut results = Vec::new();
        for _ in 0..20 {
            a.try_send(context()).unwrap();
            b.try_send(context()).unwrap();
            if let Some(done) = a.try_receive(context()).unwrap() {
                results.push(done.content);
            }
            if let Some(done) = b.try_receive(context()).unwrap() {
                results.push(done.content);
            }
        }
        assert!(results.contains(&Content::Text(text)));
        assert!(results.contains(&Content::PngNeedsValidation(vec![0xff; MAX_CHUNK_SIZE + 1])));
    }

    #[test]
    fn bridge_waits_for_negotiation_without_losing_permission_or_data_frame() {
        let (mut a, mut b) = bridges();
        a.lane.output.lock().unwrap().negotiating = true;
        a.lane.input.lock().unwrap().negotiating = true;
        assert!(!a.try_send(context()).unwrap());
        assert!(a.try_receive(context()).unwrap().is_none());
        assert!(a.permission_pending && !a.retired);
        a.lane.output.lock().unwrap().negotiating = false;
        a.lane.input.lock().unwrap().negotiating = false;
        exchange_permissions(&mut a, &mut b);
        a.start_send(1, Format::Text, b"retry".to_vec(), context())
            .unwrap();
        let frame = a.session.send_frame(context()).unwrap();
        a.lane.output.lock().unwrap().negotiating = true;
        assert!(!a.try_send(context()).unwrap());
        assert_eq!(a.session.send_frame(context()).unwrap(), frame);
        assert!(!a.retired);
        a.lane.output.lock().unwrap().negotiating = false;
        assert!(a.try_send(context()).unwrap());
        assert!(matches!(
            SessionFrame::decode(&a.lane.output.lock().unwrap().queue[0]).unwrap(),
            SessionFrame::Data {
                message: Message::Offer { id: 1, .. },
                ..
            }
        ));
    }

    #[test]
    fn generation_change_drops_delayed_old_offer_and_allows_fresh_transfer() {
        let (mut a, mut b) = bridges();
        exchange_permissions(&mut a, &mut b);
        a.start_send(1, Format::Text, b"old".to_vec(), context())
            .unwrap();
        a.try_send(context()).unwrap();
        // Simulate transport having already removed an admitted Offer from its
        // application queue: close cannot recall this packet.
        let admitted = b.lane.input.lock().unwrap().queue.pop_front().unwrap();
        let changed = SessionContext {
            local_generation: 12,
            ..context()
        };
        b.check(changed).unwrap();
        b.lane.input.lock().unwrap().queue.push_back(admitted);
        assert!(b.try_receive(changed).unwrap().is_none());
        assert!(b.session.receiving.is_none());
        assert!(!b.retired);
        // Announce the new recipient generation before permitting new offers.
        for step in 0..4 {
            b.try_send(changed).unwrap();
            if step == 0 {
                assert_eq!(
                    a.try_receive(context()),
                    Err(BridgeError::SendInterrupted { id: 1 })
                );
            } else {
                a.try_receive(context()).unwrap();
            }
            a.try_send(context()).unwrap();
            b.try_receive(changed).unwrap();
        }
        a.start_send(2, Format::Text, b"fresh".to_vec(), context())
            .unwrap();
        let mut result = None;
        for _ in 0..5 {
            a.try_send(context()).unwrap();
            if let Some(done) = b.try_receive(changed).unwrap() {
                result = Some(done.content);
            }
        }
        assert_eq!(result, Some(Content::Text("fresh".into())));
    }

    #[test]
    fn repeated_bidirectional_generations_and_future_token_rejection() {
        let (mut a, mut b) = bridges();
        exchange_permissions(&mut a, &mut b);
        let mut ac = context();
        let mut bc = context();
        for id in 1..=3 {
            ac.local_generation += 1;
            bc.local_generation += 1;
            a.check(ac).unwrap();
            b.check(bc).unwrap();
            for _ in 0..4 {
                a.try_send(ac).unwrap();
                b.try_send(bc).unwrap();
                a.try_receive(ac).unwrap();
                b.try_receive(bc).unwrap();
            }
            a.start_send(id, Format::Text, b"left".to_vec(), ac)
                .unwrap();
            b.start_send(id, Format::Text, b"right".to_vec(), bc)
                .unwrap();
            let mut completed = 0;
            for _ in 0..4 {
                a.try_send(ac).unwrap();
                b.try_send(bc).unwrap();
                if let Some(done) = a.try_receive(ac).unwrap() {
                    a.session.validate_completion(&done, ac).unwrap();
                    completed += 1;
                }
                if let Some(done) = b.try_receive(bc).unwrap() {
                    b.session.validate_completion(&done, bc).unwrap();
                    completed += 1;
                }
            }
            assert_eq!(completed, 2);
            assert!(!a.retired && !b.retired);
        }
        b.lane
            .send(
                &SessionFrame::Data {
                    token: a.local_token + 1,
                    source: b.local_token,
                    message: Message::Complete { id: 10 },
                }
                .encode()
                .unwrap(),
            )
            .unwrap();
        assert_eq!(a.try_receive(ac), Err(BridgeError::Retired));
    }

    #[test]
    fn cross_generation_barriers_preserve_racing_local_sends_for_retry() {
        let (mut a, mut b) = bridges();
        exchange_permissions(&mut a, &mut b);
        let mut ac = context();
        let mut bc = context();
        ac.local_generation += 1;
        bc.local_generation += 1;
        a.check(ac).unwrap();
        b.check(bc).unwrap();
        a.start_send(1, Format::Text, b"a-racing".to_vec(), ac)
            .unwrap();
        b.start_send(1, Format::Text, b"b-racing".to_vec(), bc)
            .unwrap();
        // Each sender has admitted its local transfer before either barrier
        // arrives. Deliver barriers crosswise; neither sender may disappear.
        a.try_send(ac).unwrap();
        b.try_send(bc).unwrap();
        a.try_receive(ac).unwrap();
        b.try_receive(bc).unwrap();
        assert!(a.session.sending.is_some());
        assert!(b.session.sending.is_some());
        // Complete the new-generation handshakes and retry both transfers.
        let mut completions = Vec::new();
        for _ in 0..8 {
            a.try_send(ac).unwrap();
            b.try_send(bc).unwrap();
            if let Some(done) = a.try_receive(ac).unwrap() {
                completions.push(done.content);
            }
            if let Some(done) = b.try_receive(bc).unwrap() {
                completions.push(done.content);
            }
        }
        assert!(completions.contains(&Content::Text("a-racing".into())));
        assert!(completions.contains(&Content::Text("b-racing".into())));
    }

    #[test]
    fn bridge_context_changes_clear_queued_frames_and_never_restore_lane() {
        for changed in [
            SessionContext {
                session_valid: false,
                ..context()
            },
            SessionContext {
                session_epoch: 8,
                ..context()
            },
            SessionContext {
                authorization_epoch: 10,
                ..context()
            },
            SessionContext {
                local_consent: Formats::default(),
                ..context()
            },
        ] {
            let (mut a, mut b) = bridges();
            exchange_permissions(&mut a, &mut b);
            a.start_send(1, Format::Text, b"private".to_vec(), context())
                .unwrap();
            a.try_send(context()).unwrap();
            assert_eq!(a.check(changed), Err(BridgeError::Retired));
            assert!(a.lane.output.lock().unwrap().queue.is_empty());
            assert!(a.session.sending.is_none());
            assert_eq!(a.try_send(context()), Err(BridgeError::Retired));
            assert_eq!(
                b.try_receive(context()),
                Err(BridgeError::Lane(crate::transport::ClipboardError::Closed))
            );
        }
    }

    #[test]
    fn bridge_peer_revoke_decode_error_and_drop_retire_queues() {
        let (mut a, mut b) = bridges();
        exchange_permissions(&mut a, &mut b);
        a.start_send(1, Format::Text, b"private".to_vec(), context())
            .unwrap();
        a.try_send(context()).unwrap();
        b.lane
            .send(
                &SessionFrame::Data {
                    token: 1,
                    source: 1,
                    message: Message::Permission {
                        text: false,
                        png: false,
                    },
                }
                .encode()
                .unwrap(),
            )
            .unwrap();
        assert_eq!(a.try_receive(context()), Err(BridgeError::Retired));
        assert!(a.lane.output.lock().unwrap().queue.is_empty());

        let (mut a, b) = bridges();
        b.lane.send(b"malformed").unwrap();
        assert!(matches!(
            a.try_receive(context()),
            Err(BridgeError::Codec(_))
        ));
        assert!(a.retired);
        let (a, b) = bridges();
        let monitor = a.lane.clone();
        drop(a);
        assert!(monitor.input.lock().unwrap().closed);
        assert!(monitor.output.lock().unwrap().queue.is_empty());
        drop(b);
    }

    #[test]
    fn both_sides_default_deny_and_wire_cannot_grant_local_consent() {
        let mut session = ClipboardSession::default();
        assert_eq!(
            session.permission(SessionContext::default()),
            Err(TransferError::SessionInvalid.into())
        );
        assert_eq!(
            session.start_send(1, Format::Text, vec![1], context()),
            Err(SessionError::PermissionDenied)
        );
        let denied = SessionContext {
            local_consent: Formats::default(),
            ..context()
        };
        session
            .receive(
                Message::Permission {
                    text: true,
                    png: true,
                },
                denied,
            )
            .unwrap();
        assert_eq!(
            session.permission(denied).unwrap(),
            Message::Permission {
                text: false,
                png: false
            }
        );
        assert_eq!(
            session.start_send(1, Format::Text, vec![1], denied),
            Err(SessionError::PermissionDenied)
        );
        assert_eq!(
            session.receive(
                Message::Offer {
                    id: 1,
                    format: Format::Text,
                    size: 1
                },
                denied
            ),
            Err(SessionError::PermissionDenied)
        );
    }

    #[test]
    fn two_sessions_exchange_simultaneously_and_retry_frames() {
        let mut a = ready();
        let mut b = ready();
        let text = "双向🙂".repeat(MAX_CHUNK_SIZE / 3);
        a.start_send(1, Format::Text, text.as_bytes().to_vec(), context())
            .unwrap();
        b.start_send(1, Format::Png, vec![0xa5; MAX_CHUNK_SIZE + 1], context())
            .unwrap();
        let mut results = Vec::new();
        loop {
            let mut progress = false;
            for reverse in [false, true] {
                let (from, to) = if reverse {
                    (&mut b, &mut a)
                } else {
                    (&mut a, &mut b)
                };
                if let Some(frame) = from.send_frame(context()).unwrap() {
                    progress = true;
                    assert_eq!(from.send_frame(context()).unwrap(), Some(frame.clone()));
                    if let Some(done) = to.receive(frame, context()).unwrap() {
                        results.push(done.content);
                    }
                    from.advance_send(1, context()).unwrap();
                }
            }
            if !progress {
                break;
            }
        }
        assert!(results.contains(&Content::Text(text)));
        assert!(results.contains(&Content::PngNeedsValidation(vec![0xa5; MAX_CHUNK_SIZE + 1])));
    }

    #[test]
    fn every_context_fence_discards_both_directions_and_never_revives() {
        let base = context();
        for changed in [
            SessionContext {
                session_valid: false,
                ..base
            },
            SessionContext {
                session_epoch: 8,
                ..base
            },
            SessionContext {
                authorization_epoch: 10,
                ..base
            },
            SessionContext {
                local_generation: 12,
                ..base
            },
            SessionContext {
                local_consent: Formats::default(),
                ..base
            },
        ] {
            let mut s = ready();
            offer(&mut s, 1, Format::Text, 2);
            s.receive(
                Message::Chunk {
                    id: 1,
                    offset: 0,
                    payload: vec![1],
                },
                base,
            )
            .unwrap();
            s.start_send(1, Format::Text, vec![1], base).unwrap();
            s.update_context(changed).unwrap();
            assert!(s.sending.is_none());
            assert!(s.receiving.is_none());
            assert_eq!(s.receiver.buffered_bytes(), 0);
            s.update_context(base).unwrap();
            assert_eq!(
                s.receive(Message::Complete { id: 1 }, base),
                Err(TransferError::NoTransfer.into())
            );
        }
    }

    #[test]
    fn peer_revocation_discards_both_and_stales_completed_outputs_even_after_restore() {
        let mut s = ready();
        let done = complete(&mut s, 1, Format::Text, b"ok").unwrap().unwrap();
        s.validate_completion(&done, context()).unwrap();
        offer(&mut s, 2, Format::Png, 1);
        s.start_send(1, Format::Text, vec![1], context()).unwrap();
        for allowed in [false, true] {
            s.receive(
                Message::Permission {
                    text: allowed,
                    png: allowed,
                },
                context(),
            )
            .unwrap();
        }
        assert!(s.sending.is_none());
        assert!(s.receiving.is_none());
        assert_eq!(
            s.validate_completion(&done, context()),
            Err(SessionError::StaleCompletion)
        );
        assert_eq!(
            s.receive(
                Message::Offer {
                    id: 2,
                    format: Format::Text,
                    size: 1
                },
                context()
            ),
            Err(SessionError::ReusedId)
        );
        assert_eq!(
            s.start_send(1, Format::Text, vec![1], context()),
            Err(SessionError::ReusedId)
        );
    }

    #[test]
    fn format_permissions_and_size_limits_apply_to_both_directions() {
        let mut s = ready();
        s.receive(
            Message::Permission {
                text: true,
                png: false,
            },
            context(),
        )
        .unwrap();
        assert_eq!(
            s.start_send(1, Format::Png, vec![1], context()),
            Err(SessionError::PermissionDenied)
        );
        assert_eq!(
            s.receive(
                Message::Offer {
                    id: 1,
                    format: Format::Png,
                    size: 1
                },
                context()
            ),
            Err(SessionError::PermissionDenied)
        );
        s.receive(
            Message::Permission {
                text: true,
                png: true,
            },
            context(),
        )
        .unwrap();
        for format in [Format::Text, Format::Png] {
            for size in [0, format.max_size() as u32 + 1] {
                assert_eq!(
                    s.receive(
                        Message::Offer {
                            id: 1,
                            format,
                            size
                        },
                        context()
                    ),
                    Err(TransferError::InvalidOfferSize.into())
                );
                assert_eq!(
                    s.start_send(1, format, vec![0; size as usize], context()),
                    Err(TransferError::InvalidOfferSize.into())
                );
            }
        }
    }

    #[test]
    fn utf8_is_checked_only_after_full_reassembly_and_invalid_text_is_consumed() {
        let mut s = ready();
        offer(&mut s, 1, Format::Text, 3);
        for (offset, byte) in [0xe4, 0xb8, 0xad].into_iter().enumerate() {
            s.receive(
                Message::Chunk {
                    id: 1,
                    offset: offset as u32,
                    payload: vec![byte],
                },
                context(),
            )
            .unwrap();
        }
        assert_eq!(
            s.receive(Message::Complete { id: 1 }, context())
                .unwrap()
                .unwrap()
                .content,
            Content::Text("中".into())
        );
        assert_eq!(
            complete(&mut s, 2, Format::Text, &[0xff]),
            Err(SessionError::InvalidUtf8)
        );
        assert_eq!(
            s.receive(Message::Complete { id: 2 }, context()),
            Err(TransferError::NoTransfer.into())
        );
        assert_eq!(
            complete(&mut s, 3, Format::Png, &[0xff])
                .unwrap()
                .unwrap()
                .content,
            Content::PngNeedsValidation(vec![0xff])
        );
    }

    #[test]
    fn malformed_chunks_abort_but_unrelated_ids_and_incomplete_finish_do_not() {
        let mut s = ready();
        offer(&mut s, 1, Format::Text, 2);
        assert_eq!(
            s.receive(Message::Complete { id: 1 }, context()),
            Err(TransferError::Incomplete.into())
        );
        assert_eq!(
            s.receive(
                Message::Chunk {
                    id: 2,
                    offset: 0,
                    payload: vec![1]
                },
                context()
            ),
            Err(TransferError::WrongTransfer.into())
        );
        assert!(s.receiving.is_some());
        assert_eq!(
            s.receive(
                Message::Chunk {
                    id: 1,
                    offset: 1,
                    payload: vec![1]
                },
                context()
            ),
            Err(TransferError::OutOfOrder.into())
        );
        assert!(s.receiving.is_none());
        assert_eq!(s.receiver.buffered_bytes(), 0);
    }

    #[test]
    fn cancel_collision_retires_both_directions_not_unrelated_ids() {
        let mut s = ready();
        offer(&mut s, 1, Format::Text, 1);
        s.start_send(1, Format::Text, vec![1], context()).unwrap();
        assert_eq!(
            s.receive(
                Message::Cancel {
                    id: 2,
                    reason: CancelReason::Aborted
                },
                context()
            ),
            Err(TransferError::WrongTransfer.into())
        );
        assert!(s.sending.is_some() && s.receiving.is_some());
        s.receive(
            Message::Cancel {
                id: 1,
                reason: CancelReason::Aborted,
            },
            context(),
        )
        .unwrap();
        assert!(s.sending.is_none() && s.receiving.is_none());
    }

    #[test]
    fn permission_is_session_scoped_and_completion_checks_current_generation() {
        let mut s = ready();
        let done = complete(&mut s, 1, Format::Text, b"ok").unwrap().unwrap();
        let changed = SessionContext {
            local_generation: 12,
            ..context()
        };
        assert_eq!(
            s.validate_completion(&done, changed),
            Err(SessionError::StaleCompletion)
        );
        assert_eq!(
            s.validate_completion(&done, context()),
            Err(SessionError::StaleCompletion)
        );
        for changed in [
            SessionContext {
                session_epoch: 8,
                ..context()
            },
            SessionContext {
                authorization_epoch: 10,
                ..context()
            },
        ] {
            s.update_context(changed).unwrap();
            assert_eq!(
                s.start_send(2, Format::Text, vec![1], changed),
                Err(SessionError::PermissionDenied)
            );
        }
    }
}
