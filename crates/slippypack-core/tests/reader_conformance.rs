//! Reader-side conformance corpus.
//!
//! For each committed golden `.rawtiles` pack, this test:
//!
//! 1. Opens the pack via `RawtilesReader`.
//! 2. Iterates every tile-index entry's `(z, x, y)`.
//! 3. Calls `reader.tile_bytes(z, x, y)` to get the bytes the reader's
//!    binary-search lookup returns.
//! 4. Computes SHA-256 of those bytes.
//! 5. Compares to a pinned per-pack `<pack>.hashes` file with one line
//!    per tile: `<z> <x> <y> <sha256-hex>`.
//!
//! What this catches that the existing `spec_layout` byte-equality
//! tests can't: a reader that opens a golden pack but returns bytes
//! for the **wrong** tile (off-by-one in binary search, wrong-zoom
//! lookup, mis-extracted index entry, …) would pass the writer-side
//! golden tests trivially. This test exercises the lookup path
//! end-to-end.
//!
//! Third-party readers can use this same corpus: parse the `.rawtiles`
//! file, parse the `.hashes` file, and verify each `tile_bytes(z, x, y)`
//! result hashes to the committed value.
//!
//! ## Bootstrap / re-bless
//!
//! ```sh
//! BLESS_READER_CONFORMANCE=1 cargo test --test reader_conformance
//! ```
//!
//! Rewrites every `.hashes` file from the actual reader output. Re-run
//! whenever a golden pack is intentionally re-blessed.

use core::fmt::Write as _;
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};
use slippypack_core::format::RawtilesReader;

fn fixtures_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
}

/// Iterate the pack's tile entries, hash each tile's bytes via the
/// reader's lookup, and return the table as a string (one tile per
/// line). The output is stable: tile entries are emitted in the order
/// `tile_entries()` yields them, which the reader guarantees is
/// ascending by `(z, x, y)`.
fn build_hash_table(pack_path: &Path) -> String {
    let bytes =
        std::fs::read(pack_path).unwrap_or_else(|e| panic!("read {} ({e})", pack_path.display()));
    let reader = RawtilesReader::open(&bytes)
        .unwrap_or_else(|e| panic!("open {} ({e:?})", pack_path.display()));

    let mut out = String::new();
    out.push_str("# rawtiles reader-conformance hashes for ");
    out.push_str(pack_path.file_name().unwrap().to_str().unwrap());
    out.push_str("\n# format: <z> <x> <y> <sha256-hex>\n");

    let entries: Vec<_> = reader.tile_entries().map(|e| (e.z, e.x, e.y)).collect();
    for (z, x, y) in entries {
        let tile = reader.tile_bytes(z, x, y).unwrap_or_else(|| {
            panic!(
                "reader missing tile ({z}, {x}, {y}) in {}",
                pack_path.display()
            )
        });
        let digest = Sha256::digest(tile);
        write!(&mut out, "{z} {x} {y} ").unwrap();
        for b in &digest {
            write!(&mut out, "{b:02x}").unwrap();
        }
        out.push('\n');
    }
    out
}

fn check_or_bless(pack_path: &Path, hashes_path: &Path) {
    let actual = build_hash_table(pack_path);
    if std::env::var("BLESS_READER_CONFORMANCE").is_ok() {
        std::fs::write(hashes_path, &actual)
            .unwrap_or_else(|e| panic!("write {} ({e})", hashes_path.display()));
        eprintln!(
            "BLESS_READER_CONFORMANCE: wrote {} bytes to {}",
            actual.len(),
            hashes_path.display(),
        );
        return;
    }
    let expected = std::fs::read_to_string(hashes_path).unwrap_or_else(|err| {
        panic!(
            "could not read {} ({err}) — run with \
             BLESS_READER_CONFORMANCE=1 to bootstrap it",
            hashes_path.display(),
        )
    });
    assert_eq!(
        actual,
        expected,
        "reader-conformance hashes drifted for {}",
        pack_path.file_name().unwrap().to_string_lossy(),
    );
}

#[test]
fn golden_grid_reader_returns_expected_tile_bytes() {
    let pack = fixtures_root().join("format").join("golden-grid.rawtiles");
    let hashes = fixtures_root().join("format").join("golden-grid.hashes");
    check_or_bless(&pack, &hashes);
}

#[test]
fn golden_pyramid_reader_returns_expected_tile_bytes() {
    let pack = fixtures_root()
        .join("format")
        .join("golden-pyramid.rawtiles");
    let hashes = fixtures_root().join("format").join("golden-pyramid.hashes");
    check_or_bless(&pack, &hashes);
}

#[test]
fn golden_attr_reader_returns_expected_tile_bytes() {
    let pack = fixtures_root().join("format").join("golden-attr.rawtiles");
    let hashes = fixtures_root().join("format").join("golden-attr.hashes");
    check_or_bless(&pack, &hashes);
}

#[test]
fn golden_png_to_pack_1tile_reader_returns_expected_tile_bytes() {
    let pack = fixtures_root()
        .join("e2e")
        .join("golden-png-to-pack-1tile.rawtiles");
    let hashes = fixtures_root()
        .join("e2e")
        .join("golden-png-to-pack-1tile.hashes");
    check_or_bless(&pack, &hashes);
}

#[test]
fn golden_png_to_pack_5tiles_reader_returns_expected_tile_bytes() {
    let pack = fixtures_root()
        .join("e2e")
        .join("golden-png-to-pack-5tiles.rawtiles");
    let hashes = fixtures_root()
        .join("e2e")
        .join("golden-png-to-pack-5tiles.hashes");
    check_or_bless(&pack, &hashes);
}
