//! The `.upack` concrete implementation of [`TileWriter`].
//!
//! Buffers metadata + registered byte sources + tile records + extension
//! records until [`UpackWriter::finalize`] runs, which streams the final
//! pack bytes through the output [`Write`] while reading any
//! [`TileContent::External`] tile bytes from the matching
//! [`TileByteSource`].
//!
//! Layout produced (offsets little-endian, see `format::header` and
//! `format::tile_index` modules for the per-section byte layouts):
//!
//! - `0..322`: header
//! - `322..(322 + N*24)`: tile index, sorted by `(z, x, y)`
//! - `..`: 0-3 bytes of zero padding to reach a 4-byte boundary
//! - tile blob: each tile starts at a 4-byte-aligned offset, followed by
//!   0-3 zero padding bytes to align the next tile
//! - extensions: each section starts at a 4-byte-aligned offset
//! - last 4 bytes: CRC-32 LE over every preceding byte

use alloc::collections::BTreeSet;
use core::marker::PhantomData;

use super::crc::Crc32;
use super::extensions::{ExtensionSection, write_extension_section};
use super::header::{
    DerivedHeaderFields, HEADER_BASE_SIZE, ZOOM_OFFSETS_COUNT, ZoomOffset, write_header,
};
use super::tile_index::{Compression, TileIndexEntry, write_index_entry};
use super::types::PackMetadata;
use super::writer_trait::{
    SourceId, TileByteSource, TileContent, TileWriter, TileWriterError, Write,
};

/// Concrete `.upack` writer. Generic over the [`TileByteSource`]'s error
/// type (`SrcErr`) and the [`Write`]'s error type (`OutErr`) so concrete
/// callers don't lose error context.
///
/// Construct via [`UpackWriter::new`] (or `Default`). Then call
/// [`TileWriter::begin_pack`] once, optionally [`register_byte_source`]
/// any number of times, [`add_tile_ref`] / [`add_extension`] as needed,
/// and finally [`finalize`].
///
/// [`register_byte_source`]: TileWriter::register_byte_source
/// [`add_tile_ref`]: TileWriter::add_tile_ref
/// [`add_extension`]: TileWriter::add_extension
/// [`finalize`]: TileWriter::finalize
pub struct UpackWriter<SrcErr, OutErr> {
    state: WriterState,
    byte_sources: Vec<Box<dyn TileByteSource<Error = SrcErr>>>,
    _phantom: PhantomData<fn() -> OutErr>,
}

enum WriterState {
    NotBegun,
    Building(BuildingState),
}

struct BuildingState {
    metadata: PackMetadata,
    tile_keys_seen: BTreeSet<(u8, u32, u32)>,
    tiles: Vec<RecordedTile>,
    extensions: Vec<RecordedExtension>,
}

struct RecordedTile {
    z: u8,
    x: u32,
    y: u32,
    content: TileContent,
}

struct RecordedExtension {
    tag: [u8; 4],
    payload: Vec<u8>,
}

impl<SrcErr, OutErr> UpackWriter<SrcErr, OutErr> {
    /// Construct a fresh writer with no metadata, sources, tiles, or
    /// extensions.
    #[must_use]
    pub fn new() -> Self {
        Self {
            state: WriterState::NotBegun,
            byte_sources: Vec::new(),
            _phantom: PhantomData,
        }
    }
}

impl<SrcErr, OutErr> Default for UpackWriter<SrcErr, OutErr> {
    fn default() -> Self {
        Self::new()
    }
}

fn validate_metadata<SrcErr, OutErr>(
    meta: &PackMetadata,
) -> Result<(), TileWriterError<SrcErr, OutErr>> {
    if meta.pack_uuid == [0_u8; 16] {
        return Err(TileWriterError::InvalidMetadata);
    }
    if meta.parent_uuid.is_some() {
        // v1 does not implement compositing — readers MUST refuse, so we
        // refuse to produce such packs from this writer.
        return Err(TileWriterError::InvalidMetadata);
    }
    if meta.tile_dim_px == 0 {
        return Err(TileWriterError::InvalidMetadata);
    }
    if meta.zoom_range.1 < meta.zoom_range.0 {
        return Err(TileWriterError::InvalidMetadata);
    }
    Ok(())
}

impl<SrcErr, OutErr> TileWriter for UpackWriter<SrcErr, OutErr> {
    type SourceError = SrcErr;
    type OutputError = OutErr;

    fn begin_pack(&mut self, meta: PackMetadata) -> Result<(), TileWriterError<SrcErr, OutErr>> {
        if !matches!(self.state, WriterState::NotBegun) {
            return Err(TileWriterError::AlreadyBegun);
        }
        validate_metadata::<SrcErr, OutErr>(&meta)?;
        self.state = WriterState::Building(BuildingState {
            metadata: meta,
            tile_keys_seen: BTreeSet::new(),
            tiles: Vec::new(),
            extensions: Vec::new(),
        });
        Ok(())
    }

    fn register_byte_source(
        &mut self,
        source: Box<dyn TileByteSource<Error = SrcErr>>,
    ) -> SourceId {
        let id =
            u32::try_from(self.byte_sources.len()).expect("too many byte sources (> u32::MAX)");
        self.byte_sources.push(source);
        id
    }

    fn add_extension(
        &mut self,
        tag: [u8; 4],
        payload: &[u8],
    ) -> Result<(), TileWriterError<SrcErr, OutErr>> {
        let WriterState::Building(state) = &mut self.state else {
            return Err(TileWriterError::NotBegun);
        };
        if u32::try_from(payload.len()).is_err() {
            return Err(TileWriterError::ExtensionTooLarge);
        }
        state.extensions.push(RecordedExtension {
            tag,
            payload: payload.to_vec(),
        });
        Ok(())
    }

    fn add_tile_ref(
        &mut self,
        z: u8,
        x: u32,
        y: u32,
        content: TileContent,
    ) -> Result<(), TileWriterError<SrcErr, OutErr>> {
        let WriterState::Building(state) = &mut self.state else {
            return Err(TileWriterError::NotBegun);
        };
        let (min, max) = state.metadata.zoom_range;
        if z < min || z > max {
            return Err(TileWriterError::TileZoomOutOfRange { z, min, max });
        }
        if (z as usize) >= ZOOM_OFFSETS_COUNT {
            return Err(TileWriterError::TileZoomTooHigh { z });
        }
        if let TileContent::Inline(bytes) = &content
            && u32::try_from(bytes.len()).is_err()
        {
            return Err(TileWriterError::TileTooLarge);
        }
        if u32::try_from(content.len()).is_err() {
            return Err(TileWriterError::TileTooLarge);
        }
        let key = (z, x, y);
        if !state.tile_keys_seen.insert(key) {
            return Err(TileWriterError::DuplicateTile { z, x, y });
        }
        state.tiles.push(RecordedTile { z, x, y, content });
        Ok(())
    }

    // The assembly function is intentionally long because layout planning
    // and streaming output share a lot of local state; extracting helpers
    // would just shuffle the state through awkward signatures.
    #[allow(clippy::too_many_lines)]
    fn finalize<W: Write<Error = OutErr>>(
        mut self,
        mut output: W,
    ) -> Result<(), TileWriterError<SrcErr, OutErr>> {
        let WriterState::Building(state) =
            core::mem::replace(&mut self.state, WriterState::NotBegun)
        else {
            return Err(TileWriterError::NotBegun);
        };

        // Sort tiles by (z, x, y). The btreeset enforced uniqueness on
        // insert; the per-tile vec just needs the explicit sort.
        let mut tiles = state.tiles;
        tiles.sort_by_key(|t| (t.z, t.x, t.y));

        // Validate every External tile refers to a registered source.
        for tile in &tiles {
            if let TileContent::External { source, .. } = &tile.content
                && (*source as usize) >= self.byte_sources.len()
            {
                return Err(TileWriterError::SourceUnregistered { source: *source });
            }
        }

        // --- Layout planning -----------------------------------------

        let tile_count = u32::try_from(tiles.len()).map_err(|_| TileWriterError::PackTooLarge)?;
        let header_size = HEADER_BASE_SIZE as u64;
        let index_size = u64::from(tile_count) * 24;
        let index_offset = header_size;
        // Tile blob starts at the next 4-byte-aligned offset after the index.
        let tile_blob_start = align_up_u64(index_offset + index_size, 4);

        // Per-tile offsets (each aligned to 4 bytes).
        let mut tile_offsets: Vec<u64> = Vec::with_capacity(tiles.len());
        let mut cursor = tile_blob_start;
        for tile in &tiles {
            tile_offsets.push(cursor);
            cursor = cursor
                .checked_add(tile.content.len())
                .ok_or(TileWriterError::PackTooLarge)?;
            cursor = align_up_u64(cursor, 4);
        }

        // Extensions follow the tile blob, with each section already
        // self-aligned (write_extension_section pads payload to 4 bytes).
        let extensions_offset = cursor;
        let mut ext_buffers: Vec<Vec<u8>> = Vec::with_capacity(state.extensions.len());
        for ext in &state.extensions {
            let section = ExtensionSection {
                tag: ext.tag,
                payload: ext.payload.clone(),
            };
            let buf = write_extension_section(&section);
            cursor = cursor
                .checked_add(buf.len() as u64)
                .ok_or(TileWriterError::PackTooLarge)?;
            ext_buffers.push(buf);
        }

        // After all extension sections, `cursor` is the CRC's byte offset.
        // The value isn't used further — we just write the CRC bytes to
        // `output` at the end of the function — but the assignment above
        // exists to express the layout invariant in code.
        let _ = cursor;

        // --- Build zoom_offsets[18] directory -------------------------
        //
        // zoom_offsets[z] = (first_index_entry_offset_for_zoom_z,
        //                    count_of_tiles_at_zoom_z).
        // For zooms not present, both fields are 0.
        let mut zoom_offsets = [ZoomOffset::default(); ZOOM_OFFSETS_COUNT];
        {
            // tiles is sorted by (z, x, y); walk it once.
            let mut entry_index: u64 = 0;
            let mut i = 0;
            while i < tiles.len() {
                let z = tiles[i].z;
                let z_us = z as usize;
                let start_entry = entry_index;
                let mut run = 0_u64;
                let mut j = i;
                while j < tiles.len() && tiles[j].z == z {
                    run += 1;
                    j += 1;
                }
                let count = u32::try_from(run).map_err(|_| TileWriterError::PackTooLarge)?;
                zoom_offsets[z_us] = ZoomOffset {
                    offset: index_offset + start_entry * 24,
                    count,
                };
                entry_index += run;
                i = j;
            }
        }

        let derived = DerivedHeaderFields {
            tile_count,
            index_offset,
            zoom_offsets,
            extensions_offset,
        };

        // --- Stream output ------------------------------------------

        let mut crc = Crc32::new();

        // Header.
        let header_bytes = write_header(&state.metadata, &derived);
        write_chunk(&mut output, &header_bytes, &mut crc)?;

        // Tile index — entries in (z, x, y) order. The writer-side index
        // entry's `offset` and `length` come from our layout plan.
        for (tile, &offset) in tiles.iter().zip(tile_offsets.iter()) {
            let length =
                u32::try_from(tile.content.len()).map_err(|_| TileWriterError::TileTooLarge)?;
            let entry = TileIndexEntry {
                z: tile.z,
                compression: Compression::None,
                flags: 0,
                x: tile.x,
                y: tile.y,
                offset,
                length,
            };
            let bytes = write_index_entry(&entry);
            write_chunk(&mut output, &bytes, &mut crc)?;
        }

        // Padding between index and tile blob.
        let pad_after_index = usize::try_from(tile_blob_start - (index_offset + index_size))
            .map_err(|_| TileWriterError::PackTooLarge)?;
        if pad_after_index > 0 {
            write_padding(&mut output, pad_after_index, &mut crc)?;
        }

        // Tile blob — each tile's bytes followed by 0-3 zero bytes of
        // alignment padding.
        for (tile, &offset) in tiles.iter().zip(tile_offsets.iter()) {
            match &tile.content {
                TileContent::Inline(bytes) => {
                    write_chunk(&mut output, bytes, &mut crc)?;
                }
                TileContent::External { source, byte_range } => {
                    let src_index = *source as usize;
                    let length = usize::try_from(byte_range.end - byte_range.start)
                        .map_err(|_| TileWriterError::PackTooLarge)?;
                    let mut buf = vec![0_u8; length];
                    self.byte_sources[src_index]
                        .read_range(byte_range.clone(), &mut buf)
                        .map_err(|err| TileWriterError::SourceIo {
                            source: *source,
                            err,
                        })?;
                    write_chunk(&mut output, &buf, &mut crc)?;
                }
            }
            // Pad this tile to 4-byte alignment.
            let tile_end = offset + tile.content.len();
            let next_aligned = align_up_u64(tile_end, 4);
            let pad = usize::try_from(next_aligned - tile_end)
                .map_err(|_| TileWriterError::PackTooLarge)?;
            if pad > 0 {
                write_padding(&mut output, pad, &mut crc)?;
            }
        }

        // Extensions.
        for buf in &ext_buffers {
            write_chunk(&mut output, buf, &mut crc)?;
        }

        // CRC footer.
        let crc_value = crc.finalize();
        let crc_bytes = crc_value.to_le_bytes();
        output
            .write_all(&crc_bytes)
            .map_err(TileWriterError::OutputIo)?;

        Ok(())
    }
}

fn write_chunk<W, OutErr, SrcErr>(
    output: &mut W,
    bytes: &[u8],
    crc: &mut Crc32,
) -> Result<(), TileWriterError<SrcErr, OutErr>>
where
    W: Write<Error = OutErr>,
{
    output.write_all(bytes).map_err(TileWriterError::OutputIo)?;
    crc.update(bytes);
    Ok(())
}

fn write_padding<W, OutErr, SrcErr>(
    output: &mut W,
    len: usize,
    crc: &mut Crc32,
) -> Result<(), TileWriterError<SrcErr, OutErr>>
where
    W: Write<Error = OutErr>,
{
    debug_assert!(len < 4, "padding should be 0-3 bytes");
    let zeros = [0_u8; 4];
    write_chunk(output, &zeros[..len], crc)
}

const fn align_up_u64(value: u64, alignment: u64) -> u64 {
    let rem = value % alignment;
    if rem == 0 {
        value
    } else {
        value + (alignment - rem)
    }
}

#[cfg(test)]
mod tests {
    use core::convert::Infallible;

    use super::super::types::{
        AddressingScheme, AxisConvention, PackMetadata, PixelFormat, Projection,
    };
    use super::super::writer_trait::{TileContent, TileWriter, TileWriterError};
    use super::UpackWriter;
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

    /// Writer type with `Infallible` source-error and `Infallible`
    /// output-error — useful for in-memory tests with `Vec<u8>` output
    /// and no external tile sources.
    type TestWriter = UpackWriter<Infallible, Infallible>;

    #[test]
    fn fresh_writer_rejects_add_tile_before_begin() {
        let mut w: TestWriter = UpackWriter::new();
        let err = w
            .add_tile_ref(4, 0, 0, TileContent::Inline(vec![0; 16]))
            .unwrap_err();
        assert!(matches!(err, TileWriterError::NotBegun));
    }

    #[test]
    fn fresh_writer_rejects_add_extension_before_begin() {
        let mut w: TestWriter = UpackWriter::new();
        let err = w.add_extension(*b"NAME", b"name").unwrap_err();
        assert!(matches!(err, TileWriterError::NotBegun));
    }

    #[test]
    fn fresh_writer_rejects_finalize_before_begin() {
        let w: TestWriter = UpackWriter::new();
        let buf: Vec<u8> = Vec::new();
        let err = w.finalize(buf).unwrap_err();
        assert!(matches!(err, TileWriterError::NotBegun));
    }

    #[test]
    fn begin_pack_twice_returns_already_begun() {
        let mut w: TestWriter = UpackWriter::new();
        w.begin_pack(baseline_metadata()).unwrap();
        let err = w.begin_pack(baseline_metadata()).unwrap_err();
        assert!(matches!(err, TileWriterError::AlreadyBegun));
    }

    #[test]
    fn begin_pack_with_zero_pack_uuid_is_invalid() {
        let mut w: TestWriter = UpackWriter::new();
        let mut m = baseline_metadata();
        m.pack_uuid = [0_u8; 16];
        let err = w.begin_pack(m).unwrap_err();
        assert!(matches!(err, TileWriterError::InvalidMetadata));
    }

    #[test]
    fn begin_pack_with_some_parent_uuid_is_invalid_in_v1() {
        let mut w: TestWriter = UpackWriter::new();
        let mut m = baseline_metadata();
        m.parent_uuid = Some([0xAB; 16]);
        let err = w.begin_pack(m).unwrap_err();
        assert!(matches!(err, TileWriterError::InvalidMetadata));
    }

    #[test]
    fn begin_pack_with_zero_tile_dim_is_invalid() {
        let mut w: TestWriter = UpackWriter::new();
        let mut m = baseline_metadata();
        m.tile_dim_px = 0;
        let err = w.begin_pack(m).unwrap_err();
        assert!(matches!(err, TileWriterError::InvalidMetadata));
    }

    #[test]
    fn begin_pack_with_inverted_zoom_range_is_invalid() {
        let mut w: TestWriter = UpackWriter::new();
        let mut m = baseline_metadata();
        m.zoom_range = (10, 5);
        let err = w.begin_pack(m).unwrap_err();
        assert!(matches!(err, TileWriterError::InvalidMetadata));
    }

    #[test]
    fn add_tile_with_zoom_outside_range_is_rejected() {
        let mut w: TestWriter = UpackWriter::new();
        w.begin_pack(baseline_metadata()).unwrap();
        // Range is 4..=6.
        let err = w
            .add_tile_ref(3, 0, 0, TileContent::Inline(vec![0; 16]))
            .unwrap_err();
        assert!(matches!(
            err,
            TileWriterError::TileZoomOutOfRange { z: 3, .. }
        ));
    }

    #[test]
    fn add_tile_with_duplicate_zxy_is_rejected() {
        let mut w: TestWriter = UpackWriter::new();
        w.begin_pack(baseline_metadata()).unwrap();
        w.add_tile_ref(4, 0, 0, TileContent::Inline(vec![0; 16]))
            .unwrap();
        let err = w
            .add_tile_ref(4, 0, 0, TileContent::Inline(vec![0; 16]))
            .unwrap_err();
        assert!(matches!(
            err,
            TileWriterError::DuplicateTile { z: 4, x: 0, y: 0 },
        ));
    }

    #[test]
    fn empty_pack_finalizes_successfully() {
        // A pack with zero tiles and zero extensions is still valid bytes.
        let mut w: TestWriter = UpackWriter::new();
        w.begin_pack(baseline_metadata()).unwrap();
        let mut buf: Vec<u8> = Vec::new();
        w.finalize(&mut buf).unwrap();
        // Header (322) + 0-3 alignment pad + tile_blob (0) + extensions (0) + CRC (4)
        // The empty index has size 0, tile_blob_start = align_up(322, 4) = 324.
        // So total = 324 + 0 + 0 + 4 = 328.
        assert_eq!(buf.len(), 328);
        // Magic check.
        assert_eq!(&buf[0..4], b"UPCK");
    }

    #[test]
    fn pack_with_one_inline_tile_finalizes() {
        let mut w: TestWriter = UpackWriter::new();
        w.begin_pack(baseline_metadata()).unwrap();
        w.add_tile_ref(4, 0, 0, TileContent::Inline(vec![0xAB; 16_384]))
            .unwrap();
        let mut buf: Vec<u8> = Vec::new();
        w.finalize(&mut buf).unwrap();
        // Layout: header (322) + index (24) = 346 → align to 348.
        // Tile blob: 16_384 bytes → 348..16732. Aligned already.
        // Extensions: 0 bytes.
        // CRC: 4 bytes.
        // Total: 348 + 16_384 + 4 = 16_736.
        assert_eq!(buf.len(), 348 + 16_384 + 4);
        assert_eq!(&buf[0..4], b"UPCK");
    }

    /// Vec<u8> as Write — convenient for in-memory tests. The
    /// `&mut Vec<u8>` reference also gets the impl through `Write for
    /// Vec<u8>` (`extend_from_slice` works through `&mut`).
    #[test]
    fn write_target_can_be_borrowed_vec() {
        let mut w: TestWriter = UpackWriter::new();
        w.begin_pack(baseline_metadata()).unwrap();
        let mut buf: Vec<u8> = Vec::new();
        w.finalize(&mut buf).unwrap();
        assert!(!buf.is_empty());
    }
}
