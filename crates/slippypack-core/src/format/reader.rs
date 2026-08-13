//! `.rawtiles` reader for round-trip tests and future "open existing pack"
//! workflows.
//!
//! This is **not** the watch-side canonical reader (that lives in
//! una-sdk's C++ `TilePack`); it's slippypack-core's reference reader
//! for verifying that a freshly-written pack parses back correctly.
//! The two readers must produce identical interpretations of the bytes
//! — verified end-to-end by the una-sdk simulator round-trip when that
//! ships (PLAN.md § Test plan, test 7).

use super::crc::Crc32;
use super::extensions::{ExtensionSection, read_extension_sections};
use super::header::{HEADER_BASE_SIZE, HeaderError, ParsedHeader, read_header};
use super::tile_index::{
    Compression, INDEX_ENTRY_SIZE, TileIndexEntry, TileIndexError, read_index_entry,
};
use super::types::PackMetadata;

/// Parsed view of a `.rawtiles` byte buffer.
///
/// Holds a reference to the original buffer; tile-byte slices returned
/// by [`RawtilesReader::tile_bytes`] borrow from it. Metadata, the parsed
/// header, the tile index, and extension sections are owned (cheap to
/// parse once at open time).
#[derive(Debug)]
pub struct RawtilesReader<'a> {
    bytes: &'a [u8],
    parsed_header: ParsedHeader,
    index: Vec<TileIndexEntry>,
    extensions: Vec<ExtensionSection>,
}

/// Reader errors. Combines errors from the byte-layout primitive parsers
/// (header, tile-index, extensions) plus CRC mismatch and offset-range
/// validation.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum ReaderError {
    /// Buffer shorter than [`HEADER_BASE_SIZE`] + 4 (header + CRC footer).
    TooShort,
    /// Header validation failed (magic, version, enum bytes, invariants).
    Header(HeaderError),
    /// A tile-index entry was malformed.
    TileIndex { entry: u32, err: TileIndexError },
    /// Extension-section parsing failed.
    Extensions(super::extensions::ExtensionError),
    /// The header's `index_offset` lies outside the buffer.
    IndexOffsetOutOfBounds,
    /// The header's `index_offset` is not exactly `HEADER_BASE_SIZE`.
    /// Spec § 4.11: v1.0 readers MUST verify `index_offset == 292`.
    IndexOffsetNotAtHeaderEnd { got: u32, expected: u32 },
    /// The header's `extensions_offset` lies outside the buffer.
    ExtensionsOffsetOutOfBounds,
    /// A tile's declared `(offset, length)` extends past the buffer.
    TileOutOfBounds { entry: u32 },
    /// The 4-byte CRC footer doesn't match the computed CRC over the
    /// preceding bytes.
    CrcMismatch { expected: u32, got: u32 },
    /// An uncompressed tile's length is not `tile_dim_px² ×
    /// bytes_per_pixel`, so the header's `tile_dim_px` does not describe
    /// the bytes the pack actually contains.
    ///
    /// Only checked for `Compression::None`: a compressed tile's on-disk
    /// length is by definition not the raw matrix size.
    TileLengthMismatch {
        entry: u32,
        got: u32,
        expected: u32,
    },
}

impl core::fmt::Display for ReaderError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::TooShort => f.write_str("buffer too short for header + CRC footer"),
            Self::Header(e) => write!(f, "header parse: {e}"),
            Self::TileIndex { entry, err } => write!(f, "tile-index entry {entry}: {err}"),
            Self::Extensions(e) => write!(f, "extension sections: {e}"),
            Self::IndexOffsetOutOfBounds => f.write_str("index_offset out of bounds"),
            Self::IndexOffsetNotAtHeaderEnd { got, expected } => write!(
                f,
                "index_offset must equal {expected} (header size) per spec § 4.11; got {got}",
            ),
            Self::ExtensionsOffsetOutOfBounds => f.write_str("extensions_offset out of bounds"),
            Self::TileOutOfBounds { entry } => write!(f, "tile-index entry {entry} out of bounds"),
            Self::CrcMismatch { expected, got } => {
                write!(
                    f,
                    "CRC mismatch: expected {expected:#010x}, computed {got:#010x}"
                )
            }
            Self::TileLengthMismatch {
                entry,
                got,
                expected,
            } => {
                write!(
                    f,
                    "tile index entry {entry}: uncompressed tile is {got} bytes but \
                     the header's tile_dim_px and pixel_format imply {expected}"
                )
            }
        }
    }
}

impl core::error::Error for ReaderError {}

impl<'a> RawtilesReader<'a> {
    /// Parse `bytes` as a `.rawtiles` pack. Validates header invariants,
    /// every tile-index entry, every extension section, and the CRC
    /// footer.
    ///
    /// # Errors
    ///
    /// See [`ReaderError`].
    ///
    /// # Panics
    ///
    /// Does not panic in practice — internal slice conversions are
    /// guarded by length checks against the input buffer.
    pub fn open(bytes: &'a [u8]) -> Result<Self, ReaderError> {
        // Minimum file size: header + CRC footer.
        if bytes.len() < HEADER_BASE_SIZE + 4 {
            return Err(ReaderError::TooShort);
        }

        // CRC over everything except the trailing 4 bytes.
        let crc_offset = bytes.len() - 4;
        let expected_crc =
            u32::from_le_bytes(bytes[crc_offset..].try_into().expect("4 bytes for CRC"));
        let computed_crc = {
            let mut c = Crc32::new();
            c.update(&bytes[..crc_offset]);
            c.finalize()
        };
        if expected_crc != computed_crc {
            return Err(ReaderError::CrcMismatch {
                expected: expected_crc,
                got: computed_crc,
            });
        }

        let parsed_header = read_header(bytes).map_err(ReaderError::Header)?;

        // Spec § 4.11: v1.0 readers MUST verify index_offset == 292
        // (= HEADER_BASE_SIZE). Tighter than the bounds-only check the
        // u32 width would otherwise allow.
        let index_offset_u32 = parsed_header.derived.index_offset;
        let expected_index_offset = u32::try_from(HEADER_BASE_SIZE).expect("header size fits u32");
        if index_offset_u32 != expected_index_offset {
            return Err(ReaderError::IndexOffsetNotAtHeaderEnd {
                got: index_offset_u32,
                expected: expected_index_offset,
            });
        }
        let index_offset = u64::from(index_offset_u32);
        let tile_count = parsed_header.derived.tile_count;
        let index_size = u64::from(tile_count) * (INDEX_ENTRY_SIZE as u64);
        let index_end = index_offset
            .checked_add(index_size)
            .ok_or(ReaderError::IndexOffsetOutOfBounds)?;
        if index_end > bytes.len() as u64 {
            return Err(ReaderError::IndexOffsetOutOfBounds);
        }

        // Parse each tile-index entry.
        let mut index = Vec::with_capacity(tile_count as usize);
        let crc_offset_u64 = crc_offset as u64;
        for i in 0..tile_count {
            let start_u64 = index_offset + u64::from(i) * INDEX_ENTRY_SIZE as u64;
            let start =
                usize::try_from(start_u64).map_err(|_| ReaderError::IndexOffsetOutOfBounds)?;
            let entry = read_index_entry(&bytes[start..start + INDEX_ENTRY_SIZE])
                .map_err(|err| ReaderError::TileIndex { entry: i, err })?;
            // Bounds-check the tile's bytes.
            let tile_end = u64::from(entry.offset)
                .checked_add(u64::from(entry.length))
                .ok_or(ReaderError::TileOutOfBounds { entry: i })?;
            if tile_end > crc_offset_u64 {
                return Err(ReaderError::TileOutOfBounds { entry: i });
            }
            index.push(entry);
        }

        // Bounds-check extensions_offset (u32 → u64 for the comparison).
        let extensions_offset_u32 = parsed_header.derived.extensions_offset;
        let extensions_offset = u64::from(extensions_offset_u32);
        if extensions_offset > crc_offset_u64 {
            return Err(ReaderError::ExtensionsOffsetOutOfBounds);
        }

        // Parse extensions: from extensions_offset to crc_offset.
        let ext_start = usize::try_from(extensions_offset)
            .map_err(|_| ReaderError::ExtensionsOffsetOutOfBounds)?;
        let extensions_slice = &bytes[ext_start..crc_offset];
        let extensions =
            read_extension_sections(extensions_slice).map_err(ReaderError::Extensions)?;

        Ok(Self {
            bytes,
            parsed_header,
            index,
            extensions,
        })
    }

    /// The pack's metadata as parsed from the header.
    #[must_use]
    pub fn metadata(&self) -> &PackMetadata {
        &self.parsed_header.metadata
    }

    /// The parsed header (metadata + derived fields).
    #[must_use]
    pub fn header(&self) -> &ParsedHeader {
        &self.parsed_header
    }

    /// Number of tiles in the pack.
    #[must_use]
    pub fn tile_count(&self) -> usize {
        self.index.len()
    }

    /// Iterate the tile index in `(z, x, y)` order.
    pub fn tile_entries(&self) -> impl Iterator<Item = &TileIndexEntry> {
        self.index.iter()
    }

    /// Look up the bytes of the tile at `(z, x, y)`. Returns `None` if
    /// no such tile exists in this pack.
    ///
    /// Uses the header's `zoom_offsets[18]` directory plus a binary
    /// search within each zoom's range — per the spec's mandatory
    /// `O(log n)` lookup.
    #[must_use]
    pub fn tile_bytes(&self, z: u8, x: u32, y: u32) -> Option<&'a [u8]> {
        let z_us = z as usize;
        if z_us >= self.parsed_header.derived.zoom_offsets.len() {
            return None;
        }
        let zoom_dir = self.parsed_header.derived.zoom_offsets[z_us];
        if zoom_dir.count == 0 {
            return None;
        }
        // Compute the slice of `self.index` covering this zoom.
        let start_entry_offset = zoom_dir.offset;
        let index_entry_size = u32::try_from(INDEX_ENTRY_SIZE).ok()?;
        let entries_before =
            (start_entry_offset - self.parsed_header.derived.index_offset) / index_entry_size;
        let start = entries_before as usize;
        let end = start + zoom_dir.count as usize;
        let zoom_slice = &self.index[start..end];

        let pos = zoom_slice
            .binary_search_by(|e| (e.x, e.y).cmp(&(x, y)))
            .ok()?;
        let entry = &zoom_slice[pos];
        let begin = entry.offset as usize;
        let len = entry.length as usize;
        Some(&self.bytes[begin..begin + len])
    }

    /// The extension sections, in declared order.
    #[must_use]
    pub fn extensions(&self) -> &[ExtensionSection] {
        &self.extensions
    }

    /// Check every uncompressed tile's length against `tile_dim_px² ×
    /// bytes_per_pixel`, returning the first entry that disagrees.
    ///
    /// **This is the only thing tying the header's `tile_dim_px` to the bytes the
    /// pack carries.** Nothing else does: bounds-checking an `(offset, length)`
    /// pair says a tile is inside the file, not that it is the size the header
    /// claims. Without this, a pack declaring 128 px tiles while holding 256 px
    /// ones parses clean, and a consumer trusting the field decodes every tile
    /// with the wrong stride — garbage on a display rather than an error on a
    /// laptop. That matters most exactly when `tile_dim_px` stops being a
    /// constant.
    ///
    /// Deliberately **not** part of [`Self::open`]: tile bytes are opaque to the
    /// byte-layout parser, and round-trip tests legitimately write short
    /// stand-in blobs to exercise index and offset arithmetic. Callers that care
    /// about a pack being self-consistent — `inspect`, and a `verify`
    /// subcommand — call this explicitly.
    ///
    /// Compressed tiles are skipped: their on-disk length is not the raw matrix
    /// size by definition.
    ///
    /// # Errors
    ///
    /// [`ReaderError::TileLengthMismatch`] for the first inconsistent entry.
    pub fn validate_tile_lengths(&self) -> Result<(), ReaderError> {
        let expected = self.parsed_header.metadata.raw_tile_len();
        // Bounded by tile_dim_px² × 2, which fits u32 for every u16 edge length
        // the header can express.
        let expected_u32 = u32::try_from(expected).unwrap_or(u32::MAX);
        for (i, entry) in self.index.iter().enumerate() {
            if entry.compression == Compression::None && u64::from(entry.length) != expected {
                return Err(ReaderError::TileLengthMismatch {
                    entry: u32::try_from(i).unwrap_or(u32::MAX),
                    got: entry.length,
                    expected: expected_u32,
                });
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use core::convert::Infallible;

    use super::super::rawtiles_writer::RawtilesWriter;
    use super::super::tile_index::Compression;
    use super::super::types::{
        AddressingScheme, AxisConvention, PackMetadata, PixelFormat, Projection,
    };
    use super::super::writer_trait::{TileContent, TileWriter};
    use super::{RawtilesReader, ReaderError};
    use crate::identity::BoundingBox;

    /// Metadata whose declared edge length matches the tile bytes a test writes.
    /// `tile_dim_px: 8` with ABGR2222 → 64 bytes per tile, which is the size the
    /// fixtures below use.
    fn coherent_metadata() -> PackMetadata {
        PackMetadata {
            tile_dim_px: 8,
            ..baseline_metadata()
        }
    }

    #[test]
    fn raw_tile_len_is_dim_squared_times_pixel_size() {
        assert_eq!(coherent_metadata().raw_tile_len(), 64);
        assert_eq!(baseline_metadata().raw_tile_len(), 128 * 128);
        let rgb565 = PackMetadata {
            pixel_format: PixelFormat::Rgb565,
            ..coherent_metadata()
        };
        assert_eq!(rgb565.raw_tile_len(), 64 * 2);
    }

    #[test]
    fn validate_tile_lengths_accepts_a_coherent_pack() {
        let mut w: RawtilesWriter<Infallible, Infallible> = RawtilesWriter::new();
        w.begin_pack(coherent_metadata()).unwrap();
        w.add_tile_ref(4, 0, 0, Compression::None, TileContent::Inline(vec![0x11; 64]))
            .unwrap();
        w.add_tile_ref(4, 1, 0, Compression::None, TileContent::Inline(vec![0x22; 64]))
            .unwrap();
        let mut bytes: Vec<u8> = Vec::new();
        w.finalize(&mut bytes).unwrap();
        let r = RawtilesReader::open(&bytes).unwrap();
        assert_eq!(r.validate_tile_lengths(), Ok(()));
    }

    #[test]
    fn validate_tile_lengths_catches_a_lying_header() {
        // The defect this exists for: the header says 128 px (16 KiB per tile)
        // while the pack holds 64-byte tiles. `open` accepts it — the bytes are
        // in bounds and the CRC is right — so only this check notices.
        let mut w: RawtilesWriter<Infallible, Infallible> = RawtilesWriter::new();
        w.begin_pack(baseline_metadata()).unwrap();
        w.add_tile_ref(4, 0, 0, Compression::None, TileContent::Inline(vec![0x11; 64]))
            .unwrap();
        let mut bytes: Vec<u8> = Vec::new();
        w.finalize(&mut bytes).unwrap();
        let r = RawtilesReader::open(&bytes).expect("structurally valid");
        assert_eq!(
            r.validate_tile_lengths(),
            Err(ReaderError::TileLengthMismatch {
                entry: 0,
                got: 64,
                expected: 16_384,
            }),
        );
    }

    #[test]
    fn validate_tile_lengths_skips_compressed_tiles() {
        // A compressed tile's on-disk length is not the raw matrix size, so
        // checking it against tile_dim_px² would reject every RLE pack.
        let mut w: RawtilesWriter<Infallible, Infallible> = RawtilesWriter::new();
        w.begin_pack(coherent_metadata()).unwrap();
        w.add_tile_ref(4, 0, 0, Compression::Rle8, TileContent::Inline(vec![0x11; 7]))
            .unwrap();
        let mut bytes: Vec<u8> = Vec::new();
        w.finalize(&mut bytes).unwrap();
        let r = RawtilesReader::open(&bytes).unwrap();
        assert_eq!(r.validate_tile_lengths(), Ok(()));
    }

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
            zoom_range: (4, 6),
            bbox: BoundingBox {
                min_lon_micro: -180_000_000,
                min_lat_micro: -85_000_000,
                max_lon_micro: 180_000_000,
                max_lat_micro: 85_000_000,
            },
            build_timestamp: 1_700_000_000,
        }
    }

    fn empty_pack_bytes() -> Vec<u8> {
        let mut w: RawtilesWriter<Infallible, Infallible> = RawtilesWriter::new();
        w.begin_pack(baseline_metadata()).unwrap();
        let mut buf: Vec<u8> = Vec::new();
        w.finalize(&mut buf).unwrap();
        buf
    }

    #[test]
    fn open_empty_pack_succeeds() {
        let buf = empty_pack_bytes();
        let r = RawtilesReader::open(&buf).expect("empty pack should parse");
        assert_eq!(r.tile_count(), 0);
        assert_eq!(r.metadata(), &baseline_metadata());
        assert!(r.extensions().is_empty());
    }

    #[test]
    fn open_too_short_returns_too_short() {
        let buf = vec![0_u8; 100];
        assert_eq!(
            RawtilesReader::open(&buf).unwrap_err(),
            ReaderError::TooShort
        );
    }

    #[test]
    fn open_with_bad_crc_returns_crc_mismatch() {
        let mut buf = empty_pack_bytes();
        let last = buf.len() - 1;
        buf[last] = buf[last].wrapping_add(1); // corrupt the CRC
        let err = RawtilesReader::open(&buf).unwrap_err();
        assert!(
            matches!(err, ReaderError::CrcMismatch { .. }),
            "got: {err:?}"
        );
    }

    #[test]
    fn open_with_corrupted_header_byte_returns_crc_mismatch() {
        let mut buf = empty_pack_bytes();
        // Flip a non-magic, non-CRC byte; CRC must catch it.
        buf[100] = buf[100].wrapping_add(1);
        let err = RawtilesReader::open(&buf).unwrap_err();
        assert!(
            matches!(err, ReaderError::CrcMismatch { .. }),
            "got: {err:?}"
        );
    }

    #[test]
    fn round_trip_one_inline_tile() {
        let tile_bytes: Vec<u8> = (0..16_384_u32).map(|i| (i % 251) as u8).collect();
        let mut w: RawtilesWriter<Infallible, Infallible> = RawtilesWriter::new();
        w.begin_pack(baseline_metadata()).unwrap();
        w.add_tile_ref(4, 0, 0, Compression::None, TileContent::Inline(tile_bytes.clone()))
            .unwrap();
        let mut buf: Vec<u8> = Vec::new();
        w.finalize(&mut buf).unwrap();

        let r = RawtilesReader::open(&buf).expect("round-trip parse");
        assert_eq!(r.tile_count(), 1);
        assert_eq!(r.tile_bytes(4, 0, 0), Some(tile_bytes.as_slice()));
        // Different (z, x, y) → None.
        assert_eq!(r.tile_bytes(4, 1, 0), None);
        assert_eq!(r.tile_bytes(5, 0, 0), None);
    }

    #[test]
    fn round_trip_multiple_tiles_at_same_zoom() {
        let mut w: RawtilesWriter<Infallible, Infallible> = RawtilesWriter::new();
        w.begin_pack(baseline_metadata()).unwrap();
        // Add tiles in arbitrary order; the writer must sort them.
        w.add_tile_ref(4, 5, 3, Compression::None, TileContent::Inline(vec![0x53; 64]))
            .unwrap();
        w.add_tile_ref(4, 0, 0, Compression::None, TileContent::Inline(vec![0x00; 64]))
            .unwrap();
        w.add_tile_ref(4, 2, 1, Compression::None, TileContent::Inline(vec![0x21; 64]))
            .unwrap();
        let mut buf: Vec<u8> = Vec::new();
        w.finalize(&mut buf).unwrap();

        let r = RawtilesReader::open(&buf).expect("round-trip parse");
        assert_eq!(r.tile_count(), 3);
        assert_eq!(r.tile_bytes(4, 0, 0).unwrap(), &vec![0x00; 64][..]);
        assert_eq!(r.tile_bytes(4, 2, 1).unwrap(), &vec![0x21; 64][..]);
        assert_eq!(r.tile_bytes(4, 5, 3).unwrap(), &vec![0x53; 64][..]);
        assert_eq!(r.tile_bytes(4, 9, 9), None);
    }

    #[test]
    fn round_trip_tiles_across_multiple_zooms() {
        let mut w: RawtilesWriter<Infallible, Infallible> = RawtilesWriter::new();
        w.begin_pack(baseline_metadata()).unwrap();
        // Spread tiles across zooms 4, 5, 6.
        for z in 4..=6 {
            w.add_tile_ref(z, u32::from(z), 0, Compression::None, TileContent::Inline(vec![z; 32]))
                .unwrap();
        }
        let mut buf: Vec<u8> = Vec::new();
        w.finalize(&mut buf).unwrap();

        let r = RawtilesReader::open(&buf).expect("round-trip parse");
        assert_eq!(r.tile_count(), 3);
        for z in 4..=6 {
            assert_eq!(
                r.tile_bytes(z, u32::from(z), 0).unwrap(),
                &vec![z; 32][..],
                "lookup at z={z}",
            );
        }
        // Tiles outside the pack are None.
        assert_eq!(r.tile_bytes(4, 99, 99), None);
    }

    #[test]
    fn round_trip_with_extension_sections() {
        let mut w: RawtilesWriter<Infallible, Infallible> = RawtilesWriter::new();
        w.begin_pack(baseline_metadata()).unwrap();
        // Add NAME first; the writer MUST sort to lex-tag order per
        // spec § 12.1. ATTR ('A' = 0x41) sorts before NAME ('N' = 0x4E).
        w.add_extension(*b"NAME", b"Local trails").unwrap();
        w.add_extension(*b"ATTR", b"\xc2\xa9 OpenStreetMap contributors")
            .unwrap();
        let mut buf: Vec<u8> = Vec::new();
        w.finalize(&mut buf).unwrap();

        let r = RawtilesReader::open(&buf).expect("round-trip parse");
        let exts = r.extensions();
        assert_eq!(exts.len(), 2);
        assert_eq!(exts[0].tag, *b"ATTR");
        assert_eq!(exts[0].payload, b"\xc2\xa9 OpenStreetMap contributors");
        assert_eq!(exts[1].tag, *b"NAME");
        assert_eq!(exts[1].payload, b"Local trails");
    }

    #[test]
    fn round_trip_tiles_and_extensions_together() {
        let mut w: RawtilesWriter<Infallible, Infallible> = RawtilesWriter::new();
        w.begin_pack(baseline_metadata()).unwrap();
        w.add_tile_ref(4, 0, 0, Compression::None, TileContent::Inline(vec![0xAB; 16_384]))
            .unwrap();
        w.add_extension(*b"NAME", b"hello").unwrap();
        let mut buf: Vec<u8> = Vec::new();
        w.finalize(&mut buf).unwrap();

        let r = RawtilesReader::open(&buf).expect("round-trip parse");
        assert_eq!(r.tile_count(), 1);
        assert_eq!(r.extensions().len(), 1);
        assert_eq!(r.tile_bytes(4, 0, 0).unwrap(), &vec![0xAB; 16_384][..]);
        assert_eq!(r.extensions()[0].tag, *b"NAME");
    }

    #[test]
    fn tile_lookup_with_odd_sized_tiles_round_trips() {
        // Tile sizes that aren't multiples of 4 — verifies the 4-byte
        // alignment padding doesn't corrupt the round-trip.
        let mut w: RawtilesWriter<Infallible, Infallible> = RawtilesWriter::new();
        w.begin_pack(baseline_metadata()).unwrap();
        w.add_tile_ref(4, 0, 0, Compression::None, TileContent::Inline(vec![0x01; 1]))
            .unwrap();
        w.add_tile_ref(4, 1, 0, Compression::None, TileContent::Inline(vec![0x02; 7]))
            .unwrap();
        w.add_tile_ref(4, 2, 0, Compression::None, TileContent::Inline(vec![0x03; 13]))
            .unwrap();
        let mut buf: Vec<u8> = Vec::new();
        w.finalize(&mut buf).unwrap();

        let r = RawtilesReader::open(&buf).expect("round-trip parse");
        assert_eq!(r.tile_bytes(4, 0, 0).unwrap(), &[0x01_u8; 1][..]);
        assert_eq!(r.tile_bytes(4, 1, 0).unwrap(), &[0x02_u8; 7][..]);
        assert_eq!(r.tile_bytes(4, 2, 0).unwrap(), &[0x03_u8; 13][..]);
    }

    #[test]
    fn iterating_tile_entries_yields_sorted_order() {
        let mut w: RawtilesWriter<Infallible, Infallible> = RawtilesWriter::new();
        w.begin_pack(baseline_metadata()).unwrap();
        // Insert out of order.
        w.add_tile_ref(6, 5, 5, Compression::None, TileContent::Inline(vec![0; 32]))
            .unwrap();
        w.add_tile_ref(4, 1, 0, Compression::None, TileContent::Inline(vec![0; 32]))
            .unwrap();
        w.add_tile_ref(5, 2, 2, Compression::None, TileContent::Inline(vec![0; 32]))
            .unwrap();
        w.add_tile_ref(4, 0, 0, Compression::None, TileContent::Inline(vec![0; 32]))
            .unwrap();
        let mut buf: Vec<u8> = Vec::new();
        w.finalize(&mut buf).unwrap();

        let r = RawtilesReader::open(&buf).unwrap();
        let entries: Vec<_> = r.tile_entries().map(|e| (e.z, e.x, e.y)).collect();
        assert_eq!(entries, vec![(4, 0, 0), (4, 1, 0), (5, 2, 2), (6, 5, 5)]);
    }

    #[test]
    fn determinism_two_identical_writes_produce_identical_bytes() {
        // Same metadata + same tiles + same extension order → identical
        // pack bytes. This is the load-bearing byte-determinism property
        // that lets the CLI and PWA produce bit-identical output.
        let build_pack = || {
            let mut w: RawtilesWriter<Infallible, Infallible> = RawtilesWriter::new();
            w.begin_pack(baseline_metadata()).unwrap();
            w.add_tile_ref(5, 3, 7, Compression::None, TileContent::Inline(vec![0x42; 16]))
                .unwrap();
            w.add_tile_ref(4, 0, 0, Compression::None, TileContent::Inline(vec![0x33; 8]))
                .unwrap();
            w.add_extension(*b"NAME", b"deterministic").unwrap();
            let mut buf: Vec<u8> = Vec::new();
            w.finalize(&mut buf).unwrap();
            buf
        };
        let a = build_pack();
        let b = build_pack();
        assert_eq!(a, b);
    }
}
