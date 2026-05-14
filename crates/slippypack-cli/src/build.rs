//! Build-pipeline orchestration: source → decode → quantise → format → output.
//!
//! Phase 1 first slice handles only `--source synthetic`. Phase 1.x adds
//! URL templates, MBTiles, PMTiles, and `dir://` source kinds via the
//! same orchestration shape.

use core::convert::Infallible;
use std::path::{Path, PathBuf};

use slippypack_core::decode::{DecodeError, decode_rgb888};
use slippypack_core::format::{
    AddressingScheme, AxisConvention, PackMetadata, PixelFormat, Projection, TileContent,
    TileWriter, TileWriterError, UpackWriter,
};
use slippypack_core::identity::BoundingBox;
use slippypack_core::quantise::quantise_rgb888;

use crate::sources::synthetic;

/// Errors a build can surface to the CLI.
#[derive(Debug)]
#[non_exhaustive]
pub enum BuildError {
    /// `--source` value couldn't be parsed.
    UnknownSourceKind(String),
    /// A PNG/JPEG decode failed.
    Decode(DecodeError),
    /// Decoded tile didn't fit the per-source expected dimensions
    /// (width × height ≠ `tile_dim_px²`).
    UnexpectedTileDimensions {
        x: u32,
        y: u32,
        got_width: u32,
        got_height: u32,
        expected_dim: u16,
    },
    /// The writer rejected metadata or a tile/extension.
    Writer(TileWriterError<Infallible, std::io::Error>),
    /// File I/O failure when opening or writing the partial output, or
    /// renaming it to the final path.
    Io(std::io::Error),
    /// `--pack-uuid` was passed but the hex couldn't be parsed.
    InvalidPackUuid(String),
}

impl core::fmt::Display for BuildError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::UnknownSourceKind(s) => write!(f, "unknown source kind: {s}"),
            Self::Decode(e) => write!(f, "decode failed: {e}"),
            Self::UnexpectedTileDimensions {
                x,
                y,
                got_width,
                got_height,
                expected_dim,
            } => write!(
                f,
                "tile ({x},{y}) decoded to {got_width}×{got_height} but \
                 the source's tile_dim_px is {expected_dim}",
            ),
            Self::Writer(e) => write!(f, "writer rejected: {e:?}"),
            Self::Io(e) => write!(f, "I/O error: {e}"),
            Self::InvalidPackUuid(s) => write!(f, "invalid --pack-uuid value: {s:?}"),
        }
    }
}

impl std::error::Error for BuildError {}

impl From<DecodeError> for BuildError {
    fn from(e: DecodeError) -> Self {
        Self::Decode(e)
    }
}

impl From<std::io::Error> for BuildError {
    fn from(e: std::io::Error) -> Self {
        Self::Io(e)
    }
}

impl From<TileWriterError<Infallible, std::io::Error>> for BuildError {
    fn from(e: TileWriterError<Infallible, std::io::Error>) -> Self {
        Self::Writer(e)
    }
}

/// CLI-supplied build parameters that don't depend on source kind.
pub struct BuildOptions {
    pub source: String,
    pub out: PathBuf,
    /// CI override: pin `build_timestamp` to a fixed value (seconds
    /// since Unix epoch). `None` → derive from inputs (always `0` for
    /// the synthetic source, which has no `Last-Modified`).
    pub timestamp_override: Option<u64>,
    /// CI override: pin `pack_uuid` to a fixed value. `None` → derive
    /// from the canonical source descriptor. For Phase 1 first slice,
    /// synthetic-source builds use a fixed descriptor-derived UUID.
    pub pack_uuid_override: Option<[u8; 16]>,
}

/// `Write` adapter so any `std::io::Write` can serve as the writer's
/// output sink. The wrapped `error::Error` type is `std::io::Error`.
struct IoWriteAdapter<W: std::io::Write> {
    inner: W,
}

impl<W: std::io::Write> slippypack_core::format::Write for IoWriteAdapter<W> {
    type Error = std::io::Error;
    fn write_all(&mut self, buf: &[u8]) -> Result<(), Self::Error> {
        self.inner.write_all(buf)
    }
}

/// RAII guard: deletes `path` on drop unless `commit()` was called. The
/// CLI uses this to keep `.upack.partial` from leaking on early exit
/// (panic, error, or SIGINT-triggered cleanup).
struct PartialFile {
    path: PathBuf,
    committed: bool,
}

impl PartialFile {
    fn new(path: PathBuf) -> Self {
        Self {
            path,
            committed: false,
        }
    }

    /// Atomically rename the partial file to `final_path`. After a
    /// successful commit the drop-time cleanup is skipped.
    fn commit(mut self, final_path: &Path) -> std::io::Result<()> {
        std::fs::rename(&self.path, final_path)?;
        self.committed = true;
        Ok(())
    }
}

impl Drop for PartialFile {
    fn drop(&mut self) {
        if !self.committed {
            // Best-effort cleanup; ignore the result (the file may
            // already have been deleted, or never created).
            let _ = std::fs::remove_file(&self.path);
        }
    }
}

/// Run the build to completion.
///
/// # Errors
///
/// Surfaces any source / decode / quantise / writer / I/O failure.
pub fn build(opts: &BuildOptions) -> Result<(), BuildError> {
    if opts.source == "synthetic" {
        build_synthetic(opts)
    } else {
        Err(BuildError::UnknownSourceKind(opts.source.clone()))
    }
}

fn build_synthetic(opts: &BuildOptions) -> Result<(), BuildError> {
    let metadata = synthetic_metadata(opts);
    let partial_path = partial_path_for(&opts.out);
    let partial = PartialFile::new(partial_path.clone());

    // Build the pack in-memory first, then write to .partial in one go.
    // For the synthetic fixture (16 tiles × ~16 bytes each), this fits
    // in RAM trivially. Phase 8 (OPFS streaming) will replace the
    // in-memory buffer with a streaming approach for country-scale packs.
    let mut writer: UpackWriter<Infallible, std::io::Error> = UpackWriter::new();
    writer.begin_pack(metadata).map_err(BuildError::Writer)?;

    for (x, y) in synthetic::all_tile_coords() {
        let png_bytes = synthetic::tile_png_bytes(x, y)
            .expect("all_tile_coords yields coords that have fixtures");
        let decoded = decode_rgb888(png_bytes).map_err(BuildError::Decode)?;
        if decoded.width != u32::from(synthetic::TILE_DIM_PX)
            || decoded.height != u32::from(synthetic::TILE_DIM_PX)
        {
            return Err(BuildError::UnexpectedTileDimensions {
                x,
                y,
                got_width: decoded.width,
                got_height: decoded.height,
                expected_dim: synthetic::TILE_DIM_PX,
            });
        }
        let mut quantised =
            vec![0_u8; usize::from(synthetic::TILE_DIM_PX) * usize::from(synthetic::TILE_DIM_PX)];
        quantise_rgb888(&decoded.rgb888, &mut quantised);
        writer
            .add_tile_ref(synthetic::ZOOM, x, y, TileContent::Inline(quantised))
            .map_err(BuildError::Writer)?;
    }

    // Create the .partial file and stream the pack into it.
    let partial_file = std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .open(&partial_path)?;
    let mut adapter = IoWriteAdapter {
        inner: partial_file,
    };
    writer.finalize(&mut adapter).map_err(BuildError::Writer)?;
    adapter.inner.sync_all()?;

    // Atomic rename: .partial → .upack.
    partial.commit(&opts.out)?;
    Ok(())
}

fn synthetic_metadata(opts: &BuildOptions) -> PackMetadata {
    // World bbox in microdegrees. The synthetic fixture nominally
    // covers the whole world at z=2.
    let bbox = BoundingBox {
        min_lon_micro: -180_000_000,
        min_lat_micro: -85_051_129,
        max_lon_micro: 180_000_000,
        max_lat_micro: 85_051_129,
    };
    // Synthetic-source pack_uuid: derived from the canonical descriptor
    // when --pack-uuid is unset. For Phase 1 first slice we use a fixed
    // deterministic value baked into the synthetic source's identity
    // (Phase 1.x will run the actual UUIDv5 derivation per the descriptor
    // schema in identity.rs).
    let pack_uuid = opts.pack_uuid_override.unwrap_or(SYNTHETIC_PACK_UUID);
    PackMetadata {
        pack_uuid,
        supersedes_uuid: None,
        parent_uuid: None,
        pixel_format: PixelFormat::Abgr2222,
        projection: Projection::WebMercator,
        tile_addressing_scheme: AddressingScheme::Quadtree,
        tile_axis_convention: AxisConvention::Xyz,
        tile_dim_px: synthetic::TILE_DIM_PX,
        zoom_range: (synthetic::ZOOM, synthetic::ZOOM),
        bbox,
        build_timestamp: opts.timestamp_override.unwrap_or(0),
    }
}

/// Stand-in `pack_uuid` for the synthetic source until Phase 1.x lands
/// the proper UUIDv5-from-canonical-descriptor derivation. Pinned so
/// the golden-synthetic test stays stable.
///
/// Bytes: `73 79 6e 74 68 65 74 69 63 5f 70 61 63 6b 21 00`
/// (`"synthetic_pack!\0"` — visibly test-like, never zero).
const SYNTHETIC_PACK_UUID: [u8; 16] = [
    0x73, 0x79, 0x6E, 0x74, 0x68, 0x65, 0x74, 0x69, 0x63, 0x5F, 0x70, 0x61, 0x63, 0x6B, 0x21, 0x00,
];

fn partial_path_for(out: &Path) -> PathBuf {
    let mut s = out.as_os_str().to_owned();
    s.push(".partial");
    PathBuf::from(s)
}

#[cfg(test)]
mod tests {
    use super::partial_path_for;
    use std::path::PathBuf;

    #[test]
    fn partial_path_appends_partial_suffix() {
        let out = PathBuf::from("/tmp/trail.upack");
        let partial = partial_path_for(&out);
        assert_eq!(partial, PathBuf::from("/tmp/trail.upack.partial"));
    }

    #[test]
    fn partial_path_handles_no_extension() {
        let out = PathBuf::from("output");
        let partial = partial_path_for(&out);
        assert_eq!(partial, PathBuf::from("output.partial"));
    }
}
