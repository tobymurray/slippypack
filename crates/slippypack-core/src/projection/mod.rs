//! Geographic projections: lat/lon ↔ slippy-map tile coordinates.
//!
//! V1 has one implementation: [`Mercator`] for quadtree-addressed packs,
//! which is the bulk of OSM-derived workflows. The Local Linear projection
//! for single-image hand-drawn packs (PLAN.md § Phase 10) lands later in
//! Phase 0 once the affine-matrix shape is wired through the `format`
//! module.

pub mod mercator;

pub use mercator::Mercator;

/// Maps geographic coordinates (lon, lat in decimal degrees) to and from
/// slippy-map tile coordinates `(x, y)` at a given integer zoom level.
///
/// All v1 implementations:
/// - Use the XYZ slippy-map convention (Y increases southward, origin
///   at the top-left). Packs with TMS-axis tiles handle the Y-flip
///   elsewhere — see PLAN.md § The CLI for the build-time rule.
/// - Clamp out-of-range inputs to the nearest valid value rather than
///   panicking. Out-of-range coordinates aren't a usage error; they
///   reflect bbox edges that extend past the projection's coverage.
pub trait Projection {
    /// Convert geographic `(lon, lat)` in decimal degrees to the slippy-map
    /// tile `(x, y)` containing that point at `zoom`. Result is in
    /// `[0, 2^zoom - 1] × [0, 2^zoom - 1]`.
    fn lonlat_to_tile(&self, lon: f64, lat: f64, zoom: u8) -> (u32, u32);

    /// Convert slippy-map tile `(x, y)` at `zoom` to the geographic
    /// `(lon, lat)` of the tile's **north-west corner**, in decimal degrees.
    fn tile_to_lonlat(&self, x: u32, y: u32, zoom: u8) -> (f64, f64);
}
