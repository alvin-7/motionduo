//! Explicit Windows/macOS PNG write foundation. No event/transport integration.
use super::{ClipboardEmulation, ClipboardError};
use input_event::clipboard_png::{MAX_PNG_BYTES, decode_png};
use std::borrow::Cow;
use tokio::sync::Semaphore;

static WORKER: Semaphore = Semaphore::const_new(1);

impl ClipboardEmulation {
    /// Synchronous PNG publication for a caller that already owns a blocking
    /// worker. This deliberately has no lifecycle guard; the focus backend
    /// performs its generation check immediately before calling it.
    pub fn set_png_sync(&self, png: &[u8]) -> Result<(), ClipboardError> {
        if png.len() > MAX_PNG_BYTES {
            return Err(ClipboardError::Set("PNG exceeds image limit".into()));
        }
        let image = decode_png(png).map_err(|e| ClipboardError::Set(e.to_string()))?;
        let mut clipboard = self
            .clipboard
            .lock()
            .map_err(|_| ClipboardError::Access("clipboard lock poisoned".into()))?;
        if clipboard.is_none() {
            *clipboard =
                Some(arboard::Clipboard::new().map_err(|e| ClipboardError::Access(e.to_string()))?);
        }
        clipboard
            .as_mut()
            .expect("clipboard was just initialized")
            .set_image(arboard::ImageData {
                width: image.width() as usize,
                height: image.height() as usize,
                bytes: Cow::Borrowed(image.pixels()),
            })
            .map_err(|e| ClipboardError::Set(e.to_string()))
    }

    /// Decode bounded PNG off-executor, then call `commit_guard` immediately
    /// before the native write, while holding the emulation clipboard mutex.
    ///
    /// The caller MUST atomically validate consent/session/local-generation in
    /// this callback and return its shared lifecycle lock guard. That guard is
    /// held through the native write; revocation and local-change publication
    /// must acquire the same lock. A check before spawning is NOT sufficient.
    /// Returning None cancels without touching the OS clipboard. There is no
    /// permissive convenience overload. This serializes application state, not
    /// unrelated OS applications: external copies can still race native writes.
    ///
    /// PNG encoding/native conversion is bounded by codec dimensions (64 MiB
    /// RGBA); arboard publishes platform-native image formats. Only one image
    /// worker in this process is admitted, even if its caller is cancelled.
    pub async fn set_png_checked<F, G>(
        &self,
        png: Vec<u8>,
        commit_guard: F,
    ) -> Result<bool, ClipboardError>
    where
        F: FnOnce() -> Option<G> + Send + 'static,
    {
        if png.len() > MAX_PNG_BYTES {
            return Err(ClipboardError::Set("PNG exceeds image limit".into()));
        }
        let permit = WORKER
            .try_acquire()
            .map_err(|_| ClipboardError::Set("image worker busy".into()))?;
        permit.forget();
        let clipboard = self.clipboard.clone();
        tokio::task::spawn_blocking(move || {
            struct Release;
            impl Drop for Release {
                fn drop(&mut self) {
                    WORKER.add_permits(1);
                }
            }
            let _release = Release;
            let image = decode_png(&png).map_err(|e| ClipboardError::Set(e.to_string()))?;
            drop(png);
            let mut clipboard = clipboard
                .lock()
                .map_err(|_| ClipboardError::Access("clipboard lock poisoned".into()))?;
            if clipboard.is_none() {
                *clipboard = Some(
                    arboard::Clipboard::new().map_err(|e| ClipboardError::Access(e.to_string()))?,
                );
            }
            commit_if_current(commit_guard, || {
                clipboard
                    .as_mut()
                    .unwrap()
                    .set_image(arboard::ImageData {
                        width: image.width() as usize,
                        height: image.height() as usize,
                        bytes: Cow::Borrowed(image.pixels()),
                    })
                    .map_err(|e| ClipboardError::Set(e.to_string()))
            })
        })
        .await
        .map_err(|_| ClipboardError::Access("image worker failed".into()))?
    }
}

fn commit_if_current<G>(
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
    use std::sync::{Arc, Mutex};
    #[test]
    fn revoked_commit_never_calls_native_writer() {
        assert!(!commit_if_current(|| None::<()>, || panic!("stale native write")).unwrap());
    }
    #[test]
    fn authorization_lock_is_held_until_write_finishes() {
        let state = Arc::new(Mutex::new(7));
        assert!(
            commit_if_current(
                || Some(state.lock().unwrap()),
                || {
                    assert!(state.try_lock().is_err());
                    Ok(())
                }
            )
            .unwrap()
        );
        assert!(state.try_lock().is_ok());
    }
    #[test]
    fn write_error_releases_authorization_lock() {
        let state = Mutex::new(());
        assert!(
            commit_if_current(
                || Some(state.lock().unwrap()),
                || Err(ClipboardError::Set("injected".into()))
            )
            .is_err()
        );
        assert!(state.try_lock().is_ok());
    }
}
