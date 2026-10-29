//! Windows clipboard transactions. Receipts are sampled and checked while locked.
#![cfg(windows)]
use super::ClipboardError;
#[cfg(feature = "clipboard-image")]
use input_event::clipboard_png::{MAX_PNG_BYTES, decode_png};

pub const PROVENANCE_FORMAT: &str = "Mousehop.Clipboard.Provenance.v1";
const CF_UNICODETEXT: u32 = 13;

/// A verified native publication, not a post-write sequence-number guess.
/// Fields are private so callers cannot construct receipts for unrelated writes.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NativeWriteReceipt {
    generation: u64,
    marker: [u8; 32],
    format: u32,
    content: Vec<u8>,
}
impl NativeWriteReceipt {
    pub fn generation(&self) -> u64 {
        self.generation
    }
    pub fn marker(&self) -> &[u8; 32] {
        &self.marker
    }
    pub fn native_format(&self) -> u32 {
        self.format
    }
    /// Original UTF-8 text or original PNG, matching capture's sample bytes.
    pub fn content(&self) -> &[u8] {
        &self.content
    }
}
#[link(name = "user32")]
unsafe extern "system" {
    fn OpenClipboard(owner: isize) -> i32;
    fn CloseClipboard() -> i32;
    fn EmptyClipboard() -> i32;
    fn SetClipboardData(format: u32, data: isize) -> isize;
    fn GetClipboardData(format: u32) -> isize;
    fn RegisterClipboardFormatW(name: *const u16) -> u32;
    fn GetClipboardSequenceNumber() -> u32;
    fn CreateWindowExW(
        ex: u32,
        class: *const u16,
        name: *const u16,
        style: u32,
        x: i32,
        y: i32,
        w: i32,
        h: i32,
        parent: isize,
        menu: isize,
        instance: isize,
        param: *const std::ffi::c_void,
    ) -> isize;
    fn DestroyWindow(window: isize) -> i32;
}
#[link(name = "kernel32")]
unsafe extern "system" {
    fn GlobalAlloc(flags: u32, bytes: usize) -> isize;
    fn GlobalLock(handle: isize) -> *mut u8;
    fn GlobalUnlock(handle: isize) -> i32;
    fn GlobalFree(handle: isize) -> isize;
    fn GlobalSize(handle: isize) -> usize;
}
#[link(name = "bcrypt")]
unsafe extern "system" {
    fn BCryptGenRandom(algorithm: isize, buffer: *mut u8, length: u32, flags: u32) -> i32;
}
fn error(op: &str) -> ClipboardError {
    ClipboardError::Set(format!("Windows {op}: {}", std::io::Error::last_os_error()))
}
fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(Some(0)).collect()
}

fn prepare_text(text: &str) -> Result<Vec<u8>, ClipboardError> {
    if text.is_empty() || text.len() > 1024 * 1024 || text.contains('\0') {
        return Err(ClipboardError::Set("invalid native text payload".into()));
    }
    Ok(wide(text).into_iter().flat_map(u16::to_le_bytes).collect())
}

#[cfg(feature = "clipboard-image")]
fn prepare_png(png: &[u8]) -> Result<Vec<u8>, ClipboardError> {
    if png.len() > MAX_PNG_BYTES {
        return Err(ClipboardError::Set("PNG exceeds image limit".into()));
    }
    // Decode before opening the clipboard so image work cannot widen the
    // generation-check critical section.
    decode_png(png).map_err(|e| ClipboardError::Set(e.to_string()))?;
    Ok(png.to_vec())
}

fn register(name: &str) -> Result<u32, ClipboardError> {
    let value = unsafe { RegisterClipboardFormatW(wide(name).as_ptr()) };
    if value == 0 {
        Err(error("RegisterClipboardFormatW"))
    } else {
        Ok(value)
    }
}

fn open_with_retry(owner: isize) -> Result<Open, ClipboardError> {
    for attempt in 0..10 {
        if unsafe { OpenClipboard(owner) } != 0 {
            return Ok(Open);
        }
        if attempt != 9 {
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
    }
    Err(error("OpenClipboard"))
}

struct Owner(isize);
impl Drop for Owner {
    fn drop(&mut self) {
        unsafe {
            DestroyWindow(self.0);
        }
    }
}
struct Open;
impl Drop for Open {
    fn drop(&mut self) {
        unsafe {
            CloseClipboard();
        }
    }
}
struct Memory(isize);
impl Memory {
    fn new(bytes: &[u8]) -> Result<Self, ClipboardError> {
        let memory = Self(unsafe { GlobalAlloc(0x0042, bytes.len()) });
        if memory.0 == 0 {
            return Err(error("GlobalAlloc"));
        }
        let target = unsafe { GlobalLock(memory.0) };
        if target.is_null() {
            return Err(error("GlobalLock"));
        }
        unsafe {
            std::ptr::copy_nonoverlapping(bytes.as_ptr(), target, bytes.len());
            GlobalUnlock(memory.0);
        }
        Ok(memory)
    }
    fn publish(mut self, format: u32) -> Result<(), ClipboardError> {
        if unsafe { SetClipboardData(format, self.0) } == 0 {
            return Err(error("SetClipboardData"));
        }
        self.0 = 0; // Ownership transferred to Windows only on success.
        Ok(())
    }
}
impl Drop for Memory {
    fn drop(&mut self) {
        if self.0 != 0 {
            unsafe {
                GlobalFree(self.0);
            }
        }
    }
}
fn matches_bytes(format: u32, expected: &[u8]) -> bool {
    let memory = unsafe { GetClipboardData(format) };
    if memory == 0 || unsafe { GlobalSize(memory) } < expected.len() {
        return false;
    }
    let bytes = unsafe { GlobalLock(memory) };
    if bytes.is_null() {
        return false;
    }
    let equal = unsafe { std::slice::from_raw_parts(bytes, expected.len()) == expected };
    unsafe {
        GlobalUnlock(memory);
    }
    equal
}

/// All publication and verification are inside one OS lock. If a later format
/// fails, clear our partially published clipboard; never issue a success receipt.
fn publish<G>(
    native: Vec<u8>,
    content: Vec<u8>,
    format: u32,
    guard: impl FnOnce() -> Option<G>,
    registered: impl FnOnce(&NativeWriteReceipt) -> bool,
) -> Result<Option<NativeWriteReceipt>, ClipboardError> {
    let mut marker = [0; 32];
    if unsafe { BCryptGenRandom(0, marker.as_mut_ptr(), 32, 2) } != 0 {
        return Err(error("BCryptGenRandom"));
    }
    let marker_format = register(PROVENANCE_FORMAT)?;
    let payload = Memory::new(&native)?;
    let marker_memory = Memory::new(&marker)?;
    // A real owner is necessary: EmptyClipboard after OpenClipboard(NULL)
    // leaves no owner and SetClipboardData then fails on Windows.
    let owner = Owner(unsafe {
        CreateWindowExW(
            0,
            wide("STATIC").as_ptr(),
            wide("").as_ptr(),
            0,
            0,
            0,
            0,
            0,
            -3,
            0,
            0,
            std::ptr::null(),
        )
    });
    if owner.0 == 0 {
        return Err(error("CreateWindowExW"));
    }
    let Some(_lifecycle) = guard() else {
        return Ok(None);
    };
    if unsafe { OpenClipboard(owner.0) } == 0 {
        return Err(error("OpenClipboard"));
    }
    let _open = Open;
    if unsafe { EmptyClipboard() } == 0 {
        return Err(error("EmptyClipboard"));
    }
    let result = (|| {
        payload.publish(format)?;
        marker_memory.publish(marker_format)?;
        let generation = unsafe { GetClipboardSequenceNumber() as u64 };
        if generation == 0
            || !matches_bytes(marker_format, &marker)
            || !matches_bytes(format, &native)
            || unsafe { GetClipboardSequenceNumber() as u64 } != generation
        {
            return Err(error("publication verification"));
        }
        let receipt = NativeWriteReceipt {
            generation,
            marker,
            format,
            content,
        };
        // Caller installs echo state under the same lifecycle guard before either
        // lock is released, even if the async waiter has been cancelled.
        if !registered(&receipt) {
            return Err(error("receipt registration rejected"));
        }
        Ok(Some(receipt))
    })();
    if result.is_err() {
        unsafe {
            EmptyClipboard();
        }
    }
    result
}

pub fn set_text<G>(
    text: String,
    guard: impl FnOnce() -> Option<G>,
    registered: impl FnOnce(&NativeWriteReceipt) -> bool,
) -> Result<Option<NativeWriteReceipt>, ClipboardError> {
    let native = prepare_text(&text)?;
    publish(native, text.into_bytes(), CF_UNICODETEXT, guard, registered)
}

/// Publish text while holding the Windows clipboard lock across both sequence
/// checks and the native write. `None` means the expected generation was stale.
pub fn set_text_if_generation(
    text: &str,
    expected_generation: u64,
) -> Result<Option<u64>, ClipboardError> {
    let native = prepare_text(text)?;
    publish_if_generation(native, CF_UNICODETEXT, expected_generation)
}

#[cfg(feature = "clipboard-image")]
pub fn set_png<G>(
    png: Vec<u8>,
    guard: impl FnOnce() -> Option<G>,
    registered: impl FnOnce(&NativeWriteReceipt) -> bool,
) -> Result<Option<NativeWriteReceipt>, ClipboardError> {
    let native = prepare_png(&png)?;
    // Publish the interoperable registered PNG format byte-for-byte. BI_RGB
    // CF_DIB does not define alpha semantics; do not silently destroy transparency.
    let format = register("PNG")?;
    publish(native, png, format, guard, registered)
}

#[cfg(feature = "clipboard-image")]
pub fn set_png_if_generation(
    png: &[u8],
    expected_generation: u64,
) -> Result<Option<u64>, ClipboardError> {
    let native = prepare_png(png)?;
    let format = register("PNG")?;
    publish_if_generation(native, format, expected_generation)
}

fn publish_if_generation(
    native: Vec<u8>,
    format: u32,
    expected_generation: u64,
) -> Result<Option<u64>, ClipboardError> {
    // All allocations and format conversion happen before OpenClipboard.
    let payload = Memory::new(&native)?;
    let owner = Owner(unsafe {
        CreateWindowExW(
            0,
            wide("STATIC").as_ptr(),
            wide("").as_ptr(),
            0,
            0,
            0,
            0,
            0,
            -3,
            0,
            0,
            std::ptr::null(),
        )
    });
    if owner.0 == 0 {
        return Err(error("CreateWindowExW"));
    }
    let _open = open_with_retry(owner.0)?;
    let current = unsafe { GetClipboardSequenceNumber() as u64 };
    if current != expected_generation {
        return Ok(None);
    }
    if unsafe { EmptyClipboard() } == 0 {
        return Err(error("EmptyClipboard"));
    }
    payload.publish(format)?;
    // Verify the payload while the clipboard remains locked. Windows may
    // advance its sequence number again when the lock is released, so the
    // in-lock generation is only useful if the content is still ours.
    if !matches_bytes(format, &native) {
        return Err(error("GetClipboardData readback"));
    }
    // Keep this read inside the OpenClipboard critical section. A later local
    // copy cannot therefore be mistaken for the generation we wrote.
    Ok(Some(unsafe { GetClipboardSequenceNumber() as u64 }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prepares_utf16_with_terminator_before_native_access() {
        let bytes = prepare_text("hello").unwrap();
        assert_eq!(bytes.len(), 12);
        assert_eq!(&bytes[10..], &[0, 0]);
    }

    #[test]
    fn native_random_markers_are_distinct() {
        let mut a = [0; 32];
        let mut b = [0; 32];
        assert_eq!(unsafe { BCryptGenRandom(0, a.as_mut_ptr(), 32, 2) }, 0);
        assert_eq!(unsafe { BCryptGenRandom(0, b.as_mut_ptr(), 32, 2) }, 0);
        assert_ne!(a, b);
    }
    #[test]
    fn rejects_nul_and_oversize_before_native_access() {
        assert!(prepare_text("a\0b").is_err());
        assert!(prepare_text(&"x".repeat(1024 * 1024 + 1)).is_err());
    }

    #[cfg(feature = "clipboard-image")]
    #[test]
    fn rejects_oversize_png_before_native_access() {
        assert!(prepare_png(&vec![0; MAX_PNG_BYTES + 1]).is_err());
    }

    #[cfg(feature = "clipboard-image")]
    struct SequenceTrace {
        before_empty: u64,
        after_empty: u64,
        after_set: u64,
        before_close: u64,
        after_close: u64,
    }

    #[cfg(feature = "clipboard-image")]
    fn trace_real_write(native: &[u8], format: u32) -> Result<SequenceTrace, ClipboardError> {
        let payload = Memory::new(native)?;
        let owner = Owner(unsafe {
            CreateWindowExW(
                0,
                wide("STATIC").as_ptr(),
                wide("").as_ptr(),
                0,
                0,
                0,
                0,
                0,
                -3,
                0,
                0,
                std::ptr::null(),
            )
        });
        if owner.0 == 0 {
            return Err(error("CreateWindowExW"));
        }
        let open = open_with_retry(owner.0)?;
        let before_empty = unsafe { GetClipboardSequenceNumber() as u64 };
        if unsafe { EmptyClipboard() } == 0 {
            return Err(error("EmptyClipboard"));
        }
        let after_empty = unsafe { GetClipboardSequenceNumber() as u64 };
        payload.publish(format)?;
        let after_set = unsafe { GetClipboardSequenceNumber() as u64 };
        if !matches_bytes(format, native) {
            return Err(error("GetClipboardData readback"));
        }
        let before_close = unsafe { GetClipboardSequenceNumber() as u64 };
        drop(open);
        let after_close = unsafe { GetClipboardSequenceNumber() as u64 };
        Ok(SequenceTrace {
            before_empty,
            after_empty,
            after_set,
            before_close,
            after_close,
        })
    }

    #[cfg(feature = "clipboard-image")]
    #[test]
    #[ignore = "destructive native clipboard measurement; run explicitly on a disposable clipboard"]
    fn measures_real_sequence_drift_for_text_and_png() {
        let text = prepare_text("Mousehop sequence drift text").unwrap();
        let text_trace = trace_real_write(&text, CF_UNICODETEXT).unwrap();
        eprintln!(
            "Windows text sequence: before_empty={}, after_empty={}, after_set={}, before_close={}, after_close={}, close_delta={}",
            text_trace.before_empty,
            text_trace.after_empty,
            text_trace.after_set,
            text_trace.before_close,
            text_trace.after_close,
            text_trace
                .after_close
                .saturating_sub(text_trace.before_close),
        );

        let image =
            input_event::clipboard_png::RgbaImage::new(1, 1, vec![0x12, 0x34, 0x56, 0xff]).unwrap();
        let png = input_event::clipboard_png::encode_png(&image).unwrap();
        let png = prepare_png(&png).unwrap();
        let format = register("PNG").unwrap();
        let png_trace = trace_real_write(&png, format).unwrap();
        eprintln!(
            "Windows PNG sequence: before_empty={}, after_empty={}, after_set={}, before_close={}, after_close={}, close_delta={}",
            png_trace.before_empty,
            png_trace.after_empty,
            png_trace.after_set,
            png_trace.before_close,
            png_trace.after_close,
            png_trace.after_close.saturating_sub(png_trace.before_close),
        );
    }

    #[test]
    fn revoked_write_never_registers() {
        assert!(
            set_text(
                "revoked".into(),
                || None::<()>,
                |_| panic!("revoked receipt")
            )
            .unwrap()
            .is_none()
        );
    }
    #[test]
    #[ignore = "destructive native clipboard test; run explicitly on a disposable clipboard"]
    fn native_text_receipt_is_verified_with_lifecycle_lock() {
        let state = std::sync::Mutex::new(());
        let receipt = set_text(
            "Mousehop receipt test 🦀".into(),
            || Some(state.lock().unwrap()),
            |receipt| {
                assert!(state.try_lock().is_err());
                assert_ne!(receipt.generation(), 0);
                assert!(matches_bytes(
                    register(PROVENANCE_FORMAT).unwrap(),
                    receipt.marker()
                ));
                true
            },
        )
        .unwrap()
        .unwrap();
        assert_eq!(receipt.content(), "Mousehop receipt test 🦀".as_bytes());
    }
}
