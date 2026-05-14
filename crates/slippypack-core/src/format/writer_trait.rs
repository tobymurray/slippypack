//! The `TileWriter` trait — the format-pluggability seam.
//!
//! See PLAN.md § `TileWriter` trait for the design context. This module
//! defines the trait itself plus the supporting types:
//!
//! - [`TileContent`]: `Inline(Vec<u8>)` or `External { source, byte_range }`.
//! - [`TileByteSource`]: trait for reading external tile bytes during finalize.
//! - [`Write`]: local Write trait (no `std::io::Write` to keep the door
//!   open for `no_std + alloc`). Signature-compatible with
//!   `embedded-io::Write`.
//! - [`TileWriterError<SrcErr, OutErr>`]: generic error type so concrete
//!   implementations keep their full error context.
//!
//! The concrete `UpackWriter` lives in [`super::upack_writer`].

use core::ops::Range;

/// Identifier handed out by [`TileWriter::register_byte_source`]. Used in
/// [`TileContent::External`] to refer back to a registered source.
pub type SourceId = u32;

/// How a tile's bytes are provided to the writer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TileContent {
    /// Tile bytes are owned inline. Small-pack-friendly: the writer
    /// holds the bytes in RAM until [`TileWriter::finalize`] runs.
    Inline(Vec<u8>),
    /// Tile bytes live in a registered [`TileByteSource`]; the writer
    /// reads them on `finalize` rather than buffering in RAM.
    ///
    /// A single [`SourceId`] can be referenced by any number of tiles;
    /// country-scale packs typically register one source (the temp-file
    /// or OPFS handle holding all decoded tiles) and use it for every
    /// `add_tile_ref` call.
    External {
        source: SourceId,
        byte_range: Range<u64>,
    },
}

impl TileContent {
    /// Number of tile bytes regardless of whether they're inline or
    /// referenced externally.
    #[must_use]
    pub fn len(&self) -> u64 {
        match self {
            Self::Inline(bytes) => bytes.len() as u64,
            Self::External { byte_range, .. } => byte_range.end - byte_range.start,
        }
    }

    /// Convenience: returns `true` if [`Self::len`] is zero.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// Source of tile bytes for [`TileContent::External`].
///
/// Implementations are not required to be `Send + Sync`; the writer
/// single-threads the read sequence inside `finalize`. The CLI's
/// implementation is a `std::fs::File` wrapper; the PWA worker's is
/// an OPFS `FileSystemSyncAccessHandle` wrapper.
pub trait TileByteSource {
    type Error;

    /// Read exactly `into.len()` bytes starting at `byte_range.start` from
    /// the source. The caller (the writer's `finalize`) guarantees
    /// `into.len() == byte_range.len()`.
    ///
    /// # Errors
    ///
    /// Returns the source's [`Self::Error`] on any underlying I/O
    /// failure (file read, OPFS access handle error, etc).
    fn read_range(&mut self, byte_range: Range<u64>, into: &mut [u8]) -> Result<(), Self::Error>;
}

/// Output sink for [`TileWriter::finalize`]. Local trait —
/// `std::io::Write` is unavailable when `slippypack-core` switches to
/// `no_std + alloc`. Signature-compatible with `embedded-io::Write`.
pub trait Write {
    type Error;

    /// Write all of `buf` to the sink, or return an error.
    ///
    /// # Errors
    ///
    /// Returns the sink's [`Self::Error`] on any underlying I/O failure.
    fn write_all(&mut self, buf: &[u8]) -> Result<(), Self::Error>;
}

/// `Vec<u8>` implements [`Write`] with `Infallible` error type — useful
/// for tests, in-memory pack assembly, and the OPFS round-trip.
impl Write for Vec<u8> {
    type Error = core::convert::Infallible;

    fn write_all(&mut self, buf: &[u8]) -> Result<(), Self::Error> {
        self.extend_from_slice(buf);
        Ok(())
    }
}

/// Blanket impl so `&mut W` is itself a [`Write`] — lets callers pass
/// `&mut buffer` to [`TileWriter::finalize`] without consuming the buffer.
impl<W: Write> Write for &mut W {
    type Error = W::Error;

    fn write_all(&mut self, buf: &[u8]) -> Result<(), Self::Error> {
        (**self).write_all(buf)
    }
}

/// Error type for [`TileWriter`] methods. Generic over the
/// [`TileByteSource`]'s error type (`SrcErr`) and the [`Write`]'s error
/// type (`OutErr`) so concrete implementations don't lose context
/// through a lossy enum.
///
/// `#[non_exhaustive]` — additional variants may be added in patch
/// releases as new failure modes surface.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum TileWriterError<SrcErr, OutErr> {
    /// A registered [`TileByteSource`] failed on a read.
    SourceIo { source: SourceId, err: SrcErr },
    /// The output [`Write`] failed.
    OutputIo(OutErr),
    /// An `add_tile_ref` referenced a [`SourceId`] that wasn't returned by
    /// any prior `register_byte_source` call.
    SourceUnregistered { source: SourceId },
    /// The pack exceeded the spec's field widths (very unlikely; the
    /// header uses u64 offsets).
    PackTooLarge,
    /// [`PackMetadata`] supplied to `begin_pack` fails spec validation
    /// (e.g. `tile_dim_px == 0`, `zoom_range.max < zoom_range.min`, or
    /// `parent_uuid != None` which is reserved-but-not-implemented in v1).
    InvalidMetadata,
    /// Two `add_tile_ref` calls used the same `(z, x, y)` key.
    DuplicateTile { z: u8, x: u32, y: u32 },
    /// `begin_pack` hasn't been called yet (or the writer was already
    /// finalized).
    NotBegun,
    /// `begin_pack` called twice on the same writer instance.
    AlreadyBegun,
    /// `add_tile_ref` provided `TileContent::Inline(bytes)` whose
    /// `bytes.len()` exceeds `u32::MAX` (the on-disk length field is u32).
    TileTooLarge,
    /// `add_extension` provided a payload whose length exceeds `u32::MAX`.
    ExtensionTooLarge,
    /// Tile zoom level is outside `[zoom_range.min, zoom_range.max]`.
    TileZoomOutOfRange { z: u8, min: u8, max: u8 },
    /// Tile zoom is `>= 24` and so doesn't fit the spec's 24-slot
    /// `zoom_offsets` directory.
    TileZoomTooHigh { z: u8 },
}

/// The format-writer trait. See PLAN.md § `TileWriter` trait — the
/// format-pluggability seam.
///
/// Implementations buffer metadata, registered byte sources, tile refs,
/// and extension records until `finalize` runs. `finalize` streams the
/// final pack bytes through the output `Write` while reading
/// `External` tile content from the matching `TileByteSource`.
///
/// Only one v1 implementation exists: [`super::upack_writer::UpackWriter`].
/// Future MBTiles or PMTiles writers are companion crates implementing
/// this same trait — no `slippypack-core` change required.
pub trait TileWriter {
    type SourceError;
    type OutputError;

    /// Set the pack metadata. Must be called once before any
    /// `add_tile_ref` / `add_extension` / `finalize` call.
    ///
    /// # Errors
    ///
    /// - [`TileWriterError::AlreadyBegun`] if `begin_pack` was already called.
    /// - [`TileWriterError::InvalidMetadata`] for spec invariant violations
    ///   (zero `pack_uuid`, non-None `parent_uuid` in v1, etc).
    fn begin_pack(
        &mut self,
        meta: super::PackMetadata,
    ) -> Result<(), TileWriterError<Self::SourceError, Self::OutputError>>;

    /// Register a [`TileByteSource`] that subsequent
    /// [`TileContent::External`] tile refs can refer back to. Returns a
    /// fresh [`SourceId`] for the registered source.
    ///
    /// Can be called any number of times; the writer accumulates sources
    /// independently of `begin_pack` state.
    fn register_byte_source(
        &mut self,
        source: Box<dyn TileByteSource<Error = Self::SourceError>>,
    ) -> SourceId;

    /// Record one extension section. The writer accumulates these and
    /// emits them in declared order during `finalize`.
    ///
    /// # Errors
    ///
    /// - [`TileWriterError::NotBegun`] if `begin_pack` hasn't run.
    /// - [`TileWriterError::ExtensionTooLarge`] if `payload.len() > u32::MAX`.
    fn add_extension(
        &mut self,
        tag: [u8; 4],
        payload: &[u8],
    ) -> Result<(), TileWriterError<Self::SourceError, Self::OutputError>>;

    /// Record one tile at `(z, x, y)`. The writer sorts by `(z, x, y)`
    /// during `finalize` to satisfy the spec's sorted-index requirement.
    ///
    /// # Errors
    ///
    /// - [`TileWriterError::NotBegun`] if `begin_pack` hasn't run.
    /// - [`TileWriterError::DuplicateTile`] if `(z, x, y)` is already
    ///   recorded.
    /// - [`TileWriterError::TileTooLarge`] if `Inline(bytes)` with
    ///   `bytes.len() > u32::MAX`.
    /// - [`TileWriterError::TileZoomOutOfRange`] if `z` is outside the
    ///   metadata's `zoom_range`.
    /// - [`TileWriterError::TileZoomTooHigh`] if `z >= 24`.
    fn add_tile_ref(
        &mut self,
        z: u8,
        x: u32,
        y: u32,
        content: TileContent,
    ) -> Result<(), TileWriterError<Self::SourceError, Self::OutputError>>;

    /// Stream the assembled pack bytes through `output`. Consumes the
    /// writer.
    ///
    /// # Errors
    ///
    /// - [`TileWriterError::NotBegun`] if `begin_pack` hasn't run.
    /// - [`TileWriterError::OutputIo`] for write failures.
    /// - [`TileWriterError::SourceIo`] for source-read failures.
    /// - [`TileWriterError::SourceUnregistered`] if a tile ref points at
    ///   an unknown `SourceId`.
    /// - [`TileWriterError::PackTooLarge`] for arithmetic overflow on
    ///   layout offsets (effectively unreachable; uses u64 throughout).
    fn finalize<W: Write<Error = Self::OutputError>>(
        self,
        output: W,
    ) -> Result<(), TileWriterError<Self::SourceError, Self::OutputError>>;
}

#[cfg(test)]
mod tests {
    use super::{TileContent, Write};

    #[test]
    fn tile_content_inline_len_is_byte_count() {
        let c = TileContent::Inline(vec![0_u8; 16_384]);
        assert_eq!(c.len(), 16_384);
        assert!(!c.is_empty());
    }

    #[test]
    fn tile_content_inline_empty() {
        let c = TileContent::Inline(vec![]);
        assert_eq!(c.len(), 0);
        assert!(c.is_empty());
    }

    #[test]
    fn tile_content_external_len_is_range_length() {
        let c = TileContent::External {
            source: 0,
            byte_range: 100..16_484,
        };
        assert_eq!(c.len(), 16_384);
    }

    #[test]
    fn vec_u8_implements_write() {
        let mut buf: Vec<u8> = Vec::new();
        buf.write_all(b"hello").unwrap();
        buf.write_all(b" world").unwrap();
        assert_eq!(buf, b"hello world");
    }
}
