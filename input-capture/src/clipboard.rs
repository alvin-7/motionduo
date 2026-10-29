#[cfg(all(feature = "clipboard-image", any(windows, target_os = "macos")))]
#[path = "clipboard_image.rs"]
pub mod image;

#[cfg(all(feature = "clipboard-image", any(target_os = "macos", test)))]
#[path = "clipboard_file_image.rs"]
mod file_image;

use arboard::Clipboard;
use input_event::{ClipboardEvent, Event};
use mousehop_ipc::AppIdent;
use std::collections::{HashSet, VecDeque};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// Maximum text payload admitted by the native capture adapter.
/// This matches the wire offer limit; native APIs may still allocate before
/// returning, so this is an admission bound rather than an allocation proof.
pub const MAX_TEXT_BYTES: usize = 1024 * 1024;
const ECHO_ENTRIES: usize = 2;
const ECHO_TTL: Duration = Duration::from_secs(5);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ClipboardFormat {
    Text,
    Png,
}

/// A bounded native sample with the OS change counter captured alongside it.
/// The generation is provenance only: other applications can mutate the
/// clipboard without going through Mousehop and may race a sample.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ClipboardSample {
    pub native_generation: u64,
    pub format: ClipboardFormat,
    pub bytes: Vec<u8>,
    /// Windows provenance read alongside the payload under one clipboard lock.
    /// Absence is normal for external copies, but cannot authorize echo matching.
    #[cfg(windows)]
    pub windows_provenance: Option<WindowsProvenance>,
}

/// Native format and opaque source marker; neither is authentication of a peer.
#[cfg(windows)]
#[derive(Clone, Eq, PartialEq)]
pub struct WindowsProvenance {
    pub native_format: u32,
    pub marker: [u8; 32],
}

#[cfg(windows)]
impl std::fmt::Debug for WindowsProvenance {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WindowsProvenance")
            .field("native_format", &self.native_format)
            .finish_non_exhaustive()
    }
}

/// Bounded shared echo hints for text and PNG. Registration is not proof that
/// Mousehop owns a generation: a sequence counter observed after an OS write
/// can already belong to an unrelated writer. Use only with independently
/// established write provenance, never a guessed post-write generation.
#[derive(Clone, Default)]
pub struct EchoSuppression {
    inner: Arc<Mutex<VecDeque<EchoEntry>>>,
    #[cfg(target_os = "macos")]
    mac_fence: Arc<Mutex<MacCaptureFence>>,
}

/// Application-level ownership fence: unlike byte matching, deliberately drops
/// *all* samples at or before a registered write generation. A later local copy
/// advances changeCount and is admitted even if it retained the custom type.
#[cfg(any(target_os = "macos", test))]
#[derive(Default)]
struct MacCaptureFence {
    through: Option<u64>,
}

#[cfg(any(target_os = "macos", test))]
impl MacCaptureFence {
    fn register(&mut self, generation: u64) -> bool {
        // NSPasteboard uses a signed NSInteger. Refuse zero/negative/wrapped
        // values rather than treating wrapping as evidence of a fresh copy.
        if generation == 0 || generation > i64::MAX as u64 {
            return false;
        }
        self.through = Some(self.through.map_or(generation, |old| old.max(generation)));
        true
    }

    #[allow(dead_code)]
    fn allows(&self, generation: u64) -> bool {
        generation != 0
            && generation <= i64::MAX as u64
            && self.through.is_none_or(|through| generation > through)
    }
}

struct EchoEntry {
    sample: ClipboardSample,
    inserted: Instant,
}

impl EchoSuppression {
    /// Register the generation from a verified macOS ownership-taking receipt.
    /// Call synchronously inside its existing lifecycle-guarded registration
    /// callback, before releasing that guard. Never pass a post-write poll.
    /// This is a conservative ownership fence, NOT marker/content atomicity:
    /// every sample through this generation is dropped, including a same-owner
    /// mutation. The fence has no TTL; a future ownership change remains usable.
    /// Capture publication/revocation must share the caller's lifecycle fence.
    #[cfg(target_os = "macos")]
    pub fn register_macos_write_generation(&self, generation: u64) -> bool {
        self.mac_fence
            .try_lock()
            .is_ok_and(|mut fence| fence.register(generation))
    }

    #[cfg(target_os = "macos")]
    #[allow(dead_code)]
    pub(super) fn macos_generation_allows(&self, generation: u64) -> bool {
        self.mac_fence
            .try_lock()
            .is_ok_and(|fence| fence.allows(generation))
    }

    /// Register independently verified native-write provenance, not an arbitrary
    /// generation read after `set_checked`. Existing emulation APIs do not return
    /// such a receipt. Returns false on oversize/unknown generation/contention.
    /// At most two bounded payloads are retained for five seconds. Exact bytes
    /// avoid hash collisions; a same-content external copy in a NEW generation
    /// is never suppressed. Native generation wrap/reuse remains an OS limit.
    pub fn register(&self, sample: ClipboardSample) -> bool {
        if sample.native_generation == 0 || !sample_is_bounded(&sample) {
            return false;
        }
        #[cfg(windows)]
        if sample.windows_provenance.is_none() {
            return false;
        }
        #[cfg(windows)]
        if !sample
            .windows_provenance
            .as_ref()
            .is_some_and(|source| match sample.format {
                ClipboardFormat::Text => source.native_format == 13,
                ClipboardFormat::Png => (0xc000..=0xffff).contains(&source.native_format),
            })
        {
            return false;
        }
        let Ok(mut entries) = self.inner.try_lock() else {
            return false;
        };
        let now = Instant::now();
        entries.retain(|entry| now.duration_since(entry.inserted) <= ECHO_TTL);
        while entries.len() >= ECHO_ENTRIES {
            entries.pop_front();
        }
        entries.push_back(EchoEntry {
            sample,
            inserted: now,
        });
        true
    }

    /// Test an exact generation/format/payload against retained write hints.
    /// Contended or poisoned state conservatively suppresses the sample.
    pub fn contains(&self, sample: &ClipboardSample) -> bool {
        // Fail closed on contention or poisoned state, not by blocking capture.
        let Ok(mut entries) = self.inner.try_lock() else {
            return true;
        };
        let now = Instant::now();
        entries.retain(|entry| now.duration_since(entry.inserted) <= ECHO_TTL);
        entries.iter().any(|entry| entry.sample == *sample)
    }
}

fn sample_is_bounded(sample: &ClipboardSample) -> bool {
    let limit = match sample.format {
        ClipboardFormat::Text => MAX_TEXT_BYTES,
        ClipboardFormat::Png => 16 * 1024 * 1024,
    };
    !sample.bytes.is_empty() && sample.bytes.len() <= limit
}

#[cfg(windows)]
fn native_generation() -> u64 {
    #[link(name = "user32")]
    unsafe extern "system" {
        fn GetClipboardSequenceNumber() -> u32;
    }
    unsafe { GetClipboardSequenceNumber() as u64 }
}

#[cfg(target_os = "macos")]
#[allow(dead_code)]
fn native_generation() -> u64 {
    use objc2_app_kit::NSPasteboard;
    objc2::rc::autoreleasepool(|_| NSPasteboard::generalPasteboard().changeCount() as u64)
}

use tokio::sync::mpsc::{self, Receiver, Sender};
use tokio::task::spawn_blocking;
use tokio::time::interval;

use crate::frontmost_app;
use crate::{CaptureError, CaptureEvent};

/// Shared, mutable suppression list. Service owns the canonical
/// `Arc<Mutex<HashSet<AppIdent>>>` and clones the handle into each
/// freshly-spawned [`ClipboardMonitor`]; mutations from
/// `Add/RemoveSuppressedApp` requests take effect immediately on
/// the next clipboard poll.
pub type SuppressionList = Arc<Mutex<HashSet<AppIdent>>>;

/// Clipboard monitor that watches for clipboard changes
pub struct ClipboardMonitor {
    event_rx: Receiver<CaptureEvent>,
    _event_tx: Sender<CaptureEvent>,
    last_content: Arc<Mutex<Option<String>>>,
    last_change: Arc<Mutex<Option<Instant>>>,
    enabled: Arc<Mutex<bool>>,
}

impl ClipboardMonitor {
    /// Construct without app-source suppression. Equivalent to
    /// `with_suppression(Default::default())` — provided as a
    /// convenience for callers that don't care about suppression
    /// (CLI smoke tests, future per-platform unit tests).
    pub fn new() -> Result<Self, CaptureError> {
        Self::with_suppression(SuppressionList::default())
    }

    /// Construct a monitor that consults `suppression` on every
    /// detected clipboard change and skips both the emit AND the
    /// `last_content` update when [`frontmost_app::frontmost_app()`]
    /// reports an app whose [`AppIdent`] is in the list. Skipping
    /// the `last_content` update is intentional: it keeps the
    /// monitor "blind" to the suppressed content so a later non-
    /// suppressed copy of the same string still emits normally.
    pub fn with_suppression(suppression: SuppressionList) -> Result<Self, CaptureError> {
        let (event_tx, event_rx) = mpsc::channel(16);
        let last_content: Arc<Mutex<Option<String>>> = Arc::new(Mutex::new(None));
        let last_change: Arc<Mutex<Option<Instant>>> = Arc::new(Mutex::new(None));
        let enabled = Arc::new(Mutex::new(true));

        let last_content_clone = last_content.clone();
        let last_change_clone = last_change.clone();
        let enabled_clone = enabled.clone();
        let event_tx_clone = event_tx.clone();
        let suppression_clone = suppression.clone();

        // Spawn monitoring task. Cadence: 100 ms on macOS (cheap
        // because `pasteboard_change_count_advanced` short-circuits
        // 99% of ticks via a single integer compare); 500 ms
        // elsewhere (no cheap precheck, full content read every
        // tick). Tighter cadence on macOS shrinks the
        // frontmost-app suppression race window from 500 ms →
        // 100 ms — the user would have to Cmd+Tab faster than
        // human reaction time after copying to defeat the check.
        #[cfg(target_os = "macos")]
        const POLL_MS: u64 = 100;
        #[cfg(not(target_os = "macos"))]
        const POLL_MS: u64 = 500;

        tokio::spawn(async move {
            let mut check_interval = interval(Duration::from_millis(POLL_MS));

            loop {
                check_interval.tick().await;

                // Check if enabled
                let is_enabled = {
                    let enabled = enabled_clone.lock().unwrap();
                    *enabled
                };

                if !is_enabled {
                    continue;
                }

                // macOS: skip the expensive content-read entirely
                // when NSPasteboard.changeCount hasn't advanced
                // since last tick. This is the canonical clipboard-
                // monitor optimization (Maccy / Alfred / Paste all
                // do it). Single integer compare per idle tick.
                #[cfg(target_os = "macos")]
                if !pasteboard_change_count_advanced() {
                    continue;
                }

                // Read clipboard in blocking task
                let last_content_clone2 = last_content_clone.clone();
                let last_change_clone2 = last_change_clone.clone();
                let event_tx_clone2 = event_tx_clone.clone();
                let suppression_clone2 = suppression_clone.clone();

                let _ = spawn_blocking(move || {
                    // Create clipboard instance
                    let mut clipboard = match Clipboard::new() {
                        Ok(c) => c,
                        Err(e) => {
                            log::debug!("Failed to create clipboard: {e}");
                            return;
                        }
                    };

                    // Get current clipboard text
                    let current_text = match clipboard.get_text() {
                        Ok(text) => {
                            log::trace!("Clipboard text read: {} bytes", text.len());
                            text
                        }
                        Err(e) => {
                            // Clipboard might be empty or contain non-text data
                            log::trace!("Failed to get clipboard text: {e}");
                            return;
                        }
                    };

                    // Check if content changed
                    let mut last_content = last_content_clone2.lock().unwrap();
                    let mut last_change = last_change_clone2.lock().unwrap();

                    let last_change_elapsed = last_change.map(|t| t.elapsed());
                    // App-source suppression. Frontmost-app lookup
                    // and the macOS concealed-pasteboard probe both
                    // run only after we've decided the content
                    // actually changed (see PollDecision::classify).
                    // Pre-compute the inputs here so the classifier
                    // stays a pure function.
                    let needs_decision = PollDecision::content_might_emit(
                        &current_text,
                        last_content.as_deref(),
                        last_change_elapsed,
                    );
                    let (concealed, suppressed) = if needs_decision {
                        let concealed = is_concealed_clipboard();
                        let suppressed = if concealed {
                            None
                        } else {
                            is_suppressed(&suppression_clone2)
                        };
                        (concealed, suppressed)
                    } else {
                        (false, None)
                    };
                    let decision = PollDecision::classify(
                        &current_text,
                        last_content.as_deref(),
                        last_change_elapsed,
                        concealed,
                        suppressed.is_some(),
                    );
                    // Always advance `last_content` after a state-
                    // changing decision (Suppressed or Emit),
                    // regardless of which suppression branch
                    // fired. The earlier "blind to suppressed
                    // value" approach left `last_content` at the
                    // previous emitted value, which made every
                    // subsequent 500ms poll see the SAME
                    // suppressed content as "changed" and re-run
                    // the suppression check. Any focus shift
                    // between polls (user alt-tabs after copying
                    // a password) would then find a non-
                    // suppressed frontmost and leak the password.
                    // PollDecision::advances_state pins this
                    // contract — see the unit tests at the
                    // bottom of this file.
                    if decision.advances_state() {
                        *last_content = Some(current_text.clone());
                        *last_change = Some(Instant::now());
                    }
                    match decision {
                        PollDecision::Unchanged => {}
                        PollDecision::Debounced => {
                            log::trace!("Clipboard changed but debounced (too recent)");
                        }
                        PollDecision::Suppressed if concealed => {
                            log::debug!("clipboard change suppressed (concealed pasteboard type)");
                        }
                        PollDecision::Suppressed => {
                            if let Some(app) = suppressed.as_ref() {
                                log::debug!(
                                    "clipboard change suppressed (frontmost app `{}`)",
                                    app.label()
                                );
                            }
                        }
                        PollDecision::Emit => {
                            log::info!("Clipboard changed, length: {} bytes", current_text.len());
                            let event = CaptureEvent::Input(Event::Clipboard(
                                ClipboardEvent::Text(current_text),
                            ));
                            let _ = event_tx_clone2.blocking_send(event);
                        }
                    }
                })
                .await;
            }
        });

        Ok(Self {
            event_rx,
            _event_tx: event_tx,
            last_content,
            last_change,
            enabled,
        })
    }

    /// Receive the next clipboard event
    pub async fn recv(&mut self) -> Option<CaptureEvent> {
        self.event_rx.recv().await
    }

    /// Enable clipboard monitoring
    pub fn enable(&self) {
        let mut enabled = self.enabled.lock().unwrap();
        *enabled = true;
        log::info!("Clipboard monitoring enabled");
    }

    /// Disable clipboard monitoring
    pub fn disable(&self) {
        let mut enabled = self.enabled.lock().unwrap();
        *enabled = false;
        log::info!("Clipboard monitoring disabled");
    }

    /// Update the last known clipboard content (called when we set the clipboard)
    /// This prevents detecting our own clipboard changes as external changes
    pub fn update_last_content(&self, content: String) {
        let mut last_content = self.last_content.lock().unwrap();
        let mut last_change = self.last_change.lock().unwrap();
        *last_content = Some(content);
        *last_change = Some(Instant::now());
    }
}

/// macOS password managers stamp `org.nspasteboard.ConcealedType`
/// on the general pasteboard so cooperating apps skip syncing
/// passwords. Returns `true` when that UTI is present on the
/// current pasteboard contents. Always `false` on non-macOS.
///
/// This is the standard "nspasteboard.com" convention — see
/// <https://nspasteboard.org/>.
#[cfg(target_os = "macos")]
fn is_concealed_clipboard() -> bool {
    use objc2_app_kit::NSPasteboard;
    use objc2_foundation::NSString;

    let pasteboard = NSPasteboard::generalPasteboard();
    let Some(types) = pasteboard.types() else {
        return false;
    };
    let concealed = NSString::from_str("org.nspasteboard.ConcealedType");
    types.iter().any(|t| t.isEqualToString(&concealed))
}

#[cfg(not(target_os = "macos"))]
fn is_concealed_clipboard() -> bool {
    false
}

/// If [`frontmost_app::frontmost_app()`] reports an app whose ident
/// is in the suppression list, return that ident. Otherwise return
/// `None`. Snapshotting the lock guard short keeps us from holding
/// the mutex across the platform call (which on Linux can shell
/// out to hyprctl/swaymsg).
fn is_suppressed(list: &SuppressionList) -> Option<AppIdent> {
    let snapshot: Vec<AppIdent> = {
        let Ok(guard) = list.lock() else {
            log::debug!("clipboard suppression: lock poisoned");
            return None;
        };
        if guard.is_empty() {
            log::debug!("clipboard suppression: list is empty");
            return None;
        }
        guard.iter().cloned().collect()
    };
    let active = frontmost_app::frontmost_app();
    log::debug!("clipboard suppression check: list={snapshot:?} active={active:?}");
    let active = active?;
    snapshot.into_iter().find(|s| s.matches(&active))
}

/// Returns `true` the first time it's called, and on every later
/// call where `NSPasteboard.generalPasteboard.changeCount` has
/// advanced since the previous call. Used as a cheap precheck so
/// the polling loop only invokes `arboard::Clipboard::get_text`
/// (which round-trips through `pboardd` via XPC) on ticks where
/// the pasteboard actually mutated.
///
/// `changeCount` reads are an Apple-blessed background-thread
/// operation — the property is designed for exactly this kind of
/// polling. No autorelease pool needed: the return value is a
/// primitive `NSInteger`, not an Objective-C object.
#[cfg(target_os = "macos")]
fn pasteboard_change_count_advanced() -> bool {
    use objc2_app_kit::NSPasteboard;
    use std::sync::atomic::{AtomicI64, Ordering};

    // Initial sentinel `i64::MIN` ensures the first call always
    // returns `true` so we read once at startup to seed the
    // diff-against-`last_content` machinery downstream.
    static LAST: AtomicI64 = AtomicI64::new(i64::MIN);

    let pb = NSPasteboard::generalPasteboard();
    let now = pb.changeCount() as i64;
    let prev = LAST.swap(now, Ordering::Relaxed);
    prev != now
}

/// Outcome of a single clipboard-poll cycle. Pulled into a pure
/// function so the focus-race invariant — "advance `last_content`
/// on every state-changing decision, even when we suppress" — is
/// pinned by unit tests rather than implicit in the closure
/// body.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PollDecision {
    /// Current text equals last recorded — nothing to do.
    Unchanged,
    /// Content changed but the 200 ms debounce window from the
    /// previous advance hasn't elapsed yet.
    Debounced,
    /// Content changed, debounce passed, but suppression
    /// (concealed pasteboard or app-list match) tells us not to
    /// emit. State must still advance so subsequent polls don't
    /// re-check the same content and leak on a focus shift.
    Suppressed,
    /// Content changed, debounce passed, suppression cleared.
    /// Caller emits the event and advances state.
    Emit,
}

impl PollDecision {
    /// Cheap pre-flight: `true` when the next call to
    /// [`PollDecision::classify`] could return `Suppressed` or
    /// `Emit`. The poll loop uses this to skip the relatively
    /// expensive `is_concealed_clipboard()` / `frontmost_app()` /
    /// `is_suppressed()` calls when the content hasn't changed
    /// or the debounce window blocks emission anyway.
    fn content_might_emit(
        current_text: &str,
        last_content: Option<&str>,
        last_change_elapsed: Option<Duration>,
    ) -> bool {
        if last_content == Some(current_text) {
            return false;
        }
        last_change_elapsed.is_none_or(|d| d > Duration::from_millis(200))
    }

    /// Decide what to do with this poll cycle. Pure function — no
    /// I/O, no global state — so the focus-race invariant is
    /// expressible as a series of `assert_eq!` calls.
    fn classify(
        current_text: &str,
        last_content: Option<&str>,
        last_change_elapsed: Option<Duration>,
        concealed: bool,
        suppressed_match: bool,
    ) -> Self {
        if last_content == Some(current_text) {
            return Self::Unchanged;
        }
        let debounce_passed = last_change_elapsed.is_none_or(|d| d > Duration::from_millis(200));
        if !debounce_passed {
            return Self::Debounced;
        }
        if concealed || suppressed_match {
            Self::Suppressed
        } else {
            Self::Emit
        }
    }

    /// True when this decision must advance `last_content` /
    /// `last_change` in the caller. Both `Suppressed` and `Emit`
    /// are state-changing — the focus-race fix lives here.
    fn advances_state(self) -> bool {
        matches!(self, Self::Suppressed | Self::Emit)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(target_os = "macos")]
    #[test]
    fn mac_fence_is_shared_nonblocking_and_fails_closed() {
        let echo = EchoSuppression::default();
        assert!(echo.register_macos_write_generation(41));
        assert!(!echo.clone().macos_generation_allows(41));
        assert!(echo.clone().macos_generation_allows(42));
        let _guard = echo.mac_fence.lock().unwrap();
        assert!(!echo.register_macos_write_generation(43));
        assert!(!echo.macos_generation_allows(42));
    }

    #[test]
    fn mac_fence_drops_remote_ownership_but_admits_next_local_copy() {
        let mut fence = MacCaptureFence::default();
        assert!(fence.allows(40));
        assert!(fence.register(41));
        assert!(!fence.allows(40)); // delayed worker, never republish old data
        assert!(!fence.allows(41)); // same owner may mutate without changeCount
        assert!(fence.allows(42)); // next local ownership, even retained marker
        assert!(fence.register(43));
        assert!(fence.register(41)); // reordered receipt cannot lower fence
        assert!(!fence.allows(42));
        assert!(!fence.allows(43));
        assert!(fence.allows(44));
        assert!(!fence.register(0));
        assert!(!fence.register(u64::MAX));
        assert!(!fence.allows(0));
        assert!(!fence.allows(u64::MAX));
        // No expiry: slow reads of our remote ownership cannot escape after TTL.
        assert!(!fence.allows(43));
    }

    fn sample(generation: u64, format: ClipboardFormat, bytes: &[u8]) -> ClipboardSample {
        ClipboardSample {
            native_generation: generation,
            format,
            bytes: bytes.to_vec(),
            #[cfg(windows)]
            windows_provenance: Some(WindowsProvenance {
                native_format: if format == ClipboardFormat::Text {
                    13
                } else {
                    0xc001
                },
                marker: [7; 32],
            }),
        }
    }

    #[test]
    fn echo_requires_exact_provenance_not_matching_external_content() {
        let echo = EchoSuppression::default();
        let written = sample(41, ClipboardFormat::Text, b"hello");
        assert!(echo.register(written.clone()));
        assert!(echo.contains(&written));
        assert!(!echo.contains(&sample(42, ClipboardFormat::Text, b"hello")));
        assert!(!echo.contains(&sample(41, ClipboardFormat::Text, b"other")));
        assert!(!echo.contains(&sample(41, ClipboardFormat::Png, b"hello")));
        assert!(!echo.register(sample(0, ClipboardFormat::Text, b"hello")));
    }

    #[cfg(windows)]
    #[test]
    fn windows_echo_requires_marker_native_format_and_complete_receipt() {
        let echo = EchoSuppression::default();
        let written = sample(41, ClipboardFormat::Text, b"private");
        assert!(echo.register(written.clone()));
        for change in 0..5 {
            let mut external = written.clone();
            match change {
                0 => external.windows_provenance = None,
                1 => external.windows_provenance.as_mut().unwrap().marker[0] ^= 1,
                2 => external.windows_provenance.as_mut().unwrap().native_format = 8,
                3 => external.native_generation += 1,
                _ => external.bytes.push(b'!'),
            }
            assert!(!echo.contains(&external));
        }
        let mut missing = written.clone();
        missing.windows_provenance = None;
        assert!(!echo.register(missing));
        let mut wrong_format = written;
        wrong_format
            .windows_provenance
            .as_mut()
            .unwrap()
            .native_format = 8;
        assert!(!echo.register(wrong_format));
    }

    #[test]
    fn echo_storage_is_bounded_shared_and_expiring() {
        let echo = EchoSuppression::default();
        for generation in 1..=3 {
            assert!(echo.register(sample(generation, ClipboardFormat::Text, b"hello")));
        }
        assert_eq!(echo.inner.lock().unwrap().len(), ECHO_ENTRIES);
        assert!(!echo.contains(&sample(1, ClipboardFormat::Text, b"hello")));
        assert!(
            echo.clone()
                .contains(&sample(3, ClipboardFormat::Text, b"hello"))
        );
        for entry in echo.inner.lock().unwrap().iter_mut() {
            entry.inserted = Instant::now() - ECHO_TTL - Duration::from_secs(1);
        }
        assert!(!echo.contains(&sample(3, ClipboardFormat::Text, b"hello")));
        assert!(!echo.register(sample(
            4,
            ClipboardFormat::Text,
            &vec![0; MAX_TEXT_BYTES + 1]
        )));
    }

    #[test]
    fn echo_contention_never_waits_and_fails_closed() {
        let echo = EchoSuppression::default();
        let _guard = echo.inner.lock().unwrap();
        assert!(!echo.register(sample(2, ClipboardFormat::Text, b"hello")));
        assert!(echo.contains(&sample(2, ClipboardFormat::Text, b"hello")));
    }

    #[test]
    fn classify_unchanged_when_text_matches_last_content() {
        let d = PollDecision::classify(
            "secret",
            Some("secret"),
            Some(Duration::from_secs(60)),
            false,
            false,
        );
        assert_eq!(d, PollDecision::Unchanged);
        assert!(!d.advances_state());
    }

    #[test]
    fn classify_debounced_when_recent_change() {
        // Content differs but only 100ms since last advance —
        // hold off so a peer's set_clipboard echo doesn't bounce
        // back as a fresh local emit.
        let d = PollDecision::classify(
            "new",
            Some("old"),
            Some(Duration::from_millis(100)),
            false,
            false,
        );
        assert_eq!(d, PollDecision::Debounced);
        assert!(!d.advances_state());
    }

    #[test]
    fn classify_emit_when_changed_unsuppressed() {
        let d = PollDecision::classify(
            "new",
            Some("old"),
            Some(Duration::from_secs(1)),
            false,
            false,
        );
        assert_eq!(d, PollDecision::Emit);
        assert!(d.advances_state());
    }

    #[test]
    fn classify_emit_on_first_change() {
        // No prior advance: the first poll always falls through
        // to Emit (assuming nothing's suppressed) so the very
        // first clipboard read after startup is broadcast.
        let d = PollDecision::classify("first", None, None, false, false);
        assert_eq!(d, PollDecision::Emit);
    }

    #[test]
    fn classify_suppressed_when_concealed() {
        let d = PollDecision::classify(
            "password",
            Some("plain"),
            Some(Duration::from_secs(1)),
            true,
            false,
        );
        assert_eq!(d, PollDecision::Suppressed);
    }

    #[test]
    fn classify_suppressed_when_app_list_matches() {
        let d = PollDecision::classify(
            "password",
            Some("plain"),
            Some(Duration::from_secs(1)),
            false,
            true,
        );
        assert_eq!(d, PollDecision::Suppressed);
    }

    /// The focus-race invariant: a Suppressed decision MUST tell
    /// the caller to advance `last_content`. If this assert
    /// fails, the regression we hit live (1Password password
    /// leaks on Ghostty alt-tab after copy) is back.
    #[test]
    fn suppressed_decision_advances_state() {
        let d = PollDecision::classify(
            "password",
            Some("plain"),
            Some(Duration::from_secs(1)),
            false,
            true,
        );
        assert!(
            d.advances_state(),
            "Suppressed must advance last_content; otherwise the \
             same content gets re-checked on the next poll and a \
             focus shift between polls will leak the password."
        );
    }

    /// Companion: Unchanged and Debounced must NOT advance state.
    /// Advancing on Debounced would defeat the 200 ms window and
    /// turn every peer-driven clipboard sync into a fresh local
    /// emit (echo loop).
    #[test]
    fn non_state_changing_decisions_do_not_advance() {
        for (current, last, elapsed) in [
            ("same", Some("same"), Some(Duration::from_secs(1))),
            ("new", Some("old"), Some(Duration::from_millis(50))),
        ] {
            let d = PollDecision::classify(current, last, elapsed, false, false);
            assert!(
                !d.advances_state(),
                "{d:?} should not advance state (current={current:?} last={last:?} elapsed={elapsed:?})"
            );
        }
    }

    #[test]
    fn content_might_emit_skips_unchanged_and_debounced() {
        // Used by the poll loop to short-circuit the expensive
        // suppression probes. Same matrix as `classify`'s "no
        // emission possible" cases.
        assert!(!PollDecision::content_might_emit(
            "same",
            Some("same"),
            None
        ));
        assert!(!PollDecision::content_might_emit(
            "new",
            Some("old"),
            Some(Duration::from_millis(50))
        ));
        assert!(PollDecision::content_might_emit(
            "new",
            Some("old"),
            Some(Duration::from_secs(1))
        ));
        assert!(PollDecision::content_might_emit("new", None, None));
    }
}
