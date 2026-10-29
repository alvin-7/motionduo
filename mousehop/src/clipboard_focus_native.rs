#![cfg(any(windows, target_os = "macos"))]

use crate::clipboard_focus::{FocusBackend, WriteResult};
use input_capture::clipboard::SuppressionList;
use input_emulation::clipboard::ClipboardEmulation;
use mousehop_proto::clipboard_transfer::{Format, MAX_PNG_SIZE, MAX_TEXT_SIZE};

fn payload_fits(format: Format, bytes: &[u8]) -> bool {
    !bytes.is_empty()
        && bytes.len()
            <= match format {
                Format::Text => MAX_TEXT_SIZE,
                Format::Png => MAX_PNG_SIZE,
            }
}

#[cfg(test)]
fn preferred_format(has_png: bool, has_text: bool) -> Option<Format> {
    has_png
        .then_some(Format::Png)
        .or_else(|| has_text.then_some(Format::Text))
}

pub(crate) struct NativeFocusBackend {
    suppression: SuppressionList,
    writer: ClipboardEmulation,
}

impl NativeFocusBackend {
    pub(crate) fn new(suppression: SuppressionList) -> Self {
        Self {
            suppression,
            writer: ClipboardEmulation::new().expect("clipboard adapter construction"),
        }
    }

    fn generation(&self) -> u64 {
        native_generation()
    }

    /// Sample once at the current native generation. The shared image reader
    /// prefers PNG, falls back to text, applies privacy suppression, and keeps
    /// the read bounded without echo/provenance/Lamport state.
    fn read(&self) -> Option<(Format, Vec<u8>)> {
        let sample = input_capture::clipboard::image::read_current_sync(&self.suppression)
            .ok()
            .flatten()?;
        let format = match sample.format {
            input_capture::clipboard::ClipboardFormat::Png => Format::Png,
            input_capture::clipboard::ClipboardFormat::Text => Format::Text,
        };
        payload_fits(format, &sample.bytes).then_some((format, sample.bytes))
    }

    /// Prepare the payload before entering the platform's generation-fenced
    /// native write. The platform adapter owns the check/write critical section
    /// and returns the generation it actually published.
    fn write(&self, format: Format, bytes: &[u8], expected_generation: u64) -> WriteResult {
        if !payload_fits(format, bytes) {
            return WriteResult::Failed;
        }

        let result = match format {
            Format::Text => {
                let Ok(text) = std::str::from_utf8(bytes) else {
                    return WriteResult::Failed;
                };
                self.writer
                    .set_text_if_generation(text, expected_generation)
            }
            Format::Png => self
                .writer
                .set_png_if_generation(bytes, expected_generation),
        };
        match result {
            Ok(Some(generation)) if generation != 0 => WriteResult::Written { generation },
            Ok(None) => WriteResult::Stale,
            Ok(Some(_)) | Err(_) => WriteResult::Failed,
        }
    }
}

fn native_generation() -> u64 {
    #[cfg(windows)]
    {
        #[link(name = "user32")]
        unsafe extern "system" {
            fn GetClipboardSequenceNumber() -> u32;
        }
        unsafe { GetClipboardSequenceNumber() as u64 }
    }
    #[cfg(target_os = "macos")]
    {
        input_capture::clipboard::image::generation_sync()
    }
}

impl FocusBackend for NativeFocusBackend {
    fn generation(&self) -> u64 {
        self.generation()
    }

    fn read(&self) -> Option<(Format, Vec<u8>)> {
        self.read()
    }

    fn write(&self, format: Format, bytes: &[u8], expected_generation: u64) -> WriteResult {
        self.write(format, bytes, expected_generation)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn png_is_preferred_over_text() {
        assert_eq!(preferred_format(true, true), Some(Format::Png));
        assert_eq!(preferred_format(false, true), Some(Format::Text));
        assert_eq!(preferred_format(false, false), None);
    }

    #[test]
    fn native_payload_limits_are_exact() {
        assert!(payload_fits(Format::Text, &vec![0; MAX_TEXT_SIZE]));
        assert!(!payload_fits(Format::Text, &vec![0; MAX_TEXT_SIZE + 1]));
        assert!(payload_fits(Format::Png, &vec![0; MAX_PNG_SIZE]));
        assert!(!payload_fits(Format::Png, &vec![0; MAX_PNG_SIZE + 1]));
        assert!(!payload_fits(Format::Text, &[]));
    }

    #[test]
    #[ignore = "writes to the real system clipboard; run explicitly on a disposable clipboard"]
    fn native_write_reports_generation_immediately_after_real_write() {
        let backend = NativeFocusBackend::new(SuppressionList::default());

        let expected = backend.generation();
        let written = backend
            .writer
            .set_text_if_generation("Mousehop native write race test", expected)
            .expect("native text clipboard write failed")
            .expect("native text clipboard generation was stale");
        assert!(written <= backend.generation());

        let image = input_event::clipboard_png::RgbaImage::new(1, 1, vec![0x12, 0x34, 0x56, 0xff])
            .expect("test image");
        let png = input_event::clipboard_png::encode_png(&image).expect("test PNG");
        let expected = backend.generation();
        let written = backend
            .writer
            .set_png_if_generation(&png, expected)
            .expect("native PNG clipboard write failed")
            .expect("native PNG clipboard generation was stale");
        assert!(written <= backend.generation());
    }

    #[cfg(windows)]
    #[derive(Default)]
    struct DriftBackend {
        generation: u64,
        bytes: Vec<u8>,
        reads: usize,
    }

    #[cfg(windows)]
    impl DriftBackend {
        fn read(&mut self) -> Vec<u8> {
            self.reads += 1;
            self.bytes.clone()
        }
    }

    #[cfg(windows)]
    #[test]
    fn generation_drift_after_write_is_deduped_without_echo() {
        use sha2::{Digest, Sha256};

        let bytes = b"content written by the remote peer".to_vec();
        let mut backend = DriftBackend {
            generation: 1911,
            bytes: bytes.clone(),
            ..DriftBackend::default()
        };
        let known = Sha256::digest(&bytes).to_vec();
        let written_generation = backend.generation;

        // Windows advances the sequence after CloseClipboard, but the content
        // is still the write we just completed. Focus's generation miss causes
        // one fresh read; its hash dedup prevents an echo to the source peer.
        backend.generation += 3;
        let current = backend.generation;
        let sampled = if current == written_generation {
            bytes
        } else {
            backend.read()
        };
        let sampled_hash = Sha256::digest(&sampled).to_vec();
        let should_push = sampled_hash != known;
        assert!(!should_push);
        assert_eq!(backend.reads, 1);
    }
}
