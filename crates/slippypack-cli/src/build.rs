//! Build-pipeline orchestration: source → decode → quantise → format → output.
//!
//! Phase 1 first slice supports `--source synthetic` and HTTPS URL
//! templates (`--source 'https://.../{z}/{x}/{y}.png'`). Phase 1.x adds
//! MBTiles, PMTiles, and `dir://` source kinds via the same shape.

use core::convert::Infallible;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use slippypack_core::decode::{DecodeError, decode_rgb888};
use slippypack_core::format::{
    AddressingScheme, AxisConvention, PackMetadata, PixelFormat, Projection, RawtilesWriter,
    TileContent, TileWriter, TileWriterError,
};
use slippypack_core::identity::{
    AuthKind, BoundingBox, FormatVersion, PackDescriptor, Source, ZoomRange, derive_pack_uuid,
};
use slippypack_core::projection::{Mercator, Projection as ProjectionTrait};
use slippypack_core::quantise::{QUANTISER_VERSION, quantise_rgb888};

use crate::sources::synthetic;
use crate::sources::url_template::{
    AuthHeader, AuthQuery, UrlFetcher, UrlTemplate, UrlTemplateError,
};

/// Decimal-degree bounding box; CLI input shape before conversion to
/// the on-disk microdegree representation.
#[derive(Debug, Clone, Copy)]
pub struct BboxDeg {
    pub min_lon: f64,
    pub min_lat: f64,
    pub max_lon: f64,
    pub max_lat: f64,
}

impl BboxDeg {
    /// Convert to the spec-pinned integer microdegree representation
    /// (lat/lon × 10⁶, banker's rounding).
    #[must_use]
    pub fn to_micro(self) -> BoundingBox {
        BoundingBox {
            min_lon_micro: deg_to_micro(self.min_lon),
            min_lat_micro: deg_to_micro(self.min_lat),
            max_lon_micro: deg_to_micro(self.max_lon),
            max_lat_micro: deg_to_micro(self.max_lat),
        }
    }
}

#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "value is in roughly [-180e6, 180e6] which fits comfortably in i32"
)]
fn deg_to_micro(deg: f64) -> i32 {
    // banker's rounding: round-half-to-even.
    let scaled = deg * 1_000_000.0;
    let rounded = scaled.round_ties_even();
    rounded as i32
}

/// Errors a build can surface to the CLI.
#[derive(Debug)]
#[non_exhaustive]
pub enum BuildError {
    /// `--source` value couldn't be parsed.
    UnknownSourceKind(String),
    /// A `--source` URL was malformed.
    UrlTemplate(UrlTemplateError),
    /// `--bbox` is required for the given source kind but wasn't provided.
    MissingBbox,
    /// `--zoom` is required for the given source kind but wasn't provided.
    MissingZoom,
    /// User-initiated cancellation (SIGINT / Ctrl-C) interrupted the build.
    /// The `.partial` file has already been cleaned up by the time this
    /// surfaces.
    Cancelled,
    /// A PNG/JPEG decode failed.
    Decode(DecodeError),
    /// Decoded tile didn't fit the per-source expected dimensions
    /// (width × height ≠ `tile_dim_px²`).
    UnexpectedTileDimensions {
        z: u8,
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
            Self::UrlTemplate(e) => write!(f, "url-template source: {e}"),
            Self::MissingBbox => f.write_str("--bbox is required for non-synthetic sources"),
            Self::MissingZoom => f.write_str("--zoom is required for non-synthetic sources"),
            Self::Cancelled => f.write_str("build cancelled"),
            Self::Decode(e) => write!(f, "decode failed: {e}"),
            Self::UnexpectedTileDimensions {
                z,
                x,
                y,
                got_width,
                got_height,
                expected_dim,
            } => write!(
                f,
                "tile (z={z}, x={x}, y={y}) decoded to {got_width}×{got_height} but \
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

impl From<UrlTemplateError> for BuildError {
    fn from(e: UrlTemplateError) -> Self {
        Self::UrlTemplate(e)
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
    pub bbox: Option<BboxDeg>,
    /// `(zoom_min, zoom_max)` inclusive.
    pub zoom_range: Option<(u8, u8)>,
    /// `--auth-header "Name: value"` entries. Applied to every
    /// URL-template request; ignored for non-URL sources. Each entry's
    /// presence is reflected in the descriptor's `auth_kinds`; the
    /// values themselves are not part of the descriptor.
    pub auth_headers: Vec<AuthHeader>,
    /// `--auth-query "key=value"` entries. Appended to every URL-template
    /// request's URL; ignored for non-URL sources. Like `auth_headers`,
    /// only the *kind* enters the descriptor.
    pub auth_query: Vec<AuthQuery>,
    /// CI override: pin `build_timestamp` to a fixed value (seconds
    /// since Unix epoch). `None` → derive from inputs.
    pub timestamp_override: Option<u64>,
    /// CI override: pin `pack_uuid` to a fixed value. `None` → derive
    /// via UUIDv5 from the canonical source descriptor.
    pub pack_uuid_override: Option<[u8; 16]>,
    /// Cancellation token. `main.rs` wires this to the SIGINT/Ctrl-C
    /// handler so a Ctrl-C interrupts the build between tile operations.
    /// `None` → cancellation is impossible (used by unit tests that
    /// drive `build()` directly).
    pub cancel: Option<Arc<AtomicBool>>,
}

/// `Write` adapter so any `std::io::Write` can serve as the writer's
/// output sink. The wrapped error type is `std::io::Error`.
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
/// CLI uses this to keep `.rawtiles.partial` from leaking on early exit
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
    } else if opts.source.starts_with("http://") || opts.source.starts_with("https://") {
        build_url_template(opts)
    } else {
        Err(BuildError::UnknownSourceKind(opts.source.clone()))
    }
}

// --- Synthetic source ----------------------------------------------

fn build_synthetic(opts: &BuildOptions) -> Result<(), BuildError> {
    let descriptor = synthetic_descriptor();
    let metadata = build_metadata(opts, &descriptor, 0);
    let cancel = opts.cancel.clone();
    let sleep_ms = debug_sleep_per_tile_ms();
    run_build(opts, metadata, |writer| {
        for (x, y) in synthetic::all_tile_coords() {
            check_cancel(cancel.as_deref())?;
            if sleep_ms > 0 {
                std::thread::sleep(std::time::Duration::from_millis(sleep_ms));
            }
            let png_bytes = synthetic::tile_png_bytes(x, y)
                .expect("all_tile_coords yields coords that have fixtures");
            add_decoded_tile(
                writer,
                synthetic::ZOOM,
                x,
                y,
                png_bytes,
                synthetic::TILE_DIM_PX,
            )?;
        }
        Ok(())
    })
}

/// Read the `SLIPPYPACK_DEBUG_SLEEP_MS` env var. When set to a parseable
/// non-zero number, every per-tile iteration sleeps that many ms. Used
/// by the SIGINT integration test to make builds slow enough for a
/// signal to land mid-flight. Production runs leave it unset (== 0).
fn debug_sleep_per_tile_ms() -> u64 {
    std::env::var("SLIPPYPACK_DEBUG_SLEEP_MS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(0)
}

/// Return [`BuildError::Cancelled`] if the cancel token is set.
fn check_cancel(cancel: Option<&AtomicBool>) -> Result<(), BuildError> {
    if cancel.is_some_and(|c| c.load(Ordering::Relaxed)) {
        return Err(BuildError::Cancelled);
    }
    Ok(())
}

/// Build the canonical [`PackDescriptor`] for the source/bbox/zoom in
/// `opts` without doing any work beyond schema construction. Shared
/// between the `make` build path and the `debug uuid` diagnostic.
///
/// # Errors
///
/// - [`BuildError::UnknownSourceKind`] for a `--source` that isn't
///   `synthetic` and doesn't start with `http://` / `https://`.
/// - [`BuildError::UrlTemplate`] if the URL template is malformed.
/// - [`BuildError::MissingBbox`] / [`BuildError::MissingZoom`] if a
///   URL-template source is missing the required `--bbox` / `--zoom`.
pub fn descriptor_for(opts: &BuildOptions) -> Result<PackDescriptor, BuildError> {
    if opts.source == "synthetic" {
        Ok(synthetic_descriptor())
    } else if opts.source.starts_with("http://") || opts.source.starts_with("https://") {
        // Validate the URL template before constructing the descriptor
        // — the descriptor would otherwise embed an invalid template.
        UrlTemplate::parse(&opts.source)?;
        let bbox = opts.bbox.ok_or(BuildError::MissingBbox)?;
        let zoom = opts.zoom_range.ok_or(BuildError::MissingZoom)?;
        Ok(url_template_descriptor(
            &opts.source,
            bbox,
            zoom,
            auth_kinds_from_options(opts),
        ))
    } else {
        Err(BuildError::UnknownSourceKind(opts.source.clone()))
    }
}

fn auth_kinds_from_options(opts: &BuildOptions) -> Vec<AuthKind> {
    let mut kinds = Vec::new();
    if !opts.auth_headers.is_empty() {
        kinds.push(AuthKind::Header);
    }
    if !opts.auth_query.is_empty() {
        kinds.push(AuthKind::Query);
    }
    kinds
}

fn synthetic_descriptor() -> PackDescriptor {
    PackDescriptor {
        bbox: world_bbox_micro(),
        format_version: FormatVersion { major: 1, minor: 0 },
        pixel_format: 1,
        projection: 1,
        quantiser_version: QUANTISER_VERSION,
        sources: vec![Source::Synthetic {
            fixture_version: SYNTHETIC_FIXTURE_VERSION,
        }],
        style_hash: None,
        tile_addressing_scheme: 1,
        tile_axis_convention: 1,
        tile_dim_px: synthetic::TILE_DIM_PX,
        zoom_range: ZoomRange {
            min: synthetic::ZOOM,
            max: synthetic::ZOOM,
        },
    }
}

/// Bumped on any change to the committed PNG fixtures under
/// `crates/slippypack-cli/fixtures/synthetic-pattern/`. Carries forward
/// to the canonical source descriptor so a fixture refresh produces a
/// distinct `pack_uuid`.
const SYNTHETIC_FIXTURE_VERSION: u32 = 1;

fn world_bbox_micro() -> BoundingBox {
    BoundingBox {
        min_lon_micro: -180_000_000,
        min_lat_micro: -85_051_129,
        max_lon_micro: 180_000_000,
        max_lat_micro: 85_051_129,
    }
}

// --- URL-template source -------------------------------------------

fn build_url_template(opts: &BuildOptions) -> Result<(), BuildError> {
    let bbox = opts.bbox.ok_or(BuildError::MissingBbox)?;
    let zoom = opts.zoom_range.ok_or(BuildError::MissingZoom)?;
    let template = UrlTemplate::parse(&opts.source)?;

    let descriptor =
        url_template_descriptor(&opts.source, bbox, zoom, auth_kinds_from_options(opts));
    let mut fetcher = UrlFetcher::new();
    fetcher.set_auth_headers(opts.auth_headers.clone());
    fetcher.set_auth_query(opts.auth_query.clone());

    // Pre-fetch all tiles before opening the writer. This lets us:
    //   1. Use `fetcher.max_last_modified()` as `build_timestamp`
    //      (PLAN.md § Pack identity: timestamp records source freshness,
    //      not build wall-clock).
    //   2. Surface fetch errors before writing any pack bytes — the
    //      .partial file is never created if any tile fails.
    //
    // For Phase 1 first slice, we hold all tile bytes in memory. Typical
    // bbox builds are < 1000 tiles × ~10 KB = ~10 MB, which is fine.
    // Phase 1.x refactors to stream fetches into the writer for larger
    // packs.
    let mut tile_bytes: Vec<(u8, u32, u32, Vec<u8>)> = Vec::new();
    for z in zoom.0..=zoom.1 {
        for (x, y) in tile_range_for_zoom(bbox, z) {
            check_cancel(opts.cancel.as_deref())?;
            let url = template.url_for(z, x, y);
            let bytes = fetcher.fetch(&url)?;
            tile_bytes.push((z, x, y, bytes));
        }
    }

    let metadata = build_metadata(opts, &descriptor, fetcher.max_last_modified());
    let cancel = opts.cancel.clone();

    run_build(opts, metadata, move |writer| {
        for (z, x, y, bytes) in tile_bytes {
            check_cancel(cancel.as_deref())?;
            // Phase 1 first slice assumes tile_dim_px = 256 — the
            // dominant slippy-map tile size. Phase 1.x will sample the
            // first response's actual decoded dimensions and use those.
            let expected_dim = 256;
            add_decoded_tile(writer, z, x, y, &bytes, expected_dim)?;
        }
        Ok(())
    })
}

fn url_template_descriptor(
    url: &str,
    bbox: BboxDeg,
    zoom: (u8, u8),
    auth_kinds: Vec<AuthKind>,
) -> PackDescriptor {
    PackDescriptor {
        bbox: bbox.to_micro(),
        format_version: FormatVersion { major: 1, minor: 0 },
        pixel_format: 1,
        projection: 1,
        quantiser_version: QUANTISER_VERSION,
        sources: vec![Source::Url {
            template: url.to_string(),
            auth_kinds,
            zoom_min: zoom.0,
            zoom_max: zoom.1,
        }],
        style_hash: None,
        tile_addressing_scheme: 1,
        tile_axis_convention: 1,
        tile_dim_px: 256,
        zoom_range: ZoomRange {
            min: zoom.0,
            max: zoom.1,
        },
    }
}

fn tile_range_for_zoom(bbox: BboxDeg, zoom: u8) -> impl Iterator<Item = (u32, u32)> {
    let m = Mercator;
    // NW corner: (min_lon, max_lat). SE corner: (max_lon, min_lat). In
    // Mercator y increases southward, so the NW corner has the smaller y.
    let (x_nw, y_nw) = m.lonlat_to_tile(bbox.min_lon, bbox.max_lat, zoom);
    let (x_se, y_se) = m.lonlat_to_tile(bbox.max_lon, bbox.min_lat, zoom);
    (y_nw..=y_se).flat_map(move |y| (x_nw..=x_se).map(move |x| (x, y)))
}

// --- Shared build scaffolding --------------------------------------

fn build_metadata(
    opts: &BuildOptions,
    descriptor: &PackDescriptor,
    build_timestamp: u64,
) -> PackMetadata {
    let pack_uuid = opts
        .pack_uuid_override
        .unwrap_or_else(|| *derive_pack_uuid(descriptor).as_bytes());
    let build_timestamp = opts.timestamp_override.unwrap_or(build_timestamp);
    let bbox = descriptor.bbox;
    let zoom_range = (descriptor.zoom_range.min, descriptor.zoom_range.max);
    PackMetadata {
        pack_uuid,
        supersedes_uuid: None,
        parent_uuid: None,
        pixel_format: PixelFormat::Abgr2222,
        projection: Projection::WebMercator,
        tile_addressing_scheme: AddressingScheme::Quadtree,
        tile_axis_convention: AxisConvention::Xyz,
        tile_dim_px: descriptor.tile_dim_px,
        zoom_range,
        bbox,
        build_timestamp,
    }
}

/// Per-tile decode → quantise → add. Verifies the decoded dimensions
/// match the metadata's `tile_dim_px`.
fn add_decoded_tile(
    writer: &mut RawtilesWriter<Infallible, std::io::Error>,
    z: u8,
    x: u32,
    y: u32,
    png_or_jpeg: &[u8],
    expected_dim: u16,
) -> Result<(), BuildError> {
    let decoded = decode_rgb888(png_or_jpeg)?;
    if decoded.width != u32::from(expected_dim) || decoded.height != u32::from(expected_dim) {
        return Err(BuildError::UnexpectedTileDimensions {
            z,
            x,
            y,
            got_width: decoded.width,
            got_height: decoded.height,
            expected_dim,
        });
    }
    let mut quantised = vec![0_u8; usize::from(expected_dim) * usize::from(expected_dim)];
    quantise_rgb888(&decoded.rgb888, &mut quantised);
    writer.add_tile_ref(z, x, y, TileContent::Inline(quantised))?;
    Ok(())
}

/// Inner build scaffolding: open the writer, run the tile-population
/// closure, write the .partial file, atomic rename to final.
fn run_build<F>(opts: &BuildOptions, metadata: PackMetadata, populate: F) -> Result<(), BuildError>
where
    F: FnOnce(&mut RawtilesWriter<Infallible, std::io::Error>) -> Result<(), BuildError>,
{
    let partial_path = partial_path_for(&opts.out);
    let partial = PartialFile::new(partial_path.clone());

    let mut writer: RawtilesWriter<Infallible, std::io::Error> = RawtilesWriter::new();
    writer.begin_pack(metadata).map_err(BuildError::Writer)?;
    populate(&mut writer)?;

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
    partial.commit(&opts.out)?;
    Ok(())
}

fn partial_path_for(out: &Path) -> PathBuf {
    let mut s = out.as_os_str().to_owned();
    s.push(".partial");
    PathBuf::from(s)
}

#[cfg(test)]
mod tests {
    use super::{
        BboxDeg, BuildError, BuildOptions, build, deg_to_micro, partial_path_for,
        tile_range_for_zoom,
    };
    use std::path::PathBuf;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};

    #[test]
    fn partial_path_appends_partial_suffix() {
        let out = PathBuf::from("/tmp/trail.rawtiles");
        let partial = partial_path_for(&out);
        assert_eq!(partial, PathBuf::from("/tmp/trail.rawtiles.partial"));
    }

    #[test]
    fn partial_path_handles_no_extension() {
        let out = PathBuf::from("output");
        let partial = partial_path_for(&out);
        assert_eq!(partial, PathBuf::from("output.partial"));
    }

    #[test]
    fn deg_to_micro_round_trip() {
        assert_eq!(deg_to_micro(-180.0), -180_000_000);
        assert_eq!(deg_to_micro(0.0), 0);
        assert_eq!(deg_to_micro(180.0), 180_000_000);
        assert_eq!(deg_to_micro(-0.1278), -127_800);
    }

    #[test]
    fn deg_to_micro_uses_banker_rounding() {
        // 0.5 ties round to even.
        assert_eq!(deg_to_micro(0.000_000_5), 0); // exactly halfway → even (0)
        assert_eq!(deg_to_micro(0.000_001_5), 2); // halfway → even (2)
    }

    #[test]
    fn tile_range_covers_london_at_zoom_10() {
        // London-ish bbox.
        let bbox = BboxDeg {
            min_lon: -0.15,
            min_lat: 51.49,
            max_lon: -0.10,
            max_lat: 51.52,
        };
        let tiles: Vec<_> = tile_range_for_zoom(bbox, 10).collect();
        // Charing Cross at z=10 is tile (511, 340); a narrow bbox around
        // it covers at least that tile.
        assert!(tiles.iter().any(|&(x, y)| x == 511 && y == 340));
    }

    #[test]
    fn tile_range_single_zoom_includes_corners() {
        // A bbox covering the whole world.
        let bbox = BboxDeg {
            min_lon: -180.0,
            min_lat: -85.0,
            max_lon: 180.0,
            max_lat: 85.0,
        };
        let tiles: Vec<_> = tile_range_for_zoom(bbox, 1).collect();
        // At z=1 there are 4 tiles total: (0,0), (0,1), (1,0), (1,1).
        assert_eq!(tiles.len(), 4);
        let mut sorted = tiles.clone();
        sorted.sort_unstable();
        assert_eq!(sorted, vec![(0, 0), (0, 1), (1, 0), (1, 1)]);
    }

    #[test]
    fn pre_set_cancel_token_aborts_build_and_cleans_partial() {
        // Pre-set the cancel token so the build's very first check_cancel
        // call returns Err(Cancelled). This validates two invariants:
        //
        //   1. The build surfaces BuildError::Cancelled rather than
        //      partial output.
        //   2. The .partial file's RAII drop runs and removes the partial
        //      file from disk. The PartialFile guard is constructed
        //      *inside* run_build, so for cancellation that fires inside
        //      the populate closure the partial file is created and then
        //      cleaned up. For cancellation BEFORE the file is opened
        //      (the early-check_cancel inside the populate closure, which
        //      fires before OpenOptions::open), the partial file is
        //      never created — equally valid.
        let tmp = std::env::temp_dir().join(format!(
            "slippypack-cancel-test-{}.rawtiles",
            std::process::id(),
        ));
        let partial = partial_path_for(&tmp);

        // Belt-and-braces cleanup in case a previous run left stragglers.
        let _ = std::fs::remove_file(&tmp);
        let _ = std::fs::remove_file(&partial);

        let cancel = Arc::new(AtomicBool::new(true));
        let opts = BuildOptions {
            source: "synthetic".to_string(),
            out: tmp.clone(),
            bbox: None,
            zoom_range: None,
            auth_headers: Vec::new(),
            auth_query: Vec::new(),
            timestamp_override: Some(0),
            pack_uuid_override: None,
            cancel: Some(Arc::clone(&cancel)),
        };

        let err = build(&opts).expect_err("pre-cancelled build should fail");
        assert!(
            matches!(err, BuildError::Cancelled),
            "expected Cancelled, got {err:?}",
        );

        assert!(
            !tmp.exists(),
            "cancelled build must not leave a final .rawtiles at {}",
            tmp.display(),
        );
        assert!(
            !partial.exists(),
            "cancelled build must not leave a .partial at {}",
            partial.display(),
        );

        // Token was set at start; should remain set (we never reset it).
        assert!(cancel.load(Ordering::Relaxed));
    }

    #[test]
    fn check_cancel_returns_ok_when_token_unset() {
        let cancel = AtomicBool::new(false);
        let res = super::check_cancel(Some(&cancel));
        assert!(res.is_ok());
    }

    #[test]
    fn check_cancel_returns_ok_when_no_token() {
        let res = super::check_cancel(None);
        assert!(res.is_ok());
    }

    #[test]
    fn check_cancel_returns_cancelled_when_token_set() {
        let cancel = AtomicBool::new(true);
        let res = super::check_cancel(Some(&cancel));
        assert!(matches!(res, Err(BuildError::Cancelled)));
    }
}
