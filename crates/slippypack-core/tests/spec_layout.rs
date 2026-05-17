//! Byte-layout-against-spec test for the `.rawtiles` writer.
//!
//! Builds three known packs (grid / pyramid / attr) using deterministic
//! tile content and metadata, then byte-compares the writer's output
//! against committed golden `.rawtiles` files in `tests/fixtures/format/`.
//!
//! Per PLAN.md § Test plan (test 4):
//!
//! > Byte-layout-against-spec test: three sub-tests, one per fixture,
//! > each byte-comparing the writer's output against the corresponding
//! > `golden-pack-*.rawtiles.hex`. **This is the test that catches off-by-
//! > one header errors, wrong endianness, mis-sized zoom_offsets
//! > entries, broken extension-section iteration, etc.** Until the
//! > una-sdk simulator round-trip exists, this is the only test that
//! > proves spec conformance.
//!
//! ## Bootstrapping / re-blessing
//!
//! If the fixture files don't exist yet, or if the on-disk spec
//! intentionally changes (writer fix, format-version bump, etc), run:
//!
//! ```sh
//! BLESS_SPEC_LAYOUT=1 cargo test --test spec_layout
//! ```
//!
//! which overwrites the golden files with the writer's current output.
//! Then commit the changed files and add a CHANGELOG entry. Routine
//! PRs that touch the writer should fail this test (without `BLESS`),
//! forcing the maintainer to consciously decide whether the drift is
//! intended.

use core::convert::Infallible;
use std::path::{Path, PathBuf};

use slippypack_core::format::{
    AddressingScheme, AxisConvention, Compression, PackMetadata, PixelFormat, Projection,
    RawtilesReader, RawtilesWriter, TAG_ATTR, TileContent, TileWriter,
};
use slippypack_core::identity::BoundingBox;

/// Fixed pack-identity bytes used by every fixture. Visibly "test-like"
/// (sequential 0x01..=0x10) so anyone inspecting a golden file knows it's
/// a fixture, not a real pack.
const FIXTURE_PACK_UUID: [u8; 16] = [
    0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0A, 0x0B, 0x0C, 0x0D, 0x0E, 0x0F, 0x10,
];

/// Fixed timestamp used by every fixture. Avoids any wall-clock or
/// file-mtime non-determinism.
const FIXTURE_BUILD_TIMESTAMP: u64 = 1_700_000_000;

/// Deterministic 16-byte tile content keyed on `(z, x, y)`. Each
/// channel-quantum-byte's value is computed from the coordinates so the
/// test can independently verify per-tile content without storing
/// every tile's bytes inline.
fn synth_tile_content(z: u8, x: u32, y: u32) -> Vec<u8> {
    let mut buf = Vec::with_capacity(16);
    buf.push(z);
    buf.extend_from_slice(&x.to_le_bytes());
    buf.extend_from_slice(&y.to_le_bytes());
    // 7 bytes of deterministic filler.
    for i in 0..7_u32 {
        let v = u32::from(z)
            .wrapping_mul(73)
            .wrapping_add(x.wrapping_mul(7))
            .wrapping_add(y.wrapping_mul(13))
            .wrapping_add(i.wrapping_mul(31));
        #[allow(clippy::cast_possible_truncation)]
        buf.push((v & 0xFF) as u8);
    }
    debug_assert_eq!(buf.len(), 16);
    buf
}

fn baseline_metadata(zoom_min: u8, zoom_max: u8) -> PackMetadata {
    PackMetadata {
        pack_uuid: FIXTURE_PACK_UUID,
        supersedes_uuid: None,
        parent_uuid: None,
        pixel_format: PixelFormat::Abgr2222,
        projection: Projection::WebMercator,
        tile_addressing_scheme: AddressingScheme::Quadtree,
        tile_axis_convention: AxisConvention::Xyz,
        tile_dim_px: 128,
        zoom_range: (zoom_min, zoom_max),
        bbox: BoundingBox {
            min_lon_micro: -180_000_000,
            min_lat_micro: -85_000_000,
            max_lon_micro: 180_000_000,
            max_lat_micro: 85_000_000,
        },
        build_timestamp: FIXTURE_BUILD_TIMESTAMP,
    }
}

/// Single-zoom grid: 25 tiles at z=4, x ∈ [0..5), y ∈ [0..5).
fn build_grid_pack() -> Vec<u8> {
    let mut w: RawtilesWriter<Infallible, Infallible> = RawtilesWriter::new();
    w.begin_pack(baseline_metadata(4, 4)).unwrap();
    for y in 0..5_u32 {
        for x in 0..5_u32 {
            w.add_tile_ref(4, x, y, Compression::None, TileContent::Inline(synth_tile_content(4, x, y)))
                .unwrap();
        }
    }
    let mut buf = Vec::new();
    w.finalize(&mut buf).unwrap();
    buf
}

/// Multi-zoom pyramid: 1 + 4 + 16 = 21 tiles across z=2..=4. Each zoom
/// covers a 2× wider region than the previous (the "pyramid cone"
/// shape: 1×1 at z=2, 2×2 at z=3, 4×4 at z=4). Exercises the
/// `zoom_offsets[18]` directory for non-trivial zoom distributions.
///
/// The plan's "synthetic-pyramid" sketch was z=2..=8 (5461 tiles); this
/// fixture trims to z=2..=4 because the larger size would commit a
/// ~150 KB golden file with no additional coverage of the per-zoom
/// directory's behavior (3 populated zooms is enough to verify the
/// directory's offset arithmetic).
fn build_pyramid_pack() -> Vec<u8> {
    let mut w: RawtilesWriter<Infallible, Infallible> = RawtilesWriter::new();
    w.begin_pack(baseline_metadata(2, 4)).unwrap();
    for z in 2..=4_u8 {
        // n_side is 1, 2, 4 for z=2, 3, 4 respectively.
        let n_side = 1_u32 << (z - 2);
        for y in 0..n_side {
            for x in 0..n_side {
                w.add_tile_ref(z, x, y, Compression::None, TileContent::Inline(synth_tile_content(z, x, y)))
                    .unwrap();
            }
        }
    }
    let mut buf = Vec::new();
    w.finalize(&mut buf).unwrap();
    buf
}

/// Single-zoom 3×3 grid at z=3 (9 tiles) plus an `ATTR` extension
/// section. Exercises the extension-section iterator's offset
/// arithmetic + the writer's `extensions_offset` layout.
fn build_attr_pack() -> Vec<u8> {
    let mut w: RawtilesWriter<Infallible, Infallible> = RawtilesWriter::new();
    w.begin_pack(baseline_metadata(3, 3)).unwrap();
    for y in 0..3_u32 {
        for x in 0..3_u32 {
            w.add_tile_ref(3, x, y, Compression::None, TileContent::Inline(synth_tile_content(3, x, y)))
                .unwrap();
        }
    }
    // Multi-source-style attribution: two newline-joined strings.
    let attribution =
        b"\xc2\xa9 OpenStreetMap contributors \xc2\xa9 MapTiler\n\xc2\xa9 U.S. Geological Survey";
    w.add_extension(TAG_ATTR, attribution).unwrap();
    let mut buf = Vec::new();
    w.finalize(&mut buf).unwrap();
    buf
}

/// Path to the golden-fixture file under `crates/slippypack-core/tests/fixtures/format/`.
fn fixture_path(name: &str) -> PathBuf {
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    Path::new(manifest_dir)
        .join("tests")
        .join("fixtures")
        .join("format")
        .join(name)
}

/// Compare `actual` against the golden fixture at `name`. If
/// `BLESS_SPEC_LAYOUT=1` is set, overwrite the fixture file instead of
/// comparing — used for bootstrap and intentional spec bumps.
fn assert_matches_golden(actual: &[u8], name: &str) {
    let path = fixture_path(name);
    if std::env::var("BLESS_SPEC_LAYOUT").is_ok() {
        std::fs::create_dir_all(path.parent().unwrap()).expect("create fixtures dir");
        std::fs::write(&path, actual).expect("write golden fixture");
        eprintln!(
            "BLESS_SPEC_LAYOUT: wrote {} bytes to {}",
            actual.len(),
            path.display(),
        );
        return;
    }
    let expected = std::fs::read(&path).unwrap_or_else(|err| {
        panic!(
            "golden fixture {} could not be read ({err}) — run with \
             BLESS_SPEC_LAYOUT=1 to bootstrap it",
            path.display(),
        )
    });
    assert_eq!(
        actual.len(),
        expected.len(),
        "byte count for {name}: actual {} != expected {}",
        actual.len(),
        expected.len(),
    );
    if actual != expected {
        // Find the first differing byte and surface a useful diff line.
        let diff_at = actual
            .iter()
            .zip(expected.iter())
            .position(|(a, b)| a != b)
            .unwrap();
        panic!(
            "fixture {name}: bytes diverged at offset {diff_at} (actual={:#04x}, expected={:#04x}). \
             Run BLESS_SPEC_LAYOUT=1 if this is an intentional spec bump and add a CHANGELOG entry.",
            actual[diff_at], expected[diff_at],
        );
    }
}

#[test]
fn grid_pack_matches_golden() {
    let bytes = build_grid_pack();
    assert_matches_golden(&bytes, "golden-grid.rawtiles");
}

#[test]
fn pyramid_pack_matches_golden() {
    let bytes = build_pyramid_pack();
    assert_matches_golden(&bytes, "golden-pyramid.rawtiles");
}

#[test]
fn attr_pack_matches_golden() {
    let bytes = build_attr_pack();
    assert_matches_golden(&bytes, "golden-attr.rawtiles");
}

/// Sanity check: every golden pack round-trips through the reader and
/// the per-zoom directory points at the right tile counts.
#[test]
fn grid_pack_round_trips() {
    let bytes = build_grid_pack();
    let r = RawtilesReader::open(&bytes).expect("grid pack should parse");
    assert_eq!(r.tile_count(), 25);
    for y in 0..5_u32 {
        for x in 0..5_u32 {
            assert_eq!(
                r.tile_bytes(4, x, y).unwrap(),
                synth_tile_content(4, x, y).as_slice(),
                "tile ({x},{y}) bytes",
            );
        }
    }
}

#[test]
fn pyramid_pack_round_trips() {
    let bytes = build_pyramid_pack();
    let r = RawtilesReader::open(&bytes).expect("pyramid pack should parse");
    assert_eq!(r.tile_count(), 1 + 4 + 16);
    // Per-zoom directory: each populated zoom's count matches 4^z.
    let header = r.header();
    assert_eq!(header.derived.zoom_offsets[2].count, 1);
    assert_eq!(header.derived.zoom_offsets[3].count, 4);
    assert_eq!(header.derived.zoom_offsets[4].count, 16);
    assert_eq!(header.derived.zoom_offsets[5].count, 0);
    // Sample-check tile content at each zoom.
    for z in 2..=4_u8 {
        let n_side = 1_u32 << (z - 2);
        for y in 0..n_side {
            for x in 0..n_side {
                assert_eq!(
                    r.tile_bytes(z, x, y).unwrap(),
                    synth_tile_content(z, x, y).as_slice(),
                    "tile (z={z}, x={x}, y={y})",
                );
            }
        }
    }
}

#[test]
fn attr_pack_round_trips() {
    let bytes = build_attr_pack();
    let r = RawtilesReader::open(&bytes).expect("attr pack should parse");
    assert_eq!(r.tile_count(), 9);
    let exts = r.extensions();
    assert_eq!(exts.len(), 1);
    assert_eq!(exts[0].tag, TAG_ATTR);
    let payload = std::str::from_utf8(&exts[0].payload).unwrap();
    assert!(payload.contains("OpenStreetMap"));
    assert!(payload.contains("U.S. Geological Survey"));
}
