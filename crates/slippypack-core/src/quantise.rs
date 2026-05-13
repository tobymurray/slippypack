//! ABGR2222 quantiser. Converts RGB888 pixels to ABGR2222 bytes using
//! integer-only arithmetic so the output is byte-identical across
//! architectures (no float semantics, no FMA, no platform-dependent
//! rounding).
//!
//! See `PLAN.md` § The load-bearing observation: shared Rust core for the
//! design context, and the una-sdk `.upack` spec's pixel-format enum for
//! the byte-level shape. Bumping [`QUANTISER_VERSION`] is required whenever
//! the quantiser's byte output changes for any input — the canonical
//! source descriptor carries the version so packs produced by different
//! quantisers get distinct `pack_uuid` values.

/// Bumped on any change to the quantiser's byte output. Carried in the
/// canonical source descriptor (per `PLAN.md` § Canonical source descriptor)
/// so that packs produced by different quantiser versions get distinct
/// `pack_uuid` values even when all other inputs match.
pub const QUANTISER_VERSION: u32 = 1;

/// Quantise a single RGB888 pixel to ABGR2222.
///
/// Each input channel is mapped to a 2-bit quantum via thresholds at the
/// midpoints between the four displayed levels `{0, 85, 170, 255}`:
///
/// | Input range  | Quantised | Displayed value |
/// |--------------|-----------|-----------------|
/// | `0..=42`     | `0`       | `0`             |
/// | `43..=127`   | `1`       | `85`            |
/// | `128..=212`  | `2`       | `170`           |
/// | `213..=255`  | `3`       | `255`           |
///
/// The output byte's bit layout is `AABBGGRR` from MSB to LSB, with alpha
/// always set to `3` (fully opaque) — slippypack always packs opaque map
/// tiles in v1.
///
/// Integer-only by construction: no floats, no FMA, no `f32` semantics
/// that vary across architectures. Const-fn so callers can quantise at
/// compile time when input is known statically.
#[inline]
#[must_use]
pub const fn quantise_pixel(r: u8, g: u8, b: u8) -> u8 {
    const fn channel_to_2bit(v: u8) -> u8 {
        if v <= 42 {
            0
        } else if v <= 127 {
            1
        } else if v <= 212 {
            2
        } else {
            3
        }
    }
    let r2 = channel_to_2bit(r);
    let g2 = channel_to_2bit(g);
    let b2 = channel_to_2bit(b);
    // ABGR2222 byte: A in bits 7-6, B in 5-4, G in 3-2, R in 1-0.
    // A = 3 (alpha bits both set) since v1 packs are opaque.
    0b1100_0000 | (b2 << 4) | (g2 << 2) | r2
}

/// Quantise a flat RGB888 buffer (3 bytes per pixel, R-G-B order) into a
/// flat ABGR2222 buffer (1 byte per pixel).
///
/// # Panics
///
/// - if `input.len()` is not a multiple of 3.
/// - if `output.len()` does not equal `input.len() / 3`.
///
/// The panics catch caller bugs at the boundary; higher layers
/// (e.g. the `format` module) validate at their own surface before
/// reaching this function.
pub fn quantise_rgb888(input: &[u8], output: &mut [u8]) {
    assert!(
        input.len().is_multiple_of(3),
        "RGB888 input length ({}) must be a multiple of 3",
        input.len(),
    );
    assert!(
        output.len() == input.len() / 3,
        "output length ({}) must equal input length / 3 ({})",
        output.len(),
        input.len() / 3,
    );
    for (out_byte, rgb) in output.iter_mut().zip(input.chunks_exact(3)) {
        *out_byte = quantise_pixel(rgb[0], rgb[1], rgb[2]);
    }
}

#[cfg(test)]
mod tests {
    use super::{QUANTISER_VERSION, quantise_pixel, quantise_rgb888};

    /// The four displayed levels of a 2-bit channel.
    const LEVELS: [u8; 4] = [0, 85, 170, 255];

    #[test]
    fn quantiser_version_starts_at_one() {
        assert_eq!(QUANTISER_VERSION, 1);
    }

    #[test]
    fn exact_levels_round_trip_to_their_bucket() {
        // The four displayed levels each quantise to their own bucket index.
        for (expected_bucket, &v) in LEVELS.iter().enumerate() {
            let q = quantise_pixel(v, v, v);
            let r_bucket = u8::try_from(expected_bucket).expect("0..=3 fits in u8");
            assert_eq!(
                q & 0b11,
                r_bucket,
                "input {v} should quantise R to bucket {expected_bucket}",
            );
        }
    }

    #[test]
    fn boundary_inputs_map_to_expected_buckets() {
        // Verify both sides of each midpoint boundary.
        // Midpoints fall between 42|43, 127|128, 212|213.
        let cases: [(u8, u8); 8] = [
            (0, 0),
            (42, 0),
            (43, 1),
            (127, 1),
            (128, 2),
            (212, 2),
            (213, 3),
            (255, 3),
        ];
        for (input, expected_bucket) in cases {
            let q = quantise_pixel(input, 0, 0);
            assert_eq!(
                q & 0b11,
                expected_bucket,
                "input {input} should quantise R to bucket {expected_bucket}",
            );
        }
    }

    #[test]
    fn alpha_is_always_fully_opaque() {
        // Every input pixel should produce alpha = 3 (top two bits set).
        // Sample across the whole u8 range at boundary-relevant points.
        for v in [0_u8, 1, 42, 43, 127, 128, 170, 200, 213, 255] {
            let q = quantise_pixel(v, v, v);
            assert_eq!(q >> 6, 0b11, "alpha bits should be 3 for input {v}");
        }
    }

    #[test]
    fn bit_layout_is_abgr_msb_to_lsb() {
        // Distinct levels per channel: R = bucket 0, G = bucket 1, B = bucket 2.
        let q = quantise_pixel(0, 85, 170);
        // Expected: A = 3 (bits 7-6), B = 2 (bits 5-4), G = 1 (bits 3-2), R = 0 (bits 1-0)
        // → 0b1110_0100 = 0xE4
        assert_eq!(q, 0b1110_0100, "byte = {q:#010b}, expected 0b1110_0100");
        // Independent extraction.
        assert_eq!(q >> 6, 3, "alpha");
        assert_eq!((q >> 4) & 0b11, 2, "blue");
        assert_eq!((q >> 2) & 0b11, 1, "green");
        assert_eq!(q & 0b11, 0, "red");
    }

    #[test]
    fn every_u8_input_produces_an_in_range_2bit_quantum() {
        // Exhaustively verify the per-channel range across all u8 inputs.
        // (Per-channel sweep, not the cubic-cardinality combined sweep.)
        for v in 0..=u8::MAX {
            let red = quantise_pixel(v, 0, 0) & 0b11;
            let green = (quantise_pixel(0, v, 0) >> 2) & 0b11;
            let blue = (quantise_pixel(0, 0, v) >> 4) & 0b11;
            assert!(red <= 3, "red quantum {red} out of range for input {v}");
            assert!(
                green <= 3,
                "green quantum {green} out of range for input {v}"
            );
            assert!(blue <= 3, "blue quantum {blue} out of range for input {v}");
        }
    }

    #[test]
    fn quantise_pixel_is_monotonic_per_channel() {
        // Within a single channel, increasing input never decreases the quantised
        // bucket — a sanity check that the function is well-formed.
        let mut prev_red = 0;
        let mut prev_green = 0;
        let mut prev_blue = 0;
        for v in 0..=u8::MAX {
            let red = quantise_pixel(v, 0, 0) & 0b11;
            let green = (quantise_pixel(0, v, 0) >> 2) & 0b11;
            let blue = (quantise_pixel(0, 0, v) >> 4) & 0b11;
            assert!(
                red >= prev_red,
                "red quantum dropped at v={v}: {prev_red} → {red}"
            );
            assert!(
                green >= prev_green,
                "green quantum dropped at v={v}: {prev_green} → {green}"
            );
            assert!(
                blue >= prev_blue,
                "blue quantum dropped at v={v}: {prev_blue} → {blue}"
            );
            prev_red = red;
            prev_green = green;
            prev_blue = blue;
        }
    }

    #[test]
    fn buffer_quantises_two_known_pixels() {
        // Pixel 0: (0,0,0) — pure black.
        // Pixel 1: (255,255,255) — pure white.
        let input: [u8; 6] = [0, 0, 0, 255, 255, 255];
        let mut output = [0_u8; 2];
        quantise_rgb888(&input, &mut output);
        // Black:  A=3, B=0, G=0, R=0 → 0b1100_0000 = 0xC0
        // White:  A=3, B=3, G=3, R=3 → 0b1111_1111 = 0xFF
        assert_eq!(output, [0xC0, 0xFF]);
    }

    #[test]
    #[should_panic(expected = "must be a multiple of 3")]
    fn buffer_rejects_non_multiple_of_3_input() {
        let input = [0_u8; 4];
        let mut output = [0_u8; 1];
        quantise_rgb888(&input, &mut output);
    }

    #[test]
    #[should_panic(expected = "must equal input length / 3")]
    fn buffer_rejects_output_size_mismatch() {
        let input = [0_u8; 6];
        let mut output = [0_u8; 1]; // wrong: should be 2
        quantise_rgb888(&input, &mut output);
    }

    #[test]
    fn quantise_pixel_is_usable_in_const_context() {
        const QUANTISED_BLACK: u8 = quantise_pixel(0, 0, 0);
        const QUANTISED_WHITE: u8 = quantise_pixel(255, 255, 255);
        assert_eq!(QUANTISED_BLACK, 0xC0);
        assert_eq!(QUANTISED_WHITE, 0xFF);
    }

    /// Determinism gate: the quantiser's output for a known 16-pixel input
    /// is locked to specific bytes. Any change here is a `QUANTISER_VERSION`
    /// bump (and a knock-on change to every committed golden-pack hex in
    /// the test fixtures, per PLAN.md § Phase 11).
    ///
    /// The CI matrix runs this test on both `x86_64` and `aarch64` (Linux,
    /// macOS, Windows); byte-identical results across all three OSes and
    /// both architectures is the load-bearing cross-platform determinism
    /// claim.
    #[test]
    fn determinism_committed_output_for_known_input() {
        // 16 sample pixels (a 4×4 RGB pattern). Each row exercises a
        // different region of the input space:
        //   Row 0: pure R, G, B, white
        //   Row 1: half-intensity primaries + 50% grey
        //   Row 2: per-channel greys at quantiser boundaries (42/43, 85, 127)
        //   Row 3: per-channel greys above the high boundary (170, 212, 213) + a
        //          three-distinct-channel pixel
        let input: [u8; 48] = [
            // Row 0
            255, 0, 0, 0, 255, 0, 0, 0, 255, 255, 255, 255, // Row 1
            128, 0, 0, 0, 128, 0, 0, 0, 128, 128, 128, 128, // Row 2
            42, 42, 42, 43, 43, 43, 85, 85, 85, 127, 127, 127, // Row 3
            170, 170, 170, 212, 212, 212, 213, 213, 213, 255, 128, 0,
        ];
        let mut output = [0_u8; 16];
        quantise_rgb888(&input, &mut output);

        // Expected output, computed by hand and committed. Any divergence
        // means the quantiser changed and QUANTISER_VERSION must bump
        // (alongside regeneration of every committed golden-pack hex).
        let expected: [u8; 16] = [
            // Row 0: pure R (3,0,0), pure G (0,3,0), pure B (0,0,3), white (3,3,3)
            0b1100_0011,
            0b1100_1100,
            0b1111_0000,
            0b1111_1111,
            // Row 1: 128 → bucket 2 in any channel.
            // (128,0,0) → A=3,B=0,G=0,R=2 = 0b1100_0010
            // (0,128,0) → A=3,B=0,G=2,R=0 = 0b1100_1000
            // (0,0,128) → A=3,B=2,G=0,R=0 = 0b1110_0000
            // (128,128,128) → A=3,B=2,G=2,R=2 = 0b1110_1010
            0b1100_0010,
            0b1100_1000,
            0b1110_0000,
            0b1110_1010,
            // Row 2: 42 → 0; 43 → 1; 85 → 1; 127 → 1
            // (42,42,42) → A=3,B=0,G=0,R=0 = 0xC0
            // (43,43,43) → A=3,B=1,G=1,R=1 = 0b1101_0101
            // (85,85,85) → same → 0b1101_0101
            // (127,127,127) → same → 0b1101_0101
            0b1100_0000,
            0b1101_0101,
            0b1101_0101,
            0b1101_0101,
            // Row 3: 170 → 2; 212 → 2; 213 → 3.
            // (170,170,170) → 0b1110_1010
            // (212,212,212) → 0b1110_1010
            // (213,213,213) → 0b1111_1111
            // (255,128,0)   → R=3, G=2, B=0 → A=3, B=0, G=2, R=3 = 0b1100_1011
            0b1110_1010,
            0b1110_1010,
            0b1111_1111,
            0b1100_1011,
        ];
        assert_eq!(
            output, expected,
            "quantiser output drifted from committed values; bump QUANTISER_VERSION if intentional",
        );
    }
}
