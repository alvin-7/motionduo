//! Opt-in macOS native clipboard writer used only by the explicit receipt API.
//!
//! Gated by `all(target_os = "macos", feature = "clipboard-image")`; called on
//! a single-admission blocking worker. Never infer this receipt from an arboard
//! write. Existing arboard APIs and their capability query remain unchanged.
//!
//! Apple documents that declareTypes:owner: returns the ownership change count,
//! changeCount increments on ownership changes, and setData:forType: returns
//! false if ownership changed. We retain that directly returned count, rather
//! than sampling a count and attributing it to an earlier write.
//! https://developer.apple.com/documentation/appkit/nspasteboard/declaretypes(_:owner:)
//! https://developer.apple.com/documentation/appkit/nspasteboard/changecount
//! https://developer.apple.com/documentation/appkit/nspasteboard/setdata(_:fortype:)
//!
//! A receipt proves a bounded observation, not continued ownership, atomic
//! multi-format publication, authentication, or protection against malicious
//! local software. Another process may replace the clipboard immediately after
//! verification. No restore/clear is attempted on error: that could destroy a
//! newer clipboard. Native calls may block, raise Objective-C exceptions, or
//! allocate NSData before its length is inspectable. No hard timeout is claimed.

#[cfg(test)]
pub(super) static TEST_WRITER: std::sync::Mutex<()> = std::sync::Mutex::new(());

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    Text,
    Png,
}

/// Private fields prevent callers from manufacturing an ownership receipt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WriteReceipt {
    generation: u64,
    format: Format,
    marker: [u8; 32],
}

impl WriteReceipt {
    pub fn generation(&self) -> u64 {
        self.generation
    }

    pub fn format(&self) -> Format {
        self.format
    }

    pub fn marker(&self) -> &[u8; 32] {
        &self.marker
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WriteError {
    Busy,
    InvalidPayload,
    NativeWrite,
    OwnershipLost,
    ReadbackMismatch,
    RegistrationRejected,
}

impl std::fmt::Display for WriteError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "macOS clipboard write: {self:?}")
    }
}

impl std::error::Error for WriteError {}

fn prepare_payload(format: Format, bytes: &[u8]) -> Result<Vec<u8>, WriteError> {
    match format {
        Format::Text => {
            if bytes.is_empty() || bytes.len() > 1024 * 1024 || std::str::from_utf8(bytes).is_err()
            {
                return Err(WriteError::InvalidPayload);
            }
            Ok(bytes.to_vec())
        }
        Format::Png => {
            use input_event::clipboard_png::{MAX_PNG_BYTES, decode_png, encode_png};
            if bytes.len() > MAX_PNG_BYTES {
                return Err(WriteError::InvalidPayload);
            }
            let image = decode_png(bytes).map_err(|_| WriteError::InvalidPayload)?;
            encode_png(&image).map_err(|_| WriteError::InvalidPayload)
        }
    }
}

// This seam deliberately models the generation returned by the ownership-taking
// operation separately from later reads. Native and deterministic tests use the
// same verifier. Reads compare in-place; do not clone external clipboard bytes.
trait Pasteboard {
    fn declare(&mut self, format: Format) -> u64;
    fn generation(&mut self) -> u64;
    fn set(&mut self, marker: bool, bytes: &[u8]) -> bool;
    fn matches(&mut self, marker: bool, bytes: &[u8]) -> bool;
}

fn publish(
    pb: &mut impl Pasteboard,
    format: Format,
    bytes: &[u8],
    marker: [u8; 32],
    gate: &input_event::clipboard_gate::ClipboardGate,
) -> Result<WriteReceipt, WriteError> {
    let mut fence = gate.try_lock().ok_or(WriteError::Busy)?;
    publish_fenced(pb, format, bytes, marker, &mut fence)
}

fn publish_fenced(
    pb: &mut impl Pasteboard,
    format: Format,
    bytes: &[u8],
    marker: [u8; 32],
    fence: &mut input_event::clipboard_gate::GateGuard<'_>,
) -> Result<WriteReceipt, WriteError> {
    let generation = pb.declare(format);
    // This ownership is suppressed even when ANY later operation fails. No
    // rollback: failed marker/readback/registration can leave remote content.
    if !fence.record_remote(generation) {
        return Err(WriteError::OwnershipLost);
    }
    if pb.generation() != generation {
        return Err(WriteError::OwnershipLost);
    }
    // Publish content first: do not leave a successful-looking marker before
    // writing the payload. Both native calls must independently succeed.
    if !pb.set(false, bytes) || !pb.set(true, &marker) {
        return Err(WriteError::NativeWrite);
    }
    if pb.generation() != generation {
        return Err(WriteError::OwnershipLost);
    }
    let matches = pb.matches(true, &marker) && pb.matches(false, bytes);
    if pb.generation() != generation {
        return Err(WriteError::OwnershipLost);
    }
    if !matches {
        return Err(WriteError::ReadbackMismatch);
    }
    Ok(WriteReceipt {
        generation,
        format,
        marker,
    })
}

#[cfg(all(target_os = "macos", feature = "clipboard-image"))]
mod native {
    use super::*;
    use input_event::clipboard_png::{MAX_PNG_BYTES, decode_png};
    use objc2::rc::Retained;
    use objc2_app_kit::NSPasteboard;
    use objc2_foundation::{NSArray, NSData, NSString};
    use std::sync::Mutex;

    /// A custom pasteboard type, not a privacy/concealed-content marker.
    pub const PROVENANCE_TYPE: &str = "org.mousehop.clipboard-write.v1";
    static WRITER: Mutex<()> = Mutex::new(());

    unsafe extern "C" {
        fn arc4random_buf(buffer: *mut std::ffi::c_void, size: usize);
    }

    struct Native {
        pb: Retained<NSPasteboard>,
        payload_type: Retained<NSString>,
        marker_type: Retained<NSString>,
    }

    impl Pasteboard for Native {
        fn declare(&mut self, _format: Format) -> u64 {
            let types = NSArray::from_slice(&[&*self.payload_type, &*self.marker_type]);
            // SAFETY: nil owner; no callback object with a lifetime to uphold.
            unsafe { self.pb.declareTypes_owner(&types, None) as u64 }
        }

        fn generation(&mut self) -> u64 {
            self.pb.changeCount() as u64
        }

        fn set(&mut self, marker: bool, bytes: &[u8]) -> bool {
            let ty = if marker {
                &self.marker_type
            } else {
                &self.payload_type
            };
            let data = NSData::with_bytes(bytes);
            self.pb.setData_forType(Some(&data), ty)
        }

        fn matches(&mut self, marker: bool, bytes: &[u8]) -> bool {
            let ty = if marker {
                &self.marker_type
            } else {
                &self.payload_type
            };
            let Some(data) = self.pb.dataForType(ty) else {
                return false;
            };
            // NSData itself is materialized by AppKit before this length check.
            data.len() == bytes.len() && data.iter().eq(bytes.iter().copied())
        }
    }

    /// Write directly against the expected pasteboard generation. Payload
    /// decoding and NSData construction happen before the first changeCount
    /// read. NSPasteboard has no Windows-style lock: the check-to-clear window
    /// is only minimized, and the post-write check rejects a detected race.
    pub fn write_if_generation(
        format: Format,
        bytes: &[u8],
        expected_generation: u64,
    ) -> Result<Option<u64>, WriteError> {
        let payload = prepare_payload(format, bytes)?;
        objc2::rc::autoreleasepool(|_| {
            let data = NSData::with_bytes(&payload);
            let data_type = NSString::from_str(match format {
                Format::Text => "public.utf8-plain-text",
                Format::Png => "public.png",
            });
            let pasteboard = NSPasteboard::generalPasteboard();
            if pasteboard.changeCount() as u64 != expected_generation {
                return Ok(None);
            }
            let generation =
                u64::try_from(pasteboard.clearContents()).map_err(|_| WriteError::NativeWrite)?;
            if generation == 0 {
                return Err(WriteError::NativeWrite);
            }
            if !pasteboard.setData_forType(Some(&data), &data_type) {
                return Err(WriteError::NativeWrite);
            }
            if pasteboard.changeCount() as u64 != generation {
                return Ok(None);
            }
            Ok(Some(generation))
        })
    }

    /// Blocking, process-serialized text/PNG publication with ownership evidence.
    ///
    /// `commit_guard` must validate consent/session/local-generation and return
    /// their shared lock guard. Revocation and local-change publication MUST use
    /// that same lock. It is held through publication AND verification; returning
    /// None means cancellation with no native clipboard access. The caller must
    /// retain its worker admission permit until this function returns, even if
    /// the async waiter is cancelled. Do not call directly on the input executor.
    ///
    /// The process-wide input-event ClipboardGate is acquired without waiting
    /// before taking ownership. Its remote fence is installed before payload
    /// publication and retained on every later error, including registration
    /// failure. Capture and final service enqueue must use that same shared gate.
    ///
    /// Text is UTF-8 bytes under public.utf8-plain-text. PNG is decoded and
    /// re-encoded using the bounded shared codec to strip metadata. Receipts refer
    /// to the resulting canonical PNG bytes, not necessarily the input PNG bytes.
    /// `on_verified` runs under the lifecycle guard and must synchronously register
    /// the receipt and exact published bytes for echo suppression. It must not
    /// reacquire that guard or block waiting for a capture worker. A later async
    /// registration after this function returns would leave an unsafe gap.
    pub fn write_checked<F, G, V>(
        format: Format,
        bytes: &[u8],
        commit_guard: F,
        on_verified: V,
    ) -> Result<Option<(WriteReceipt, Vec<u8>)>, WriteError>
    where
        F: FnOnce() -> Option<G>,
        V: FnOnce(&WriteReceipt, &[u8]) -> bool,
    {
        let _writer = WRITER.try_lock().map_err(|_| WriteError::Busy)?;
        let payload = match format {
            Format::Text => {
                if bytes.is_empty()
                    || bytes.len() > 1024 * 1024
                    || std::str::from_utf8(bytes).is_err()
                {
                    return Err(WriteError::InvalidPayload);
                }
                bytes.to_vec()
            }
            Format::Png => {
                if bytes.len() > MAX_PNG_BYTES {
                    return Err(WriteError::InvalidPayload);
                }
                let image = decode_png(bytes).map_err(|_| WriteError::InvalidPayload)?;
                input_event::clipboard_png::encode_png(&image)
                    .map_err(|_| WriteError::InvalidPayload)?
            }
        };
        let Some(guard) = commit_guard() else {
            return Ok(None);
        };
        let result = objc2::rc::autoreleasepool(|_| {
            let mut marker = [0; 32];
            // SAFETY: the array is writable for exactly 32 bytes. macOS system
            // arc4random_buf initializes all bytes using the platform CSPRNG.
            unsafe { arc4random_buf(marker.as_mut_ptr().cast(), marker.len()) };
            let mut pb = Native {
                pb: NSPasteboard::generalPasteboard(),
                payload_type: NSString::from_str(match format {
                    Format::Text => "public.utf8-plain-text",
                    Format::Png => "public.png",
                }),
                marker_type: NSString::from_str(PROVENANCE_TYPE),
            };
            publish(
                &mut pb,
                format,
                &payload,
                marker,
                &input_event::clipboard_gate::ClipboardGate::shared(),
            )
        });
        if let Ok(receipt) = &result {
            if !on_verified(receipt, &payload) {
                return Err(WriteError::RegistrationRejected);
            }
        }
        drop(guard);
        result.map(|receipt| Some((receipt, payload)))
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        use std::cell::Cell;

        // These tests stop before constructing NSPasteboard: they never access
        // the general pasteboard or change the user's clipboard.
        #[test]
        fn rejected_lifecycle_never_registers_text_or_png() {
            let _test = TEST_WRITER.lock().unwrap();
            let image =
                input_event::clipboard_png::RgbaImage::new(1, 1, vec![1, 2, 3, 255]).unwrap();
            let png = input_event::clipboard_png::encode_png(&image).unwrap();
            for (format, payload) in [(Format::Text, b"hello".as_slice()), (Format::Png, &png)] {
                let result = write_checked(
                    format,
                    payload,
                    || None::<()>,
                    |_, _| panic!("cancelled publication must not register a receipt"),
                );
                assert_eq!(result, Ok(None));
            }
        }

        #[test]
        fn invalid_payload_never_enters_lifecycle_or_registers_receipt() {
            let _test = TEST_WRITER.lock().unwrap();
            let guard_called = Cell::new(false);
            for (format, payload) in [
                (Format::Text, b"".as_slice()),
                (Format::Text, b"\xff".as_slice()),
                (Format::Png, b"not a PNG".as_slice()),
            ] {
                let result = write_checked(
                    format,
                    payload,
                    || {
                        guard_called.set(true);
                        Some(())
                    },
                    |_, _| panic!("invalid payload must not register a receipt"),
                );
                assert_eq!(result, Err(WriteError::InvalidPayload));
                assert!(!guard_called.get());
            }
        }
    }
}

#[cfg(all(target_os = "macos", feature = "clipboard-image"))]
pub use native::{write_checked, write_if_generation};

#[cfg(test)]
mod preparation_tests {
    use super::*;

    #[test]
    fn generation_fenced_text_preparation_is_bounded_and_utf8() {
        assert!(prepare_payload(Format::Text, b"").is_err());
        assert!(prepare_payload(Format::Text, &[0xff]).is_err());
        assert_eq!(prepare_payload(Format::Text, b"hello").unwrap(), b"hello");
    }

    #[test]
    fn generation_fenced_png_preparation_canonicalizes_without_native_access() {
        let image =
            input_event::clipboard_png::RgbaImage::new(1, 1, vec![0x12, 0x34, 0x56, 0xff]).unwrap();
        let png = input_event::clipboard_png::encode_png(&image).unwrap();
        let prepared = prepare_payload(Format::Png, &png).unwrap();
        assert_eq!(
            input_event::clipboard_png::decode_png(&prepared).unwrap(),
            image
        );
        assert!(prepare_payload(Format::Png, b"not a PNG").is_err());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Default)]
    struct Mock {
        generation: u64,
        content: Vec<u8>,
        marker: Vec<u8>,
        fail_write: Option<bool>,
        replace_after_read: bool,
        mismatch: bool,
        change_after_declare: bool,
    }

    impl Pasteboard for Mock {
        fn declare(&mut self, _: Format) -> u64 {
            self.generation += 1;
            let acquired = self.generation;
            if self.change_after_declare {
                self.generation += 1;
            }
            acquired
        }
        fn generation(&mut self) -> u64 {
            self.generation
        }
        fn set(&mut self, marker: bool, bytes: &[u8]) -> bool {
            if self.fail_write == Some(marker) {
                return false;
            }
            if marker {
                &mut self.marker
            } else {
                &mut self.content
            }
            .extend_from_slice(bytes);
            true
        }
        fn matches(&mut self, marker: bool, bytes: &[u8]) -> bool {
            let result =
                !self.mismatch && (if marker { &self.marker } else { &self.content }) == bytes;
            if self.replace_after_read {
                self.generation += 1;
            }
            result
        }
    }

    #[test]
    fn partial_write_and_readback_failure_keep_ownership_fenced() {
        for (fail_write, mismatch) in [(Some(true), false), (None, true)] {
            let gate = input_event::clipboard_gate::ClipboardGate::default();
            let mut pb = Mock {
                fail_write,
                mismatch,
                ..Mock::default()
            };
            assert!(publish(&mut pb, Format::Text, b"hello", [7; 32], &gate).is_err());
            assert_eq!(pb.content, b"hello");
            assert!(!gate.try_publish(pb.generation, || panic!("partial remote write escaped")));
            assert!(gate.try_publish(pb.generation + 1, || {}));
        }
    }

    #[test]
    fn busy_gate_prevents_declare_and_payload_write() {
        let gate = input_event::clipboard_gate::ClipboardGate::default();
        let _capture = gate.try_lock().unwrap();
        let mut pb = Mock::default();
        assert_eq!(
            publish(&mut pb, Format::Text, b"hello", [7; 32], &gate),
            Err(WriteError::Busy)
        );
        assert_eq!(pb.generation, 0);
        assert!(pb.content.is_empty());
    }

    #[test]
    fn receipt_uses_ownership_operation_generation_and_exact_marker() {
        let mut pb = Mock::default();
        let receipt = publish(
            &mut pb,
            Format::Text,
            b"hello",
            [7; 32],
            &input_event::clipboard_gate::ClipboardGate::default(),
        )
        .unwrap();
        assert_eq!(receipt.generation(), 1);
        assert_eq!(receipt.marker(), &[7; 32]);
        assert_eq!(receipt.format(), Format::Text);
        assert_eq!(pb.content, b"hello");
    }

    #[test]
    fn ownership_lost_before_write_never_writes() {
        let mut pb = Mock {
            change_after_declare: true,
            ..Mock::default()
        };
        assert_eq!(
            publish(
                &mut pb,
                Format::Text,
                b"hello",
                [7; 32],
                &input_event::clipboard_gate::ClipboardGate::default()
            ),
            Err(WriteError::OwnershipLost)
        );
        assert!(pb.content.is_empty());
        assert!(pb.marker.is_empty());
    }

    #[test]
    fn either_native_write_failure_prevents_receipt() {
        for fail_write in [Some(false), Some(true)] {
            let mut pb = Mock {
                fail_write,
                ..Mock::default()
            };
            assert_eq!(
                publish(
                    &mut pb,
                    Format::Png,
                    b"png",
                    [7; 32],
                    &input_event::clipboard_gate::ClipboardGate::default()
                ),
                Err(WriteError::NativeWrite)
            );
        }
    }

    #[test]
    fn content_or_marker_mismatch_prevents_receipt() {
        let mut pb = Mock {
            mismatch: true,
            ..Mock::default()
        };
        assert_eq!(
            publish(
                &mut pb,
                Format::Text,
                b"hello",
                [7; 32],
                &input_event::clipboard_gate::ClipboardGate::default()
            ),
            Err(WriteError::ReadbackMismatch)
        );
    }

    #[test]
    fn replacement_during_readback_prevents_receipt() {
        let mut pb = Mock {
            replace_after_read: true,
            ..Mock::default()
        };
        assert_eq!(
            publish(
                &mut pb,
                Format::Text,
                b"hello",
                [7; 32],
                &input_event::clipboard_gate::ClipboardGate::default()
            ),
            Err(WriteError::OwnershipLost)
        );
    }
}
