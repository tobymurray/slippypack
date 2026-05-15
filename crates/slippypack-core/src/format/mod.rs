//! `.rawtiles` byte-layout primitives.
//!
//! The authoritative `.rawtiles` byte-level specification lives in its
//! own repository at <https://github.com/tobymurray/rawtiles>. PLAN.md
//! provides slippypack-level design context. This module implements the
//! spec:
//!
//! - [`types`]: `PixelFormat`, `Projection`, `AddressingScheme`,
//!   `AxisConvention`, `PackMetadata`, format-version + magic constants.
//! - [`header`]: 394-byte header serialization/parsing with spec
//!   validation.
//! - [`tile_index`]: 24-byte tile-index-entry layout with reserved-byte
//!   and compression/flags spec validation.
//! - [`extensions`]: tagged extension-section framing with 4-byte alignment.
//! - [`crc`]: streaming CRC-32/ISO-HDLC for the pack footer.
//! - [`writer_trait`] + [`rawtiles_writer`] + [`reader`]: high-level
//!   composition over the byte-layout primitives.

pub mod crc;
pub mod extensions;
pub mod header;
pub mod rawtiles_writer;
pub mod reader;
pub mod tile_index;
pub mod types;
pub mod writer_trait;

pub use crc::{Crc32, crc32_ieee};
pub use extensions::{
    ExtensionError, ExtensionSection, NameSectionError, SECTION_HEADER_SIZE, TAG_AFFN, TAG_ATTR,
    TAG_NAME, TAG_PLET, TAG_SRCD, build_name_payload, parse_name_payload, read_extension_sections,
    write_extension_section, write_extension_sections,
};
pub use header::{
    DerivedHeaderFields, HEADER_BASE_SIZE, HeaderError, ParsedHeader, ZOOM_OFFSETS_COUNT,
    ZoomOffset, read_header, write_header,
};
pub use rawtiles_writer::RawtilesWriter;
pub use reader::{RawtilesReader, ReaderError};
pub use tile_index::{
    Compression, INDEX_ENTRY_SIZE, TileIndexEntry, TileIndexError, read_index_entry,
    write_index_entry,
};
pub use types::{
    AddressingScheme, AxisConvention, FORMAT_VERSION, MAGIC, PackMetadata, PixelFormat, Projection,
};
pub use writer_trait::{SourceId, TileByteSource, TileContent, TileWriter, TileWriterError, Write};
