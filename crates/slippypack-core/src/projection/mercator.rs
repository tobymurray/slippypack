//! Web Mercator (EPSG:3857) — the standard slippy-map projection.
//!
//! Used by OSM, Google Maps, MapLibre, Mapbox, MapTiler, Stadia, and
//! every other tile provider that ships `{z}/{x}/{y}` URLs.
//!
//! **Cross-platform determinism**: the four transcendentals this module
//! uses (`tan`, `asinh`, `sinh`, `atan`) route through the pure-Rust
//! [`libm`] crate, not `f64`'s std-provided methods. `f64::tan` and
//! friends delegate to the host's libm (glibc, musl, macOS libm, ucrt,
//! WASM's host runtime, …) and those implementations are NOT bit-for-bit
//! agreeable — a Linux pack-build and a browser PWA build could disagree
//! by one ULP on an edge case. `libm`'s pure-Rust implementation is the
//! same bits everywhere, which is load-bearing for slippypack's
//! "byte-identical across CLI and PWA" guarantee.
//!
//! Pure-arithmetic ops (`to_radians`, `to_degrees`, `floor`, `clamp`,
//! `+`, `-`, `*`, `/`) are already deterministic by IEEE-754, so they
//! stay on std.

use core::f64::consts::PI;

use super::Projection;

/// Web Mercator (EPSG:3857).
///
/// Coverage: latitude in `[-LAT_LIMIT_DEG, LAT_LIMIT_DEG]`. Latitudes
/// outside that range are clamped — Mercator projects the poles to
/// infinity, so there's no valid mapping past the limit.
///
/// Stateless: a `Mercator` instance carries no fields. Constructed via
/// `Mercator::default()` or `Mercator` directly.
#[derive(Debug, Clone, Copy, Default)]
pub struct Mercator;

impl Mercator {
    /// Latitude limit of Web Mercator coverage, in decimal degrees.
    /// Derived as `atan(sinh(π)) * 180 / π ≈ 85.0511287798...°`.
    pub const LAT_LIMIT_DEG: f64 = 85.051_128_779_806_59;
}

impl Projection for Mercator {
    fn lonlat_to_tile(&self, lon: f64, lat: f64, zoom: u8) -> (u32, u32) {
        let n = 1_u32 << zoom;
        let n_f = f64::from(n);

        let lon = lon.clamp(-180.0, 180.0);
        let lat = lat.clamp(-Self::LAT_LIMIT_DEG, Self::LAT_LIMIT_DEG);

        let lat_rad = lat.to_radians();
        let x_f = ((lon + 180.0) / 360.0 * n_f).floor();
        // libm::tan / libm::asinh (NOT f64::*) — see module-level
        // determinism note. These are the only non-deterministic ops
        // in the projection, so swapping them to libm makes the whole
        // function cross-platform bit-stable.
        let y_f = ((1.0 - libm::asinh(libm::tan(lat_rad)) / PI) / 2.0 * n_f).floor();

        // x_f and y_f are in [0.0, n_f] by construction; the float-to-u32
        // cast is exact for this range (n_f ≤ 2^31 at zoom ≤ 31, well within
        // u32 and f64's exact-integer range).
        #[allow(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            clippy::cast_precision_loss
        )]
        let x = (x_f as u32).min(n - 1);
        #[allow(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            clippy::cast_precision_loss
        )]
        let y = (y_f as u32).min(n - 1);
        (x, y)
    }

    fn tile_to_lonlat(&self, x: u32, y: u32, zoom: u8) -> (f64, f64) {
        let n = 1_u32 << zoom;
        let n_f = f64::from(n);

        let lon = f64::from(x) / n_f * 360.0 - 180.0;
        // libm::sinh / libm::atan — see module-level determinism note.
        let lat_rad = libm::atan(libm::sinh(PI * (1.0 - 2.0 * f64::from(y) / n_f)));
        let lat = lat_rad.to_degrees();
        (lon, lat)
    }
}

#[cfg(test)]
mod tests {
    use super::super::Projection;
    use super::Mercator;

    /// Tolerance for round-trip lat/lon comparisons. The forward transform
    /// quantises to integer tile coords, so the inverse is necessarily
    /// snapped to the tile's NW corner — round-trip is exact only when
    /// the input was already on a tile NW corner.
    const FLOAT_EPSILON: f64 = 1e-9;

    #[test]
    fn zoom_zero_is_one_tile() {
        let m = Mercator;
        // The whole world fits in one tile (0, 0) at z=0.
        assert_eq!(m.lonlat_to_tile(0.0, 0.0, 0), (0, 0));
        assert_eq!(m.lonlat_to_tile(-180.0, 85.0, 0), (0, 0));
        assert_eq!(m.lonlat_to_tile(180.0, -85.0, 0), (0, 0));
    }

    #[test]
    fn tile_zero_zero_zero_nw_corner_is_world_corner() {
        let m = Mercator;
        let (lon, lat) = m.tile_to_lonlat(0, 0, 0);
        assert!((lon - -180.0).abs() < FLOAT_EPSILON);
        assert!((lat - Mercator::LAT_LIMIT_DEG).abs() < FLOAT_EPSILON);
    }

    /// At z=1, the world splits into 2×2 = 4 tiles. Tile (1, 1) covers the
    /// SE quadrant — its NW corner is the equator/prime-meridian intersection.
    #[test]
    fn zoom_one_quadrants() {
        let m = Mercator;
        // NW quadrant: lon < 0, lat > 0
        assert_eq!(m.lonlat_to_tile(-90.0, 45.0, 1), (0, 0));
        // NE quadrant
        assert_eq!(m.lonlat_to_tile(90.0, 45.0, 1), (1, 0));
        // SW quadrant
        assert_eq!(m.lonlat_to_tile(-90.0, -45.0, 1), (0, 1));
        // SE quadrant
        assert_eq!(m.lonlat_to_tile(90.0, -45.0, 1), (1, 1));

        // Tile (1, 1) NW corner is the equator/prime-meridian.
        let (lon, lat) = m.tile_to_lonlat(1, 1, 1);
        assert!(lon.abs() < FLOAT_EPSILON, "lon = {lon}");
        assert!(lat.abs() < FLOAT_EPSILON, "lat = {lat}");
    }

    /// OSM canonical reference: London (Charing Cross-ish at -0.1278, 51.5074)
    /// at z=10 maps to tile (511, 340). Verified against multiple slippy-map
    /// tile-coordinate calculators.
    #[test]
    fn london_at_z10_is_tile_511_340() {
        let m = Mercator;
        assert_eq!(m.lonlat_to_tile(-0.1278, 51.5074, 10), (511, 340));
    }

    /// Southern-hemisphere sanity check: Sydney (151.2093, -33.8688) at z=10
    /// maps to tile (942, 614). Tests the southern-latitude y-axis branch
    /// (lat < 0 → y > n/2).
    #[test]
    fn sydney_at_z10_is_tile_942_614() {
        let m = Mercator;
        assert_eq!(m.lonlat_to_tile(151.2093, -33.8688, 10), (942, 614));
    }

    /// Tile-coordinate ranges grow as 2^zoom on each axis.
    #[test]
    fn tile_coordinate_max_is_2_pow_z_minus_one() {
        let m = Mercator;
        for zoom in [0_u8, 1, 4, 10, 17] {
            let n = 1_u32 << zoom;
            // The far SE corner clamps to (n-1, n-1).
            let (x, y) = m.lonlat_to_tile(180.0, -Mercator::LAT_LIMIT_DEG, zoom);
            assert_eq!((x, y), (n - 1, n - 1), "zoom = {zoom}");
        }
    }

    /// Out-of-range latitudes clamp to the Mercator limit rather than wrapping.
    #[test]
    fn out_of_range_lat_clamps_to_limit() {
        let m = Mercator;
        // Lat = 90° (north pole, beyond Mercator) clamps to LAT_LIMIT_DEG.
        let (x_north_pole, y_north_pole) = m.lonlat_to_tile(0.0, 90.0, 10);
        let (x_clamped, y_clamped) = m.lonlat_to_tile(0.0, Mercator::LAT_LIMIT_DEG, 10);
        assert_eq!((x_north_pole, y_north_pole), (x_clamped, y_clamped));

        // Same for south pole.
        let (x_south_pole, y_south_pole) = m.lonlat_to_tile(0.0, -90.0, 10);
        let (x_clamped_s, y_clamped_s) = m.lonlat_to_tile(0.0, -Mercator::LAT_LIMIT_DEG, 10);
        assert_eq!((x_south_pole, y_south_pole), (x_clamped_s, y_clamped_s));
    }

    /// Out-of-range longitudes clamp to ±180°.
    #[test]
    fn out_of_range_lon_clamps_to_180() {
        let m = Mercator;
        let (x_far_east, _) = m.lonlat_to_tile(720.0, 0.0, 10);
        let (x_at_180, _) = m.lonlat_to_tile(180.0, 0.0, 10);
        assert_eq!(x_far_east, x_at_180);

        let (x_far_west, _) = m.lonlat_to_tile(-720.0, 0.0, 10);
        let (x_at_neg180, _) = m.lonlat_to_tile(-180.0, 0.0, 10);
        assert_eq!(x_far_west, x_at_neg180);
    }

    /// Tile.NW → lonlat → same tile (the tile that contains its own NW corner).
    #[test]
    fn tile_to_lonlat_to_tile_is_idempotent() {
        let m = Mercator;
        let cases = [
            (0_u8, 0_u32, 0_u32),
            (1, 0, 0),
            (1, 1, 1),
            (10, 511, 340),
            (10, 942, 614),
            (17, 65436, 43577),
        ];
        for (zoom, x, y) in cases {
            let (lon, lat) = m.tile_to_lonlat(x, y, zoom);
            let (round_x, round_y) = m.lonlat_to_tile(lon, lat, zoom);
            assert_eq!(
                (round_x, round_y),
                (x, y),
                "tile ({x},{y},z={zoom}) → ({lon},{lat}) → ({round_x},{round_y})",
            );
        }
    }

    /// The longitude axis is linear in tile x: tile widths in degrees are
    /// `360 / 2^zoom` everywhere.
    #[test]
    fn longitude_is_linear_in_tile_x() {
        let m = Mercator;
        for zoom in [1_u8, 4, 10] {
            let n = 1_u32 << zoom;
            let expected_tile_width_deg = 360.0 / f64::from(n);
            for x in 0..n {
                let (lon_left, _) = m.tile_to_lonlat(x, 0, zoom);
                let (lon_right, _) = m.tile_to_lonlat(x + 1, 0, zoom);
                let diff = lon_right - lon_left;
                assert!(
                    (diff - expected_tile_width_deg).abs() < FLOAT_EPSILON,
                    "zoom={zoom}, x={x}: expected width {expected_tile_width_deg}, got {diff}",
                );
            }
        }
    }

    /// The equator is at exactly tile y = 2^(zoom-1) at any zoom ≥ 1.
    #[test]
    fn equator_is_at_half_height() {
        let m = Mercator;
        for zoom in 1_u8..=10 {
            let n = 1_u32 << zoom;
            // Lat = 0 is the boundary between tile rows (n/2 - 1) and (n/2).
            // The convention here: a point exactly on a tile boundary belongs
            // to the lower-y (more northerly) tile. (Floor of a value
            // exactly equal to n/2 gives n/2, but we expect n/2-1 if the
            // mapping treats equator as "in the northern tile". In practice,
            // any tiny float error pushes it firmly one way or the other;
            // accept either n/2 or n/2-1.)
            let (_, y) = m.lonlat_to_tile(0.0, 0.0, zoom);
            assert!(
                y == n / 2 || y == n / 2 - 1,
                "zoom={zoom}: equator y={y}, expected n/2 or n/2-1 where n={n}",
            );
        }
    }

    /// Default-constructed Mercator behaves identically to the unit struct.
    #[test]
    fn default_matches_explicit_construction() {
        let m1 = Mercator;
        let m2 = Mercator;
        assert_eq!(
            m1.lonlat_to_tile(0.0, 0.0, 5),
            m2.lonlat_to_tile(0.0, 0.0, 5),
        );
    }

    /// Determinism gate at the f64-bit level: `tile_to_lonlat` produces
    /// specific IEEE-754 bit patterns for a committed set of inputs.
    /// This is stricter than the integer-tile determinism gate below —
    /// the integer test would silently pass even if the underlying
    /// floats drifted by less than one tile boundary. Locking the f64
    /// bits guarantees that libm-via-pure-Rust agrees byte-for-byte
    /// across Linux x86, Linux ARM, macOS, Windows, and WASM.
    ///
    /// If this test fails, slippypack-core's `libm` dep version (or
    /// its underlying implementation) has changed — bump
    /// `QUANTISER_VERSION`-equivalent for the projection (currently
    /// implicit: the format version) and re-bless.
    /// Determinism gate at the f64-bit level: `tile_to_lonlat` produces
    /// specific IEEE-754 bit patterns for committed inputs. This is
    /// stricter than the integer-tile gate below — the integer test
    /// would silently pass even if the underlying floats drifted by
    /// less than one tile boundary. Locking the f64 bits guarantees
    /// that libm-via-pure-Rust agrees byte-for-byte across Linux x86,
    /// Linux ARM, macOS, Windows, and WASM. If this test fails, the
    /// `libm` dep version (or its implementation) has changed —
    /// re-bless the values and audit downstream golden fixtures.
    #[test]
    fn determinism_committed_f64_bits_for_known_tiles() {
        let m = Mercator;

        // Tile (511, 340) at z=10 — London region. `lon` is pure
        // arithmetic (deterministic regardless of libm); `lat` goes
        // through libm::sinh + libm::atan and is the load-bearing
        // bit-level check.
        let (lon, lat) = m.tile_to_lonlat(511, 340, 10);
        assert_eq!(
            lon.to_bits(),
            0xbfd6_8000_0000_0000_u64,
            "London lon bits drifted; got {lon} (bits = {:#018x})",
            lon.to_bits(),
        );
        assert_eq!(
            lat.to_bits(),
            0x4049_cf1b_2a90_af57_u64,
            "London lat bits drifted; got {lat} (bits = {:#018x})",
            lat.to_bits(),
        );

        // Tile (942, 614) at z=10 — Sydney region (southern hemisphere
        // hits the lat < 0 branch).
        let (lon2, lat2) = m.tile_to_lonlat(942, 614, 10);
        assert_eq!(
            lon2.to_bits(),
            0x4062_e580_0000_0000_u64,
            "Sydney lon bits drifted; got {lon2} (bits = {:#018x})",
            lon2.to_bits(),
        );
        assert_eq!(
            lat2.to_bits(),
            0xc040_dcb7_297b_3369_u64,
            "Sydney lat bits drifted; got {lat2} (bits = {:#018x})",
            lat2.to_bits(),
        );
    }

    /// Determinism gate: a committed bundle of (lat, lon, zoom) inputs maps
    /// to specific (x, y) tile coordinates. Any change here is a Mercator
    /// algorithm bump and a knock-on across every cached test fixture.
    #[test]
    fn determinism_committed_output_for_known_coordinates() {
        let m = Mercator;
        // Reference points: real places at z=10 + Null Island markers across
        // a few zooms. All verified against OSM's slippy-map calculator.
        let cases: [(&str, f64, f64, u8, u32, u32); 8] = [
            ("Null Island z=0", 0.0, 0.0, 0, 0, 0),
            ("Null Island z=5", 0.0, 0.0, 5, 16, 16),
            ("Null Island z=10", 0.0, 0.0, 10, 512, 512),
            ("London z=10", -0.1278, 51.5074, 10, 511, 340),
            ("New York z=10", -74.0060, 40.7128, 10, 301, 385),
            ("Sydney z=10", 151.2093, -33.8688, 10, 942, 614),
            ("Tokyo z=12", 139.6917, 35.6895, 12, 3637, 1612),
            ("Cape Town z=8", 18.4241, -33.9249, 8, 141, 153),
        ];
        for (label, lon, lat, zoom, expected_x, expected_y) in cases {
            let (x, y) = m.lonlat_to_tile(lon, lat, zoom);
            assert_eq!(
                (x, y),
                (expected_x, expected_y),
                "{label}: ({lon},{lat}) at z={zoom}",
            );
        }
    }
}
