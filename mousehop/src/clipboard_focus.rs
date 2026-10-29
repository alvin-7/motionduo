//! Focus-sync state primitives. This module is not yet connected to Service.

use mousehop_proto::clipboard_transfer::Format;
use sha2::{Digest, Sha256};
use std::{
    collections::HashMap,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

type Hash = [u8; 32];

#[derive(Clone, Debug, PartialEq, Eq)]
struct Content {
    format: Format,
    hash: Hash,
    bytes: Arc<[u8]>,
}

impl Content {
    fn new(format: Format, bytes: Vec<u8>) -> Option<Self> {
        if bytes.is_empty() || bytes.len() > format.max_size() {
            return None;
        }
        let mut digest = Sha256::new();
        digest.update([format as u8]);
        digest.update(&bytes);
        Some(Self {
            format,
            hash: digest.finalize().into(),
            bytes: bytes.into(),
        })
    }
}

#[derive(Default)]
struct Peer {
    known: Option<Hash>,
    outgoing: Option<Content>,
    incoming: Option<(u64, Format, Vec<u8>)>,
    live: Arc<AtomicBool>,
}

#[derive(Default)]
struct FocusState {
    peers: HashMap<String, Peer>,
    // A denied/unsupported sample is cached too; it must not be forwarded.
    cache: Option<(u64, Option<Content>)>,
}

pub(crate) trait FocusBackend: Send + Sync + 'static {
    fn generation(&self) -> u64;
    fn read(&self) -> Option<(Format, Vec<u8>)>;
    fn write(&self, format: Format, bytes: &[u8], expected_generation: u64) -> WriteResult;
}

pub(crate) enum WriteResult {
    Written { generation: u64 },
    Stale,
    Failed,
}

impl FocusState {
    fn authorize(&mut self, peer: &str) {
        self.peers.entry(peer.to_owned()).or_insert_with(|| Peer {
            live: Arc::new(AtomicBool::new(true)),
            ..Peer::default()
        });
    }

    fn revoke(&mut self, peer: &str) {
        if let Some(peer) = self.peers.remove(peer) {
            peer.live.store(false, Ordering::Release);
        }
    }

    fn sample(
        &mut self,
        generation: u64,
        read: impl FnOnce() -> Option<Content>,
    ) -> Option<Content> {
        if self
            .cache
            .as_ref()
            .is_none_or(|(old, _)| *old != generation)
        {
            self.cache = Some((generation, read()));
        }
        self.cache.as_ref().and_then(|(_, value)| value.clone())
    }

    fn focus(&mut self, peer: &str, content: Content) -> bool {
        let Some(peer) = self.peers.get_mut(peer) else {
            return false;
        };
        if peer.known == Some(content.hash) {
            peer.outgoing = None;
            return false;
        }
        peer.outgoing = Some(content);
        true
    }

    fn accepted(&mut self, peer: &str, hash: Hash) {
        if let Some(peer) = self.peers.get_mut(peer) {
            peer.known = Some(hash);
            if peer.outgoing.as_ref().is_some_and(|c| c.hash == hash) {
                peer.outgoing = None;
            }
        }
    }

    fn offer_raw(&mut self, peer: &str, generation: u64, format: Format, bytes: Vec<u8>) -> bool {
        if bytes.is_empty() || bytes.len() > format.max_size() {
            return false;
        }
        let Some(peer) = self.peers.get_mut(peer) else {
            return false;
        };
        peer.incoming = Some((generation, format, bytes));
        true
    }

    #[cfg(test)]
    fn offer(&mut self, peer: &str, generation: u64, content: Content) -> bool {
        self.offer_raw(peer, generation, content.format, content.bytes.to_vec())
    }

    #[cfg(test)]
    fn finish(
        &mut self,
        peer: &str,
        current_generation: u64,
        write: impl FnOnce(&Content) -> WriteResult,
    ) {
        let Some(peer) = self.peers.get_mut(peer) else {
            return;
        };
        let Some((generation, format, bytes)) = peer.incoming.take() else {
            return;
        };
        if generation != current_generation {
            return;
        }
        let content = Content::new(format, bytes).expect("validated incoming content");
        match write(&content) {
            WriteResult::Written { generation } => {
                peer.known = Some(content.hash);
                self.cache = Some((generation, Some(content)));
            }
            WriteResult::Failed => peer.known = Some(content.hash),
            WriteResult::Stale => {}
        }
    }
}

use crate::clipboard_session::{
    BridgeError, Content as ReceivedContent, Lane, LaneBridge, SessionContext, SessionError,
};
use futures::FutureExt;
use tokio::task::JoinHandle;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct LaneKey {
    pub inbound: bool,
    pub session: u64,
}

struct Link<L: Lane> {
    peer: String,
    bridge: LaneBridge<L>,
    context: SessionContext,
    epoch: u64,
    next_id: u64,
    sending: Option<(u64, Content)>,
    receiving: Option<(u64, u64)>,
}

type WriteCompletion = (String, u64, Content, WriteResult);

pub(crate) struct FocusManager<L: Lane, B: FocusBackend> {
    backend: Arc<B>,
    state: FocusState,
    links: HashMap<LaneKey, Link<L>>,
    peer_epochs: HashMap<String, u64>,
    epoch: u64,
    read: Option<JoinHandle<(u64, Option<Content>)>>,
    requests: HashMap<String, (u64, u64)>,
    write: Option<JoinHandle<WriteCompletion>>,
}

impl<L: Lane, B: FocusBackend> FocusManager<L, B> {
    pub(crate) fn new(backend: Arc<B>) -> Self {
        Self {
            backend,
            state: FocusState::default(),
            links: HashMap::new(),
            peer_epochs: HashMap::new(),
            epoch: 0,
            read: None,
            requests: HashMap::new(),
            write: None,
        }
    }

    pub(crate) fn attach(
        &mut self,
        peer: String,
        key: LaneKey,
        lane: L,
        mut context: SessionContext,
    ) {
        self.detach_lane(key);
        context.local_generation = 0;
        if let Ok(bridge) = LaneBridge::new(lane, context) {
            self.epoch = self.epoch.checked_add(1).expect("focus epoch exhausted");
            self.peer_epochs.entry(peer.clone()).or_insert(self.epoch);
            self.state.authorize(&peer);
            self.links.insert(
                key,
                Link {
                    peer,
                    bridge,
                    context,
                    epoch: self.epoch,
                    next_id: 1,
                    sending: None,
                    receiving: None,
                },
            );
        }
    }

    pub(crate) fn contains(&self, key: LaneKey) -> bool {
        self.links.contains_key(&key)
    }

    pub(crate) fn lanes(&self) -> Vec<(String, LaneKey)> {
        self.links
            .iter()
            .map(|(key, link)| (link.peer.clone(), *key))
            .collect()
    }

    pub(crate) fn detach_lane(&mut self, key: LaneKey) {
        if let Some(link) = self.links.remove(&key) {
            if !self.links.values().any(|other| other.peer == link.peer) {
                self.requests.remove(&link.peer);
                self.peer_epochs.remove(&link.peer);
                self.state.revoke(&link.peer);
            }
        }
    }

    pub(crate) fn detach_peer(&mut self, peer: &str) {
        let keys: Vec<_> = self
            .links
            .iter()
            .filter_map(|(key, link)| (link.peer == peer).then_some(*key))
            .collect();
        for key in keys {
            self.detach_lane(key);
        }
    }

    pub(crate) fn focus_out(&mut self, peer: &str) {
        let Some(epoch) = self.peer_epochs.get(peer).copied() else {
            return;
        };
        let generation = self.backend.generation();
        if let Some((cached_gen, content)) = &self.state.cache {
            if *cached_gen == generation {
                if let Some(content) = content.clone() {
                    self.state.focus(peer, content);
                }
                return;
            }
        }
        self.requests.insert(peer.to_owned(), (epoch, generation));
        self.start_read();
    }

    fn start_read(&mut self) {
        if self.read.is_some() || self.requests.is_empty() {
            return;
        }
        let generation = self.backend.generation();
        let backend = self.backend.clone();
        self.read = Some(tokio::task::spawn_blocking(move || {
            let content = backend
                .read()
                .and_then(|(format, bytes)| Content::new(format, bytes));
            (generation, content)
        }));
    }

    pub(crate) fn tick(&mut self) {
        if self.read.as_ref().is_some_and(JoinHandle::is_finished) {
            let handle = self.read.take().unwrap();
            if let Ok((generation, content)) = handle.now_or_never().expect("finished task") {
                if self.backend.generation() == generation {
                    self.state.sample(generation, || content.clone());
                    self.requests.retain(|peer, (epoch, requested)| {
                        if *requested == generation {
                            if self
                                .peer_epochs
                                .get(peer)
                                .is_some_and(|current| current == epoch)
                            {
                                if let Some(content) = content.clone() {
                                    self.state.focus(peer, content);
                                }
                            }
                            false
                        } else {
                            true
                        }
                    });
                }
            }
            // Requests made during an old read follow the latest native generation.
            let current = self.backend.generation();
            for (_, generation) in self.requests.values_mut() {
                *generation = current;
            }
        }
        self.start_read();
        if self.write.as_ref().is_some_and(JoinHandle::is_finished) {
            let handle = self.write.take().unwrap();
            if let Ok((peer, epoch, content, result)) =
                handle.now_or_never().expect("finished task")
            {
                if self
                    .peer_epochs
                    .get(&peer)
                    .is_some_and(|current| *current == epoch)
                {
                    if !matches!(result, WriteResult::Stale) {
                        if let Some(p) = self.state.peers.get_mut(&peer) {
                            p.known = Some(content.hash);
                        }
                    }
                    if let WriteResult::Written { generation } = result {
                        self.state.cache = Some((generation, Some(content)));
                    }
                }
            }
        }
        let mut latest = HashMap::new();
        let mut active = std::collections::HashSet::new();
        for (key, link) in &self.links {
            let entry = latest
                .entry(link.peer.clone())
                .or_insert((link.epoch, *key));
            if link.epoch > entry.0 {
                *entry = (link.epoch, *key);
            }
            if link.sending.is_some() {
                active.insert(link.peer.clone());
            }
        }
        let mut dropped = Vec::new();
        for (key, link) in &mut self.links {
            let peer = &link.peer;
            let context = link.context;
            if latest
                .get(peer)
                .is_some_and(|(_, selected)| selected == key)
                && !active.contains(peer)
                && link.sending.is_none()
            {
                if let Some(content) = self.state.peers.get(peer).and_then(|p| p.outgoing.clone()) {
                    match link.bridge.start_send(
                        link.next_id,
                        content.format,
                        content.bytes.to_vec(),
                        context,
                    ) {
                        Ok(()) => {
                            let id = link.next_id;
                            link.next_id =
                                link.next_id.checked_add(1).expect("transfer id exhausted");
                            link.sending = Some((id, content));
                        }
                        Err(BridgeError::Session(SessionError::PermissionDenied)) => {}
                        Err(_) => {
                            dropped.push(*key);
                            continue;
                        }
                    }
                }
            }
            let mut failed = false;
            for _ in 0..64 {
                match link.bridge.try_send(context) {
                    Ok(true) => {}
                    Ok(false) => break,
                    Err(_) => {
                        failed = true;
                        break;
                    }
                }
            }
            if failed {
                dropped.push(*key);
                continue;
            }
            if let Some((id, content)) = &link.sending {
                if link.bridge.completed_send() == *id {
                    self.state.accepted(peer, content.hash);
                    link.sending = None;
                } else if !link.bridge.is_sending() {
                    link.sending = None;
                }
            }
            for _ in 0..64 {
                match link.bridge.try_receive(context) {
                    Ok(completion) => {
                        let receiving_id = link.bridge.receiving_id();
                        if let Some(id) = receiving_id {
                            if link.receiving.is_none_or(|(old, _)| old != id) {
                                link.receiving = Some((id, self.backend.generation()));
                            }
                        }
                        if let Some(completion) = completion {
                            let start = link.receiving.take().map(|(_, generation)| generation);
                            if link
                                .bridge
                                .validate_completion(&completion, context)
                                .is_ok()
                            {
                                if let Some(expected) =
                                    start.filter(|g| *g == self.backend.generation())
                                {
                                    let (format, bytes) = match completion.content {
                                        ReceivedContent::Text(text) => {
                                            (Format::Text, text.into_bytes())
                                        }
                                        ReceivedContent::PngNeedsValidation(bytes) => {
                                            (Format::Png, bytes)
                                        }
                                    };
                                    self.state.offer_raw(peer, expected, format, bytes);
                                }
                            }
                        } else if receiving_id.is_none() {
                            link.receiving = None;
                        }
                        if !link.bridge.received_frame() {
                            break;
                        }
                    }
                    Err(BridgeError::SendInterrupted { .. }) => {
                        link.sending = None;
                    }
                    Err(_) => {
                        failed = true;
                        break;
                    }
                }
            }
            if failed {
                dropped.push(*key);
            }
        }
        for key in dropped {
            self.detach_lane(key);
        }
        if self.write.is_none() {
            if let Some((peer, epoch, expected, format, bytes, live)) =
                self.state.peers.iter_mut().find_map(|(peer, state)| {
                    let (expected, format, bytes) = state.incoming.take()?;
                    let epoch = *self.peer_epochs.get(peer)?;
                    Some((
                        peer.clone(),
                        epoch,
                        expected,
                        format,
                        bytes,
                        state.live.clone(),
                    ))
                })
            {
                let backend = self.backend.clone();
                self.write = Some(tokio::task::spawn_blocking(move || {
                    let content = Content::new(format, bytes).expect("validated incoming content");
                    // Revocation can cancel preparation, not retract an admitted native write.
                    let result = if !live.load(Ordering::Acquire) {
                        WriteResult::Stale
                    } else if content.format == Format::Png && !valid_png(&content.bytes) {
                        WriteResult::Failed
                    } else if !live.load(Ordering::Acquire) {
                        WriteResult::Stale
                    } else {
                        backend.write(content.format, &content.bytes, expected)
                    };
                    (peer, epoch, content, result)
                }));
            }
        }
    }
}

fn valid_png(bytes: &[u8]) -> bool {
    input_event::clipboard_png::decode_png(bytes).is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text(s: &str) -> Content {
        Content::new(Format::Text, s.as_bytes().to_vec()).unwrap()
    }

    #[test]
    fn raw_incoming_is_bounded_and_latest_offer_wins() {
        let mut state = FocusState::default();
        state.authorize("p");
        assert!(!state.offer_raw("p", 1, Format::Text, Vec::new()));
        assert!(!state.offer_raw("other", 1, Format::Text, b"ignored".to_vec()));
        assert!(state.offer_raw("p", 1, Format::Text, b"old".to_vec()));
        assert!(state.offer_raw("p", 2, Format::Text, b"new".to_vec()));
        state.finish("p", 2, |content| {
            assert_eq!(&*content.bytes, b"new");
            WriteResult::Written { generation: 3 }
        });
        assert_eq!(state.peers["p"].known, Some(text("new").hash));
        assert_eq!(state.cache.as_ref().unwrap().0, 3);
    }

    #[test]
    fn revoked_peer_invalidates_worker_even_after_reauthorization() {
        let mut state = FocusState::default();
        state.authorize("p");
        let old = state.peers["p"].live.clone();
        state.revoke("p");
        state.authorize("p");
        assert!(!old.load(Ordering::Acquire));
        assert!(state.peers["p"].live.load(Ordering::Acquire));
    }

    #[test]
    fn deduplication_and_three_peer_forwarding() {
        let mut a = FocusState::default();
        a.authorize("b");
        a.authorize("c");
        let value = text("hello");
        assert!(a.offer("b", 1, value.clone()));
        a.finish("b", 1, |_| WriteResult::Written { generation: 2 });
        assert!(!a.focus("b", value.clone()));
        assert!(a.focus("c", value.clone()));
        a.accepted("c", value.hash);
        assert!(!a.focus("c", value));
    }

    #[test]
    fn supersession_revocation_and_stale_receive() {
        let mut state = FocusState::default();
        state.authorize("p");
        let first = text("first");
        let second = text("second");
        assert!(state.focus("p", first.clone()));
        assert!(state.focus("p", second.clone()));
        state.accepted("p", first.hash);
        assert_eq!(state.peers["p"].outgoing, Some(second.clone()));
        assert_eq!(state.peers["p"].known, Some(first.hash));
        assert!(state.offer("p", 7, first.clone()));
        assert!(state.offer("p", 7, second));
        state.finish("p", 8, |_| panic!("stale write"));
        assert_eq!(state.peers["p"].known, Some(first.hash));
        state.revoke("p");
        assert!(!state.focus("p", text("third")));
        assert!(!state.offer("p", 8, text("third")));
    }

    #[test]
    fn failure_and_cache_decisions() {
        let mut state = FocusState::default();
        state.authorize("p");
        let value = text("remote");
        assert!(state.offer("p", 3, value.clone()));
        state.finish("p", 3, |_| WriteResult::Failed);
        assert_eq!(state.peers["p"].known, Some(value.hash));
        assert!(state.offer("p", 4, text("new")));
        state.finish("p", 4, |_| WriteResult::Stale);
        assert_eq!(state.peers["p"].known, Some(value.hash));
        assert_eq!(state.sample(8, || None), None);
        assert_eq!(state.sample(8, || panic!("cached rejection")), None);
        assert_eq!(state.sample(9, || Some(value.clone())), Some(value));
    }

    use crate::{clipboard_session::Formats, transport::ClipboardError};
    use std::{
        collections::VecDeque,
        sync::Mutex,
        time::{Duration, Instant},
    };

    type Queue = Arc<Mutex<VecDeque<Vec<u8>>>>;
    struct FakeLane {
        outgoing: Queue,
        incoming: Queue,
        capacity: usize,
    }
    impl Lane for FakeLane {
        fn send(&self, bytes: &[u8]) -> Result<(), ClipboardError> {
            let mut queue = self.outgoing.lock().unwrap();
            if queue.len() >= self.capacity {
                return Err(ClipboardError::Backpressure);
            }
            queue.push_back(bytes.to_vec());
            Ok(())
        }
        fn receive(&self) -> Result<Option<Vec<u8>>, ClipboardError> {
            Ok(self.incoming.lock().unwrap().pop_front())
        }
        fn close(&self) {}
    }
    fn pair(capacity: usize) -> (FakeLane, FakeLane) {
        let a = Arc::new(Mutex::new(VecDeque::new()));
        let b = Arc::new(Mutex::new(VecDeque::new()));
        (
            FakeLane {
                outgoing: a.clone(),
                incoming: b.clone(),
                capacity,
            },
            FakeLane {
                outgoing: b,
                incoming: a,
                capacity,
            },
        )
    }
    type FakeClipboard = (u64, Option<(Format, Vec<u8>)>);
    #[derive(Default)]
    struct FakeBackend {
        value: Mutex<FakeClipboard>,
        delay: Duration,
    }
    impl FakeBackend {
        fn set(&self, format: Format, bytes: &[u8]) {
            let mut state = self.value.lock().unwrap();
            state.0 += 1;
            state.1 = Some((format, bytes.to_vec()));
        }
        fn value(&self) -> Option<(Format, Vec<u8>)> {
            self.value.lock().unwrap().1.clone()
        }
    }
    impl FocusBackend for FakeBackend {
        fn generation(&self) -> u64 {
            self.value.lock().unwrap().0
        }
        fn read(&self) -> Option<(Format, Vec<u8>)> {
            std::thread::sleep(self.delay);
            self.value()
        }
        fn write(&self, format: Format, bytes: &[u8], expected: u64) -> WriteResult {
            let mut state = self.value.lock().unwrap();
            if state.0 != expected {
                return WriteResult::Stale;
            }
            state.0 += 1;
            state.1 = Some((format, bytes.to_vec()));
            WriteResult::Written {
                generation: state.0,
            }
        }
    }
    type Manager = FocusManager<FakeLane, FakeBackend>;
    fn manager() -> (Manager, Arc<FakeBackend>) {
        let backend = Arc::new(FakeBackend::default());
        (FocusManager::new(backend.clone()), backend)
    }
    fn connect(a: &mut Manager, an: &str, b: &mut Manager, bn: &str) {
        connect_limited(a, an, b, bn, usize::MAX);
    }
    fn connect_limited(a: &mut Manager, an: &str, b: &mut Manager, bn: &str, capacity: usize) {
        let (x, y) = pair(capacity);
        let context = SessionContext {
            session_valid: true,
            session_epoch: 1,
            authorization_epoch: 1,
            local_consent: Formats {
                text: true,
                png: true,
            },
            local_generation: 0,
        };
        let key = LaneKey {
            inbound: false,
            session: u64::from(an.as_bytes()[0]) + u64::from(bn.as_bytes()[0]),
        };
        a.attach(bn.to_owned(), key, x, context);
        b.attach(an.to_owned(), key, y, context);
    }
    async fn rounds(managers: &mut [&mut Manager], n: usize) {
        for _ in 0..n {
            for m in managers.iter_mut() {
                m.tick();
            }
            tokio::time::sleep(Duration::from_millis(2)).await;
        }
    }
    fn sent(m: &Manager, peer: &str) -> u64 {
        m.links
            .values()
            .find(|link| link.peer == peer)
            .unwrap()
            .next_id
    }

    #[tokio::test]
    async fn fake_text_transfer() {
        let (mut a, ab) = manager();
        let (mut b, bb) = manager();
        connect(&mut a, "a", &mut b, "b");
        ab.set(Format::Text, b"hello");
        a.focus_out("b");
        rounds(&mut [&mut a, &mut b], 30).await;
        assert_eq!(bb.value(), Some((Format::Text, b"hello".to_vec())));
    }
    #[tokio::test]
    async fn fake_no_echo() {
        let (mut a, ab) = manager();
        let (mut b, _) = manager();
        connect(&mut a, "a", &mut b, "b");
        ab.set(Format::Text, b"hello");
        a.focus_out("b");
        rounds(&mut [&mut a, &mut b], 30).await;
        let before = sent(&b, "a");
        b.focus_out("a");
        rounds(&mut [&mut a, &mut b], 20).await;
        assert_eq!(sent(&b, "a"), before);
    }
    #[tokio::test]
    async fn fake_reply_with_new_content() {
        let (mut a, ab) = manager();
        let (mut b, bb) = manager();
        connect(&mut a, "a", &mut b, "b");
        ab.set(Format::Text, b"one");
        a.focus_out("b");
        rounds(&mut [&mut a, &mut b], 30).await;
        bb.set(Format::Text, b"two");
        b.focus_out("a");
        rounds(&mut [&mut a, &mut b], 30).await;
        assert_eq!(ab.value(), Some((Format::Text, b"two".to_vec())));
    }
    #[tokio::test]
    async fn fake_unchanged_dedup() {
        let (mut a, ab) = manager();
        let (mut b, _) = manager();
        connect(&mut a, "a", &mut b, "b");
        ab.set(Format::Text, b"one");
        a.focus_out("b");
        rounds(&mut [&mut a, &mut b], 30).await;
        let before = sent(&a, "b");
        a.focus_out("b");
        rounds(&mut [&mut a, &mut b], 20).await;
        assert_eq!(sent(&a, "b"), before);
    }
    #[tokio::test]
    async fn fake_three_device_relay() {
        let (mut a, ab) = manager();
        let (mut b, _) = manager();
        let (mut c, cb) = manager();
        connect(&mut a, "a", &mut b, "b");
        connect(&mut b, "b", &mut c, "c");
        ab.set(Format::Text, b"relay");
        a.focus_out("b");
        rounds(&mut [&mut a, &mut b, &mut c], 30).await;
        b.focus_out("c");
        rounds(&mut [&mut a, &mut b, &mut c], 30).await;
        assert_eq!(cb.value(), Some((Format::Text, b"relay".to_vec())));
        let before = sent(&c, "b");
        c.focus_out("b");
        rounds(&mut [&mut a, &mut b, &mut c], 20).await;
        assert_eq!(sent(&c, "b"), before);
    }
    #[tokio::test]
    async fn fake_stale_receive() {
        let (mut a, ab) = manager();
        let (mut b, bb) = manager();
        connect_limited(&mut a, "a", &mut b, "b", 1);
        rounds(&mut [&mut a, &mut b], 15).await;
        ab.set(Format::Text, b"remote");
        a.focus_out("b");
        while a
            .links
            .values()
            .find(|link| link.peer == "b")
            .unwrap()
            .sending
            .is_none()
        {
            a.tick();
            b.tick();
            tokio::time::sleep(Duration::from_millis(2)).await;
        }
        bb.set(Format::Text, b"local");
        b.tick();
        rounds(&mut [&mut a, &mut b], 20).await;
        assert_eq!(bb.value(), Some((Format::Text, b"local".to_vec())));
    }
    #[tokio::test]
    async fn fake_superseding_push() {
        let (mut a, ab) = manager();
        let (mut b, bb) = manager();
        connect_limited(&mut a, "a", &mut b, "b", 1);
        rounds(&mut [&mut a, &mut b], 15).await;
        ab.set(Format::Text, b"first");
        a.focus_out("b");
        while a
            .links
            .values()
            .find(|link| link.peer == "b")
            .unwrap()
            .sending
            .is_none()
        {
            a.tick();
            b.tick();
            tokio::time::sleep(Duration::from_millis(2)).await;
        }
        ab.set(Format::Text, b"second");
        a.focus_out("b");
        rounds(&mut [&mut a, &mut b], 50).await;
        assert_eq!(bb.value(), Some((Format::Text, b"second".to_vec())));
    }
    #[tokio::test]
    async fn fake_detach() {
        let (mut a, ab) = manager();
        let (mut b, bb) = manager();
        connect(&mut a, "a", &mut b, "b");
        rounds(&mut [&mut a, &mut b], 15).await;
        a.detach_peer("b");
        ab.set(Format::Text, b"no");
        a.focus_out("b");
        rounds(&mut [&mut a, &mut b], 20).await;
        assert_eq!(bb.value(), None);
    }
    #[tokio::test]
    async fn fake_slow_read_is_nonblocking() {
        let backend = Arc::new(FakeBackend {
            delay: Duration::from_millis(500),
            ..Default::default()
        });
        let mut a = FocusManager::new(backend);
        let (mut b, _) = manager();
        connect(&mut a, "a", &mut b, "b");
        let start = Instant::now();
        a.focus_out("b");
        assert!(start.elapsed() < Duration::from_millis(10));
    }
    #[tokio::test]
    async fn fake_invalid_png() {
        let (mut a, ab) = manager();
        let (mut b, bb) = manager();
        connect(&mut a, "a", &mut b, "b");
        ab.set(Format::Png, b"not a png");
        a.focus_out("b");
        rounds(&mut [&mut a, &mut b], 30).await;
        assert_eq!(bb.value(), None);
    }

    #[tokio::test]
    async fn fake_two_lanes_share_known_and_detach() {
        let (mut a, ab) = manager();
        let (mut b, _) = manager();
        connect(&mut a, "a", &mut b, "b");
        rounds(&mut [&mut a, &mut b], 10).await;
        ab.set(Format::Text, b"shared");
        a.focus_out("b");
        rounds(&mut [&mut a, &mut b], 20).await;
        let (x, y) = pair(usize::MAX);
        let key = LaneKey {
            inbound: true,
            session: 99,
        };
        let context = a.links.values().next().unwrap().context;
        a.attach("b".into(), key, x, context);
        b.attach("a".into(), key, y, context);
        rounds(&mut [&mut a, &mut b], 10).await;
        assert!(b.contains(key));
        assert_eq!(b.lanes().len(), 2);
        b.focus_out("a");
        rounds(&mut [&mut a, &mut b], 15).await;
        assert_eq!(b.links[&key].next_id, 1);
        b.detach_lane(key);
        assert!(b.state.peers["a"].known.is_some());
        b.detach_peer("a");
        assert!(!b.state.peers.contains_key("a"));
    }

    #[tokio::test]
    async fn fake_generation_change_after_offer_does_not_set_known() {
        let (mut a, ab) = manager();
        let (mut b, bb) = manager();
        connect_limited(&mut a, "a", &mut b, "b", 1);
        rounds(&mut [&mut a, &mut b], 12).await;
        ab.set(Format::Text, b"remote");
        a.focus_out("b");
        for _ in 0..100 {
            a.tick();
            b.tick();
            if b.links.values().any(|l| l.receiving.is_some()) {
                break;
            }
            tokio::time::sleep(Duration::from_millis(2)).await;
        }
        assert!(b.links.values().any(|l| l.receiving.is_some()));
        bb.set(Format::Text, b"local");
        b.tick();
        rounds(&mut [&mut a, &mut b], 20).await;
        assert_eq!(bb.value(), Some((Format::Text, b"local".to_vec())));
        assert_eq!(b.state.peers["a"].known, None);
    }

    #[tokio::test]
    async fn fake_permission_revocation_rebuilds_lane() {
        use mousehop_proto::clipboard_transfer::{Message, SessionFrame};
        let (mut a, ab) = manager();
        let (mut b, bb) = manager();
        let (x, y) = pair(1);
        let inbound_queue = y.outgoing.clone();
        let key = LaneKey {
            inbound: false,
            session: 1,
        };
        let context = SessionContext {
            session_epoch: 1,
            session_valid: true,
            authorization_epoch: 1,
            local_consent: Formats {
                text: true,
                png: true,
            },
            local_generation: 0,
        };
        a.attach("b".into(), key, x, context);
        b.attach("a".into(), key, y, context);
        rounds(&mut [&mut a, &mut b], 12).await;
        ab.set(Format::Text, b"first");
        a.focus_out("b");
        for _ in 0..100 {
            a.tick();
            if a.links[&key].sending.is_some() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(2)).await;
        }
        assert!(a.links[&key].sending.is_some());
        let revoke = SessionFrame::Data {
            token: 1,
            source: 1,
            message: Message::Permission {
                text: false,
                png: false,
            },
        }
        .encode()
        .unwrap();
        inbound_queue.lock().unwrap().push_back(revoke);
        a.tick();
        assert!(!a.contains(key));
        assert!(!a.state.peers.contains_key("b"));
        b.detach_peer("a");
        connect(&mut a, "a", &mut b, "b");
        rounds(&mut [&mut a, &mut b], 12).await;
        ab.set(Format::Text, b"restored");
        a.focus_out("b");
        rounds(&mut [&mut a, &mut b], 25).await;
        assert_eq!(bb.value(), Some((Format::Text, b"restored".to_vec())));
    }

    async fn fake_sixteen_mib_transfer(performance_limit: Option<Duration>) {
        let (mut a, ab) = manager();
        let (mut b, bb) = manager();
        connect(&mut a, "a", &mut b, "b");
        rounds(&mut [&mut a, &mut b], 12).await;
        let image = input_event::clipboard_png::RgbaImage::new(1, 1, vec![1, 2, 3, 255]).unwrap();
        let mut bytes = input_event::clipboard_png::encode_png(&image).unwrap();
        let end = bytes.split_off(bytes.len() - 12);
        let payload_len =
            mousehop_proto::clipboard_transfer::MAX_PNG_SIZE - bytes.len() - end.len() - 12;
        bytes.extend_from_slice(&(payload_len as u32).to_be_bytes());
        let chunk_start = bytes.len();
        bytes.extend_from_slice(b"npAD");
        bytes.resize(bytes.len() + payload_len, 42);
        let mut table = [0u32; 256];
        for (i, entry) in table.iter_mut().enumerate() {
            let mut value = i as u32;
            for _ in 0..8 {
                value = if value & 1 == 0 {
                    value >> 1
                } else {
                    (value >> 1) ^ 0xedb88320
                };
            }
            *entry = value;
        }
        let mut crc = u32::MAX;
        for byte in &bytes[chunk_start..] {
            crc = table[((crc ^ u32::from(*byte)) & 255) as usize] ^ (crc >> 8);
        }
        bytes.extend_from_slice(&(!crc).to_be_bytes());
        bytes.extend_from_slice(&end);
        assert_eq!(
            bytes.len(),
            mousehop_proto::clipboard_transfer::MAX_PNG_SIZE
        );
        assert!(valid_png(&bytes));
        ab.set(Format::Png, &bytes);
        let transfer_started = Instant::now();
        a.focus_out("b");
        tokio::time::timeout(Duration::from_secs(30), async {
            while !a.read.as_ref().unwrap().is_finished() {
                tokio::time::sleep(Duration::from_millis(2)).await;
            }
        })
        .await
        .expect("clipboard read worker hung");
        let mut ticks = 0;
        while ticks < 8 {
            a.tick();
            b.tick();
            ticks += 1;
            if b.write.is_some() {
                break;
            }
        }
        assert!(
            b.write.is_some(),
            "transfer did not finish in {ticks} ticks"
        );
        assert!(ticks <= 8);
        tokio::time::timeout(Duration::from_secs(30), async {
            while !b.write.as_ref().unwrap().is_finished() {
                tokio::time::sleep(Duration::from_millis(1)).await;
            }
        })
        .await
        .expect("clipboard write worker hung");
        let elapsed = transfer_started.elapsed();
        b.tick();
        assert!(b.state.peers["a"].known.is_some());
        assert_eq!(bb.value(), Some((Format::Png, bytes)));
        if let Some(limit) = performance_limit {
            eprintln!("16 MiB in-memory transfer completed in {elapsed:?}");
            assert!(elapsed < limit, "16 MiB transfer took {elapsed:?}");
        }
    }

    #[tokio::test]
    async fn fake_sixteen_mib_transfer_within_eight_ticks() {
        fake_sixteen_mib_transfer(None).await;
    }

    // In-memory lane and clipboard only; this is not native or network latency.
    #[cfg(not(debug_assertions))]
    #[tokio::test]
    async fn fake_sixteen_mib_release_performance() {
        fake_sixteen_mib_transfer(Some(Duration::from_secs(1))).await;
    }

    #[tokio::test]
    async fn fake_valid_png_and_latest_lane_selection() {
        let (mut a, ab) = manager();
        let (mut b, bb) = manager();
        connect(&mut a, "a", &mut b, "b");
        let first = *a.links.keys().next().unwrap();
        let (x, y) = pair(usize::MAX);
        let second = LaneKey {
            inbound: true,
            session: 456,
        };
        let context = a.links[&first].context;
        a.attach("b".into(), second, x, context);
        b.attach("a".into(), second, y, context);
        rounds(&mut [&mut a, &mut b], 12).await;
        let image = input_event::clipboard_png::RgbaImage::new(1, 1, vec![1, 2, 3, 255]).unwrap();
        let png = input_event::clipboard_png::encode_png(&image).unwrap();
        ab.set(Format::Png, &png);
        a.focus_out("b");
        rounds(&mut [&mut a, &mut b], 25).await;
        assert_eq!(bb.value(), Some((Format::Png, png)));
        assert_eq!(a.links[&first].next_id, 1);
        assert_eq!(a.links[&second].next_id, 2);
    }
}
