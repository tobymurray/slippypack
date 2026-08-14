//! slippypack-web — base WASM module for the PWA.
//!
//! See `PLAN.md` § Phase 4 — PWA WASM bootstrapping.
//!
//! This crate is a **facade, not a pipeline**. Every byte-producing
//! decision — quantisation, compression, hashing, container layout,
//! `pack_uuid` derivation — lives in [`slippypack_core::builder`], so
//! that the CLI and the browser cannot drift. What lives here is the
//! `wasm-bindgen` boundary and the RGBA-shaped entry point a canvas
//! readback actually has.
//!
//! # The pixel layout, which is easy to get wrong
//!
//! A canvas `ImageData.data` is **RGBA8888 in memory order** — byte 0
//! is red, byte 3 is alpha — fixed by the HTML spec regardless of the
//! machine's endianness. That is *not* "ARGB8888", which names the
//! 32-bit integer `0xAARRGGBB` and is `B, G, R, A` in memory on a
//! little-endian machine. The pack's own `ABGR2222` is a third
//! convention again (packed bits, alpha in the high two). Three
//! similar-looking names, three different layouts; [`PackBuilder`]
//! takes the canvas one.
//!
//! Alpha is dropped rather than blended, and a pixel that is not fully
//! opaque is an error: v1 packs are opaque, so a transparent pixel
//! means the page rendered without opaque ground beneath it and the
//! colour channels are not what anyone intended.
//!
//! # Feed tiles in sorted order
//!
//! [`PackBuilder::add_tile_rgba`] requires ascending `(z, x, y)`,
//! because the source `content_hash` is a hash of a byte *stream* and
//! the order is therefore part of the value. A renderer that works in
//! blocks — which is the only kind fast enough, per the 2026-08-14 X4
//! investigation — must sort a block-row before feeding it.

use slippypack_core::builder::{BuilderError, PackBuilder as CorePackBuilder, PackBuilderConfig};
use slippypack_core::format::Compression;
use slippypack_core::identity::{
    BoundingBox, FormatVersion, PackDescriptor, Source, ZoomRange,
};
use slippypack_core::quantise::{Abgr2222Palette, PaletteSlot, Quantiser};
use wasm_bindgen::prelude::*;

/// Degrees → integer microdegrees, the descriptor's bbox unit.
///
/// Half-away-from-zero rounding, matching what the CLI's `to_micro`
/// does for the same values. Two inputs differing by less than 1e-6°
/// collapse to the same descriptor and therefore the same `pack_uuid`.
fn to_micro(degrees: f64) -> i32 {
    let scaled = degrees * 1_000_000.0;
    #[allow(clippy::cast_possible_truncation)]
    {
        scaled.round() as i32
    }
}

/// Builds one `.rawtiles` pack from tiles rendered in the browser.
///
/// ```js
/// const b = new PackBuilder(128, 13, 17, bbox, paletteRgb, paletteCodes, styleHash, attr, ts);
/// for (const t of tilesSortedByZxy) b.add_tile_rgba(t.z, t.x, t.y, t.pixels);
/// const bytes = b.finish();   // Uint8Array
/// ```
#[wasm_bindgen]
pub struct PackBuilder {
    inner: CorePackBuilder,
    quantiser_version: u32,
    pixel_format: u8,
    bbox: BoundingBox,
    zoom_min: u8,
    zoom_max: u8,
    tile_dim_px: u16,
    style_hash: [u8; 32],
}

#[wasm_bindgen]
impl PackBuilder {
    /// Create a builder.
    ///
    /// - `bbox_deg` is `[min_lon, min_lat, max_lon, max_lat]` in degrees.
    /// - `palette_rgb` is `3N` bytes, R-G-B per declared slot;
    ///   `palette_codes` is the matching `N` ABGR2222 bytes. Order is
    ///   significant — it breaks ties in the nearest-slot search, so it
    ///   must match the style's declaration order to stay reproducible.
    /// - `style_hash` is the 32-byte SHA-256 of the MapLibre style JSON.
    /// - `build_timestamp` is Unix seconds (as `f64` because JS has no
    ///   `u64`; fractional parts are truncated).
    ///
    /// # Errors
    ///
    /// Returns a JS error for a malformed palette, a bad `style_hash`
    /// length, or an invalid tile dimension or zoom range.
    #[wasm_bindgen(constructor)]
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        tile_dim_px: u16,
        zoom_min: u8,
        zoom_max: u8,
        bbox_deg: &[f64],
        palette_rgb: &[u8],
        palette_codes: &[u8],
        style_hash: &[u8],
        attribution: Option<String>,
        build_timestamp: f64,
    ) -> Result<PackBuilder, JsError> {
        if bbox_deg.len() != 4 {
            return Err(JsError::new(
                "bbox_deg must be [min_lon, min_lat, max_lon, max_lat]",
            ));
        }
        if palette_rgb.len() != palette_codes.len() * 3 {
            return Err(JsError::new(
                "palette_rgb must be exactly 3 bytes per entry in palette_codes",
            ));
        }
        let style_hash: [u8; 32] = style_hash
            .try_into()
            .map_err(|_| JsError::new("style_hash must be 32 bytes (SHA-256)"))?;

        let slots: Vec<PaletteSlot> = palette_rgb
            .chunks_exact(3)
            .zip(palette_codes)
            .map(|(rgb, &code)| PaletteSlot {
                rgb: [rgb[0], rgb[1], rgb[2]],
                code,
            })
            .collect();
        let quantiser = Abgr2222Palette::new(&slots)?;
        let quantiser_version = quantiser.version();
        let pixel_format = quantiser.pixel_format();

        let inner = CorePackBuilder::new(PackBuilderConfig {
            tile_dim_px,
            zoom_range: (zoom_min, zoom_max),
            compression: Compression::Rle8,
            quantiser: Box::new(quantiser),
            attribution,
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            build_timestamp: build_timestamp as u64,
        })?;

        Ok(Self {
            inner,
            quantiser_version,
            pixel_format,
            bbox: BoundingBox {
                min_lon_micro: to_micro(bbox_deg[0]),
                min_lat_micro: to_micro(bbox_deg[1]),
                max_lon_micro: to_micro(bbox_deg[2]),
                max_lat_micro: to_micro(bbox_deg[3]),
            },
            zoom_min,
            zoom_max,
            tile_dim_px,
            style_hash,
        })
    }

    /// Add one tile from a canvas `ImageData.data` slice (RGBA8888).
    ///
    /// Tiles must arrive in ascending `(z, x, y)`.
    ///
    /// # Errors
    ///
    /// Returns a JS error if the buffer is the wrong size, the tile is
    /// out of order or out of the zoom range, or any pixel is not fully
    /// opaque.
    pub fn add_tile_rgba(&mut self, z: u8, x: u32, y: u32, rgba: &[u8]) -> Result<(), JsError> {
        self.inner.add_tile_rgba8888(z, x, y, rgba)?;
        Ok(())
    }

    /// Number of tiles accepted so far — for progress reporting.
    #[wasm_bindgen(getter)]
    #[must_use]
    pub fn tile_count(&self) -> usize {
        self.inner.tile_count()
    }

    /// SHA-256 of the pre-quantisation RGB888 stream fed so far.
    ///
    /// This is what [`Self::finish`] puts in the descriptor's source
    /// `content_hash`. Exposed so a caller can log or compare it
    /// without building the pack.
    #[must_use]
    pub fn rendered_content_hash(&self) -> Vec<u8> {
        self.inner.rendered_content_hash().to_vec()
    }

    /// Assemble the pack and return its bytes.
    ///
    /// The descriptor identifies both halves of what made this pack:
    /// `Source::Style::content_hash` is the pre-quantisation RGB888
    /// stream that was rendered, and `style_hash` is the style that
    /// rendered it. Keeping them separate is what stops two renders of
    /// the same style — which the X4 investigation measured differing
    /// by 2–7 pixels per tile at different block sizes — from deriving
    /// the same `pack_uuid`. See `DECISIONS.md` I-011.
    ///
    /// # Errors
    ///
    /// Returns a JS error if no tiles were added or the writer rejects
    /// the pack.
    pub fn finish(self) -> Result<Vec<u8>, JsError> {
        let descriptor = PackDescriptor {
            affn: None,
            bbox: self.bbox,
            format_version: FormatVersion { major: 1, minor: 0 },
            pixel_format: self.pixel_format,
            projection: 1,
            quantiser_version: self.quantiser_version,
            sources: vec![Source::Style {
                content_hash: self.inner.rendered_content_hash(),
                zoom_min: self.zoom_min,
                zoom_max: self.zoom_max,
            }],
            style_hash: Some(self.style_hash),
            tile_addressing_scheme: 1,
            tile_axis_convention: 1,
            tile_dim_px: self.tile_dim_px,
            zoom_range: ZoomRange {
                min: self.zoom_min,
                max: self.zoom_max,
            },
        };
        Ok(self.inner.finish(&descriptor)?)
    }
}

/// `BuilderError` and `PaletteError` reach JS as plain `Error`s with
/// their `Display` text, which is written to be actionable.
fn _assert_errors_convert(e: BuilderError) -> JsError {
    JsError::from(e)
}

#[cfg(test)]
mod tests {
    use super::to_micro;

    #[test]
    fn degrees_round_to_microdegrees() {
        assert_eq!(to_micro(-76.015), -76_015_000);
        assert_eq!(to_micro(44.662), 44_662_000);
        assert_eq!(to_micro(0.0), 0);
    }

    /// Two coordinates closer than a microdegree collapse to the same
    /// descriptor value, and therefore the same `pack_uuid`.
    #[test]
    fn sub_microdegree_differences_collapse() {
        assert_eq!(to_micro(44.662_000_1), to_micro(44.662_000_4));
    }
}
