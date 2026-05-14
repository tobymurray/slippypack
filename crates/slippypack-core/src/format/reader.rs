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
use super::tile_index::{INDEX_ENTRY_SIZE, TileIndexEntry, TileIndexError, read_index_entry};
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
    /// The header's `extensions_offset` lies outside the buffer.
    ExtensionsOffsetOutOfBounds,
    /// A tile's declared `(offset, length)` extends past the buffer.
    TileOutOfBounds { entry: u32 },
    /// The 4-byte CRC footer doesn't match the computed CRC over the
    /// preceding bytes.
    CrcMismatch { expected: u32, got: u32 },
}

impl core::fmt::Display for ReaderError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::TooShort => f.write_str("buffer too short for header + CRC footer"),
            Self::Header(e) => write!(f, "header parse: {e}"),
            Self::TileIndex { entry, err } => write!(f, "tile-index entry {entry}: {err}"),
            Self::Extensions(e) => write!(f, "extension sections: {e}"),
            Self::IndexOffsetOutOfBounds => f.write_str("index_offset out of bounds"),
            Self::ExtensionsOffsetOutOfBounds => f.write_str("extensions_offset out of bounds"),
            Self::TileOutOfBounds { entry } => write!(f, "tile-index entry {entry} out of bounds"),
            Self::CrcMismatch { expected, got } => {
                write!(
                    f,
                    "CRC mismatch: expected {expected:#010x}, computed {got:#010x}"
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

        // Bounds-check index_offset.
        let index_offset = parsed_header.derived.index_offset;
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
            let tile_end = entry
                .offset
                .checked_add(u64::from(entry.length))
                .ok_or(ReaderError::TileOutOfBounds { entry: i })?;
            if tile_end > crc_offset_u64 {
                return Err(ReaderError::TileOutOfBounds { entry: i });
            }
            index.push(entry);
        }

        // Bounds-check extensions_offset.
        let extensions_offset = parsed_header.derived.extensions_offset;
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
        let entries_before = (start_entry_offset - self.parsed_header.derived.index_offset)
            / INDEX_ENTRY_SIZE as u64;
        let start = usize::try_from(entries_before).ok()?;
        let end = start + zoom_dir.count as usize;
        let zoom_slice = &self.index[start..end];

        let pos = zoom_slice
            .binary_search_by(|e| (e.x, e.y).cmp(&(x, y)))
            .ok()?;
        let entry = &zoom_slice[pos];
        let begin = usize::try_from(entry.offset).ok()?;
        let len = entry.length as usize;
        Some(&self.bytes[begin..begin + len])
    }

    /// The extension sections, in declared order.
    #[must_use]
    pub fn extensions(&self) -> &[ExtensionSection] {
        &self.extensions
    }
}

#[cfg(test)]
mod tests {
    use core::convert::Infallible;

    use super::super::rawtiles_writer::RawtilesWriter;
    use super::super::types::{
        AddressingScheme, AxisConvention, PackMetadata, PixelFormat, Projection,
    };
    use super::super::writer_trait::{TileContent, TileWriter};
    use super::{RawtilesReader, ReaderError};
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
        w.add_tile_ref(4, 0, 0, TileContent::Inline(tile_bytes.clone()))
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
        w.add_tile_ref(4, 5, 3, TileContent::Inline(vec![0x53; 64]))
            .unwrap();
        w.add_tile_ref(4, 0, 0, TileContent::Inline(vec![0x00; 64]))
            .unwrap();
        w.add_tile_ref(4, 2, 1, TileContent::Inline(vec![0x21; 64]))
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
            w.add_tile_ref(z, u32::from(z), 0, TileContent::Inline(vec![z; 32]))
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
        w.add_extension(*b"NAME", b"Local trails").unwrap();
        w.add_extension(*b"ATTR", b"\xc2\xa9 OpenStreetMap contributors")
            .unwrap();
        let mut buf: Vec<u8> = Vec::new();
        w.finalize(&mut buf).unwrap();

        let r = RawtilesReader::open(&buf).expect("round-trip parse");
        let exts = r.extensions();
        assert_eq!(exts.len(), 2);
        assert_eq!(exts[0].tag, *b"NAME");
        assert_eq!(exts[0].payload, b"Local trails");
        assert_eq!(exts[1].tag, *b"ATTR");
        assert_eq!(exts[1].payload, b"\xc2\xa9 OpenStreetMap contributors");
    }

    #[test]
    fn round_trip_tiles_and_extensions_together() {
        let mut w: RawtilesWriter<Infallible, Infallible> = RawtilesWriter::new();
        w.begin_pack(baseline_metadata()).unwrap();
        w.add_tile_ref(4, 0, 0, TileContent::Inline(vec![0xAB; 16_384]))
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
        w.add_tile_ref(4, 0, 0, TileContent::Inline(vec![0x01; 1]))
            .unwrap();
        w.add_tile_ref(4, 1, 0, TileContent::Inline(vec![0x02; 7]))
            .unwrap();
        w.add_tile_ref(4, 2, 0, TileContent::Inline(vec![0x03; 13]))
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
        w.add_tile_ref(6, 5, 5, TileContent::Inline(vec![0; 32]))
            .unwrap();
        w.add_tile_ref(4, 1, 0, TileContent::Inline(vec![0; 32]))
            .unwrap();
        w.add_tile_ref(5, 2, 2, TileContent::Inline(vec![0; 32]))
            .unwrap();
        w.add_tile_ref(4, 0, 0, TileContent::Inline(vec![0; 32]))
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
            w.add_tile_ref(5, 3, 7, TileContent::Inline(vec![0x42; 16]))
                .unwrap();
            w.add_tile_ref(4, 0, 0, TileContent::Inline(vec![0x33; 8]))
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
