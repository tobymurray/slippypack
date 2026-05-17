//! End-to-end pipeline test: PNG → decode → quantise → format.
//!
//! Verifies the three stages compose correctly. The unit tests on each
//! stage (decode, quantise, format) check that stage in isolation
//! against hand-picked inputs; this test plays the whole pipeline
//! against a committed PNG fixture and byte-compares the resulting
//! pack against a committed golden `.rawtiles`.
//!
//! What this catches that the per-stage tests don't:
//!
//! - Shape contracts between stages (RGB888 stride, ABGR2222 byte
//!   ordering, tile-bytes length).
//! - Subtle channel-order regressions in either decode or quantise
//!   that the other stage's tests wouldn't notice.
//! - Pack composition with real decoded + quantised tile bytes, vs
//!   `spec_layout`'s raw deterministic patterns.
//!
//! What this **does not** catch (still gated on una-sdk MapTrack
//! Phase 2's simulator round-trip):
//!
//! - ABGR2222 bit-order matching the watch's `TouchGFX` framebuffer (Q-001).
//! - Header byte layout matching the una-sdk `TilePack` reader (F-001 etc).
//! - CRC-32 variant matching the watch's parser (F-007).
//!
//! ## Bootstrap / re-bless
//!
//! ```sh
//! BLESS_E2E=1 cargo test --test roundtrip
//! ```
//!
//! Overwrites the golden file with the current pipeline's output.
//! Routine PRs that affect any stage in the pipeline fail this test
//! without `BLESS_E2E`, forcing a conscious decision and a CHANGELOG
//! entry.

use core::convert::Infallible;
use std::path::{Path, PathBuf};

use slippypack_core::decode::decode_rgb888;
use slippypack_core::format::{
    AddressingScheme, AxisConvention, Compression, PackMetadata, PixelFormat, Projection,
    RawtilesReader, RawtilesWriter, TileContent, TileWriter,
};
use slippypack_core::identity::BoundingBox;
use slippypack_core::quantise::quantise_rgb888;

/// PNG fixture: a 2×2 RGB image with pixels (red, green, blue, white)
/// in row-major order. Generated via `ImageMagick` with metadata stripped
/// (93 bytes, palette-indexed PNG color type 3 — exercises the decode
/// module's palette → RGB conversion path).
const FIXTURE_PNG: &[u8] = include_bytes!("fixtures/e2e/input-2x2-rgb.png");

/// Visibly "test-like" pack identity; distinct from the `spec_layout`
/// fixtures' UUID so anyone inspecting either golden file can tell
/// which test it came from.
const FIXTURE_PACK_UUID: [u8; 16] = [
    0x20, 0x21, 0x22, 0x23, 0x24, 0x25, 0x26, 0x27, 0x28, 0x29, 0x2A, 0x2B, 0x2C, 0x2D, 0x2E, 0x2F,
];

const FIXTURE_BUILD_TIMESTAMP: u64 = 1_700_000_000;

fn metadata_for_2x2(zoom_min: u8, zoom_max: u8) -> PackMetadata {
    PackMetadata {
        pack_uuid: FIXTURE_PACK_UUID,
        supersedes_uuid: None,
        parent_uuid: None,
        pixel_format: PixelFormat::Abgr2222,
        projection: Projection::WebMercator,
        tile_addressing_scheme: AddressingScheme::Quadtree,
        tile_axis_convention: AxisConvention::Xyz,
        // 2 to match the PNG fixture; the watch reader would reject this
        // (it requires tile_dim_px = 128) — that's fine for testing the
        // pipeline composition without committing larger fixtures.
        tile_dim_px: 2,
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

fn fixture_path(name: &str) -> PathBuf {
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    Path::new(manifest_dir)
        .join("tests")
        .join("fixtures")
        .join("e2e")
        .join(name)
}

fn assert_matches_golden(actual: &[u8], name: &str) {
    let path = fixture_path(name);
    if std::env::var("BLESS_E2E").is_ok() {
        std::fs::create_dir_all(path.parent().unwrap()).expect("create fixtures dir");
        std::fs::write(&path, actual).expect("write golden fixture");
        eprintln!(
            "BLESS_E2E: wrote {} bytes to {}",
            actual.len(),
            path.display(),
        );
        return;
    }
    let expected = std::fs::read(&path).unwrap_or_else(|err| {
        panic!(
            "golden fixture {} could not be read ({err}) — run with \
             BLESS_E2E=1 to bootstrap it",
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
        let diff_at = actual
            .iter()
            .zip(expected.iter())
            .position(|(a, b)| a != b)
            .unwrap();
        panic!(
            "fixture {name}: bytes diverged at offset {diff_at} (actual={:#04x}, \
             expected={:#04x}). Run BLESS_E2E=1 if this is an intentional pipeline change \
             and add a CHANGELOG entry.",
            actual[diff_at], expected[diff_at],
        );
    }
}

/// Run the pipeline up to the quantise stage. Asserts that each
/// intermediate output matches what each stage's unit tests already
/// pin — anchoring the test against per-stage drift.
fn decode_and_quantise() -> Vec<u8> {
    // Stage 1: PNG → RGB888.
    let decoded = decode_rgb888(FIXTURE_PNG).expect("PNG fixture should decode");
    assert_eq!(decoded.width, 2);
    assert_eq!(decoded.height, 2);
    assert_eq!(decoded.rgb888.len(), 12);
    // Spot-check the decoded pixels match the input pattern.
    assert_eq!(decoded.pixel(0, 0), Some([255, 0, 0]), "decoded (0,0)");
    assert_eq!(decoded.pixel(1, 0), Some([0, 255, 0]), "decoded (1,0)");
    assert_eq!(decoded.pixel(0, 1), Some([0, 0, 255]), "decoded (0,1)");
    assert_eq!(decoded.pixel(1, 1), Some([255, 255, 255]), "decoded (1,1)");

    // Stage 2: RGB888 → ABGR2222.
    let mut quantised = vec![0_u8; 4];
    quantise_rgb888(&decoded.rgb888, &mut quantised);
    // Expected ABGR2222 bytes (AABBGGRR from MSB to LSB, A=3 always):
    //   red    (255,0,0)     → A=3, B=0, G=0, R=3 → 0b1100_0011 = 0xC3
    //   green  (0,255,0)     → A=3, B=0, G=3, R=0 → 0b1100_1100 = 0xCC
    //   blue   (0,0,255)     → A=3, B=3, G=0, R=0 → 0b1111_0000 = 0xF0
    //   white  (255,255,255) → A=3, B=3, G=3, R=3 → 0b1111_1111 = 0xFF
    assert_eq!(
        quantised,
        vec![0xC3, 0xCC, 0xF0, 0xFF],
        "quantised ABGR2222 bytes",
    );

    quantised
}

#[test]
fn single_tile_pack_pipeline_round_trips() {
    // Pure 1-tile sanity test: smallest possible end-to-end shape.
    let quantised = decode_and_quantise();
    let mut w: RawtilesWriter<Infallible, Infallible> = RawtilesWriter::new();
    w.begin_pack(metadata_for_2x2(0, 0)).unwrap();
    w.add_tile_ref(0, 0, 0, Compression::None, TileContent::Inline(quantised.clone()))
        .unwrap();
    let mut buf = Vec::new();
    w.finalize(&mut buf).unwrap();

    assert_matches_golden(&buf, "golden-png-to-pack-1tile.rawtiles");

    // Reader round-trip confirms the pack opens and yields the same
    // quantised bytes back.
    let r = RawtilesReader::open(&buf).expect("reader should open golden");
    assert_eq!(r.tile_count(), 1);
    assert_eq!(r.tile_bytes(0, 0, 0).unwrap(), quantised.as_slice());
}

#[test]
fn multi_zoom_pack_pipeline_round_trips() {
    // 1 tile at z=0 + 4 tiles at z=1, each using the same quantised
    // content from the PNG fixture. Exercises the per-zoom directory
    // for a multi-zoom pack and verifies the same decoded+quantised
    // tile bytes can be referenced from multiple (z, x, y) coords.
    let quantised = decode_and_quantise();
    let mut w: RawtilesWriter<Infallible, Infallible> = RawtilesWriter::new();
    w.begin_pack(metadata_for_2x2(0, 1)).unwrap();
    w.add_tile_ref(0, 0, 0, Compression::None, TileContent::Inline(quantised.clone()))
        .unwrap();
    for y in 0..2_u32 {
        for x in 0..2_u32 {
            w.add_tile_ref(1, x, y, Compression::None, TileContent::Inline(quantised.clone()))
                .unwrap();
        }
    }
    let mut buf = Vec::new();
    w.finalize(&mut buf).unwrap();

    assert_matches_golden(&buf, "golden-png-to-pack-5tiles.rawtiles");

    // Reader round-trip.
    let r = RawtilesReader::open(&buf).expect("reader should open golden");
    assert_eq!(r.tile_count(), 5);
    // Verify zoom_offsets directory.
    assert_eq!(r.header().derived.zoom_offsets[0].count, 1);
    assert_eq!(r.header().derived.zoom_offsets[1].count, 4);
    assert_eq!(r.header().derived.zoom_offsets[2].count, 0);
    // Verify every tile contains the same quantised bytes.
    assert_eq!(r.tile_bytes(0, 0, 0).unwrap(), quantised.as_slice());
    for y in 0..2_u32 {
        for x in 0..2_u32 {
            assert_eq!(
                r.tile_bytes(1, x, y).unwrap(),
                quantised.as_slice(),
                "tile (z=1, x={x}, y={y})",
            );
        }
    }
}

#[test]
fn pipeline_is_deterministic_across_invocations() {
    // Same PNG input → same pack bytes, every time. This is the
    // load-bearing determinism property: it survives the full
    // decode → quantise → format pipeline, not just any one stage.
    let bytes_a = {
        let q = decode_and_quantise();
        let mut w: RawtilesWriter<Infallible, Infallible> = RawtilesWriter::new();
        w.begin_pack(metadata_for_2x2(0, 0)).unwrap();
        w.add_tile_ref(0, 0, 0, Compression::None, TileContent::Inline(q)).unwrap();
        let mut buf = Vec::new();
        w.finalize(&mut buf).unwrap();
        buf
    };
    let bytes_b = {
        let q = decode_and_quantise();
        let mut w: RawtilesWriter<Infallible, Infallible> = RawtilesWriter::new();
        w.begin_pack(metadata_for_2x2(0, 0)).unwrap();
        w.add_tile_ref(0, 0, 0, Compression::None, TileContent::Inline(q)).unwrap();
        let mut buf = Vec::new();
        w.finalize(&mut buf).unwrap();
        buf
    };
    assert_eq!(bytes_a, bytes_b);
}
