//! The `--source synthetic` built-in fixture.
//!
//! 16 distinct-hue PNG tiles arranged in a 4×4 grid at zoom 2, embedded
//! into the CLI binary via `include_bytes!`. No network, no API key —
//! lets developers and tyre-kickers verify the decode → quantise →
//! format pipeline end-to-end without setting up a tile source.
//!
//! Per PLAN.md § Source-kind details:
//!
//! > Synthetic — `--source synthetic`. Builds a tiny pack from a
//! > committed gradient-pattern fixture (no network, no key, no
//! > real-world tiles). The fixture bytes are `include_bytes!`-embedded
//! > into the CLI binary at compile time, so `cargo install`-installed
//! > binaries work without the source repo present.

/// Dimension of each synthetic tile in pixels. Matches the `tile_dim_px`
/// header field the CLI sets for `--source synthetic` packs.
///
/// Note: this is **not** the spec-mandated `128` — synthetic packs are
/// for pipeline validation, not for watch-loadability (the watch's
/// reader will refuse `tile_dim_px != 128` per the una-sdk spec).
pub const TILE_DIM_PX: u16 = 16;

/// Side length of the synthetic 4×4 tile grid.
pub const GRID_SIDE: u32 = 4;

/// Zoom level at which the synthetic fixture is placed. z=2 means the
/// full 16-tile grid covers the entire `2^2 × 2^2` = 4×4 tile pyramid,
/// which is the smallest zoom that fits the 4×4 fixture exactly.
pub const ZOOM: u8 = 2;

/// Raw PNG bytes for the tile at `(x, y)` in the 4×4 grid. Returns
/// `None` for out-of-range coordinates (`x >= 4` or `y >= 4`).
///
/// Tiles are addressed in slippy-map convention: `x` is column (left to
/// right), `y` is row (top to bottom).
#[must_use]
#[allow(
    clippy::match_same_arms,
    reason = "each arm's `include_bytes!` references a different file; \
              the macro expansions produce distinct byte arrays even though \
              the surface syntax looks identical"
)]
pub fn tile_png_bytes(x: u32, y: u32) -> Option<&'static [u8]> {
    match (x, y) {
        (0, 0) => Some(include_bytes!(
            "../../fixtures/synthetic-pattern/tile-0-0.png"
        )),
        (0, 1) => Some(include_bytes!(
            "../../fixtures/synthetic-pattern/tile-0-1.png"
        )),
        (0, 2) => Some(include_bytes!(
            "../../fixtures/synthetic-pattern/tile-0-2.png"
        )),
        (0, 3) => Some(include_bytes!(
            "../../fixtures/synthetic-pattern/tile-0-3.png"
        )),
        (1, 0) => Some(include_bytes!(
            "../../fixtures/synthetic-pattern/tile-1-0.png"
        )),
        (1, 1) => Some(include_bytes!(
            "../../fixtures/synthetic-pattern/tile-1-1.png"
        )),
        (1, 2) => Some(include_bytes!(
            "../../fixtures/synthetic-pattern/tile-1-2.png"
        )),
        (1, 3) => Some(include_bytes!(
            "../../fixtures/synthetic-pattern/tile-1-3.png"
        )),
        (2, 0) => Some(include_bytes!(
            "../../fixtures/synthetic-pattern/tile-2-0.png"
        )),
        (2, 1) => Some(include_bytes!(
            "../../fixtures/synthetic-pattern/tile-2-1.png"
        )),
        (2, 2) => Some(include_bytes!(
            "../../fixtures/synthetic-pattern/tile-2-2.png"
        )),
        (2, 3) => Some(include_bytes!(
            "../../fixtures/synthetic-pattern/tile-2-3.png"
        )),
        (3, 0) => Some(include_bytes!(
            "../../fixtures/synthetic-pattern/tile-3-0.png"
        )),
        (3, 1) => Some(include_bytes!(
            "../../fixtures/synthetic-pattern/tile-3-1.png"
        )),
        (3, 2) => Some(include_bytes!(
            "../../fixtures/synthetic-pattern/tile-3-2.png"
        )),
        (3, 3) => Some(include_bytes!(
            "../../fixtures/synthetic-pattern/tile-3-3.png"
        )),
        _ => None,
    }
}

/// Iterate over all `(x, y)` coordinates that have a tile in the
/// synthetic fixture (the 4×4 grid at zoom 2).
pub fn all_tile_coords() -> impl Iterator<Item = (u32, u32)> {
    (0..GRID_SIDE).flat_map(|y| (0..GRID_SIDE).map(move |x| (x, y)))
}

#[cfg(test)]
mod tests {
    use super::{GRID_SIDE, TILE_DIM_PX, ZOOM, all_tile_coords, tile_png_bytes};

    #[test]
    fn every_grid_position_has_a_fixture() {
        for y in 0..GRID_SIDE {
            for x in 0..GRID_SIDE {
                let bytes = tile_png_bytes(x, y)
                    .unwrap_or_else(|| panic!("missing fixture for ({x}, {y})"));
                assert!(!bytes.is_empty(), "fixture for ({x}, {y}) is empty");
                // PNG signature check.
                assert_eq!(
                    &bytes[..8],
                    b"\x89PNG\r\n\x1a\n",
                    "fixture for ({x}, {y}) is not a PNG",
                );
            }
        }
    }

    #[test]
    fn out_of_range_coords_return_none() {
        assert!(tile_png_bytes(4, 0).is_none());
        assert!(tile_png_bytes(0, 4).is_none());
        assert!(tile_png_bytes(99, 99).is_none());
    }

    #[test]
    fn all_tile_coords_yields_16_unique_pairs() {
        let coords: Vec<_> = all_tile_coords().collect();
        assert_eq!(coords.len(), 16);
        // Verify uniqueness via dedup-equality.
        let mut sorted = coords.clone();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(sorted.len(), 16);
        // Verify range.
        for (x, y) in coords {
            assert!(x < GRID_SIDE);
            assert!(y < GRID_SIDE);
        }
    }

    #[test]
    fn fixture_constants_are_sane() {
        assert_eq!(GRID_SIDE, 4);
        assert_eq!(TILE_DIM_PX, 16);
        assert_eq!(ZOOM, 2);
    }

    #[test]
    fn distinct_grid_positions_have_distinct_pngs() {
        // The fixture is meant to be a "gradient pattern" — each tile
        // should have distinct content. Two random positions are checked
        // to be byte-different.
        let a = tile_png_bytes(0, 0).unwrap();
        let b = tile_png_bytes(3, 3).unwrap();
        assert_ne!(a, b, "tile (0,0) and tile (3,3) should have distinct bytes");
    }
}
