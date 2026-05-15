//! Integration test: `slippypack make --source synthetic` produces the
//! committed `golden-synthetic.rawtiles` and the output round-trips through
//! `slippypack_core::format::RawtilesReader`.
//!
//! This is PLAN.md's test 5b: "CLI smoke test (synthetic). Invoke
//! `slippypack make --source synthetic --out test.rawtiles` against the
//! binary's embedded `synthetic-pattern/` fixture, verify the file
//! parses via the core's reader and matches a committed
//! `golden-synthetic.rawtiles.hex`. Fully deterministic (no network);
//! guards the path the README points new users at."
//!
//! ## Bootstrap / re-bless
//!
//! ```sh
//! BLESS_CLI_SYNTHETIC=1 cargo test --test cli_synthetic
//! ```
//!
//! Overwrites the golden with the current CLI's output. Routine PRs
//! that touch the CLI / synthetic source / pipeline fail this test
//! without `BLESS_CLI_SYNTHETIC`, forcing a conscious CHANGELOG entry
//! on intentional changes.

use std::path::{Path, PathBuf};
use std::process::Command;

use slippypack_core::format::RawtilesReader;

/// Path to the `slippypack` binary produced by Cargo's test harness.
fn binary_path() -> &'static str {
    env!("CARGO_BIN_EXE_slippypack")
}

fn fixture_path(name: &str) -> PathBuf {
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    Path::new(manifest_dir)
        .join("tests")
        .join("fixtures")
        .join(name)
}

fn assert_matches_golden(actual: &[u8], name: &str) {
    let path = fixture_path(name);
    if std::env::var("BLESS_CLI_SYNTHETIC").is_ok() {
        std::fs::create_dir_all(path.parent().unwrap()).expect("create fixtures dir");
        std::fs::write(&path, actual).expect("write golden fixture");
        eprintln!(
            "BLESS_CLI_SYNTHETIC: wrote {} bytes to {}",
            actual.len(),
            path.display(),
        );
        return;
    }
    let expected = std::fs::read(&path).unwrap_or_else(|err| {
        panic!(
            "golden fixture {} could not be read ({err}) — run with \
             BLESS_CLI_SYNTHETIC=1 to bootstrap it",
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
             expected={:#04x}). Run BLESS_CLI_SYNTHETIC=1 if this is an intentional change \
             and add a CHANGELOG entry.",
            actual[diff_at], expected[diff_at],
        );
    }
}

/// Create a unique temp-file path under the OS temp dir. Tests that
/// share this helper get distinct paths so they don't race over the
/// same file.
fn temp_pack_path(test_name: &str) -> PathBuf {
    let mut p = std::env::temp_dir();
    let pid = std::process::id();
    p.push(format!("slippypack-test-{pid}-{test_name}.rawtiles"));
    // Ensure no stale leftover from a prior run.
    let _ = std::fs::remove_file(&p);
    let _ = std::fs::remove_file(p.with_extension("upack.partial"));
    p
}

#[test]
fn cli_synthetic_produces_golden_pack() {
    let out = temp_pack_path("golden");
    let status = Command::new(binary_path())
        .arg("make")
        .arg("--source")
        .arg("synthetic")
        .arg("--out")
        .arg(&out)
        .status()
        .expect("spawn slippypack binary");
    assert!(
        status.success(),
        "slippypack make exited non-zero: {status}"
    );

    let bytes = std::fs::read(&out).expect("read built pack");
    assert_matches_golden(&bytes, "golden-synthetic.rawtiles");

    // Cleanup.
    let _ = std::fs::remove_file(&out);
}

#[test]
fn cli_synthetic_pack_round_trips_through_reader() {
    let out = temp_pack_path("roundtrip");
    let status = Command::new(binary_path())
        .arg("make")
        .arg("--source")
        .arg("synthetic")
        .arg("--out")
        .arg(&out)
        .status()
        .expect("spawn slippypack binary");
    assert!(
        status.success(),
        "slippypack make exited non-zero: {status}"
    );

    let bytes = std::fs::read(&out).expect("read built pack");
    let reader = RawtilesReader::open(&bytes).expect("pack should parse");
    // 4×4 grid of tiles at zoom 2.
    assert_eq!(reader.tile_count(), 16);
    let header = reader.header();
    assert_eq!(header.derived.zoom_offsets[2].count, 16);
    // tile_dim_px = 16 per the synthetic fixture choice.
    assert_eq!(reader.metadata().tile_dim_px, 16);
    // Each tile has 16*16 = 256 bytes of ABGR2222 content.
    for entry in reader.tile_entries() {
        assert_eq!(
            entry.length, 256,
            "tile (z={}, x={}, y={})",
            entry.z, entry.x, entry.y
        );
    }

    let _ = std::fs::remove_file(&out);
}

#[test]
fn cli_leaves_no_partial_file_after_success() {
    let out = temp_pack_path("partial_cleanup");
    let partial = out.with_extension("upack.partial");
    let status = Command::new(binary_path())
        .arg("make")
        .arg("--source")
        .arg("synthetic")
        .arg("--out")
        .arg(&out)
        .status()
        .expect("spawn slippypack binary");
    assert!(status.success());
    assert!(out.exists(), "final pack should exist");
    assert!(
        !partial.exists(),
        "no .partial should be left behind after success: {}",
        partial.display(),
    );
    let _ = std::fs::remove_file(&out);
}

#[test]
fn cli_unknown_source_kind_exits_non_zero() {
    let out = temp_pack_path("unknown_source");
    let status = Command::new(binary_path())
        .arg("make")
        .arg("--source")
        .arg("definitely-not-a-source")
        .arg("--out")
        .arg(&out)
        .status()
        .expect("spawn slippypack binary");
    assert!(!status.success(), "unknown source should exit non-zero");
    assert!(
        !out.exists(),
        "no output file should be created on failure: {}",
        out.display(),
    );
}

#[test]
fn cli_with_pack_uuid_override_uses_provided_uuid() {
    let out = temp_pack_path("uuid_override");
    let custom = "deadbeef-cafe-babe-1234-567890abcdef";
    let status = Command::new(binary_path())
        .arg("make")
        .arg("--source")
        .arg("synthetic")
        .arg("--out")
        .arg(&out)
        .arg("--pack-uuid")
        .arg(custom)
        .status()
        .expect("spawn slippypack binary");
    assert!(status.success());

    let bytes = std::fs::read(&out).expect("read built pack");
    let reader = RawtilesReader::open(&bytes).expect("parse");
    // Check the pack_uuid header bytes match the override.
    let expected = [
        0xDE, 0xAD, 0xBE, 0xEF, 0xCA, 0xFE, 0xBA, 0xBE, 0x12, 0x34, 0x56, 0x78, 0x90, 0xAB, 0xCD,
        0xEF,
    ];
    assert_eq!(reader.metadata().pack_uuid, expected);
    let _ = std::fs::remove_file(&out);
}

#[test]
fn cli_with_invalid_pack_uuid_exits_non_zero() {
    let out = temp_pack_path("invalid_uuid");
    let status = Command::new(binary_path())
        .arg("make")
        .arg("--source")
        .arg("synthetic")
        .arg("--out")
        .arg(&out)
        .arg("--pack-uuid")
        .arg("not-a-valid-uuid")
        .status()
        .expect("spawn slippypack binary");
    assert!(!status.success());
    assert!(!out.exists());
}

#[test]
fn cli_with_zero_pack_uuid_exits_non_zero() {
    // Spec invariant: pack_uuid must be non-zero. The CLI rejects.
    let out = temp_pack_path("zero_uuid");
    let status = Command::new(binary_path())
        .arg("make")
        .arg("--source")
        .arg("synthetic")
        .arg("--out")
        .arg(&out)
        .arg("--pack-uuid")
        .arg("00000000-0000-0000-0000-000000000000")
        .status()
        .expect("spawn slippypack binary");
    assert!(!status.success());
    assert!(!out.exists());
}

#[test]
fn cli_attribution_flag_emits_attr_section() {
    use slippypack_core::format::TAG_ATTR;
    let out = temp_pack_path("attribution_flag");
    let status = Command::new(binary_path())
        .arg("make")
        .arg("--source")
        .arg("synthetic")
        .arg("--out")
        .arg(&out)
        .arg("--attribution")
        .arg("\u{00a9} OpenStreetMap contributors")
        .status()
        .expect("spawn slippypack binary");
    assert!(status.success(), "exit status: {status}");

    let bytes = std::fs::read(&out).expect("read built pack");
    let reader = RawtilesReader::open(&bytes).expect("pack should parse");
    let attr = reader
        .extensions()
        .iter()
        .find(|e| e.tag == TAG_ATTR)
        .expect("ATTR section should be present");
    assert_eq!(attr.payload, "\u{00a9} OpenStreetMap contributors".as_bytes());
    let _ = std::fs::remove_file(&out);
}

#[test]
fn cli_attribution_with_lf_exits_non_zero() {
    // Spec § 7.3 forbids LF inside a single-source attribution string;
    // clap rejects it at parse time via the value_parser, so no output
    // is written.
    let out = temp_pack_path("attribution_lf");
    let status = Command::new(binary_path())
        .arg("make")
        .arg("--source")
        .arg("synthetic")
        .arg("--out")
        .arg(&out)
        .arg("--attribution")
        .arg("\u{00a9} OSM\n\u{00a9} SRTM")
        .status()
        .expect("spawn slippypack binary");
    assert!(!status.success(), "LF in --attribution should be rejected");
    assert!(!out.exists());
}

#[test]
fn cli_attribution_empty_exits_non_zero() {
    // Empty string is invalid per § 7.3 (zero-length payload); user
    // should omit --attribution instead.
    let out = temp_pack_path("attribution_empty");
    let status = Command::new(binary_path())
        .arg("make")
        .arg("--source")
        .arg("synthetic")
        .arg("--out")
        .arg(&out)
        .arg("--attribution")
        .arg("")
        .status()
        .expect("spawn slippypack binary");
    assert!(!status.success());
    assert!(!out.exists());
}

#[test]
fn cli_two_invocations_produce_byte_identical_packs() {
    // Determinism property: same args + same fixture → identical bytes.
    let out_a = temp_pack_path("determinism_a");
    let out_b = temp_pack_path("determinism_b");
    for out in [&out_a, &out_b] {
        let status = Command::new(binary_path())
            .arg("make")
            .arg("--source")
            .arg("synthetic")
            .arg("--out")
            .arg(out)
            .status()
            .expect("spawn slippypack binary");
        assert!(status.success());
    }
    let a = std::fs::read(&out_a).unwrap();
    let b = std::fs::read(&out_b).unwrap();
    assert_eq!(
        a, b,
        "two builds with identical args produced different bytes"
    );
    let _ = std::fs::remove_file(&out_a);
    let _ = std::fs::remove_file(&out_b);
}
