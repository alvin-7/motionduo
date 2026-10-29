//! Opt-in native text/PNG sampling, independent of service and transport.
//! One source chooses PNG before text and consumes a generation even when
//! privacy rejects it. No claim of native-write ownership or authorization.
use super::{
    ClipboardFormat, ClipboardSample, EchoSuppression, MAX_TEXT_BYTES, SuppressionList,
    is_concealed_clipboard, is_suppressed, native_generation,
};
use input_event::clipboard_png::{MAX_PNG_BYTES, RgbaImage, decode_png, encode_png};
use std::sync::{Arc, Mutex};
use tokio::sync::Semaphore;

#[cfg(target_os = "macos")]
#[path = "clipboard_mac_file.rs"]
mod mac_file;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("image clipboard worker busy")]
    Busy,
    #[error("native clipboard unavailable or changed during sampling")]
    Unavailable,
    #[error("native image format unsupported or exceeds limits")]
    Unsupported,
    #[error(transparent)]
    Codec(#[from] input_event::clipboard_png::Error),
    #[error("image clipboard worker failed")]
    Worker,
}

/// No image cache or image event queue. A single outstanding sample is admitted;
/// the process-wide permit remains held if its async caller is cancelled.
#[derive(Clone)]
pub struct ImageReader {
    last_generation: Arc<Mutex<Option<u64>>>,
    suppression: SuppressionList,
    echo: EchoSuppression,
}

/// Unified text/PNG source. Clones share consumption and echo state; constructing
/// separate readers does not deduplicate generations across those readers.
pub type NativeClipboardReader = ImageReader;
static WORKER: Semaphore = Semaphore::const_new(1);

impl ImageReader {
    pub fn new(suppression: SuppressionList) -> Self {
        Self::with_echo(suppression, EchoSuppression::default())
    }

    pub fn with_echo(suppression: SuppressionList, echo: EchoSuppression) -> Self {
        Self {
            last_generation: Arc::new(Mutex::new(None)),
            suppression,
            echo,
        }
    }

    /// Legacy image-only convenience. Prefer `sample` to retain provenance.
    /// Do not poll both APIs independently: they intentionally share consumption.
    pub async fn sample_png(&self) -> Result<Option<Vec<u8>>, Error> {
        Ok(self.sample_inner(false).await?.map(|sample| sample.bytes))
    }

    /// Explicit-consent caller only. One bounded sample per native generation,
    /// preferring supported PNG/DIB to text; no arbitrary native conversions.
    /// Native calls/codec work run off-executor with process-wide try-admission.
    /// Native APIs can block indefinitely (including delayed rendering); we bound
    /// concurrency, not execution time. Cancellation retains worker admission.
    /// Windows text buffers are bounded before copying; macOS may allocate native
    /// NSData before the limit can be checked. Consume-on-attempt means transient
    /// contention/unavailability is dropped rather than retried after focus shifts.
    pub async fn sample(&self) -> Result<Option<ClipboardSample>, Error> {
        self.sample_inner(true).await
    }

    /// Read the current value without consuming a generation or applying echo
    /// suppression. The caller must enforce peer consent and deduplication.
    pub async fn read_current(&self) -> Result<Option<ClipboardSample>, Error> {
        let permit = WORKER.try_acquire().map_err(|_| Error::Busy)?;
        let suppression = self.suppression.clone();
        permit.forget();
        tokio::task::spawn_blocking(move || {
            struct Release;
            impl Drop for Release {
                fn drop(&mut self) {
                    WORKER.add_permits(1);
                }
            }
            let _release = Release;
            let generation = native_generation();
            if generation == 0 {
                return Err(Error::Unavailable);
            }
            if !privacy_allows(&suppression) {
                return Ok(None);
            }
            #[cfg(windows)]
            let sample = native::sample(generation, true, None)?;
            #[cfg(target_os = "macos")]
            let sample = native::sample(generation, true)?;
            if native_generation() != generation || !privacy_allows(&suppression) {
                return Ok(None);
            }
            Ok(sample)
        })
        .await
        .map_err(|_| Error::Worker)?
    }

    async fn sample_inner(&self, include_text: bool) -> Result<Option<ClipboardSample>, Error> {
        let permit = WORKER.try_acquire().map_err(|_| Error::Busy)?;
        let reader = self.clone();
        // Transfer an owned process-wide admission token into the worker without
        // allowing cancellation to release admission before native work completes.
        permit.forget();
        tokio::task::spawn_blocking(move || {
            struct Release;
            impl Drop for Release {
                fn drop(&mut self) {
                    WORKER.add_permits(1);
                }
            }
            let _release = Release;
            #[cfg(target_os = "macos")]
            let gate = input_event::clipboard_gate::ClipboardGate::shared();
            #[cfg(target_os = "macos")]
            let Some(mut gate_guard) = gate.try_lock() else {
                return Ok(None);
            };
            let generation = native_generation();
            #[cfg(target_os = "macos")]
            if !gate_guard.observe(generation) || !gate_guard.allows(generation) {
                return Ok(None);
            }
            if generation == 0 {
                return Err(Error::Unavailable);
            }
            if !consume_generation(
                &mut *reader
                    .last_generation
                    .try_lock()
                    .map_err(|_| Error::Worker)?,
                generation,
            ) {
                return Ok(None);
            }
            // Consume BEFORE probing privacy: changing focus cannot expose the
            // same previously suppressed generation. Poisoned policy fails closed.
            if !privacy_allows(&reader.suppression) {
                return Ok(None);
            }
            #[cfg(windows)]
            let sample = native::sample(generation, include_text, Some(&reader.echo))?;
            #[cfg(target_os = "macos")]
            let sample = if reader.echo.macos_generation_allows(generation) {
                native::sample(generation, include_text)?
            } else {
                None
            };
            // The completion counter check is AFTER decoding/encoding too. A
            // returned sample is never relabelled with a later OS generation.
            if native_generation() != generation || !privacy_allows(&reader.suppression) {
                return Ok(None);
            }
            #[cfg(target_os = "macos")]
            let sample = if reader.echo.macos_generation_allows(generation) {
                sample.filter(|sample| !reader.echo.contains(sample))
            } else {
                None
            };
            Ok(sample)
        })
        .await
        .map_err(|_| Error::Worker)?
    }
}

/// Synchronous current-generation sample for a caller that already runs on a
/// blocking worker. It shares the native format preference and privacy checks
/// with [`ImageReader`], but intentionally does not consume a generation or
/// apply echo suppression.
pub fn read_current_sync(suppression: &SuppressionList) -> Result<Option<ClipboardSample>, Error> {
    let generation = native_generation();
    if generation == 0 || !privacy_allows(suppression) {
        return Ok(None);
    }
    #[cfg(windows)]
    let sample = native::sample(generation, true, None)?;
    #[cfg(target_os = "macos")]
    let sample = native::sample(generation, true)?;
    if native_generation() != generation || !privacy_allows(suppression) {
        return Ok(None);
    }
    Ok(sample)
}

/// Return the native generation for a caller that already owns a blocking
/// worker. The platform-specific implementation remains in this module so
/// users do not need to duplicate Objective-C bindings.
pub fn generation_sync() -> u64 {
    native_generation()
}

fn privacy_allows(suppression: &SuppressionList) -> bool {
    // Never wait for an application policy lock. Work from a bounded-lock
    // snapshot; frontmost-app is a heuristic, not clipboard-owner provenance.
    let snapshot = {
        let Ok(list) = suppression.try_lock() else {
            return false;
        };
        list.clone()
    };
    !is_concealed_clipboard() && is_suppressed(&Arc::new(Mutex::new(snapshot))).is_none()
}

fn consume_generation(last: &mut Option<u64>, generation: u64) -> bool {
    if *last == Some(generation) {
        return false;
    }
    *last = Some(generation);
    true
}

#[cfg(any(target_os = "macos", test))]
fn select_file_reference<T>(count: usize, get: impl FnOnce() -> Option<T>) -> Result<T, Error> {
    if count != 1 {
        return Err(Error::Unsupported);
    }
    get().ok_or(Error::Unsupported)
}

#[cfg(any(target_os = "macos", test))]
fn sample_macos_policy(
    expected: u64,
    include_text: bool,
    mut generation: impl FnMut() -> u64,
    png: impl FnOnce() -> Result<Option<Vec<u8>>, Error>,
    has_file: impl FnOnce() -> bool,
    file: impl FnOnce() -> Result<Vec<u8>, Error>,
    text: impl FnOnce() -> Result<Option<Vec<u8>>, Error>,
) -> Result<Option<ClipboardSample>, Error> {
    if generation() != expected {
        return Err(Error::Unavailable);
    }
    let result = if let Some(bytes) = png()? {
        Some((ClipboardFormat::Png, bytes))
    } else if has_file() {
        Some((ClipboardFormat::Png, file()?))
    } else if include_text {
        text()?.map(|bytes| (ClipboardFormat::Text, bytes))
    } else {
        None
    };
    if generation() != expected {
        return Err(Error::Unavailable);
    }
    Ok(result.map(|(format, bytes)| ClipboardSample {
        native_generation: expected,
        format,
        bytes,
        #[cfg(windows)]
        windows_provenance: None,
    }))
}

#[cfg(windows)]
mod native {
    use super::*;
    use std::ffi::c_void;
    #[link(name = "user32")]
    unsafe extern "system" {
        fn OpenClipboard(owner: *mut c_void) -> i32;
        fn CloseClipboard() -> i32;
        fn GetClipboardData(format: u32) -> *mut c_void;
        fn RegisterClipboardFormatW(name: *const u16) -> u32;
    }
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GlobalSize(handle: *mut c_void) -> usize;
        fn GlobalLock(handle: *mut c_void) -> *const u8;
        fn GlobalUnlock(handle: *mut c_void) -> i32;
    }
    /// The expected generation was already consumed before privacy checks. Never
    /// relabel a raced transaction as a new generation (which might be private).
    pub fn sample(
        expected: u64,
        include_text: bool,
        echo: Option<&EchoSuppression>,
    ) -> Result<Option<ClipboardSample>, Error> {
        unsafe {
            if OpenClipboard(std::ptr::null_mut()) == 0 {
                return Err(Error::Unavailable);
            }
        }
        struct Close;
        impl Drop for Close {
            fn drop(&mut self) {
                unsafe {
                    CloseClipboard();
                }
            }
        }
        let close = Close;
        if native_generation() != expected {
            return Err(Error::Unavailable);
        }
        let marker = if echo.is_some() {
            let marker_name: Vec<u16> = "Mousehop.Clipboard.Provenance.v1"
                .encode_utf16()
                .chain(Some(0))
                .collect();
            let marker_format = unsafe { RegisterClipboardFormatW(marker_name.as_ptr()) };
            if marker_format == 0 {
                return Err(Error::Unavailable);
            }
            // Exact fixed-size marker; never allocate an attacker-selected buffer.
            unsafe { bytes(marker_format, 32)? }
                .map(|raw| <[u8; 32]>::try_from(raw).map_err(|_| Error::Unsupported))
                .transpose()?
        } else {
            None
        };
        let png = unsafe { RegisterClipboardFormatW([80, 78, 71, 0].as_ptr()) };
        if png == 0 {
            return Err(Error::Unavailable);
        }
        let mut selected = None;
        for (format, limit) in [
            (png, MAX_PNG_BYTES),
            (17, 64 * 1024 * 1024 + 4096),
            (8, 64 * 1024 * 1024 + 4096),
        ] {
            if let Some(raw) = unsafe { bytes(format, limit)? } {
                selected = Some((format, raw));
                break;
            }
        }
        if selected.is_none() && include_text {
            selected = unsafe { bytes(13, (MAX_TEXT_BYTES + 1) * 2)? }.map(|raw| (13, raw));
        }
        if native_generation() != expected {
            return Err(Error::Unavailable);
        }
        drop(close);
        // Decode only our bounded copies, outside the clipboard lock. The final
        // generation/privacy check remains in the worker after codec completion.
        let Some((format, raw)) = selected else {
            return Ok(None);
        };
        let kind = if format == 13 {
            ClipboardFormat::Text
        } else {
            ClipboardFormat::Png
        };
        let content = if format == 13 {
            let Some(text) = decode_unicode_text(&raw)? else {
                return Ok(None);
            };
            text
        } else if format == png {
            raw
        } else {
            encode_png(&decode_dib(&raw)?)?
        };
        let mut sample = ClipboardSample {
            native_generation: expected,
            format: kind,
            bytes: content,
            windows_provenance: marker.map(|marker| crate::clipboard::WindowsProvenance {
                native_format: format,
                marker,
            }),
        };
        if echo.is_some_and(|echo| echo.contains(&sample)) {
            return Ok(None);
        }
        if format == png {
            // Receipt comparison uses exact published bytes before canonicalizing
            // PNG, so harmless metadata/encoding differences cannot defeat echo.
            let canonical = encode_png(&decode_png(&sample.bytes)?)?;
            if canonical != sample.bytes {
                // Provenance cannot assert exact native bytes after rewriting.
                sample.windows_provenance = None;
            }
            sample.bytes = canonical;
        }
        Ok(Some(sample))
    }
    unsafe fn bytes(format: u32, limit: usize) -> Result<Option<Vec<u8>>, Error> {
        let handle = unsafe { GetClipboardData(format) };
        if handle.is_null() {
            return Ok(None);
        }
        let len = unsafe { GlobalSize(handle) };
        if len == 0 || len > limit {
            return Err(Error::Unsupported);
        }
        let data = unsafe { GlobalLock(handle) };
        if data.is_null() {
            return Err(Error::Unavailable);
        }
        struct Unlock(*mut c_void);
        impl Drop for Unlock {
            fn drop(&mut self) {
                unsafe {
                    GlobalUnlock(self.0);
                }
            }
        }
        let _unlock = Unlock(handle);
        let mut out = Vec::new();
        out.try_reserve_exact(len).map_err(|_| Error::Unsupported)?;
        out.extend_from_slice(unsafe { std::slice::from_raw_parts(data, len) });
        Ok(Some(out))
    }
}

#[cfg(windows)]
fn decode_unicode_text(raw: &[u8]) -> Result<Option<Vec<u8>>, Error> {
    if !raw.len().is_multiple_of(2) {
        return Err(Error::Unsupported);
    }
    let mut text = String::new();
    let mut terminated = false;
    let mut units = Vec::new();
    for chunk in raw.chunks_exact(2) {
        let unit = u16::from_le_bytes([chunk[0], chunk[1]]);
        if unit == 0 {
            terminated = true;
            break;
        }
        units.push(unit);
    }
    if !terminated {
        return Err(Error::Unsupported);
    }
    for c in char::decode_utf16(units) {
        let c = c.map_err(|_| Error::Unsupported)?;
        if text.len() + c.len_utf8() > MAX_TEXT_BYTES {
            return Err(Error::Unsupported);
        }
        text.push(c);
    }
    Ok((!text.is_empty()).then(|| text.into_bytes()))
}

/// Conservative DIB subset: BI_RGB 24/32-bit and standard BGRA BI_BITFIELDS.
/// Arbitrary masks, indexed/compressed data, profiles and native file handles
/// are not followed. BI_RGB's undefined fourth byte is treated as opaque.
#[cfg(any(windows, test))]
fn decode_dib(bytes: &[u8]) -> Result<RgbaImage, Error> {
    use input_event::clipboard_png::{MAX_DIMENSION, MAX_PIXELS};
    let u32_at = |n: usize| -> Result<u32, Error> {
        Ok(u32::from_le_bytes(
            bytes
                .get(n..n + 4)
                .ok_or(Error::Unsupported)?
                .try_into()
                .unwrap(),
        ))
    };
    let header = u32_at(0)? as usize;
    if ![40, 108, 124].contains(&header) || bytes.len() < header {
        return Err(Error::Unsupported);
    }
    let width = u32_at(4)? as i32;
    let signed_height = u32_at(8)? as i32;
    let height = signed_height.checked_abs().ok_or(Error::Unsupported)? as u32;
    if width <= 0
        || width as u32 > MAX_DIMENSION
        || height == 0
        || height > MAX_DIMENSION
        || width as usize * height as usize > MAX_PIXELS
    {
        return Err(Error::Unsupported);
    }
    if bytes[12..14] != [1, 0] {
        return Err(Error::Unsupported);
    }
    let bits = u16::from_le_bytes(bytes[14..16].try_into().unwrap());
    let compression = u32_at(16)?;
    if ![24, 32].contains(&bits) || !(compression == 0 || (compression == 3 && bits == 32)) {
        return Err(Error::Unsupported);
    }
    let mut offset = header;
    let mut alpha = false;
    if compression == 3 {
        if u32_at(40)? != 0x00ff0000 || u32_at(44)? != 0x0000ff00 || u32_at(48)? != 0x000000ff {
            return Err(Error::Unsupported);
        }
        if header == 40 {
            offset += 12;
        } else {
            let mask = u32_at(52)?;
            if mask != 0 && mask != 0xff000000 {
                return Err(Error::Unsupported);
            }
            alpha = mask != 0;
        }
    }
    // Color tables can be attached even to truecolor DIBs.
    if u32_at(32)? != 0 {
        return Err(Error::Unsupported);
    }
    if header == 124 && (u32_at(112)? != 0 || u32_at(116)? != 0) {
        return Err(Error::Unsupported);
    }
    let width = width as usize;
    let stride = (width * bits as usize).div_ceil(32) * 4;
    let source = bytes
        .get(offset..offset + stride * height as usize)
        .ok_or(Error::Unsupported)?;
    let mut pixels = Vec::new();
    pixels
        .try_reserve_exact(width * height as usize * 4)
        .map_err(|_| Error::Unsupported)?;
    for y in 0..height as usize {
        let row = if signed_height < 0 {
            y
        } else {
            height as usize - 1 - y
        };
        for x in 0..width {
            let p = &source[row * stride + x * (bits as usize / 8)..];
            pixels.extend_from_slice(&[p[2], p[1], p[0], if alpha { p[3] } else { 255 }]);
        }
    }
    RgbaImage::new(width as u32, height, pixels).map_err(Error::from)
}

#[cfg(target_os = "macos")]
mod native {
    use super::*;
    use objc2_app_kit::NSPasteboard;
    use objc2_foundation::NSString;
    /// Marker types are not an atomic snapshot or authorization. Echo prevention
    /// uses the shared application ownership fence before and after this read,
    /// never independently read marker bytes or a marker-presence blacklist.
    pub fn sample(expected: u64, include_text: bool) -> Result<Option<ClipboardSample>, Error> {
        objc2::rc::autoreleasepool(|_| {
            let pb = NSPasteboard::generalPasteboard();
            sample_macos_policy(
                expected,
                include_text,
                || pb.changeCount() as u64,
                || {
                    read()?
                        .map(|image| encode_png(&image).map_err(Error::from))
                        .transpose()
                },
                || has_file_reference(&pb),
                || read_file_reference(&pb),
                read_text,
            )
        })
    }
    fn has_file_reference(pb: &NSPasteboard) -> bool {
        let Some(types) = pb.types() else {
            // Failure to inspect the formats must not expose a path as text.
            return true;
        };
        let file_url = NSString::from_str("public.file-url");
        let filenames = NSString::from_str("NSFilenamesPboardType");
        types
            .iter()
            .any(|kind| kind.isEqualToString(&file_url) || kind.isEqualToString(&filenames))
    }
    fn read_file_reference(pb: &NSPasteboard) -> Result<Vec<u8>, Error> {
        let started = std::time::Instant::now();
        let result = (|| {
            let items = pb
                .pasteboardItems()
                .ok_or(("unavailable", Error::Unavailable))?;
            let kind = NSString::from_str("public.file-url");
            let data =
                select_file_reference(items.len(), || items.objectAtIndex(0).dataForType(&kind))
                    .map_err(|error| ("unsupported-reference-or-count", error))?;
            if data.length() > 32 * 1024 {
                return Err(("reference-oversize", Error::Unsupported));
            }
            let url = unsafe { data.as_bytes_unchecked() };
            let raw = mac_file::read(url).map_err(|_| ("file-rejected", Error::Unsupported))?;
            super::super::file_image::convert(&raw)
                .map_err(|_| ("decode-rejected", Error::Unsupported))
        })();
        log::debug!(
            "clipboard file image conversion {} in {:?}",
            result
                .as_ref()
                .map(|_| "succeeded")
                .unwrap_or_else(|(stage, _)| stage),
            started.elapsed()
        );
        result.map_err(|(_, err)| err)
    }
    pub fn read_text() -> Result<Option<Vec<u8>>, Error> {
        objc2::rc::autoreleasepool(|_| {
            let pb = NSPasteboard::generalPasteboard();
            let kind = NSString::from_str("public.utf8-plain-text");
            let Some(data) = pb.dataForType(&kind) else {
                return Ok(None);
            };
            if data.length() > MAX_TEXT_BYTES {
                return Err(Error::Unsupported);
            }
            let bytes = unsafe { data.as_bytes_unchecked() };
            std::str::from_utf8(bytes).map_err(|_| Error::Unsupported)?;
            Ok((!bytes.is_empty()).then(|| bytes.to_vec()))
        })
    }
    pub fn read() -> Result<Option<RgbaImage>, Error> {
        objc2::rc::autoreleasepool(|_| {
            let pb = NSPasteboard::generalPasteboard();
            // Only public.png: do not ask AppKit to synthesize PNG from arbitrary
            // TIFF/PDF/NSImage, which could allocate unbounded decoded memory.
            let kind = NSString::from_str("public.png");
            let Some(types) = pb.types() else {
                return Ok(None);
            };
            if !types.iter().any(|t| t.isEqualToString(&kind)) {
                return Ok(None);
            }
            let Some(data) = pb.dataForType(&kind) else {
                return Err(Error::Unavailable);
            };
            if data.length() > MAX_PNG_BYTES {
                return Err(Error::Unsupported);
            }
            decode_png(unsafe { data.as_bytes_unchecked() })
                .map(Some)
                .map_err(Error::from)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn file_reference_selection_rejects_multiple_and_unsupported() {
        assert_eq!(select_file_reference(1, || Some(42)).unwrap(), 42);
        assert!(select_file_reference(1, || None::<u8>).is_err());
        for count in [0, 2, 3] {
            assert!(
                select_file_reference(count, || -> Option<u8> { panic!("must not access item") })
                    .is_err()
            );
        }
        for (count, supported) in [(1, true), (1, false), (2, true)] {
            let result = sample_macos_policy(
                7,
                true,
                || 7,
                || Ok(None),
                || true,
                || select_file_reference(count, || supported.then(|| vec![4])),
                || panic!("file references must not fall back to text"),
            );
            if count == 1 && supported {
                assert_eq!(result.unwrap().unwrap().bytes, [4]);
            } else {
                assert!(result.is_err());
            }
        }
    }
    #[test]
    fn mac_policy_prioritizes_png_and_never_falls_back_on_failure() {
        let panic_file = || -> Result<Vec<u8>, Error> { panic!("file must not be read") };
        let panic_text = || -> Result<Option<Vec<u8>>, Error> { panic!("text must not be read") };
        let sample = sample_macos_policy(
            7,
            true,
            || 7,
            || Ok(Some(vec![1])),
            || panic!("file type must not be queried"),
            panic_file,
            panic_text,
        )
        .unwrap()
        .unwrap();
        assert_eq!(sample.format, ClipboardFormat::Png);
        assert_eq!(sample.bytes, [1]);
        assert!(
            sample_macos_policy(
                7,
                true,
                || 7,
                || Err(Error::Unsupported),
                || panic!("file type must not be queried"),
                panic_file,
                panic_text
            )
            .is_err()
        );
        assert!(
            sample_macos_policy(
                7,
                true,
                || 7,
                || Ok(None),
                || true,
                || Err(Error::Unsupported),
                panic_text
            )
            .is_err()
        );
        let text = sample_macos_policy(
            7,
            true,
            || 7,
            || Ok(None),
            || false,
            panic_file,
            || Ok(Some(vec![2])),
        )
        .unwrap()
        .unwrap();
        assert_eq!(text.format, ClipboardFormat::Text);
    }

    #[test]
    fn mac_policy_discards_file_when_generation_changes() {
        let mut calls = 0;
        let result = sample_macos_policy(
            7,
            true,
            || {
                calls += 1;
                if calls == 1 { 7 } else { 8 }
            },
            || Ok(None),
            || true,
            || Ok(vec![9]),
            || panic!("text must not be read"),
        );
        assert!(matches!(result, Err(Error::Unavailable)));
        assert_eq!(calls, 2);
    }
    #[test]
    fn suppressed_generation_cannot_reappear_after_focus_change() {
        let mut last = None;
        assert!(consume_generation(&mut last, 4));
        assert!(!consume_generation(&mut last, 4));
        assert!(consume_generation(&mut last, 5));
    }
    fn dib(height: i32) -> Vec<u8> {
        let mut b = vec![0; 48];
        b[0..4].copy_from_slice(&40u32.to_le_bytes());
        b[4..8].copy_from_slice(&1i32.to_le_bytes());
        b[8..12].copy_from_slice(&height.to_le_bytes());
        b[12] = 1;
        b[14] = 24;
        b[40..48].copy_from_slice(&[1, 2, 3, 0, 4, 5, 6, 0]);
        b
    }
    #[test]
    fn dib_stride_orientation_and_channels() {
        assert_eq!(
            decode_dib(&dib(2)).unwrap().pixels(),
            &[6, 5, 4, 255, 3, 2, 1, 255]
        );
        assert_eq!(
            decode_dib(&dib(-2)).unwrap().pixels(),
            &[3, 2, 1, 255, 6, 5, 4, 255]
        );
    }
    #[test]
    fn dib_v5_alpha_is_straight_and_rgb_reserved_byte_is_opaque() {
        let mut b = vec![0; 128];
        b[0..4].copy_from_slice(&124u32.to_le_bytes());
        b[4..8].copy_from_slice(&1i32.to_le_bytes());
        b[8..12].copy_from_slice(&(-1i32).to_le_bytes());
        b[12] = 1;
        b[14] = 32;
        b[16] = 3;
        for (offset, mask) in [
            (40, 0x00ff0000u32),
            (44, 0x0000ff00),
            (48, 0x000000ff),
            (52, 0xff000000),
        ] {
            b[offset..offset + 4].copy_from_slice(&mask.to_le_bytes());
        }
        b[124..].copy_from_slice(&[10, 20, 30, 64]);
        assert_eq!(decode_dib(&b).unwrap().pixels(), &[30, 20, 10, 64]);
        b[16] = 0;
        assert_eq!(decode_dib(&b).unwrap().pixels(), &[30, 20, 10, 255]);
        b[112] = 1;
        assert!(decode_dib(&b).is_err());
    }
    #[test]
    fn dib_rejects_truncation_dimensions_and_masks() {
        for n in 0..48 {
            assert!(decode_dib(&dib(2)[..n]).is_err());
        }
        assert!(decode_dib(&dib(i32::MIN)).is_err());
        assert!(decode_dib(&dib(8193)).is_err());
        let mut b = dib(2);
        b[16] = 3;
        assert!(decode_dib(&b).is_err());
    }
}
