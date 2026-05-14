//! Format-level enums and the [`PackMetadata`] struct that callers hand
//! to a [`TileWriter`] before adding tiles.
//!
//! The enum byte values match the rawtiles v1.0 spec exactly — these
//! are wire-format identifiers and renumbering them would break
//! cross-implementation compatibility.
//!
//! [`TileWriter`]: super::TileWriter

use crate::identity::{BoundingBox, FormatVersion};

/// `.rawtiles` magic bytes. Every conforming pack starts with these
/// 4 ASCII characters at offset 0.
pub const MAGIC: [u8; 4] = *b"RAWT";

/// rawtiles format version baked into the writer. v1 packs declare
/// `(1, 0)` in the header. Bumping the **major** is a breaking-format
/// change; bumping the **minor** is an additive change that earlier-minor
/// readers MUST accept with unknown extension tags skipped (per
/// `spec/rawtiles-v1.0-rc1.md`).
pub const FORMAT_VERSION: FormatVersion = FormatVersion { major: 1, minor: 0 };

/// Pixel-format enum byte. Per PLAN.md § Pixel format enum and the
/// una-sdk spec, v1 supports only `Abgr2222 = 1`. Other values are
/// reserved for future format minor-bumps.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[non_exhaustive]
#[repr(u8)]
pub enum PixelFormat {
    /// 8-bit ABGR2222 (2 bits per channel). The v1 default and only
    /// legal value. Bit layout `AABBGGRR` from MSB to LSB.
    Abgr2222 = 1,
    // Reserved: L4 indexed (2), L2 indexed (3), BW (4).
}

impl PixelFormat {
    /// Wire-format byte value.
    #[must_use]
    pub const fn as_byte(self) -> u8 {
        self as u8
    }

    /// Parse from a wire-format byte. Returns `None` for unknown values
    /// (i.e. values reserved-but-not-implemented in v1). v1 readers
    /// MUST reject packs with unknown `pixel_format` per the spec.
    #[must_use]
    pub const fn from_byte(b: u8) -> Option<Self> {
        match b {
            1 => Some(Self::Abgr2222),
            _ => None,
        }
    }
}

/// Projection enum byte. Per PLAN.md § Projection and tile addressing
/// scheme, v1 implements `WebMercator = 1` and `LocalLinear = 3`. The
/// `2` value (equirectangular) and `4..N` are reserved.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[non_exhaustive]
#[repr(u8)]
pub enum Projection {
    /// Web Mercator (EPSG:3857). The standard slippy-map projection.
    /// Used with [`AddressingScheme::Quadtree`].
    WebMercator = 1,
    /// Local Linear (corner-to-lat/lon affine). For single-image
    /// hand-drawn packs (PLAN.md § Phase 10). Used with
    /// [`AddressingScheme::SingleImage`].
    LocalLinear = 3,
}

impl Projection {
    #[must_use]
    pub const fn as_byte(self) -> u8 {
        self as u8
    }

    #[must_use]
    pub const fn from_byte(b: u8) -> Option<Self> {
        match b {
            1 => Some(Self::WebMercator),
            3 => Some(Self::LocalLinear),
            _ => None,
        }
    }
}

/// Tile-addressing scheme enum byte. Per PLAN.md, v1 implements
/// `Quadtree = 1` and `SingleImage = 2`. Future schemes (irregular
/// grids, etc) are reserved.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[non_exhaustive]
#[repr(u8)]
pub enum AddressingScheme {
    /// `(z, x, y)` quadtree. Used with [`Projection::WebMercator`].
    Quadtree = 1,
    /// Single image, no pyramid. Used with [`Projection::LocalLinear`].
    SingleImage = 2,
}

impl AddressingScheme {
    #[must_use]
    pub const fn as_byte(self) -> u8 {
        self as u8
    }

    #[must_use]
    pub const fn from_byte(b: u8) -> Option<Self> {
        match b {
            1 => Some(Self::Quadtree),
            2 => Some(Self::SingleImage),
            _ => None,
        }
    }
}

/// Tile-axis convention enum byte. Per PLAN.md § Tile coordinate
/// convention, v1 accepts XYZ (slippy-map default) and TMS
/// (gdal2tiles default). The watch normalises to XYZ at query time.
///
/// Meaningful only when [`AddressingScheme::Quadtree`]; for other
/// addressing schemes the field is ignored.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[non_exhaustive]
#[repr(u8)]
pub enum AxisConvention {
    /// Slippy-map / OSM / MapLibre / Mapbox. Y increases southward.
    Xyz = 1,
    /// `gdal2tiles --profile mercator`. Y increases northward.
    Tms = 2,
}

impl AxisConvention {
    #[must_use]
    pub const fn as_byte(self) -> u8 {
        self as u8
    }

    #[must_use]
    pub const fn from_byte(b: u8) -> Option<Self> {
        match b {
            1 => Some(Self::Xyz),
            2 => Some(Self::Tms),
            _ => None,
        }
    }
}

/// Caller-supplied metadata for [`super::TileWriter::begin_pack`]. The
/// writer uses these to populate the on-disk header; fields the writer
/// derives itself (`tile_count`, `index_offset`, `zoom_offsets[24]`,
/// `extensions_offset`) are NOT in this struct.
///
/// `format_version` is NOT here either — it's a constant ([`FORMAT_VERSION`])
/// baked into the writer, not a caller choice.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackMetadata {
    /// UUIDv5 derived via [`crate::identity::derive_pack_uuid`] — or
    /// whatever opaque 16-byte ID the caller picks. Required, never zero.
    pub pack_uuid: [u8; 16],
    /// UUID of the pack this one replaces, or `None` for "no
    /// supersession". Serialised as all-zero bytes when `None`.
    pub supersedes_uuid: Option<[u8; 16]>,
    /// UUID of a base pack this one composites on top of. Reserved for
    /// future compositing support — v1 packs MUST NOT set this; v1
    /// readers MUST refuse packs with a non-zero `parent_uuid`. v1
    /// writers therefore only allow `None` here.
    pub parent_uuid: Option<[u8; 16]>,
    pub pixel_format: PixelFormat,
    pub projection: Projection,
    pub tile_addressing_scheme: AddressingScheme,
    pub tile_axis_convention: AxisConvention,
    /// `128` for quadtree packs in v1; `≤ 240` for single-image packs.
    pub tile_dim_px: u16,
    /// `(min_zoom, max_zoom)` inclusive on both ends.
    pub zoom_range: (u8, u8),
    pub bbox: BoundingBox,
    /// Seconds since the Unix epoch. Per PLAN.md, this is **source
    /// freshness** (most-recent input mtime / `Last-Modified`), not
    /// build wall-clock. The writer stamps it into the header verbatim;
    /// the caller is responsible for computing it.
    pub build_timestamp: u64,
}

#[cfg(test)]
mod tests {
    use super::{AddressingScheme, AxisConvention, FORMAT_VERSION, MAGIC, PixelFormat, Projection};

    #[test]
    fn magic_is_rawt_ascii() {
        assert_eq!(MAGIC, [0x52, 0x41, 0x57, 0x54]);
        assert_eq!(&MAGIC, b"RAWT");
    }

    #[test]
    fn format_version_is_v1_0() {
        assert_eq!(FORMAT_VERSION.major, 1);
        assert_eq!(FORMAT_VERSION.minor, 0);
    }

    #[test]
    fn pixel_format_abgr2222_is_byte_1() {
        assert_eq!(PixelFormat::Abgr2222.as_byte(), 1);
        assert_eq!(PixelFormat::from_byte(1), Some(PixelFormat::Abgr2222));
    }

    #[test]
    fn pixel_format_rejects_reserved_values() {
        for reserved in [0_u8, 2, 3, 4, 5, 255] {
            assert_eq!(PixelFormat::from_byte(reserved), None);
        }
    }

    #[test]
    fn projection_byte_values_match_spec() {
        assert_eq!(Projection::WebMercator.as_byte(), 1);
        assert_eq!(Projection::LocalLinear.as_byte(), 3);
        assert_eq!(Projection::from_byte(1), Some(Projection::WebMercator));
        assert_eq!(Projection::from_byte(3), Some(Projection::LocalLinear));
        // 2 is reserved for equirectangular; readers reject it.
        assert_eq!(Projection::from_byte(2), None);
        assert_eq!(Projection::from_byte(4), None);
    }

    #[test]
    fn addressing_scheme_byte_values_match_spec() {
        assert_eq!(AddressingScheme::Quadtree.as_byte(), 1);
        assert_eq!(AddressingScheme::SingleImage.as_byte(), 2);
        assert_eq!(
            AddressingScheme::from_byte(1),
            Some(AddressingScheme::Quadtree),
        );
        assert_eq!(
            AddressingScheme::from_byte(2),
            Some(AddressingScheme::SingleImage),
        );
        assert_eq!(AddressingScheme::from_byte(0), None);
        assert_eq!(AddressingScheme::from_byte(3), None);
    }

    #[test]
    fn axis_convention_byte_values_match_spec() {
        assert_eq!(AxisConvention::Xyz.as_byte(), 1);
        assert_eq!(AxisConvention::Tms.as_byte(), 2);
        assert_eq!(AxisConvention::from_byte(1), Some(AxisConvention::Xyz));
        assert_eq!(AxisConvention::from_byte(2), Some(AxisConvention::Tms));
        assert_eq!(AxisConvention::from_byte(3), None);
    }

    #[test]
    fn all_enums_round_trip_through_bytes() {
        // For each declared variant, byte → variant → byte returns the
        // same byte.
        let pf = PixelFormat::Abgr2222;
        assert_eq!(PixelFormat::from_byte(pf.as_byte()), Some(pf));
        for p in [Projection::WebMercator, Projection::LocalLinear] {
            assert_eq!(Projection::from_byte(p.as_byte()), Some(p));
        }
        for a in [AddressingScheme::Quadtree, AddressingScheme::SingleImage] {
            assert_eq!(AddressingScheme::from_byte(a.as_byte()), Some(a));
        }
        for a in [AxisConvention::Xyz, AxisConvention::Tms] {
            assert_eq!(AxisConvention::from_byte(a.as_byte()), Some(a));
        }
    }
}
