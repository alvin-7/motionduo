#[cfg(all(feature = "clipboard-image", any(windows, target_os = "macos")))]
#[path = "clipboard_image.rs"]
pub mod image;

use arboard::Clipboard;
use input_event::ClipboardEvent;
use std::sync::{Arc, Mutex};
use thiserror::Error;
use tokio::task::spawn_blocking;

#[cfg(windows)]
#[path = "windows_clipboard.rs"]
mod windows_clipboard;
#[cfg(windows)]
pub use windows_clipboard::NativeWriteReceipt;
#[cfg(all(target_os = "macos", feature = "clipboard-image"))]
#[path = "clipboard_macos.rs"]
mod clipboard_macos;
#[cfg(all(target_os = "macos", feature = "clipboard-image"))]
pub use clipboard_macos::{Format as MacClipboardFormat, WriteReceipt as MacWriteReceipt};

#[derive(Debug, Error)]
pub enum ClipboardError {
    #[error("Failed to access clipboard: {0}")]
    Access(String),
    #[error("Failed to set clipboard: {0}")]
    Set(String),
}

/// Clipboard emulation that sets clipboard content
#[derive(Clone)]
pub struct ClipboardEmulation {
    // Use Arc<Mutex<>> to share clipboard across threads
    clipboard: Arc<Mutex<Option<Clipboard>>>,
}

impl ClipboardEmulation {
    /// Whether this adapter can prove the native generation of its own write.
    ///
    /// This check never opens or writes the clipboard. A successful checked write
    /// only proves lifecycle admission and native API success, not provenance.
    /// Do not register capture echoes from a generation sampled after that write.
    pub const fn verified_native_write_receipt_support() -> Result<(), &'static str> {
        #[cfg(windows)]
        {
            Err(
                "Windows arboard writes do not expose the native clipboard transaction or a verified write sequence receipt",
            )
        }
        #[cfg(target_os = "macos")]
        {
            Err(
                "macOS arboard writes do not bind pasteboard publication to a verified change-count receipt",
            )
        }
        #[cfg(not(any(windows, target_os = "macos")))]
        {
            Err("verified native clipboard write receipts are not implemented on this platform")
        }
    }

    pub fn new() -> Result<Self, ClipboardError> {
        // Try to create initial clipboard instance
        let clipboard = match Clipboard::new() {
            Ok(c) => Some(c),
            Err(e) => {
                log::warn!("Failed to create clipboard instance: {e}");
                None
            }
        };

        Ok(Self {
            clipboard: Arc::new(Mutex::new(clipboard)),
        })
    }

    /// Set clipboard content from a clipboard event unconditionally.
    pub async fn set(&self, event: ClipboardEvent) -> Result<(), ClipboardError> {
        self.set_checked(event, || Some(())).await.map(|_| ())
    }

    /// Synchronous counterpart for callers that already run on a blocking
    /// worker, such as the focus-sync native adapter.
    pub fn set_text_sync(&self, text: &str) -> Result<(), ClipboardError> {
        let mut clipboard_guard = self
            .clipboard
            .lock()
            .map_err(|_| ClipboardError::Access("clipboard lock poisoned".into()))?;
        let clipboard = match clipboard_guard.as_mut() {
            Some(clipboard) => clipboard,
            None => {
                let clipboard =
                    Clipboard::new().map_err(|error| ClipboardError::Access(format!("{error}")))?;
                *clipboard_guard = Some(clipboard);
                clipboard_guard.as_mut().expect("clipboard was just stored")
            }
        };
        clipboard
            .set_text(text)
            .map_err(|error| ClipboardError::Set(format!("{error}")))
    }

    /// Publish text only when the native clipboard still has `expected_generation`.
    /// The emulation mutex is held across the native transaction so this adapter
    /// cannot run two generation-fenced writes concurrently.
    pub fn set_text_if_generation(
        &self,
        text: &str,
        expected_generation: u64,
    ) -> Result<Option<u64>, ClipboardError> {
        let _clipboard_guard = self
            .clipboard
            .lock()
            .map_err(|_| ClipboardError::Access("clipboard lock poisoned".into()))?;

        #[cfg(windows)]
        {
            windows_clipboard::set_text_if_generation(text, expected_generation)
        }
        #[cfg(all(target_os = "macos", feature = "clipboard-image"))]
        {
            clipboard_macos::write_if_generation(
                clipboard_macos::Format::Text,
                text.as_bytes(),
                expected_generation,
            )
            .map_err(|error| ClipboardError::Set(error.to_string()))
        }
        #[cfg(all(target_os = "macos", not(feature = "clipboard-image")))]
        {
            let _ = (text, expected_generation);
            Err(ClipboardError::Set(
                "generation-fenced clipboard writing requires clipboard-image".into(),
            ))
        }
        #[cfg(not(any(windows, target_os = "macos")))]
        {
            let _ = (text, expected_generation);
            Err(ClipboardError::Set(
                "generation-fenced clipboard writing is unsupported on this platform".into(),
            ))
        }
    }

    /// Set text only if `commit_guard` returns a current lifecycle guard.
    ///
    /// The callback runs on the blocking worker immediately before the native
    /// write, with the emulation clipboard mutex held. It MUST atomically check
    /// consent/session/local-generation and return the shared lifecycle lock
    /// guard. Revocation and local-change publication must acquire that same
    /// lock; it is held until the native write finishes, including on error.
    /// Returning None cancels the write and returns false. A check before
    /// spawning is not sufficient, since a cancelled async caller does not stop
    /// an already running blocking worker.
    ///
    /// This fences application state only: unrelated applications can still
    /// change the OS clipboard independently. Native support and ownership
    /// semantics remain those of arboard on the current platform.
    pub async fn set_checked<F, G>(
        &self,
        event: ClipboardEvent,
        commit_guard: F,
    ) -> Result<bool, ClipboardError>
    where
        F: FnOnce() -> Option<G> + Send + 'static,
    {
        match event {
            ClipboardEvent::Text(text) => {
                let clipboard_arc = self.clipboard.clone();

                spawn_blocking(move || {
                    let mut clipboard_guard = clipboard_arc
                        .lock()
                        .map_err(|_| ClipboardError::Access("clipboard lock poisoned".into()))?;

                    // Try to get or create clipboard
                    let clipboard = match clipboard_guard.as_mut() {
                        Some(c) => c,
                        None => {
                            // Try to create a new clipboard instance
                            match Clipboard::new() {
                                Ok(c) => {
                                    *clipboard_guard = Some(c);
                                    clipboard_guard.as_mut().unwrap()
                                }
                                Err(e) => {
                                    return Err(ClipboardError::Access(format!("{e}")));
                                }
                            }
                        }
                    };

                    commit_text_if_current(commit_guard, || {
                        clipboard
                            .set_text(text.clone())
                            .map_err(|e| ClipboardError::Set(format!("{e}")))?;

                        log::debug!("Clipboard set, length: {} bytes", text.len());
                        Ok(())
                    })
                })
                .await
                .map_err(|e| ClipboardError::Access(format!("Task join error: {e}")))?
            }
        }
    }

    /// Publish and register a verified Windows receipt before releasing the native
    /// clipboard and lifecycle locks. `guard` must validate the current session,
    /// consent and local generation. `registered` must atomically install echo
    /// state without reacquiring that lifecycle lock; false fails the write.
    /// Cancellation of this async waiter does not cancel the worker or registration.
    #[cfg(windows)]
    pub async fn set_text_with_receipt_checked<F, G, R>(
        &self,
        text: String,
        guard: F,
        registered: R,
    ) -> Result<Option<NativeWriteReceipt>, ClipboardError>
    where
        F: FnOnce() -> Option<G> + Send + 'static,
        R: FnOnce(&NativeWriteReceipt) -> bool + Send + 'static,
    {
        let clipboard = self.clipboard.clone();
        spawn_blocking(move || {
            let _serial = clipboard
                .lock()
                .map_err(|_| ClipboardError::Access("clipboard lock poisoned".into()))?;
            windows_clipboard::set_text(text, guard, registered)
        })
        .await
        .map_err(|e| ClipboardError::Access(format!("Task join error: {e}")))?
    }

    /// Publish a PNG only when the native clipboard still has
    /// `expected_generation`. PNG preparation happens before the native check.
    pub fn set_png_if_generation(
        &self,
        png: &[u8],
        expected_generation: u64,
    ) -> Result<Option<u64>, ClipboardError> {
        let _clipboard_guard = self
            .clipboard
            .lock()
            .map_err(|_| ClipboardError::Access("clipboard lock poisoned".into()))?;

        #[cfg(all(windows, feature = "clipboard-image"))]
        {
            windows_clipboard::set_png_if_generation(png, expected_generation)
        }
        #[cfg(all(target_os = "macos", feature = "clipboard-image"))]
        {
            clipboard_macos::write_if_generation(
                clipboard_macos::Format::Png,
                png,
                expected_generation,
            )
            .map_err(|error| ClipboardError::Set(error.to_string()))
        }
        #[cfg(not(any(
            all(windows, feature = "clipboard-image"),
            all(target_os = "macos", feature = "clipboard-image")
        )))]
        {
            let _ = (png, expected_generation);
            Err(ClipboardError::Set(
                "generation-fenced PNG writing requires clipboard-image".into(),
            ))
        }
    }

    /// PNG counterpart of `set_text_with_receipt_checked`. Publishes registered
    /// PNG bytes losslessly (not alpha-ambiguous BI_RGB DIB). One worker admitted.
    #[cfg(all(windows, feature = "clipboard-image"))]
    pub async fn set_png_with_receipt_checked<F, G, R>(
        &self,
        png: Vec<u8>,
        guard: F,
        registered: R,
    ) -> Result<Option<NativeWriteReceipt>, ClipboardError>
    where
        F: FnOnce() -> Option<G> + Send + 'static,
        R: FnOnce(&NativeWriteReceipt) -> bool + Send + 'static,
    {
        static WORKER: tokio::sync::Semaphore = tokio::sync::Semaphore::const_new(1);
        let permit = WORKER
            .try_acquire()
            .map_err(|_| ClipboardError::Set("receipt image worker busy".into()))?;
        permit.forget();
        let clipboard = self.clipboard.clone();
        spawn_blocking(move || {
            struct Release;
            impl Drop for Release {
                fn drop(&mut self) {
                    WORKER.add_permits(1);
                }
            }
            let _release = Release;
            let _serial = clipboard
                .lock()
                .map_err(|_| ClipboardError::Access("clipboard lock poisoned".into()))?;
            windows_clipboard::set_png(png, guard, registered)
        })
        .await
        .map_err(|e| ClipboardError::Access(format!("Task join error: {e}")))?
    }

    /// Opt-in macOS text/PNG publication with directly verified ownership evidence.
    /// One process-wide blocking worker is admitted without queuing. Its owned
    /// permit survives async cancellation until the worker finishes or is dropped.
    /// `guard` must check consent/session/local generation and return the same
    /// lock used by revocation. It is held through native verification and the
    /// synchronous `registered` callback. The callback receives canonical bytes,
    /// must not reacquire that lock, and must return true only after echo state is
    /// installed. False reports an error but cannot retract an admitted OS write.
    /// Existing arboard APIs/capability queries are unchanged; no service uses
    /// this API. Native calls can block indefinitely and external apps can race.
    #[cfg(all(target_os = "macos", feature = "clipboard-image"))]
    pub async fn set_with_receipt_checked<F, G, R>(
        &self,
        format: MacClipboardFormat,
        bytes: Vec<u8>,
        guard: F,
        registered: R,
    ) -> Result<Option<MacWriteReceipt>, ClipboardError>
    where
        F: FnOnce() -> Option<G> + Send + 'static,
        G: Send + 'static,
        R: FnOnce(&MacWriteReceipt, &[u8]) -> bool + Send + 'static,
    {
        static WORKER: std::sync::LazyLock<Arc<tokio::sync::Semaphore>> =
            std::sync::LazyLock::new(|| Arc::new(tokio::sync::Semaphore::new(1)));
        let permit = WORKER
            .clone()
            .try_acquire_owned()
            .map_err(|_| ClipboardError::Set("receipt clipboard worker busy".into()))?;
        let clipboard = self.clipboard.clone();
        spawn_blocking(move || {
            let _permit = permit;
            let _serial = clipboard
                .lock()
                .map_err(|_| ClipboardError::Access("clipboard lock poisoned".into()))?;
            clipboard_macos::write_checked(format, &bytes, guard, registered)
                .map(|result| result.map(|(receipt, _)| receipt))
                .map_err(|error| ClipboardError::Set(error.to_string()))
        })
        .await
        .map_err(|e| ClipboardError::Access(format!("Task join error: {e}")))?
    }
    pub async fn get(&self) -> Result<String, ClipboardError> {
        let clipboard_arc = self.clipboard.clone();

        spawn_blocking(move || {
            let mut clipboard_guard = clipboard_arc.lock().unwrap();

            let clipboard = match clipboard_guard.as_mut() {
                Some(c) => c,
                None => match Clipboard::new() {
                    Ok(c) => {
                        *clipboard_guard = Some(c);
                        clipboard_guard.as_mut().unwrap()
                    }
                    Err(e) => {
                        return Err(ClipboardError::Access(format!("{e}")));
                    }
                },
            };

            clipboard
                .get_text()
                .map_err(|e| ClipboardError::Access(format!("{e}")))
        })
        .await
        .map_err(|e| ClipboardError::Access(format!("Task join error: {e}")))?
    }
}

fn commit_text_if_current<G>(
    guard: impl FnOnce() -> Option<G>,
    write: impl FnOnce() -> Result<(), ClipboardError>,
) -> Result<bool, ClipboardError> {
    let Some(guard) = guard() else {
        return Ok(false);
    };
    write()?;
    drop(guard);
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(all(target_os = "macos", feature = "clipboard-image"))]
    #[test]
    fn mac_receipt_api_cancellation_never_opens_pasteboard() {
        let _test = clipboard_macos::TEST_WRITER.lock().unwrap();
        // Construct without arboard initialization: cancellation must not access
        // the native pasteboard through either adapter.
        let writer = ClipboardEmulation {
            clipboard: Arc::new(Mutex::new(None)),
        };
        let runtime = tokio::runtime::Builder::new_current_thread()
            .build()
            .unwrap();
        assert!(
            runtime
                .block_on(writer.set_with_receipt_checked(
                    MacClipboardFormat::Text,
                    b"safe test".to_vec(),
                    || None::<()>,
                    |_, _| panic!("cancelled publication registered a receipt"),
                ))
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn verified_receipts_fail_closed_without_native_access() {
        let reason = ClipboardEmulation::verified_native_write_receipt_support().unwrap_err();
        assert!(!reason.is_empty());
        #[cfg(windows)]
        assert!(reason.contains("Windows"));
        #[cfg(all(target_os = "macos", feature = "clipboard-image"))]
        assert!(reason.contains("macOS"));
    }

    #[test]
    fn revoked_text_commit_skips_native_write() {
        assert!(!commit_text_if_current(|| None::<()>, || panic!("stale text write")).unwrap());
    }

    #[test]
    fn text_commit_holds_lifecycle_lock_through_write() {
        let state = Mutex::new(7);
        let mut writes = 0;
        assert!(
            commit_text_if_current(
                || Some(state.lock().unwrap()),
                || {
                    assert!(state.try_lock().is_err());
                    writes += 1;
                    Ok(())
                }
            )
            .unwrap()
        );
        assert_eq!(writes, 1);
        assert!(state.try_lock().is_ok());
    }

    #[test]
    fn text_write_error_releases_lifecycle_lock() {
        let state = Mutex::new(());
        let result = commit_text_if_current(
            || Some(state.lock().unwrap()),
            || {
                assert!(state.try_lock().is_err());
                Err(ClipboardError::Set("injected".into()))
            },
        );
        assert!(matches!(result, Err(ClipboardError::Set(message)) if message == "injected"));
        assert!(state.try_lock().is_ok());
    }

    #[tokio::test]
    async fn text_commit_rechecks_lifecycle_on_blocking_worker() {
        // No OS clipboard access: model revocation while a worker is queued.
        let current = Arc::new(Mutex::new(true));
        let (ready_tx, ready_rx) = tokio::sync::oneshot::channel();
        let (resume_tx, resume_rx) = std::sync::mpsc::channel();
        let worker_state = current.clone();
        let worker = spawn_blocking(move || {
            ready_tx.send(()).unwrap();
            resume_rx.recv().unwrap();
            commit_text_if_current(
                || {
                    let guard = worker_state.lock().unwrap();
                    (*guard).then_some(guard)
                },
                || panic!("revoked queued text write"),
            )
        });
        ready_rx.await.unwrap();
        *current.lock().unwrap() = false;
        resume_tx.send(()).unwrap();
        assert!(!worker.await.unwrap().unwrap());
    }
}
