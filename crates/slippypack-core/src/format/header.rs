//! Fixed-size `.rawtiles` header serialization and parsing.
//!
//! Layout (all multi-byte integers little-endian):
//!
//! | Offset | Size  | Field                       |
//! |-------:|------:|-----------------------------|
//! |   0    |    4  | `magic` (`"RAWT"`)          |
//! |   4    |    1  | `format_version.major`      |
//! |   5    |    1  | `format_version.minor`      |
//! |   6    |   16  | `pack_uuid`                 |
//! |  22    |   16  | `supersedes_uuid`           |
//! |  38    |   16  | `parent_uuid` (reserved 0)  |
//! |  54    |    1  | `pixel_format`              |
//! |  55    |    1  | `projection`                |
//! |  56    |    1  | `tile_addressing_scheme`    |
//! |  57    |    1  | `tile_axis_convention`      |
//! |  58    |    2  | `tile_dim_px` (u16 LE)      |
//! |  60    |    1  | `zoom_range.min`            |
//! |  61    |    1  | `zoom_range.max`            |
//! |  62    |   16  | `bbox` (4×i32 µ° LE)        |
//! |  78    |    8  | `build_timestamp` (u64 LE)  |
//! |  86    |    4  | `tile_count` (u32 LE)       |
//! |  90    |    8  | `index_offset` (u64 LE)     |
//! |  98    |  288  | `zoom_offsets[24]`          |
//! |        |       |   per-zoom: u64 offset + u32 count |
//! | 386    |    8  | `extensions_offset` (u64 LE)|
//! | **394** |     | **header base size**        |
//!
//! Fields the writer derives itself (`tile_count`, `index_offset`,
//! `zoom_offsets`, `extensions_offset`) are passed via
//! [`DerivedHeaderFields`]; everything else comes from [`PackMetadata`].

use crate::identity::{BoundingBox, FormatVersion};

use super::types::{
    AddressingScheme, AxisConvention, FORMAT_VERSION, MAGIC, PackMetadata, PixelFormat, Projection,
};

/// Number of bytes in the fixed-size header.
///
/// `98` fixed-field bytes + `ZOOM_OFFSETS_COUNT × 12` zoom-directory
/// bytes + `8` for `extensions_offset` = `98 + 24*12 + 8 = 394`.
pub const HEADER_BASE_SIZE: usize = 98 + ZOOM_OFFSETS_COUNT * 12 + 8;

/// Number of per-zoom directory entries baked into the header. The spec
/// reserves 24 slots (zooms 0..=23 inclusive). Packs with zoom levels
/// outside this range MUST set the corresponding `zoom_offsets[z]` to
/// all-zero.
///
/// **Sizing rationale**: z=22 is the deepest zoom OSM and Google Maps
/// publish (~5 cm tiles at the equator); z=23 leaves one slot of headroom
/// for very-high-detail kiosk / car-nav / GIS workflows. The watch use
/// case lives at z=12..17 and doesn't strain this. Cost of the extra
/// slots: 72 bytes in every pack's header — negligible.
pub const ZOOM_OFFSETS_COUNT: usize = 24;

/// File offset where the per-zoom directory begins. Pinned to keep
/// the pre-directory fields stable across header-size changes.
const ZOOM_OFFSETS_START: usize = 98;

/// File offset of the trailing `extensions_offset` u64 (the last 8
/// bytes of the header, just past the zoom-directory).
const EXTENSIONS_OFFSET_AT: usize = ZOOM_OFFSETS_START + ZOOM_OFFSETS_COUNT * 12;

/// Per-zoom directory entry: `(offset, count)` pair describing where
/// the tile index for zoom `z` starts and how many tile-index entries
/// it covers. Both fields are zero when the pack contains no tiles at
/// zoom `z`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ZoomOffset {
    pub offset: u64,
    pub count: u32,
}

impl ZoomOffset {
    /// Per-zoom entry serialized size: u64 offset + u32 count = 12 bytes.
    pub const SIZE: usize = 12;
}

/// Header fields the writer computes from its accumulated state rather
/// than from caller-supplied [`PackMetadata`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DerivedHeaderFields {
    pub tile_count: u32,
    pub index_offset: u64,
    pub zoom_offsets: [ZoomOffset; ZOOM_OFFSETS_COUNT],
    pub extensions_offset: u64,
}

/// Result of parsing a `.rawtiles` header back into typed fields.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedHeader {
    pub format_version: FormatVersion,
    pub metadata: PackMetadata,
    pub derived: DerivedHeaderFields,
}

/// Header parse / validation errors.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum HeaderError {
    /// Input slice shorter than [`HEADER_BASE_SIZE`].
    TooShort,
    /// First 4 bytes aren't `b"RAWT"`.
    BadMagic,
    /// Major version differs from this build's [`FORMAT_VERSION`].
    /// Higher *minor* versions are accepted per the format's
    /// forward-compat contract (additive only — see [`read_header`]).
    UnsupportedMajorVersion { got: u8, supported: u8 },
    /// `pixel_format` byte is reserved or unknown.
    InvalidPixelFormat(u8),
    /// `projection` byte is reserved or unknown.
    InvalidProjection(u8),
    /// `tile_addressing_scheme` byte is reserved or unknown.
    InvalidAddressingScheme(u8),
    /// `tile_axis_convention` byte is reserved or unknown (only checked
    /// for quadtree-addressed packs; other addressing schemes ignore the
    /// field per the spec, but the parser still surfaces the byte).
    InvalidAxisConvention(u8),
    /// `parent_uuid` is non-zero. v1 readers MUST refuse compositing.
    ParentUuidNotZero,
    /// `pack_uuid` is all zero. The spec mandates it be non-zero.
    PackUuidIsZero,
    /// `tile_dim_px == 0`. Spec invariant: must be ≥ 1.
    InvalidTileDim,
    /// `zoom_range.max < zoom_range.min`.
    InvalidZoomRange { min: u8, max: u8 },
}

impl core::fmt::Display for HeaderError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match *self {
            Self::TooShort => write!(f, "header input is shorter than {HEADER_BASE_SIZE} bytes"),
            Self::BadMagic => f.write_str("magic bytes are not \"RAWT\""),
            Self::UnsupportedMajorVersion { got, supported } => {
                write!(
                    f,
                    "unsupported format major version {got} (supports {supported})"
                )
            }
            Self::InvalidPixelFormat(b) => write!(f, "invalid pixel_format byte {b}"),
            Self::InvalidProjection(b) => write!(f, "invalid projection byte {b}"),
            Self::InvalidAddressingScheme(b) => {
                write!(f, "invalid tile_addressing_scheme byte {b}")
            }
            Self::InvalidAxisConvention(b) => write!(f, "invalid tile_axis_convention byte {b}"),
            Self::ParentUuidNotZero => f.write_str("parent_uuid must be all-zero in v1 packs"),
            Self::PackUuidIsZero => f.write_str("pack_uuid must be non-zero"),
            Self::InvalidTileDim => f.write_str("tile_dim_px must be ≥ 1"),
            Self::InvalidZoomRange { min, max } => {
                write!(f, "zoom_range.max ({max}) < zoom_range.min ({min})")
            }
        }
    }
}

impl core::error::Error for HeaderError {}

/// Serialize [`PackMetadata`] + [`DerivedHeaderFields`] into the fixed
/// [`HEADER_BASE_SIZE`]-byte header layout.
///
/// The function is infallible: the type system enforces that every
/// enum value is legal (no `InvalidPixelFormat` reachable from valid
/// input). Spec invariants like "`pack_uuid` != 0" or "`parent_uuid` == 0
/// in v1" are caller responsibilities — they're checked at parse time
/// via [`read_header`].
#[must_use]
pub fn write_header(
    metadata: &PackMetadata,
    derived: &DerivedHeaderFields,
) -> [u8; HEADER_BASE_SIZE] {
    let mut buf = [0_u8; HEADER_BASE_SIZE];

    // Magic + version (offsets 0..6).
    buf[0..4].copy_from_slice(&MAGIC);
    buf[4] = FORMAT_VERSION.major;
    buf[5] = FORMAT_VERSION.minor;

    // Three UUIDs (offsets 6..54).
    buf[6..22].copy_from_slice(&metadata.pack_uuid);
    if let Some(s) = metadata.supersedes_uuid {
        buf[22..38].copy_from_slice(&s);
    }
    if let Some(p) = metadata.parent_uuid {
        buf[38..54].copy_from_slice(&p);
    }

    // Four enum bytes + tile_dim_px + zoom range (offsets 54..62).
    buf[54] = metadata.pixel_format.as_byte();
    buf[55] = metadata.projection.as_byte();
    buf[56] = metadata.tile_addressing_scheme.as_byte();
    buf[57] = metadata.tile_axis_convention.as_byte();
    buf[58..60].copy_from_slice(&metadata.tile_dim_px.to_le_bytes());
    buf[60] = metadata.zoom_range.0;
    buf[61] = metadata.zoom_range.1;

    // Bounding box: 4 × i32 LE (offsets 62..78).
    buf[62..66].copy_from_slice(&metadata.bbox.min_lon_micro.to_le_bytes());
    buf[66..70].copy_from_slice(&metadata.bbox.min_lat_micro.to_le_bytes());
    buf[70..74].copy_from_slice(&metadata.bbox.max_lon_micro.to_le_bytes());
    buf[74..78].copy_from_slice(&metadata.bbox.max_lat_micro.to_le_bytes());

    // Build timestamp (offsets 78..86).
    buf[78..86].copy_from_slice(&metadata.build_timestamp.to_le_bytes());

    // Tile count + index offset (offsets 86..98).
    buf[86..90].copy_from_slice(&derived.tile_count.to_le_bytes());
    buf[90..98].copy_from_slice(&derived.index_offset.to_le_bytes());

    // Zoom offsets directory (offsets 98..98 + ZOOM_OFFSETS_COUNT * 12).
    for (i, zo) in derived.zoom_offsets.iter().enumerate() {
        let base = ZOOM_OFFSETS_START + i * ZoomOffset::SIZE;
        buf[base..base + 8].copy_from_slice(&zo.offset.to_le_bytes());
        buf[base + 8..base + 12].copy_from_slice(&zo.count.to_le_bytes());
    }

    // Extensions offset: u64 LE, the last 8 bytes of the header.
    buf[EXTENSIONS_OFFSET_AT..HEADER_BASE_SIZE]
        .copy_from_slice(&derived.extensions_offset.to_le_bytes());

    buf
}

/// Parse the first [`HEADER_BASE_SIZE`] bytes of `input` as a `.rawtiles`
/// header, validating spec invariants.
///
/// Returns `Err(HeaderError::TooShort)` if `input.len() < HEADER_BASE_SIZE`;
/// returns the validated [`ParsedHeader`] on success.
///
/// # Errors
///
/// See [`HeaderError`] for the full list. The parser refuses
/// compositing packs (`parent_uuid != 0`) per the v1 forward-
/// compatibility rules, and refuses zero `pack_uuid`, zero
/// `tile_dim_px`, and inverted zoom ranges.
///
/// # Panics
///
/// Does not panic in practice — the length check at the top
/// guarantees the internal slice-to-array conversions succeed.
#[allow(
    clippy::too_many_lines,
    reason = "linear header walk; splitting buys nothing"
)]
pub fn read_header(input: &[u8]) -> Result<ParsedHeader, HeaderError> {
    if input.len() < HEADER_BASE_SIZE {
        return Err(HeaderError::TooShort);
    }

    if input[0..4] != MAGIC {
        return Err(HeaderError::BadMagic);
    }

    let format_version = FormatVersion {
        major: input[4],
        minor: input[5],
    };
    if format_version.major != FORMAT_VERSION.major {
        return Err(HeaderError::UnsupportedMajorVersion {
            got: format_version.major,
            supported: FORMAT_VERSION.major,
        });
    }
    // Higher minor versions MUST be accepted — the format's forward-
    // compat contract is "header layout frozen per major, additive
    // changes via extension tags". Unknown extension tags are skipped
    // by the extension iterator (already minor-version-agnostic).
    // Callers that want to know whether the pack is from a newer
    // minor than this build supports can inspect
    // `ParsedHeader::format_version.minor`.

    let mut pack_uuid = [0_u8; 16];
    pack_uuid.copy_from_slice(&input[6..22]);
    if pack_uuid == [0_u8; 16] {
        return Err(HeaderError::PackUuidIsZero);
    }

    let mut supersedes_buf = [0_u8; 16];
    supersedes_buf.copy_from_slice(&input[22..38]);
    let supersedes_uuid = if supersedes_buf == [0_u8; 16] {
        None
    } else {
        Some(supersedes_buf)
    };

    let mut parent_buf = [0_u8; 16];
    parent_buf.copy_from_slice(&input[38..54]);
    if parent_buf != [0_u8; 16] {
        return Err(HeaderError::ParentUuidNotZero);
    }
    let parent_uuid = None;

    let pixel_format =
        PixelFormat::from_byte(input[54]).ok_or(HeaderError::InvalidPixelFormat(input[54]))?;
    let projection =
        Projection::from_byte(input[55]).ok_or(HeaderError::InvalidProjection(input[55]))?;
    let tile_addressing_scheme = AddressingScheme::from_byte(input[56])
        .ok_or(HeaderError::InvalidAddressingScheme(input[56]))?;
    let tile_axis_convention = AxisConvention::from_byte(input[57])
        .ok_or(HeaderError::InvalidAxisConvention(input[57]))?;

    let tile_dim_px = u16::from_le_bytes([input[58], input[59]]);
    if tile_dim_px == 0 {
        return Err(HeaderError::InvalidTileDim);
    }

    let zoom_min = input[60];
    let zoom_max = input[61];
    if zoom_max < zoom_min {
        return Err(HeaderError::InvalidZoomRange {
            min: zoom_min,
            max: zoom_max,
        });
    }

    let bbox = BoundingBox {
        min_lon_micro: i32::from_le_bytes(input[62..66].try_into().expect("4 bytes")),
        min_lat_micro: i32::from_le_bytes(input[66..70].try_into().expect("4 bytes")),
        max_lon_micro: i32::from_le_bytes(input[70..74].try_into().expect("4 bytes")),
        max_lat_micro: i32::from_le_bytes(input[74..78].try_into().expect("4 bytes")),
    };

    let build_timestamp = u64::from_le_bytes(input[78..86].try_into().expect("8 bytes"));
    let tile_count = u32::from_le_bytes(input[86..90].try_into().expect("4 bytes"));
    let index_offset = u64::from_le_bytes(input[90..98].try_into().expect("8 bytes"));

    let mut zoom_offsets = [ZoomOffset::default(); ZOOM_OFFSETS_COUNT];
    for (i, zo) in zoom_offsets.iter_mut().enumerate() {
        let base = ZOOM_OFFSETS_START + i * ZoomOffset::SIZE;
        zo.offset = u64::from_le_bytes(input[base..base + 8].try_into().expect("8 bytes"));
        zo.count = u32::from_le_bytes(input[base + 8..base + 12].try_into().expect("4 bytes"));
    }

    let extensions_offset = u64::from_le_bytes(
        input[EXTENSIONS_OFFSET_AT..HEADER_BASE_SIZE]
            .try_into()
            .expect("8 bytes"),
    );

    let metadata = PackMetadata {
        pack_uuid,
        supersedes_uuid,
        parent_uuid,
        pixel_format,
        projection,
        tile_addressing_scheme,
        tile_axis_convention,
        tile_dim_px,
        zoom_range: (zoom_min, zoom_max),
        bbox,
        build_timestamp,
    };

    Ok(ParsedHeader {
        format_version,
        metadata,
        derived: DerivedHeaderFields {
            tile_count,
            index_offset,
            zoom_offsets,
            extensions_offset,
        },
    })
}

#[cfg(test)]
mod tests {
    use super::super::types::{
        AddressingScheme, AxisConvention, FORMAT_VERSION, PackMetadata, PixelFormat, Projection,
    };
    use super::{
        DerivedHeaderFields, HEADER_BASE_SIZE, HeaderError, ZOOM_OFFSETS_COUNT, ZoomOffset,
        read_header, write_header,
    };
    use crate::identity::BoundingBox;

    fn baseline_metadata() -> PackMetadata {
        PackMetadata {
            pack_uuid: *b"\x01\x02\x03\x04\x05\x06\x07\x08\x09\x0a\x0b\x0c\x0d\x0e\x0f\x10",
            supersedes_uuid: None,
            parent_uuid: None,
            pixel_format: PixelFormat::Abgr2222,
            projection: Projection::WebMercator,
            tile_addressing_scheme: AddressingScheme::Quadtree,
            tile_axis_convention: AxisConvention::Xyz,
            tile_dim_px: 128,
            zoom_range: (6, 12),
            bbox: BoundingBox {
                min_lon_micro: -180_000_000,
                min_lat_micro: -85_000_000,
                max_lon_micro: 180_000_000,
                max_lat_micro: 85_000_000,
            },
            build_timestamp: 1_700_000_000,
        }
    }

    fn baseline_derived() -> DerivedHeaderFields {
        let mut zoom_offsets = [ZoomOffset::default(); ZOOM_OFFSETS_COUNT];
        // Populate a few zooms to exercise the directory.
        zoom_offsets[6] = ZoomOffset {
            offset: 1_000,
            count: 4,
        };
        zoom_offsets[12] = ZoomOffset {
            offset: 5_000,
            count: 64,
        };
        DerivedHeaderFields {
            tile_count: 68,
            // First plausible offset past the header. Whatever
            // HEADER_BASE_SIZE is, the index can start there.
            index_offset: HEADER_BASE_SIZE as u64,
            zoom_offsets,
            extensions_offset: 50_000,
        }
    }

    #[test]
    fn header_size_is_394_bytes() {
        let buf = write_header(&baseline_metadata(), &baseline_derived());
        assert_eq!(buf.len(), HEADER_BASE_SIZE);
        assert_eq!(buf.len(), 394);
    }

    #[test]
    fn magic_is_first_four_bytes() {
        let buf = write_header(&baseline_metadata(), &baseline_derived());
        assert_eq!(&buf[0..4], b"RAWT");
    }

    #[test]
    fn version_bytes_match_constant() {
        let buf = write_header(&baseline_metadata(), &baseline_derived());
        assert_eq!(buf[4], FORMAT_VERSION.major);
        assert_eq!(buf[5], FORMAT_VERSION.minor);
        assert_eq!(buf[4], 1);
        assert_eq!(buf[5], 0);
    }

    #[test]
    fn pack_uuid_at_offset_6_through_22() {
        let m = baseline_metadata();
        let buf = write_header(&m, &baseline_derived());
        assert_eq!(&buf[6..22], &m.pack_uuid);
    }

    #[test]
    fn none_supersedes_and_parent_serialize_as_zero() {
        let buf = write_header(&baseline_metadata(), &baseline_derived());
        assert_eq!(&buf[22..38], &[0_u8; 16]);
        assert_eq!(&buf[38..54], &[0_u8; 16]);
    }

    #[test]
    fn some_supersedes_uuid_serializes_at_offset_22() {
        let mut m = baseline_metadata();
        m.supersedes_uuid =
            Some(*b"\xaa\xbb\xcc\xdd\x00\x11\x22\x33\x44\x55\x66\x77\x88\x99\xa0\xb0");
        let buf = write_header(&m, &baseline_derived());
        assert_eq!(&buf[22..38], &m.supersedes_uuid.unwrap());
    }

    #[test]
    fn enum_bytes_at_correct_offsets() {
        let m = baseline_metadata();
        let buf = write_header(&m, &baseline_derived());
        assert_eq!(buf[54], m.pixel_format.as_byte());
        assert_eq!(buf[55], m.projection.as_byte());
        assert_eq!(buf[56], m.tile_addressing_scheme.as_byte());
        assert_eq!(buf[57], m.tile_axis_convention.as_byte());
    }

    #[test]
    fn tile_dim_px_is_u16_little_endian() {
        let mut m = baseline_metadata();
        m.tile_dim_px = 0x1234;
        let buf = write_header(&m, &baseline_derived());
        assert_eq!(&buf[58..60], &[0x34, 0x12]);
    }

    #[test]
    fn zoom_range_bytes() {
        let mut m = baseline_metadata();
        m.zoom_range = (4, 17);
        let buf = write_header(&m, &baseline_derived());
        assert_eq!(buf[60], 4);
        assert_eq!(buf[61], 17);
    }

    #[test]
    fn bbox_is_four_i32_little_endian() {
        let m = baseline_metadata();
        let buf = write_header(&m, &baseline_derived());
        assert_eq!(
            i32::from_le_bytes(buf[62..66].try_into().unwrap()),
            m.bbox.min_lon_micro,
        );
        assert_eq!(
            i32::from_le_bytes(buf[66..70].try_into().unwrap()),
            m.bbox.min_lat_micro,
        );
        assert_eq!(
            i32::from_le_bytes(buf[70..74].try_into().unwrap()),
            m.bbox.max_lon_micro,
        );
        assert_eq!(
            i32::from_le_bytes(buf[74..78].try_into().unwrap()),
            m.bbox.max_lat_micro,
        );
    }

    #[test]
    fn build_timestamp_is_u64_little_endian() {
        let mut m = baseline_metadata();
        m.build_timestamp = 0x0123_4567_89AB_CDEF;
        let buf = write_header(&m, &baseline_derived());
        assert_eq!(
            &buf[78..86],
            &[0xEF, 0xCD, 0xAB, 0x89, 0x67, 0x45, 0x23, 0x01],
        );
    }

    #[test]
    fn tile_count_and_index_offset_serialize() {
        let mut d = baseline_derived();
        d.tile_count = 0x1234_5678;
        d.index_offset = 0xDEAD_BEEF_CAFE_BABE;
        let buf = write_header(&baseline_metadata(), &d);
        assert_eq!(
            u32::from_le_bytes(buf[86..90].try_into().unwrap()),
            0x1234_5678,
        );
        assert_eq!(
            u64::from_le_bytes(buf[90..98].try_into().unwrap()),
            0xDEAD_BEEF_CAFE_BABE,
        );
    }

    #[test]
    fn zoom_offsets_serialize_at_offset_98() {
        let d = baseline_derived();
        let buf = write_header(&baseline_metadata(), &d);
        // zoom_offsets[6] starts at 98 + 6*12 = 170.
        let base = 98 + 6 * ZoomOffset::SIZE;
        assert_eq!(
            u64::from_le_bytes(buf[base..base + 8].try_into().unwrap()),
            d.zoom_offsets[6].offset,
        );
        assert_eq!(
            u32::from_le_bytes(buf[base + 8..base + 12].try_into().unwrap()),
            d.zoom_offsets[6].count,
        );
    }

    #[test]
    fn extensions_offset_at_offset_386() {
        let mut d = baseline_derived();
        d.extensions_offset = 0xFEDC_BA98_7654_3210;
        let buf = write_header(&baseline_metadata(), &d);
        // Extensions-offset u64 is the trailing 8 bytes of the header.
        assert_eq!(
            u64::from_le_bytes(buf[386..394].try_into().unwrap()),
            0xFEDC_BA98_7654_3210,
        );
    }

    // --- read_header tests / round-trips --------------------------------

    #[test]
    fn write_then_read_round_trips() {
        let m = baseline_metadata();
        let d = baseline_derived();
        let buf = write_header(&m, &d);
        let parsed = read_header(&buf).expect("baseline header should parse");
        assert_eq!(parsed.format_version, FORMAT_VERSION);
        assert_eq!(parsed.metadata, m);
        assert_eq!(parsed.derived, d);
    }

    #[test]
    fn read_rejects_too_short_input() {
        let buf = [0_u8; HEADER_BASE_SIZE - 1];
        assert_eq!(read_header(&buf), Err(HeaderError::TooShort));
    }

    #[test]
    fn read_rejects_bad_magic() {
        let mut buf = write_header(&baseline_metadata(), &baseline_derived());
        buf[0] = b'X';
        assert_eq!(read_header(&buf), Err(HeaderError::BadMagic));
    }

    #[test]
    fn read_rejects_unsupported_major_version() {
        let mut buf = write_header(&baseline_metadata(), &baseline_derived());
        buf[4] = 99; // major
        assert_eq!(
            read_header(&buf),
            Err(HeaderError::UnsupportedMajorVersion {
                got: 99,
                supported: FORMAT_VERSION.major,
            }),
        );
    }

    #[test]
    fn read_accepts_newer_minor_version() {
        // Forward-compat contract: higher minor must succeed. The
        // header layout is frozen for a given major version, and any
        // additive changes ride on the extension-tag mechanism (which
        // already skips unknown tags). Callers that care can inspect
        // `ParsedHeader::format_version.minor` after reading.
        let mut buf = write_header(&baseline_metadata(), &baseline_derived());
        buf[5] = 99; // minor
        let parsed = read_header(&buf).expect("higher minor must read");
        assert_eq!(parsed.format_version.major, FORMAT_VERSION.major);
        assert_eq!(parsed.format_version.minor, 99);
    }

    #[test]
    fn read_accepts_max_minor_version() {
        // Sanity check at the u8 boundary.
        let mut buf = write_header(&baseline_metadata(), &baseline_derived());
        buf[5] = 255;
        let parsed = read_header(&buf).expect("minor=255 must read");
        assert_eq!(parsed.format_version.minor, 255);
    }

    #[test]
    fn read_rejects_zero_pack_uuid() {
        let mut buf = write_header(&baseline_metadata(), &baseline_derived());
        for byte in &mut buf[6..22] {
            *byte = 0;
        }
        assert_eq!(read_header(&buf), Err(HeaderError::PackUuidIsZero));
    }

    #[test]
    fn read_rejects_non_zero_parent_uuid() {
        let mut buf = write_header(&baseline_metadata(), &baseline_derived());
        buf[38] = 1;
        assert_eq!(read_header(&buf), Err(HeaderError::ParentUuidNotZero));
    }

    #[test]
    fn read_rejects_invalid_pixel_format() {
        let mut buf = write_header(&baseline_metadata(), &baseline_derived());
        buf[54] = 2; // reserved L4 indexed
        assert_eq!(read_header(&buf), Err(HeaderError::InvalidPixelFormat(2)));
    }

    #[test]
    fn read_rejects_invalid_projection() {
        let mut buf = write_header(&baseline_metadata(), &baseline_derived());
        buf[55] = 2; // reserved
        assert_eq!(read_header(&buf), Err(HeaderError::InvalidProjection(2)));
    }

    #[test]
    fn read_rejects_invalid_addressing_scheme() {
        let mut buf = write_header(&baseline_metadata(), &baseline_derived());
        buf[56] = 0;
        assert_eq!(
            read_header(&buf),
            Err(HeaderError::InvalidAddressingScheme(0)),
        );
    }

    #[test]
    fn read_rejects_zero_tile_dim_px() {
        let mut buf = write_header(&baseline_metadata(), &baseline_derived());
        buf[58] = 0;
        buf[59] = 0;
        assert_eq!(read_header(&buf), Err(HeaderError::InvalidTileDim));
    }

    #[test]
    fn read_rejects_inverted_zoom_range() {
        let mut buf = write_header(&baseline_metadata(), &baseline_derived());
        buf[60] = 12;
        buf[61] = 6;
        assert_eq!(
            read_header(&buf),
            Err(HeaderError::InvalidZoomRange { min: 12, max: 6 }),
        );
    }

    #[test]
    fn header_with_supersedes_some_round_trips() {
        let mut m = baseline_metadata();
        m.supersedes_uuid = Some([0xAA; 16]);
        let buf = write_header(&m, &baseline_derived());
        let parsed = read_header(&buf).unwrap();
        assert_eq!(parsed.metadata.supersedes_uuid, Some([0xAA; 16]));
    }
}
