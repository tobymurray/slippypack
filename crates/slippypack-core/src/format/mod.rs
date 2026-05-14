//! `.upack` byte-layout primitives.
//!
//! See PLAN.md § `TileWriter` trait and the una-sdk MapTrack spec for the
//! design context. This module currently ships the byte-layout primitives:
//!
//! - [`types`]: `PixelFormat`, `Projection`, `AddressingScheme`,
//!   `AxisConvention`, `PackMetadata`, format-version + magic constants.
//! - [`header`]: 322-byte header serialization/parsing with spec
//!   validation.
//! - [`tile_index`]: 24-byte tile-index-entry layout with reserved-byte
//!   and compression/flags spec validation.
//! - [`extensions`]: tagged extension-section framing with 4-byte alignment.
//! - [`crc`]: streaming CRC-32/ISO-HDLC for the pack footer.
//!
//! The `TileWriter` trait + `UpackWriter` + reader land in a follow-up
//! slice that composes these primitives.

pub mod crc;
pub mod extensions;
pub mod header;
pub mod reader;
pub mod tile_index;
pub mod types;
pub mod upack_writer;
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
pub use reader::{ReaderError, UpackReader};
pub use tile_index::{
    Compression, INDEX_ENTRY_SIZE, TileIndexEntry, TileIndexError, read_index_entry,
    write_index_entry,
};
pub use types::{
    AddressingScheme, AxisConvention, FORMAT_VERSION, MAGIC, PackMetadata, PixelFormat, Projection,
};
pub use upack_writer::UpackWriter;
pub use writer_trait::{SourceId, TileByteSource, TileContent, TileWriter, TileWriterError, Write};
