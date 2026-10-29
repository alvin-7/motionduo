//! Bounded PNG/RGBA8 conversion for future Windows/macOS clipboard adapters.
//!
//! No OS clipboard access, event variants, serialization, or transport integration.
//! Pixels are top-to-bottom, tightly packed, straight-alpha RGBA8 (not BGRA or
//! premultiplied alpha). PNG metadata is deliberately not preserved. These are
//! synchronous, CPU-bound helpers: future async callers must use a bounded worker.

use std::io::{self, Cursor, Write};

/// Compressed input and encoded output limit (16 MiB).
pub const MAX_PNG_BYTES: usize = 16 * 1024 * 1024;
/// Maximum width or height, independently of the pixel budget.
pub const MAX_DIMENSION: u32 = 8192;
/// Maximum pixel count, limiting RGBA output to 64 MiB.
pub const MAX_PIXELS: usize = 16 * 1024 * 1024;
/// Best-effort budget for allocations internal to the png decoder. This is not
/// a process-wide memory ceiling; caller input/output and codec overhead are extra.
pub const DECODER_MEMORY_BYTES: usize = 32 * 1024 * 1024;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("PNG exceeds the compressed byte limit")]
    PngTooLarge,
    #[error("image dimensions exceed clipboard limits or are zero")]
    InvalidDimensions,
    #[error("RGBA buffer does not match dimensions")]
    InvalidRgbaLength,
    #[error("animated PNG is not supported")]
    AnimationUnsupported,
    #[error("unsupported decoded PNG pixel format")]
    UnsupportedFormat,
    #[error("unable to reserve image memory")]
    Allocation,
    #[error("invalid PNG: {0}")]
    Decode(#[from] png::DecodingError),
    #[error("PNG encoding failed: {0}")]
    Encode(#[from] png::EncodingError),
}

/// Validated clipboard image. Construction cannot bypass size/length checks.
#[derive(Debug, PartialEq, Eq)]
pub struct RgbaImage {
    width: u32,
    height: u32,
    pixels: Vec<u8>,
}

impl RgbaImage {
    pub fn new(width: u32, height: u32, pixels: Vec<u8>) -> Result<Self, Error> {
        if pixels.len() != rgba_len(width, height)? {
            return Err(Error::InvalidRgbaLength);
        }
        Ok(Self {
            width,
            height,
            pixels,
        })
    }

    pub fn width(&self) -> u32 {
        self.width
    }
    pub fn height(&self) -> u32 {
        self.height
    }
    pub fn pixels(&self) -> &[u8] {
        &self.pixels
    }
}

fn rgba_len(width: u32, height: u32) -> Result<usize, Error> {
    if width == 0 || height == 0 || width > MAX_DIMENSION || height > MAX_DIMENSION {
        return Err(Error::InvalidDimensions);
    }
    let pixels = (width as usize)
        .checked_mul(height as usize)
        .filter(|&n| n <= MAX_PIXELS)
        .ok_or(Error::InvalidDimensions)?;
    pixels.checked_mul(4).ok_or(Error::InvalidDimensions)
}

fn zeroed(len: usize) -> Result<Vec<u8>, Error> {
    let mut bytes = Vec::new();
    bytes
        .try_reserve_exact(len)
        .map_err(|_| Error::Allocation)?;
    bytes.resize(len, 0);
    Ok(bytes)
}

/// Decode one static PNG, expanding palette/transparency and stripping 16-bit
/// samples to their high byte. Reject APNG rather than silently using frame one.
/// CRC/truncation errors, including errors after IDAT, propagate to the caller.
pub fn decode_png(bytes: &[u8]) -> Result<RgbaImage, Error> {
    if bytes.len() > MAX_PNG_BYTES {
        return Err(Error::PngTooLarge);
    }
    let mut decoder = png::Decoder::new_with_limits(
        Cursor::new(bytes),
        png::Limits {
            bytes: DECODER_MEMORY_BYTES,
        },
    );
    decoder.set_ignore_text_chunk(true);
    decoder.set_ignore_iccp_chunk(true);
    decoder.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
    // Inspect IHDR before read_info can allocate image/row buffers or read metadata.
    let header = decoder.read_header_info()?;
    let (width, height) = (header.width, header.height);
    let len = rgba_len(width, height)?;
    let mut reader = decoder.read_info()?;
    if reader.info().animation_control.is_some() {
        return Err(Error::AnimationUnsupported);
    }
    let output_len = reader
        .output_buffer_size()
        .ok_or(Error::InvalidDimensions)?;
    if output_len > len {
        return Err(Error::UnsupportedFormat);
    }
    // Allocate one RGBA-sized buffer; expand smaller pixels backwards in place.
    let mut pixels = zeroed(len)?;
    let frame = reader.next_frame(&mut pixels[..output_len])?;
    if frame.bit_depth != png::BitDepth::Eight || frame.width != width || frame.height != height {
        return Err(Error::UnsupportedFormat);
    }
    reader.finish()?;
    if reader.info().animation_control.is_some() {
        return Err(Error::AnimationUnsupported);
    }
    let channels = match frame.color_type {
        png::ColorType::Grayscale => 1,
        png::ColorType::GrayscaleAlpha => 2,
        png::ColorType::Rgb => 3,
        png::ColorType::Rgba => 4,
        png::ColorType::Indexed => return Err(Error::UnsupportedFormat),
    };
    for i in (0..len / 4).rev() {
        let source = i * channels;
        let rgba = match channels {
            1 => [pixels[source], pixels[source], pixels[source], 255],
            2 => [
                pixels[source],
                pixels[source],
                pixels[source],
                pixels[source + 1],
            ],
            3 => [pixels[source], pixels[source + 1], pixels[source + 2], 255],
            _ => [
                pixels[source],
                pixels[source + 1],
                pixels[source + 2],
                pixels[source + 3],
            ],
        };
        pixels[i * 4..i * 4 + 4].copy_from_slice(&rgba);
    }
    RgbaImage::new(width, height, pixels)
}

struct BoundedWriter {
    bytes: Vec<u8>,
    limit: usize,
    exceeded: bool,
}

impl Write for BoundedWriter {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if bytes.len() > self.limit.saturating_sub(self.bytes.len()) {
            self.exceeded = true;
            return Err(io::Error::other("PNG output limit exceeded"));
        }
        self.bytes
            .try_reserve_exact(bytes.len())
            .map_err(io::Error::other)?;
        self.bytes.extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

/// Encode only RGBA8 pixels, without copying metadata from untrusted PNG input.
pub fn encode_png(image: &RgbaImage) -> Result<Vec<u8>, Error> {
    let mut output = BoundedWriter {
        bytes: Vec::new(),
        limit: MAX_PNG_BYTES,
        exceeded: false,
    };
    let result = (|| {
        let mut encoder = png::Encoder::new(&mut output, image.width, image.height);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header()?;
        writer.write_image_data(&image.pixels)?;
        writer.finish()
    })();
    if output.exceeded {
        return Err(Error::PngTooLarge);
    }
    result?;
    Ok(output.bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(color: png::ColorType, depth: png::BitDepth, data: &[u8]) -> Vec<u8> {
        let mut bytes = Vec::new();
        {
            let mut encoder = png::Encoder::new(&mut bytes, 2, 1);
            encoder.set_color(color);
            encoder.set_depth(depth);
            if color == png::ColorType::Indexed {
                encoder.set_palette(vec![255, 0, 0, 0, 255, 0]);
                encoder.set_trns(vec![0, 128]);
            }
            let mut writer = encoder.write_header().unwrap();
            writer.write_image_data(data).unwrap();
            writer.finish().unwrap();
        }
        bytes
    }

    #[test]
    fn rgba_roundtrip_preserves_channels_and_straight_alpha() {
        let image = RgbaImage::new(2, 1, vec![255, 2, 3, 0, 4, 5, 200, 128]).unwrap();
        let png = encode_png(&image).unwrap();
        assert_eq!(decode_png(&png).unwrap(), image);
    }

    #[test]
    fn expands_rgb_gray_gray_alpha_and_palette() {
        let cases = [
            (
                png::ColorType::Rgb,
                vec![1, 2, 3, 4, 5, 6],
                vec![1, 2, 3, 255, 4, 5, 6, 255],
            ),
            (
                png::ColorType::Grayscale,
                vec![1, 2],
                vec![1, 1, 1, 255, 2, 2, 2, 255],
            ),
            (
                png::ColorType::GrayscaleAlpha,
                vec![1, 3, 2, 4],
                vec![1, 1, 1, 3, 2, 2, 2, 4],
            ),
            (
                png::ColorType::Indexed,
                vec![0, 1],
                vec![255, 0, 0, 0, 0, 255, 0, 128],
            ),
        ];
        for (color, data, expected) in cases {
            assert_eq!(
                decode_png(&fixture(color, png::BitDepth::Eight, &data))
                    .unwrap()
                    .pixels(),
                expected
            );
        }
    }

    #[test]
    fn expands_one_bit_and_strips_sixteen_bit() {
        let gray = decode_png(&fixture(
            png::ColorType::Grayscale,
            png::BitDepth::One,
            &[0x40],
        ))
        .unwrap();
        assert_eq!(gray.pixels(), &[0, 0, 0, 255, 255, 255, 255, 255]);
        let gray = decode_png(&fixture(
            png::ColorType::Grayscale,
            png::BitDepth::Sixteen,
            &[0x12, 0x34, 0xab, 0xcd],
        ))
        .unwrap();
        assert_eq!(
            gray.pixels(),
            &[0x12, 0x12, 0x12, 255, 0xab, 0xab, 0xab, 255]
        );
    }

    #[test]
    fn rejects_zero_oversized_and_excessive_pixel_dimensions() {
        for (w, h) in [
            (0, 1),
            (1, 0),
            (MAX_DIMENSION + 1, 1),
            (1, u32::MAX),
            (8192, 8192),
        ] {
            assert!(matches!(
                RgbaImage::new(w, h, vec![]),
                Err(Error::InvalidDimensions)
            ));
        }
        assert_eq!(rgba_len(8192, 2048).unwrap(), MAX_PIXELS * 4);
    }

    #[test]
    fn rejects_oversized_ihdr_before_missing_image_data() {
        for (width, height) in [(8193, 1), (8192, 8192)] {
            let mut bytes = Vec::new();
            {
                let encoder = png::Encoder::new(&mut bytes, width, height);
                // Header only: the dimensions must be rejected before IDAT parsing.
                let _writer = encoder.write_header().unwrap();
            }
            assert!(matches!(decode_png(&bytes), Err(Error::InvalidDimensions)));
        }
    }

    #[test]
    fn rejects_mismatched_rgba_length() {
        for n in [0, 3, 5] {
            assert!(matches!(
                RgbaImage::new(1, 1, vec![0; n]),
                Err(Error::InvalidRgbaLength)
            ));
        }
    }

    #[test]
    fn rejects_invalid_and_truncated_png() {
        for bytes in [&b""[..], &b"not a PNG"[..], &b"\x89PNG\r\n\x1a\n"[..]] {
            assert!(decode_png(bytes).is_err());
        }
        let png = fixture(png::ColorType::Rgb, png::BitDepth::Eight, &[1; 6]);
        for end in 0..png.len() {
            assert!(decode_png(&png[..end]).is_err(), "length {end}");
        }
    }

    #[test]
    fn rejects_crc_corruption_in_header_data_and_end() {
        let png = fixture(png::ColorType::Rgb, png::BitDepth::Eight, &[1; 6]);
        for index in [29, 41, png.len() - 1] {
            let mut corrupt = png.clone();
            corrupt[index] ^= 1;
            assert!(decode_png(&corrupt).is_err());
        }
    }

    #[test]
    fn rejects_encoded_input_over_limit_before_parsing() {
        assert!(matches!(
            decode_png(&vec![0; MAX_PNG_BYTES + 1]),
            Err(Error::PngTooLarge)
        ));
    }

    #[test]
    fn bounded_writer_refuses_growth_without_partial_write() {
        let mut writer = BoundedWriter {
            bytes: vec![],
            limit: 4,
            exceeded: false,
        };
        writer.write_all(&[1; 4]).unwrap();
        assert!(writer.write_all(&[2]).is_err());
        assert!(writer.exceeded);
        assert_eq!(writer.bytes, [1; 4]);
    }

    #[test]
    fn rejects_apng() {
        let mut bytes = Vec::new();
        {
            let mut encoder = png::Encoder::new(&mut bytes, 1, 1);
            encoder.set_color(png::ColorType::Rgba);
            encoder.set_animated(1, 0).unwrap();
            let mut writer = encoder.write_header().unwrap();
            writer.write_image_data(&[0; 4]).unwrap();
            writer.finish().unwrap();
        }
        assert!(matches!(
            decode_png(&bytes),
            Err(Error::AnimationUnsupported)
        ));
    }
}
