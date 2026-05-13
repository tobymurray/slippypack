//! PNG / JPEG decoding to RGB888.
//!
//! Wraps the `image` crate (configured with `default-features = false,
//! features = ["png", "jpeg"]` — see `Cargo.toml`) to keep the dependency
//! surface small and the WASM binary size minimal (see PLAN.md § Phase 4).
//!
//! **Format coverage:** PNG (RGB, RGBA, grayscale, palette, 8-bit) and
//! JPEG (RGB, grayscale, 8-bit). Other formats — TIFF, WebP, AVIF, GIF —
//! are deliberately not compiled in. Input with any unsupported magic
//! bytes (or no recognised magic) produces [`DecodeError::DecodeFailed`].
//!
//! **Alpha handling:** alpha channels are discarded. The downstream
//! `quantise` module sets the output ABGR2222 alpha to fully-opaque
//! (`3`) regardless of input alpha. For a PNG with transparency, this
//! means the RGB channels are used as-is (no compositing over a
//! background colour). Real-world map tiles are essentially always
//! opaque, so this matches what callers expect.
//!
//! **Grayscale / palette inputs:** flattened to RGB via the `image`
//! crate's `to_rgb8` conversion. Grayscale is broadcast across R=G=B;
//! palette indexes are resolved to their per-palette RGB values.

use image::ImageError;

/// A decoded tile with raw RGB888 pixels in row-major order (top-to-bottom,
/// left-to-right). `rgb888.len() == width * height * 3`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DecodedTile {
    pub width: u32,
    pub height: u32,
    pub rgb888: Vec<u8>,
}

impl DecodedTile {
    /// Expected length of `rgb888` for this tile's dimensions, computed
    /// as `width * height * 3` (saturating on overflow).
    #[must_use]
    pub fn expected_byte_count(&self) -> usize {
        (self.width as usize)
            .saturating_mul(self.height as usize)
            .saturating_mul(3)
    }

    /// Borrow the RGB pixel at `(x, y)` as a `[R, G, B]` slice. Returns
    /// `None` if the coordinates are out of bounds.
    #[must_use]
    pub fn pixel(&self, x: u32, y: u32) -> Option<[u8; 3]> {
        if x >= self.width || y >= self.height {
            return None;
        }
        let i = ((y as usize) * (self.width as usize) + (x as usize)) * 3;
        Some([self.rgb888[i], self.rgb888[i + 1], self.rgb888[i + 2]])
    }
}

/// Failure modes for [`decode_rgb888`].
///
/// The variants are deliberately payload-free to keep the public surface
/// stable across `image`-crate version bumps. If detailed context is
/// needed downstream (rare for slippypack's pipeline — decode failures
/// are usually "skip this tile, log the URL, continue"), we can revisit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum DecodeError {
    /// Input is zero-length.
    EmptyInput,
    /// Decoder failed: input is not a recognised PNG or JPEG, is
    /// truncated, or is otherwise malformed.
    DecodeFailed,
    /// The decoded image has zero width or zero height.
    ZeroDimension,
}

impl core::fmt::Display for DecodeError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let msg = match self {
            Self::EmptyInput => "input bytes are empty",
            Self::DecodeFailed => "decoder failed (not PNG/JPEG, truncated, or malformed)",
            Self::ZeroDimension => "decoded image has zero width or zero height",
        };
        f.write_str(msg)
    }
}

impl core::error::Error for DecodeError {}

/// Decode PNG or JPEG bytes into [`DecodedTile`] with RGB888 pixels.
///
/// Format is auto-detected from the input's magic bytes (`image::load_from_memory`).
/// With `image` configured for only `png` + `jpeg` features, any other
/// format yields [`DecodeError::DecodeFailed`].
///
/// # Errors
///
/// - [`DecodeError::EmptyInput`] for zero-length input.
/// - [`DecodeError::DecodeFailed`] for unrecognised, truncated, or malformed input.
/// - [`DecodeError::ZeroDimension`] if the decoder accepts a 0×N or N×0 image.
pub fn decode_rgb888(bytes: &[u8]) -> Result<DecodedTile, DecodeError> {
    if bytes.is_empty() {
        return Err(DecodeError::EmptyInput);
    }
    let dynamic = image::load_from_memory(bytes).map_err(map_image_err)?;
    let rgb = dynamic.to_rgb8();
    let width = rgb.width();
    let height = rgb.height();
    if width == 0 || height == 0 {
        return Err(DecodeError::ZeroDimension);
    }
    let rgb888 = rgb.into_raw();
    Ok(DecodedTile {
        width,
        height,
        rgb888,
    })
}

fn map_image_err(_err: ImageError) -> DecodeError {
    // The image crate's error enum carries useful detail (truncation site,
    // format limit, etc) but exposing it ties slippypack's public surface
    // to the dep's version. For v1 we coalesce all decoder failures into
    // a single variant. If/when detailed errors are needed downstream we
    // can grow a richer enum without breaking the simple case.
    DecodeError::DecodeFailed
}

#[cfg(test)]
mod tests {
    use super::{DecodeError, DecodedTile, decode_rgb888};

    /// 2×2 RGB PNG with known pixel values:
    /// `(0,0)`=red, `(1,0)`=green, `(0,1)`=blue, `(1,1)`=white.
    /// 93 bytes, metadata stripped, palette-indexed PNG (color type 3)
    /// — exercises `image::to_rgb8`'s palette → RGB conversion path.
    const FIXTURE_PNG_2X2: &[u8] = &[
        0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0x00, 0x00, 0x00, 0x0d, 0x49, 0x48, 0x44,
        0x52, 0x00, 0x00, 0x00, 0x02, 0x00, 0x00, 0x00, 0x02, 0x02, 0x03, 0x00, 0x00, 0x00, 0x0f,
        0xd8, 0xe5, 0xb7, 0x00, 0x00, 0x00, 0x0c, 0x50, 0x4c, 0x54, 0x45, 0xff, 0x00, 0x00, 0x00,
        0xff, 0x00, 0x00, 0x00, 0xff, 0xff, 0xff, 0xff, 0xfb, 0x00, 0x60, 0xf6, 0x00, 0x00, 0x00,
        0x0c, 0x49, 0x44, 0x41, 0x54, 0x08, 0xd7, 0x63, 0x10, 0x60, 0xd8, 0x00, 0x00, 0x00, 0xe4,
        0x00, 0xc1, 0xf6, 0x8b, 0xf7, 0x08, 0x00, 0x00, 0x00, 0x00, 0x49, 0x45, 0x4e, 0x44, 0xae,
        0x42, 0x60, 0x82,
    ];

    /// 2×2 JPEG of the same RGB pattern, quality 95. Pixel values are
    /// **approximate** (JPEG is lossy); the test tolerates per-channel
    /// drift via [`assert_pixel_close`].
    const FIXTURE_JPEG_2X2: &[u8] = &[
        0xff, 0xd8, 0xff, 0xe0, 0x00, 0x10, 0x4a, 0x46, 0x49, 0x46, 0x00, 0x01, 0x01, 0x00, 0x00,
        0x01, 0x00, 0x01, 0x00, 0x00, 0xff, 0xdb, 0x00, 0x43, 0x00, 0x02, 0x01, 0x01, 0x01, 0x01,
        0x01, 0x02, 0x01, 0x01, 0x01, 0x02, 0x02, 0x02, 0x02, 0x02, 0x04, 0x03, 0x02, 0x02, 0x02,
        0x02, 0x05, 0x04, 0x04, 0x03, 0x04, 0x06, 0x05, 0x06, 0x06, 0x06, 0x05, 0x06, 0x06, 0x06,
        0x07, 0x09, 0x08, 0x06, 0x07, 0x09, 0x07, 0x06, 0x06, 0x08, 0x0b, 0x08, 0x09, 0x0a, 0x0a,
        0x0a, 0x0a, 0x0a, 0x06, 0x08, 0x0b, 0x0c, 0x0b, 0x0a, 0x0c, 0x09, 0x0a, 0x0a, 0x0a, 0xff,
        0xdb, 0x00, 0x43, 0x01, 0x02, 0x02, 0x02, 0x02, 0x02, 0x02, 0x05, 0x03, 0x03, 0x05, 0x0a,
        0x07, 0x06, 0x07, 0x0a, 0x0a, 0x0a, 0x0a, 0x0a, 0x0a, 0x0a, 0x0a, 0x0a, 0x0a, 0x0a, 0x0a,
        0x0a, 0x0a, 0x0a, 0x0a, 0x0a, 0x0a, 0x0a, 0x0a, 0x0a, 0x0a, 0x0a, 0x0a, 0x0a, 0x0a, 0x0a,
        0x0a, 0x0a, 0x0a, 0x0a, 0x0a, 0x0a, 0x0a, 0x0a, 0x0a, 0x0a, 0x0a, 0x0a, 0x0a, 0x0a, 0x0a,
        0x0a, 0x0a, 0x0a, 0x0a, 0x0a, 0x0a, 0x0a, 0x0a, 0xff, 0xc0, 0x00, 0x11, 0x08, 0x00, 0x02,
        0x00, 0x02, 0x03, 0x01, 0x11, 0x00, 0x02, 0x11, 0x01, 0x03, 0x11, 0x01, 0xff, 0xc4, 0x00,
        0x14, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x00, 0x09, 0xff, 0xc4, 0x00, 0x1c, 0x10, 0x00, 0x02, 0x02, 0x03, 0x01, 0x01,
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01, 0x02, 0x04, 0x06, 0x03,
        0x05, 0x07, 0x09, 0x00, 0xff, 0xc4, 0x00, 0x15, 0x01, 0x01, 0x01, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x05, 0x06, 0xff, 0xc4, 0x00,
        0x1e, 0x11, 0x00, 0x03, 0x00, 0x02, 0x02, 0x03, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x00, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x00, 0x11, 0x21, 0x12, 0xff,
        0xda, 0x00, 0x0c, 0x03, 0x01, 0x00, 0x02, 0x11, 0x03, 0x11, 0x00, 0x3f, 0x00, 0x58, 0x3c,
        0xe0, 0xe1, 0x1c, 0x3e, 0xcb, 0xe7, 0x8f, 0x05, 0xb1, 0x58, 0xb8, 0xd5, 0x53, 0x61, 0xb0,
        0xd8, 0x71, 0x7a, 0xb4, 0x99, 0xf3, 0xe6, 0xd7, 0x63, 0x65, 0xcd, 0x27, 0x33, 0xea, 0x62,
        0xb3, 0xe4, 0xc8, 0xec, 0x85, 0x9d, 0xd9, 0x89, 0x62, 0xc4, 0x92, 0x49, 0x24, 0xfc, 0x87,
        0x68, 0xf5, 0xef, 0x01, 0xc5, 0xec, 0xcd, 0xdc, 0x21, 0xa9, 0xc6, 0x44, 0x4c, 0xcc, 0x90,
        0xaa, 0x21, 0x20, 0x00, 0x16, 0x70, 0x00, 0x01, 0x3d, 0x00, 0x07, 0xc0, 0x07, 0xc0, 0x3c,
        0x8f, 0xc8, 0xe1, 0x7c, 0x3b, 0x7f, 0x77, 0xd9, 0xec, 0xf5, 0xb8, 0xf7, 0xc9, 0xb9, 0x34,
        0xad, 0x69, 0x19, 0xbd, 0x29, 0x47, 0x3f, 0xa7, 0xa5, 0x1d, 0x94, 0xb3, 0xbb, 0xb1, 0x2c,
        0xee, 0xc4, 0xb3, 0x31, 0x24, 0x92, 0x4f, 0x9f, 0xff, 0xd9,
    ];

    /// JPEG-tolerance comparison: each channel may differ from the
    /// target by at most `JPEG_PER_CHANNEL_TOLERANCE`.
    const JPEG_PER_CHANNEL_TOLERANCE: u8 = 16;

    fn assert_pixel_close(actual: [u8; 3], expected: [u8; 3], label: &str) {
        for i in 0..3 {
            let diff = actual[i].abs_diff(expected[i]);
            assert!(
                diff <= JPEG_PER_CHANNEL_TOLERANCE,
                "{label}: channel {i} drifted by {diff} (actual {} vs expected {})",
                actual[i],
                expected[i],
            );
        }
    }

    #[test]
    fn decodes_png_fixture_with_expected_dimensions() {
        let tile = decode_rgb888(FIXTURE_PNG_2X2).expect("PNG fixture should decode");
        assert_eq!(tile.width, 2);
        assert_eq!(tile.height, 2);
        assert_eq!(tile.rgb888.len(), 12);
        assert_eq!(tile.expected_byte_count(), 12);
    }

    #[test]
    fn decodes_png_fixture_with_expected_pixels() {
        let tile = decode_rgb888(FIXTURE_PNG_2X2).expect("PNG fixture should decode");
        // PNG is lossless — pixels are exact.
        assert_eq!(tile.pixel(0, 0), Some([255, 0, 0]), "(0,0) should be red");
        assert_eq!(tile.pixel(1, 0), Some([0, 255, 0]), "(1,0) should be green");
        assert_eq!(tile.pixel(0, 1), Some([0, 0, 255]), "(0,1) should be blue");
        assert_eq!(
            tile.pixel(1, 1),
            Some([255, 255, 255]),
            "(1,1) should be white"
        );
    }

    #[test]
    fn decodes_jpeg_fixture_with_expected_dimensions() {
        let tile = decode_rgb888(FIXTURE_JPEG_2X2).expect("JPEG fixture should decode");
        assert_eq!(tile.width, 2);
        assert_eq!(tile.height, 2);
        assert_eq!(tile.rgb888.len(), 12);
    }

    #[test]
    fn decodes_jpeg_fixture_with_pixels_close_to_expected() {
        let tile = decode_rgb888(FIXTURE_JPEG_2X2).expect("JPEG fixture should decode");
        // JPEG is lossy — pixels are approximate.
        assert_pixel_close(tile.pixel(0, 0).unwrap(), [255, 0, 0], "(0,0) red");
        assert_pixel_close(tile.pixel(1, 0).unwrap(), [0, 255, 0], "(1,0) green");
        assert_pixel_close(tile.pixel(0, 1).unwrap(), [0, 0, 255], "(0,1) blue");
        assert_pixel_close(tile.pixel(1, 1).unwrap(), [255, 255, 255], "(1,1) white");
    }

    #[test]
    fn empty_input_returns_empty_input_error() {
        assert_eq!(decode_rgb888(&[]), Err(DecodeError::EmptyInput));
    }

    #[test]
    fn random_garbage_returns_decode_failed() {
        // Bytes that don't match any image format's magic.
        let bytes = [0xa1, 0xb2, 0xc3, 0xd4, 0xe5, 0xf6, 0x07, 0x08];
        assert_eq!(decode_rgb888(&bytes), Err(DecodeError::DecodeFailed));
    }

    #[test]
    fn truncated_png_returns_decode_failed() {
        // First 30 bytes of the PNG fixture — past the signature but before
        // a complete IHDR.
        let truncated = &FIXTURE_PNG_2X2[..30];
        assert_eq!(decode_rgb888(truncated), Err(DecodeError::DecodeFailed));
    }

    #[test]
    fn truncated_jpeg_returns_decode_failed() {
        // First 20 bytes — JPEG SOI + APP0 header, missing actual image data.
        let truncated = &FIXTURE_JPEG_2X2[..20];
        assert_eq!(decode_rgb888(truncated), Err(DecodeError::DecodeFailed));
    }

    #[test]
    fn webp_magic_returns_decode_failed() {
        // RIFF...WEBP magic. The image crate is compiled without WebP
        // support, so this must fail rather than silently decode.
        let webp_header: &[u8] = b"RIFF\x00\x00\x00\x00WEBPVP8 ";
        assert_eq!(decode_rgb888(webp_header), Err(DecodeError::DecodeFailed));
    }

    #[test]
    fn bmp_magic_returns_decode_failed() {
        // BMP magic — also not enabled in the image crate features.
        let bmp_header: &[u8] = b"BM\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00";
        assert_eq!(decode_rgb888(bmp_header), Err(DecodeError::DecodeFailed));
    }

    #[test]
    fn pixel_out_of_bounds_returns_none() {
        let tile = decode_rgb888(FIXTURE_PNG_2X2).unwrap();
        assert_eq!(tile.pixel(2, 0), None);
        assert_eq!(tile.pixel(0, 2), None);
        assert_eq!(tile.pixel(u32::MAX, 0), None);
    }

    #[test]
    fn expected_byte_count_saturates_on_overflow() {
        // A pathologically large tile description — function should not panic.
        let tile = DecodedTile {
            width: u32::MAX,
            height: u32::MAX,
            rgb888: vec![],
        };
        // Saturating arithmetic gives usize::MAX (or close), not a panic.
        let _ = tile.expected_byte_count();
    }

    #[test]
    fn decode_error_display_is_human_readable() {
        let cases = [
            (DecodeError::EmptyInput, "input bytes are empty"),
            (
                DecodeError::DecodeFailed,
                "decoder failed (not PNG/JPEG, truncated, or malformed)",
            ),
            (
                DecodeError::ZeroDimension,
                "decoded image has zero width or zero height",
            ),
        ];
        for (err, expected) in cases {
            assert_eq!(format!("{err}"), expected);
        }
    }

    #[test]
    fn decode_error_implements_error_trait() {
        // Compile-time check that DecodeError is `Error` (for `?` propagation
        // in callers that want it).
        fn assert_error<E: core::error::Error>(_: &E) {}
        assert_error(&DecodeError::EmptyInput);
    }
}
