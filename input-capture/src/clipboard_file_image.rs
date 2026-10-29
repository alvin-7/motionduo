//! Content-based, bounded file image conversion; no filesystem or clipboard access.
use image::{ImageDecoder, Limits};
use input_event::clipboard_png::{
    Error, MAX_DIMENSION, MAX_PIXELS, MAX_PNG_BYTES, RgbaImage, decode_png, encode_png,
};
use std::io::Cursor;

pub(crate) fn convert(bytes: &[u8]) -> Result<Vec<u8>, ()> {
    if bytes.len() > MAX_PNG_BYTES {
        return Err(());
    }
    if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        return encode_png(&decode_png(bytes).map_err(|_| ())?).map_err(|_| ());
    }
    if !bytes.starts_with(&[0xff, 0xd8, 0xff]) {
        return Err(());
    }
    let mut decoder = image::codecs::jpeg::JpegDecoder::new(Cursor::new(bytes)).map_err(|_| ())?;
    let (width, height) = decoder.dimensions();
    if width == 0
        || height == 0
        || width > MAX_DIMENSION
        || height > MAX_DIMENSION
        || (width as usize)
            .checked_mul(height as usize)
            .filter(|n| *n <= MAX_PIXELS)
            .is_none()
    {
        return Err(());
    }
    let mut limits = Limits::default();
    limits.max_image_width = Some(MAX_DIMENSION);
    limits.max_image_height = Some(MAX_DIMENSION);
    limits.max_alloc = Some(64 * 1024 * 1024);
    decoder.set_limits(limits).map_err(|_| ())?;
    let orientation = decoder.orientation().map_err(|_| ())?;
    let mut decoded = image::DynamicImage::from_decoder(decoder).map_err(|_| ())?;
    decoded.apply_orientation(orientation);
    let rgba = decoded.into_rgba8();
    let (width, height) = rgba.dimensions();
    let rgba = RgbaImage::new(width, height, rgba.into_raw()).map_err(|_: Error| ())?;
    encode_png(&rgba).map_err(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::metadata::Orientation;

    fn jpeg() -> Vec<u8> {
        let mut out = Vec::new();
        image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, 100)
            .encode(
                &[
                    255, 0, 0, 0, 255, 0, 0, 0, 255, 255, 255, 255, 0, 0, 0, 90, 100, 110,
                ],
                2,
                3,
                image::ExtendedColorType::Rgb8,
            )
            .unwrap();
        out
    }

    fn with_orientation(jpeg: &[u8], value: u16) -> Vec<u8> {
        let mut exif = b"Exif\0\0II\x2a\0\x08\0\0\0\x01\0\x12\x01\x03\0\x01\0\0\0".to_vec();
        exif.extend_from_slice(&value.to_le_bytes());
        exif.extend_from_slice(&[0; 6]);
        let mut out = jpeg[..2].to_vec();
        out.extend_from_slice(&[0xff, 0xe1]);
        out.extend_from_slice(&((exif.len() + 2) as u16).to_be_bytes());
        out.extend_from_slice(&exif);
        out.extend_from_slice(&jpeg[2..]);
        out
    }

    #[test]
    fn jpeg_orientation_and_content_detection() {
        let source = jpeg();
        let plain = decode_png(&convert(&source).unwrap()).unwrap();
        let decoded =
            image::load_from_memory_with_format(&source, image::ImageFormat::Jpeg).unwrap();
        assert_eq!(plain.pixels(), decoded.clone().into_rgba8().as_raw());
        for n in 1..=8 {
            let encoded = convert(&with_orientation(&source, n)).unwrap();
            let actual = decode_png(&encoded).unwrap();
            let mut expected = decoded.clone();
            expected.apply_orientation(Orientation::from_exif(n as u8).unwrap());
            let expected = expected.into_rgba8();
            assert_eq!((actual.width(), actual.height()), expected.dimensions());
            assert_eq!(actual.pixels(), expected.as_raw(), "orientation {n}");
        }
    }

    #[test]
    fn png_alpha_and_rejections() {
        let image = RgbaImage::new(1, 1, vec![1, 2, 3, 4]).unwrap();
        let source = encode_png(&image).unwrap();
        assert_eq!(decode_png(&convert(&source).unwrap()).unwrap(), image);
        assert!(convert(&source[..source.len() - 4]).is_err());
        assert!(convert(&jpeg()[..10]).is_err());
        assert!(convert(b"not an image.jpg").is_err());
        assert!(convert(&vec![0; MAX_PNG_BYTES + 1]).is_err());
    }

    #[test]
    fn jpeg_rejects_dimension_and_pixel_limits_before_decode() {
        let source = jpeg();
        let sof = source.windows(2).position(|w| w == [0xff, 0xc0]).unwrap();
        for (width, height) in [(8193u16, 1u16), (8192, 8192)] {
            let mut oversized = source.clone();
            oversized[sof + 5..sof + 7].copy_from_slice(&height.to_be_bytes());
            oversized[sof + 7..sof + 9].copy_from_slice(&width.to_be_bytes());
            assert!(convert(&oversized).is_err());
        }
    }
}
