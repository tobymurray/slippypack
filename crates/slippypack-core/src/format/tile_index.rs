//! Tile-index entry serialization. 20 bytes per entry, little-endian.
//!
//! Layout per `spec/rawtiles-v1.0-rc1.md` § 5.1:
//!
//! | Offset | Size | Field            | v1 |
//! |-------:|-----:|------------------|----|
//! |  0     |   1  | `z`              |    |
//! |  1     |   1  | `compression`    | 0 (none) |
//! |  2     |   1  | `flags`          | 0 |
//! |  3     |   1  | `reserved`       | 0 (MUST) |
//! |  4     |   4  | `x` (u32 LE)     |    |
//! |  8     |   4  | `y` (u32 LE)     |    |
//! | 12     |   4  | `offset` (u32 LE)|    |
//! | 16     |   4  | `length` (u32 LE)|    |
//! | **20** |      | **entry size**   |    |
//!
//! The index is sorted by `(z, x, y)` ascending per the spec — readers
//! binary-search within each zoom's range using the header's
//! `zoom_offsets[24]` directory.

/// Size of one tile-index entry in bytes.
pub const INDEX_ENTRY_SIZE: usize = 20;

/// Compression byte values. v1 supports only [`Compression::None`];
/// reserved values fail the parser per the spec's v1 reader rules.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[non_exhaustive]
#[repr(u8)]
pub enum Compression {
    /// No compression. The only legal v1 value.
    None = 0,
    // Reserved: LZ4 (1), QOI (2).
}

impl Compression {
    #[must_use]
    pub const fn as_byte(self) -> u8 {
        self as u8
    }

    #[must_use]
    pub const fn from_byte(b: u8) -> Option<Self> {
        match b {
            0 => Some(Self::None),
            _ => None,
        }
    }
}

/// A single tile-index entry. The reserved byte (position 3) is enforced
/// by the writer to be zero; the reader rejects non-zero values.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TileIndexEntry {
    pub z: u8,
    pub compression: Compression,
    /// Per-tile flag bitmask. v1 packs MUST set 0; v1 readers refuse
    /// non-zero. Reserved for future per-tile attributes (transparent,
    /// encrypted, placeholder, etc).
    pub flags: u8,
    pub x: u32,
    pub y: u32,
    /// Byte offset of the tile's bytes within the pack file (from
    /// offset 0). Must be 4-byte-aligned per the tile-blob alignment
    /// rule in the spec. u32 caps total pack size at 4 GiB, which is
    /// well past any realistic sideload-format use case.
    pub offset: u32,
    /// Tile-bytes length. Per the spec, capped at `u32::MAX` — a single
    /// 128² ABGR2222 tile is 16 KB raw, well below the cap.
    pub length: u32,
}

/// Tile-index parse errors.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum TileIndexError {
    /// Slice shorter than [`INDEX_ENTRY_SIZE`].
    TooShort,
    /// `compression` byte is reserved-but-not-supported in v1.
    UnsupportedCompression(u8),
    /// `flags` byte is non-zero. v1 readers MUST refuse non-zero flags.
    NonZeroFlags(u8),
    /// `reserved` byte is non-zero. The spec mandates zero.
    NonZeroReservedByte(u8),
}

impl core::fmt::Display for TileIndexError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match *self {
            Self::TooShort => f.write_str("tile-index entry input is shorter than 20 bytes"),
            Self::UnsupportedCompression(b) => {
                write!(f, "unsupported compression value {b} (v1 requires 0)")
            }
            Self::NonZeroFlags(b) => write!(f, "non-zero flags byte {b:#04x} (v1 requires 0)"),
            Self::NonZeroReservedByte(b) => {
                write!(f, "non-zero reserved byte {b:#04x} (spec requires 0)")
            }
        }
    }
}

impl core::error::Error for TileIndexError {}

/// Serialize a [`TileIndexEntry`] to its 20-byte on-disk form.
#[must_use]
pub fn write_index_entry(entry: &TileIndexEntry) -> [u8; INDEX_ENTRY_SIZE] {
    let mut buf = [0_u8; INDEX_ENTRY_SIZE];
    buf[0] = entry.z;
    buf[1] = entry.compression.as_byte();
    buf[2] = entry.flags;
    // buf[3] = 0 (reserved); already zero from initialization.
    buf[4..8].copy_from_slice(&entry.x.to_le_bytes());
    buf[8..12].copy_from_slice(&entry.y.to_le_bytes());
    buf[12..16].copy_from_slice(&entry.offset.to_le_bytes());
    buf[16..20].copy_from_slice(&entry.length.to_le_bytes());
    buf
}

/// Parse a 20-byte tile-index entry, validating spec invariants
/// (`compression`, `flags`, `reserved` v1 rules).
///
/// # Errors
///
/// See [`TileIndexError`]. The function takes the first 20 bytes of
/// `input`; extra bytes after byte 20 are ignored.
///
/// # Panics
///
/// Does not panic in practice — the length check at the top
/// guarantees the internal slice-to-array conversions succeed.
pub fn read_index_entry(input: &[u8]) -> Result<TileIndexEntry, TileIndexError> {
    if input.len() < INDEX_ENTRY_SIZE {
        return Err(TileIndexError::TooShort);
    }
    let compression =
        Compression::from_byte(input[1]).ok_or(TileIndexError::UnsupportedCompression(input[1]))?;
    if input[2] != 0 {
        return Err(TileIndexError::NonZeroFlags(input[2]));
    }
    if input[3] != 0 {
        return Err(TileIndexError::NonZeroReservedByte(input[3]));
    }
    Ok(TileIndexEntry {
        z: input[0],
        compression,
        flags: input[2],
        x: u32::from_le_bytes(input[4..8].try_into().expect("4 bytes")),
        y: u32::from_le_bytes(input[8..12].try_into().expect("4 bytes")),
        offset: u32::from_le_bytes(input[12..16].try_into().expect("4 bytes")),
        length: u32::from_le_bytes(input[16..20].try_into().expect("4 bytes")),
    })
}

#[cfg(test)]
mod tests {
    use super::{
        Compression, INDEX_ENTRY_SIZE, TileIndexEntry, TileIndexError, read_index_entry,
        write_index_entry,
    };

    fn baseline_entry() -> TileIndexEntry {
        TileIndexEntry {
            z: 10,
            compression: Compression::None,
            flags: 0,
            x: 511,
            y: 340,
            offset: 290,
            length: 16_384,
        }
    }

    #[test]
    fn entry_size_is_20_bytes() {
        let buf = write_index_entry(&baseline_entry());
        assert_eq!(buf.len(), INDEX_ENTRY_SIZE);
        assert_eq!(buf.len(), 20);
    }

    #[test]
    fn z_byte_at_offset_0() {
        let mut e = baseline_entry();
        e.z = 17;
        let buf = write_index_entry(&e);
        assert_eq!(buf[0], 17);
    }

    #[test]
    fn compression_byte_at_offset_1_is_zero_for_v1() {
        let buf = write_index_entry(&baseline_entry());
        assert_eq!(buf[1], 0);
    }

    #[test]
    fn flags_byte_at_offset_2_is_zero_for_v1() {
        let buf = write_index_entry(&baseline_entry());
        assert_eq!(buf[2], 0);
    }

    #[test]
    fn reserved_byte_at_offset_3_is_zero() {
        let buf = write_index_entry(&baseline_entry());
        assert_eq!(buf[3], 0);
    }

    #[test]
    fn x_and_y_are_u32_little_endian() {
        let mut e = baseline_entry();
        e.x = 0xDEAD_BEEF;
        e.y = 0xCAFE_BABE;
        let buf = write_index_entry(&e);
        assert_eq!(
            u32::from_le_bytes(buf[4..8].try_into().unwrap()),
            0xDEAD_BEEF,
        );
        assert_eq!(
            u32::from_le_bytes(buf[8..12].try_into().unwrap()),
            0xCAFE_BABE,
        );
    }

    #[test]
    fn offset_is_u32_little_endian() {
        let mut e = baseline_entry();
        e.offset = 0x89AB_CDEF;
        let buf = write_index_entry(&e);
        assert_eq!(
            u32::from_le_bytes(buf[12..16].try_into().unwrap()),
            0x89AB_CDEF,
        );
    }

    #[test]
    fn length_is_u32_little_endian() {
        let mut e = baseline_entry();
        e.length = 0x1234_5678;
        let buf = write_index_entry(&e);
        assert_eq!(
            u32::from_le_bytes(buf[16..20].try_into().unwrap()),
            0x1234_5678,
        );
    }

    #[test]
    fn write_then_read_round_trips() {
        let e = baseline_entry();
        let buf = write_index_entry(&e);
        let parsed = read_index_entry(&buf).unwrap();
        assert_eq!(parsed, e);
    }

    #[test]
    fn read_rejects_too_short_input() {
        let buf = [0_u8; INDEX_ENTRY_SIZE - 1];
        assert_eq!(read_index_entry(&buf), Err(TileIndexError::TooShort));
    }

    #[test]
    fn read_rejects_unsupported_compression() {
        let mut buf = write_index_entry(&baseline_entry());
        buf[1] = 1; // reserved LZ4
        assert_eq!(
            read_index_entry(&buf),
            Err(TileIndexError::UnsupportedCompression(1)),
        );
    }

    #[test]
    fn read_rejects_non_zero_flags() {
        let mut buf = write_index_entry(&baseline_entry());
        buf[2] = 0x01;
        assert_eq!(
            read_index_entry(&buf),
            Err(TileIndexError::NonZeroFlags(0x01)),
        );
    }

    #[test]
    fn read_rejects_non_zero_reserved_byte() {
        let mut buf = write_index_entry(&baseline_entry());
        buf[3] = 0xFF;
        assert_eq!(
            read_index_entry(&buf),
            Err(TileIndexError::NonZeroReservedByte(0xFF)),
        );
    }

    #[test]
    fn extra_bytes_after_20_are_ignored() {
        let buf = write_index_entry(&baseline_entry());
        let mut padded = vec![0_u8; INDEX_ENTRY_SIZE + 10];
        padded[..INDEX_ENTRY_SIZE].copy_from_slice(&buf);
        padded[INDEX_ENTRY_SIZE..].fill(0xFF);
        let parsed = read_index_entry(&padded).unwrap();
        assert_eq!(parsed, baseline_entry());
    }
}
