//! Tile-stream → `.rawtiles` assembly: the pipeline both front-ends run.
//!
//! [`crate::format`] gives you a writer that takes *already encoded*
//! tile bytes. This module is the layer above it — the one that turns
//! rendered pixels into a finished pack:
//!
//! ```text
//! RGBA8888 / RGB888 pixels
//!   └─ hash the pre-quantisation RGB888 stream   (source content_hash)
//!        └─ quantise                              (canonical or palette-snap)
//!             └─ compress                         (None or spec § 9.11 RLE8)
//!                  └─ RawtilesWriter              (header, index, extensions, CRC)
//!                       └─ .rawtiles bytes
//! ```
//!
//! It lives in `slippypack-core`, not in either front-end, because
//! "one core library, two front-ends, both writing byte-identical
//! packs" is only true if the *whole* pipeline is shared. A CLI that
//! quantises in Rust and a PWA that quantises in JavaScript would agree
//! on the container and disagree on the pixels, which is the one
//! disagreement `pack_uuid` cannot express.
//!
//! # Tile order is part of the contract
//!
//! [`PackBuilder::add_tile_rgb888`] requires tiles in ascending
//! `(z, x, y)` order and rejects anything else. Two reasons, and the
//! second is the load-bearing one:
//!
//! - It is the pack's own order (spec § 5.2's sorted index), so the
//!   writer does no work to honour it.
//! - **The source `content_hash` is a hash of a byte stream, so the
//!   stream's order is part of the value.** Sorted `(z, x, y)` is the
//!   only order that is a property of the pack rather than of whatever
//!   loop the caller happened to write. A caller that renders in some
//!   other order — a tiled renderer working in blocks, say — must sort
//!   before feeding, not after.
//!
//! # What this module does not decide
//!
//! It does not build the [`PackDescriptor`]. [`PackBuilder::finish`]
//! takes one, and [`PackBuilder::rendered_content_hash`] hands back the
//! hash to put in it. That split is deliberate: which `Source` variant
//! describes "a vector archive rendered through a style" is an open
//! question against the rawtiles spec (its § A.4 classifies `pmtiles`
//! and `mbtiles` as *raster* sources and reserves vector rendering for
//! a future minor), and this module should not answer it by accident.
//! The caller answers it explicitly.

use alloc::boxed::Box;
use alloc::string::String;
use alloc::vec::Vec;
use core::convert::Infallible;

use sha2::{Digest, Sha256};

use crate::format::{
    AddressingScheme, AxisConvention, Compression, PackMetadata, Projection, RawtilesWriter,
    TAG_ATTR, TileContent, TileWriter, TileWriterError, rle8,
};
use crate::identity::{PackDescriptor, derive_pack_uuid};
use crate::quantise::Quantiser;

/// How a pack under construction is configured. Everything here is
/// fixed for the pack's lifetime; per-tile data arrives through
/// [`PackBuilder::add_tile_rgb888`].
pub struct PackBuilderConfig {
    /// Tile edge length in pixels. Every tile fed in must be exactly
    /// `tile_dim_px²` pixels.
    pub tile_dim_px: u16,
    /// Inclusive zoom range. Tiles outside it are rejected.
    pub zoom_range: (u8, u8),
    /// Per-tile compression. [`Compression::Rle8`] is spec § 9.11 and
    /// is what every watch-targeted pack should use.
    pub compression: Compression,
    /// The quantiser that turns RGB888 into the pack's pixel format.
    /// Its `pixel_format()` and `version()` must match the descriptor
    /// passed to [`PackBuilder::finish`].
    pub quantiser: Box<dyn Quantiser>,
    /// Attribution string for the `ATTR` extension (spec § 7.3).
    /// `None` writes no section — which is only correct for packs whose
    /// data carries no attribution obligation.
    pub attribution: Option<String>,
    /// `build_timestamp` for the header.
    pub build_timestamp: u64,
}

/// Errors from [`PackBuilder`].
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum BuilderError {
    /// `tile_dim_px` was zero, or `tile_dim_px²` overflowed `usize`.
    InvalidTileDim { tile_dim_px: u16 },
    /// `zoom_range.0 > zoom_range.1`.
    InvalidZoomRange { min: u8, max: u8 },
    /// A tile's pixel buffer was the wrong length.
    TileSize { expected: usize, got: usize },
    /// A tile arrived at or before the previous tile in `(z, x, y)`
    /// order. See the module docs for why this is rejected rather than
    /// sorted.
    OutOfOrder {
        previous: (u8, u32, u32),
        got: (u8, u32, u32),
    },
    /// A tile's zoom was outside the configured range.
    ZoomOutOfRange { z: u8, min: u8, max: u8 },
    /// An RGBA8888 tile carried a pixel that was not fully opaque. v1
    /// packs are opaque (spec § 9.1); a transparent pixel means the
    /// caller's canvas had no opaque ground beneath it, and its colour
    /// channels are not the colour anyone intended.
    NotOpaque { index: usize, alpha: u8 },
    /// The descriptor handed to [`PackBuilder::finish`] disagrees with
    /// how the builder was configured, which would produce a
    /// `pack_uuid` that does not describe the bytes.
    DescriptorMismatch { field: &'static str },
    /// No tiles were added.
    Empty,
    /// The underlying writer rejected something.
    Writer(String),
}

impl core::fmt::Display for BuilderError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::InvalidTileDim { tile_dim_px } => {
                write!(f, "invalid tile_dim_px: {tile_dim_px}")
            }
            Self::InvalidZoomRange { min, max } => {
                write!(f, "zoom range min ({min}) exceeds max ({max})")
            }
            Self::TileSize { expected, got } => {
                write!(f, "tile buffer is {got} bytes; expected {expected}")
            }
            Self::OutOfOrder { previous, got } => write!(
                f,
                "tile {got:?} does not follow {previous:?} in ascending (z, x, y) order; \
                 tiles must be fed sorted because the source content_hash is order-dependent",
            ),
            Self::ZoomOutOfRange { z, min, max } => {
                write!(f, "tile zoom {z} is outside the pack's range {min}..={max}")
            }
            Self::NotOpaque { index, alpha } => write!(
                f,
                "pixel {index} has alpha {alpha}; v1 packs are opaque and RGBA input \
                 must be fully opaque",
            ),
            Self::DescriptorMismatch { field } => write!(
                f,
                "descriptor field `{field}` does not match the builder's configuration",
            ),
            Self::Empty => f.write_str("pack has no tiles"),
            Self::Writer(msg) => write!(f, "writer rejected the pack: {msg}"),
        }
    }
}

impl core::error::Error for BuilderError {}

impl<S: core::fmt::Debug, O: core::fmt::Debug> From<TileWriterError<S, O>> for BuilderError {
    fn from(e: TileWriterError<S, O>) -> Self {
        Self::Writer(alloc::format!("{e:?}"))
    }
}

/// Assembles a `.rawtiles` pack from a sorted stream of rendered tiles.
///
/// See the [module docs](self) for the pipeline and the ordering
/// contract.
pub struct PackBuilder {
    tile_dim_px: u16,
    pixels_per_tile: usize,
    zoom_min: u8,
    zoom_max: u8,
    compression: Compression,
    quantiser: Box<dyn Quantiser>,
    attribution: Option<String>,
    build_timestamp: u64,

    /// SHA-256 over the pre-quantisation RGB888 stream, in the order
    /// tiles were fed — which the ordering check pins to sorted
    /// `(z, x, y)`.
    hasher: Sha256,
    previous_key: Option<(u8, u32, u32)>,
    tiles: Vec<(u8, u32, u32, Vec<u8>)>,

    /// Reused across tiles so a 160,000-tile build does not allocate
    /// 160,000 scratch buffers.
    rgb_scratch: Vec<u8>,
    quantise_scratch: Vec<u8>,
}

impl PackBuilder {
    /// Create a builder.
    ///
    /// # Errors
    ///
    /// - [`BuilderError::InvalidTileDim`] if `tile_dim_px` is zero or its
    ///   square overflows `usize`.
    /// - [`BuilderError::InvalidZoomRange`] if `min > max`.
    pub fn new(config: PackBuilderConfig) -> Result<Self, BuilderError> {
        let PackBuilderConfig {
            tile_dim_px,
            zoom_range: (zoom_min, zoom_max),
            compression,
            quantiser,
            attribution,
            build_timestamp,
        } = config;

        let pixels_per_tile = usize::from(tile_dim_px)
            .checked_mul(usize::from(tile_dim_px))
            .filter(|n| *n > 0)
            .ok_or(BuilderError::InvalidTileDim { tile_dim_px })?;

        if zoom_min > zoom_max {
            return Err(BuilderError::InvalidZoomRange {
                min: zoom_min,
                max: zoom_max,
            });
        }

        let bytes_per_pixel = quantiser.bytes_per_pixel();
        Ok(Self {
            tile_dim_px,
            pixels_per_tile,
            zoom_min,
            zoom_max,
            compression,
            quantiser,
            attribution,
            build_timestamp,
            hasher: Sha256::new(),
            previous_key: None,
            tiles: Vec::new(),
            rgb_scratch: Vec::new(),
            quantise_scratch: alloc::vec![0_u8; pixels_per_tile * bytes_per_pixel],
        })
    }

    /// Number of tiles accepted so far.
    #[must_use]
    pub fn tile_count(&self) -> usize {
        self.tiles.len()
    }

    /// SHA-256 of the pre-quantisation RGB888 stream fed so far.
    ///
    /// Call this after the last tile to get the value that belongs in
    /// the descriptor's source `content_hash`. Cloning the hasher means
    /// this is a peek, not a consume — the builder stays usable.
    #[must_use]
    pub fn rendered_content_hash(&self) -> [u8; 32] {
        self.hasher.clone().finalize().into()
    }

    /// Add one tile from a flat RGB888 buffer (3 bytes per pixel).
    ///
    /// # Errors
    ///
    /// - [`BuilderError::TileSize`] if the buffer is not
    ///   `tile_dim_px² × 3` bytes.
    /// - [`BuilderError::ZoomOutOfRange`] if `z` is outside the pack's
    ///   zoom range.
    /// - [`BuilderError::OutOfOrder`] if `(z, x, y)` does not strictly
    ///   follow the previous tile.
    pub fn add_tile_rgb888(
        &mut self,
        z: u8,
        x: u32,
        y: u32,
        rgb888: &[u8],
    ) -> Result<(), BuilderError> {
        let expected = self.pixels_per_tile * 3;
        if rgb888.len() != expected {
            return Err(BuilderError::TileSize {
                expected,
                got: rgb888.len(),
            });
        }
        if z < self.zoom_min || z > self.zoom_max {
            return Err(BuilderError::ZoomOutOfRange {
                z,
                min: self.zoom_min,
                max: self.zoom_max,
            });
        }
        let key = (z, x, y);
        if let Some(previous) = self.previous_key
            && key <= previous
        {
            return Err(BuilderError::OutOfOrder { previous, got: key });
        }
        self.previous_key = Some(key);

        // Hash before quantising: § A.4 pins the pre-quantisation stream.
        self.hasher.update(rgb888);

        self.quantiser.quantise(rgb888, &mut self.quantise_scratch);
        let encoded = match self.compression {
            Compression::None => self.quantise_scratch.clone(),
            Compression::Rle8 => rle8::encode(&self.quantise_scratch),
        };
        self.tiles.push((z, x, y, encoded));
        Ok(())
    }

    /// Add one tile from a flat RGBA8888 buffer (4 bytes per pixel,
    /// R-G-B-A in memory order — what a canvas `ImageData` hands you).
    ///
    /// Alpha is dropped, not blended: v1 packs are opaque, so every
    /// pixel must already be. The RGB888 that remains is what gets
    /// hashed, so a pack built this way is byte-identical to one built
    /// from the same pixels through [`Self::add_tile_rgb888`].
    ///
    /// # Errors
    ///
    /// As [`Self::add_tile_rgb888`], plus [`BuilderError::NotOpaque`] if
    /// any pixel's alpha is not `255`.
    pub fn add_tile_rgba8888(
        &mut self,
        z: u8,
        x: u32,
        y: u32,
        rgba8888: &[u8],
    ) -> Result<(), BuilderError> {
        let expected = self.pixels_per_tile * 4;
        if rgba8888.len() != expected {
            return Err(BuilderError::TileSize {
                expected,
                got: rgba8888.len(),
            });
        }
        self.rgb_scratch.clear();
        self.rgb_scratch.reserve(self.pixels_per_tile * 3);
        for (index, px) in rgba8888.chunks_exact(4).enumerate() {
            if px[3] != 0xFF {
                return Err(BuilderError::NotOpaque {
                    index,
                    alpha: px[3],
                });
            }
            self.rgb_scratch.extend_from_slice(&px[..3]);
        }
        // `rgb_scratch` is borrowed immutably below while `self` is
        // borrowed mutably, so hand the delegate its own slice.
        let rgb = core::mem::take(&mut self.rgb_scratch);
        let result = self.add_tile_rgb888(z, x, y, &rgb);
        self.rgb_scratch = rgb;
        result
    }

    /// Assemble the pack and return its bytes.
    ///
    /// `descriptor` is the caller's — see the [module docs](self) for
    /// why this module does not build it. Its `tile_dim_px`,
    /// `zoom_range`, `pixel_format` and `quantiser_version` are checked
    /// against the builder's configuration, because a descriptor that
    /// disagrees derives a `pack_uuid` describing a pack that was never
    /// built.
    ///
    /// # Errors
    ///
    /// - [`BuilderError::Empty`] if no tiles were added.
    /// - [`BuilderError::DescriptorMismatch`] if the descriptor
    ///   contradicts the builder's configuration.
    /// - [`BuilderError::Writer`] if the underlying writer rejects the
    ///   pack.
    pub fn finish(self, descriptor: &PackDescriptor) -> Result<Vec<u8>, BuilderError> {
        if self.tiles.is_empty() {
            return Err(BuilderError::Empty);
        }
        if descriptor.tile_dim_px != self.tile_dim_px {
            return Err(BuilderError::DescriptorMismatch {
                field: "tile_dim_px",
            });
        }
        if descriptor.zoom_range.min != self.zoom_min || descriptor.zoom_range.max != self.zoom_max
        {
            return Err(BuilderError::DescriptorMismatch {
                field: "zoom_range",
            });
        }
        if descriptor.pixel_format != self.quantiser.pixel_format() {
            return Err(BuilderError::DescriptorMismatch {
                field: "pixel_format",
            });
        }
        if descriptor.quantiser_version != self.quantiser.version() {
            return Err(BuilderError::DescriptorMismatch {
                field: "quantiser_version",
            });
        }

        let pixel_format = crate::format::PixelFormat::from_byte(descriptor.pixel_format)
            .ok_or(BuilderError::DescriptorMismatch {
                field: "pixel_format",
            })?;

        let mut writer: RawtilesWriter<Infallible, Infallible> = RawtilesWriter::new();
        writer.begin_pack(PackMetadata {
            pack_uuid: *derive_pack_uuid(descriptor).as_bytes(),
            supersedes_uuid: None,
            parent_uuid: None,
            pixel_format,
            projection: Projection::WebMercator,
            tile_addressing_scheme: AddressingScheme::Quadtree,
            tile_axis_convention: AxisConvention::Xyz,
            tile_dim_px: self.tile_dim_px,
            zoom_range: (self.zoom_min, self.zoom_max),
            bbox: descriptor.bbox,
            build_timestamp: self.build_timestamp,
        })?;

        if let Some(attribution) = &self.attribution {
            writer.add_extension(TAG_ATTR, attribution.as_bytes())?;
        }

        for (z, x, y, bytes) in &self.tiles {
            writer.add_tile_ref(
                *z,
                *x,
                *y,
                self.compression,
                TileContent::Inline(bytes.clone()),
            )?;
        }

        let mut out = Vec::new();
        writer.finalize(&mut out)?;
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::{BuilderError, PackBuilder, PackBuilderConfig};
    use crate::format::{Compression, PixelFormat, RawtilesReader};
    use crate::identity::{BoundingBox, FormatVersion, PackDescriptor, Source, ZoomRange};
    use crate::quantise::{Abgr2222, Abgr2222Palette, PaletteSlot, Quantiser};
    use alloc::boxed::Box;
    use alloc::string::ToString;
    use alloc::vec::Vec;

    const DIM: u16 = 4;
    const PIXELS: usize = (DIM as usize) * (DIM as usize);

    fn palette() -> Abgr2222Palette {
        Abgr2222Palette::new(&[
            PaletteSlot { rgb: [0xFF, 0xFF, 0xFF], code: 0xFF },
            PaletteSlot { rgb: [0x38, 0x38, 0x38], code: 0xC0 },
            PaletteSlot { rgb: [0x62, 0xB7, 0xD5], code: 0xF4 },
        ])
        .unwrap()
    }

    fn config(quantiser: Box<dyn Quantiser>) -> PackBuilderConfig {
        PackBuilderConfig {
            tile_dim_px: DIM,
            zoom_range: (3, 4),
            compression: Compression::Rle8,
            quantiser,
            attribution: Some("Map data from OpenStreetMap (ODbL)".to_string()),
            build_timestamp: 1_760_000_000,
        }
    }

    fn descriptor(q: &dyn Quantiser, content_hash: [u8; 32]) -> PackDescriptor {
        PackDescriptor {
            affn: None,
            bbox: BoundingBox {
                min_lon_micro: -76_015_000,
                min_lat_micro: 44_590_000,
                max_lon_micro: -75_889_000,
                max_lat_micro: 44_662_000,
            },
            format_version: FormatVersion { major: 1, minor: 0 },
            pixel_format: q.pixel_format(),
            projection: 1,
            quantiser_version: q.version(),
            sources: alloc::vec![Source::Style {
                content_hash,
                zoom_min: 3,
                zoom_max: 4,
            }],
            style_hash: None,
            tile_addressing_scheme: 1,
            tile_axis_convention: 1,
            tile_dim_px: DIM,
            zoom_range: ZoomRange { min: 3, max: 4 },
        }
    }

    /// A solid-colour tile in RGB888.
    fn tile_rgb(rgb: [u8; 3]) -> Vec<u8> {
        rgb.iter().copied().cycle().take(PIXELS * 3).collect()
    }

    /// The same tile in RGBA8888, fully opaque.
    fn tile_rgba(rgb: [u8; 3]) -> Vec<u8> {
        let mut v = Vec::with_capacity(PIXELS * 4);
        for _ in 0..PIXELS {
            v.extend_from_slice(&[rgb[0], rgb[1], rgb[2], 0xFF]);
        }
        v
    }

    fn build_two_tiles(rgba: bool) -> Vec<u8> {
        let q = palette();
        let mut b = PackBuilder::new(config(Box::new(q.clone()))).unwrap();
        for (i, (z, x, y)) in [(3_u8, 1_u32, 1_u32), (4, 2, 3)].into_iter().enumerate() {
            let rgb = if i == 0 { [0xFF, 0xFF, 0xFF] } else { [0x38, 0x38, 0x38] };
            if rgba {
                b.add_tile_rgba8888(z, x, y, &tile_rgba(rgb)).unwrap();
            } else {
                b.add_tile_rgb888(z, x, y, &tile_rgb(rgb)).unwrap();
            }
        }
        let d = descriptor(&q, b.rendered_content_hash());
        b.finish(&d).unwrap()
    }

    #[test]
    fn builds_a_readable_pack() {
        let bytes = build_two_tiles(false);
        let reader = RawtilesReader::open(bytes.as_slice()).expect("pack should be readable");
        assert_eq!(reader.metadata().tile_dim_px, DIM);
        assert_eq!(reader.metadata().zoom_range, (3, 4));
        assert_eq!(reader.metadata().pixel_format, PixelFormat::Abgr2222);
    }

    /// The RGBA path exists to save the caller a 4→3 repack, not to
    /// change the pack. Same pixels in, same bytes out.
    #[test]
    fn rgba_and_rgb_inputs_produce_identical_packs() {
        assert_eq!(build_two_tiles(true), build_two_tiles(false));
    }

    #[test]
    fn rgba_rejects_a_non_opaque_pixel() {
        let mut b = PackBuilder::new(config(Box::new(palette()))).unwrap();
        let mut rgba = tile_rgba([0xFF, 0xFF, 0xFF]);
        rgba[7] = 0xFE; // second pixel's alpha
        assert_eq!(
            b.add_tile_rgba8888(3, 0, 0, &rgba),
            Err(BuilderError::NotOpaque { index: 1, alpha: 0xFE }),
        );
    }

    #[test]
    fn tiles_must_arrive_in_ascending_zxy_order() {
        let mut b = PackBuilder::new(config(Box::new(palette()))).unwrap();
        b.add_tile_rgb888(3, 5, 5, &tile_rgb([0xFF, 0xFF, 0xFF])).unwrap();
        assert_eq!(
            b.add_tile_rgb888(3, 5, 4, &tile_rgb([0xFF, 0xFF, 0xFF])),
            Err(BuilderError::OutOfOrder { previous: (3, 5, 5), got: (3, 5, 4) }),
        );
        // Equal keys are also rejected — a duplicate tile is not sorted.
        assert!(matches!(
            b.add_tile_rgb888(3, 5, 5, &tile_rgb([0xFF, 0xFF, 0xFF])),
            Err(BuilderError::OutOfOrder { .. }),
        ));
    }

    /// The reason order is enforced: the hash is over a stream, so a
    /// different feed order is a different `content_hash` and therefore
    /// a different `pack_uuid`.
    #[test]
    fn feed_order_changes_the_content_hash() {
        let mut a = PackBuilder::new(config(Box::new(palette()))).unwrap();
        a.add_tile_rgb888(3, 0, 0, &tile_rgb([0xFF, 0xFF, 0xFF])).unwrap();
        a.add_tile_rgb888(3, 0, 1, &tile_rgb([0x38, 0x38, 0x38])).unwrap();

        let mut b = PackBuilder::new(config(Box::new(palette()))).unwrap();
        b.add_tile_rgb888(3, 0, 0, &tile_rgb([0x38, 0x38, 0x38])).unwrap();
        b.add_tile_rgb888(3, 0, 1, &tile_rgb([0xFF, 0xFF, 0xFF])).unwrap();

        assert_ne!(a.rendered_content_hash(), b.rendered_content_hash());
    }

    #[test]
    fn zoom_outside_the_range_is_rejected() {
        let mut b = PackBuilder::new(config(Box::new(palette()))).unwrap();
        assert_eq!(
            b.add_tile_rgb888(9, 0, 0, &tile_rgb([0xFF, 0xFF, 0xFF])),
            Err(BuilderError::ZoomOutOfRange { z: 9, min: 3, max: 4 }),
        );
    }

    #[test]
    fn wrong_sized_tiles_are_rejected() {
        let mut b = PackBuilder::new(config(Box::new(palette()))).unwrap();
        assert_eq!(
            b.add_tile_rgb888(3, 0, 0, &[0_u8; 12]),
            Err(BuilderError::TileSize { expected: PIXELS * 3, got: 12 }),
        );
    }

    #[test]
    fn an_empty_pack_is_rejected() {
        let q = palette();
        let b = PackBuilder::new(config(Box::new(q.clone()))).unwrap();
        let d = descriptor(&q, b.rendered_content_hash());
        assert_eq!(b.finish(&d), Err(BuilderError::Empty));
    }

    /// A descriptor that disagrees with the build would derive a
    /// `pack_uuid` for a pack nobody built.
    #[test]
    fn a_descriptor_that_contradicts_the_build_is_rejected() {
        let q = palette();
        let mut b = PackBuilder::new(config(Box::new(q.clone()))).unwrap();
        b.add_tile_rgb888(3, 0, 0, &tile_rgb([0xFF, 0xFF, 0xFF])).unwrap();

        let mut d = descriptor(&q, b.rendered_content_hash());
        d.tile_dim_px = DIM * 2;
        assert_eq!(
            b.finish(&d),
            Err(BuilderError::DescriptorMismatch { field: "tile_dim_px" }),
        );
    }

    /// Swapping the quantiser must not slip past the descriptor check —
    /// canonical and palette-snap have different `quantiser_version`s
    /// precisely so this is catchable.
    #[test]
    fn a_descriptor_for_the_other_quantiser_is_rejected() {
        let mut b = PackBuilder::new(config(Box::new(palette()))).unwrap();
        b.add_tile_rgb888(3, 0, 0, &tile_rgb([0xFF, 0xFF, 0xFF])).unwrap();
        let d = descriptor(&Abgr2222, b.rendered_content_hash());
        assert_eq!(
            b.finish(&d),
            Err(BuilderError::DescriptorMismatch { field: "quantiser_version" }),
        );
    }

    #[test]
    fn compression_changes_the_bytes_but_not_the_content_hash() {
        let make = |compression| {
            let q = palette();
            let mut cfg = config(Box::new(q.clone()));
            cfg.compression = compression;
            let mut b = PackBuilder::new(cfg).unwrap();
            b.add_tile_rgb888(3, 0, 0, &tile_rgb([0xFF, 0xFF, 0xFF])).unwrap();
            let hash = b.rendered_content_hash();
            let d = descriptor(&q, hash);
            (b.finish(&d).unwrap(), hash)
        };
        let (rle, rle_hash) = make(Compression::Rle8);
        let (none, none_hash) = make(Compression::None);
        assert_eq!(rle_hash, none_hash, "content_hash is pre-quantisation");
        assert!(rle.len() < none.len(), "a solid tile should compress");
    }
}
