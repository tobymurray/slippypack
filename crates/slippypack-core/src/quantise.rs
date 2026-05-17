//! Quantisers from RGB888 to a tile pack's on-disk pixel format.
//!
//! v1 ships two quantisers:
//!
//! - [`Abgr2222`] (spec § 9.1.1): each RGB channel mapped to a 2-bit
//!   quantum (4 displayed levels per channel), packed `AABBGGRR` MSB→LSB
//!   in one byte per pixel with alpha always opaque. Watch-tuned by
//!   default; ~MB-scale packs for country-size bounding boxes.
//! - [`Rgb565`] (spec § 9.2.1): RGB888 → RGB565 by bit-truncation
//!   (drop the low 3 bits of R/B and the low 2 bits of G), stored as
//!   16-bit little-endian on disk. The native framebuffer format for
//!   the `ST77xx` and `ILI93xx` LCD-controller families that dominate
//!   low-power-wearable hardware.
//!
//! The [`Quantiser`] trait names the seam for future pixel formats
//! (RGB565 for ~480p+ displays, RGB888 for phone/desktop offline-nav,
//! indexed-palette for e-readers). Adding a new quantiser is a
//! companion impl plus a `pixel_format` enum value; the format
//! [`PackMetadata`]-side stays unchanged. Bumping a quantiser's
//! `VERSION` is required whenever its byte output changes for any
//! input — the canonical source descriptor carries the version so
//! packs produced by different quantiser versions get distinct
//! `pack_uuid` values even when all other inputs match.
//!
//! Integer-only by construction across every impl: no floats, no FMA,
//! no `f32` semantics that vary across architectures. Cross-platform
//! byte-identical output is a load-bearing claim of the format.
//!
//! [`PackMetadata`]: crate::format::PackMetadata

/// Bumped on any change to the quantiser's byte output. Carried in the
/// canonical source descriptor (per `PLAN.md` § Canonical source descriptor)
/// so that packs produced by different quantiser versions get distinct
/// `pack_uuid` values even when all other inputs match.
///
/// Equal to [`Abgr2222::VERSION`] — retained as a module-level constant
/// for callers that don't want to name the quantiser type.
pub const QUANTISER_VERSION: u32 = Abgr2222::VERSION;

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

/// A pixel-format-specific quantiser. Each impl converts an RGB888
/// buffer to its target pixel format's bytes, integer-only, byte-
/// identical across architectures.
///
/// **Adding a new quantiser** (RGB565, indexed-palette, etc.):
///
/// 1. Implement [`Quantiser`] on a zero-sized type.
/// 2. Pin a [`VERSION`] (start at 1) and a [`PIXEL_FORMAT`] byte that
///    matches a reserved value in the format's pixel-format enum.
/// 3. Add a determinism test that locks the byte output for a fixed
///    input. Bumping `VERSION` later is mandatory for any output-byte
///    change.
///
/// [`VERSION`]: Abgr2222::VERSION
/// [`PIXEL_FORMAT`]: Abgr2222::PIXEL_FORMAT
pub trait Quantiser {
    /// This quantiser's output-version. Equal to its impl's
    /// `const VERSION: u32`. Bumped on any byte-output change.
    fn version(&self) -> u32;

    /// The pixel-format byte the output bytes target (matches the
    /// format's pixel-format enum — `1` for ABGR2222, future values
    /// reserved).
    fn pixel_format(&self) -> u8;

    /// Number of bytes per quantised output pixel.
    fn bytes_per_pixel(&self) -> usize;

    /// Quantise a flat RGB888 buffer (3 bytes per pixel, R-G-B order)
    /// into a flat output buffer ([`bytes_per_pixel`] per pixel).
    ///
    /// `output.len() == input.len() / 3 * bytes_per_pixel()` is a
    /// caller invariant; impls panic if it's violated.
    ///
    /// [`bytes_per_pixel`]: Quantiser::bytes_per_pixel
    fn quantise(&self, rgb888: &[u8], output: &mut [u8]);
}

/// The ABGR2222 quantiser. 2 bits per channel + 2 bits alpha (always
/// opaque) packed into one byte per pixel, `AABBGGRR` MSB→LSB.
///
/// Zero-sized: construct via `Abgr2222` or `Abgr2222::default()`.
/// The trait impl delegates to [`quantise_rgb888`].
#[derive(Debug, Clone, Copy, Default)]
pub struct Abgr2222;

impl Abgr2222 {
    /// Quantiser version. Bumped on any output-byte change.
    pub const VERSION: u32 = 1;
    /// Pixel-format enum byte for ABGR2222 (matches the format's
    /// `PixelFormat::Abgr2222 = 1`).
    pub const PIXEL_FORMAT: u8 = 1;
    /// One output byte per pixel.
    pub const BYTES_PER_PIXEL: usize = 1;
}

impl Quantiser for Abgr2222 {
    fn version(&self) -> u32 {
        Self::VERSION
    }
    fn pixel_format(&self) -> u8 {
        Self::PIXEL_FORMAT
    }
    fn bytes_per_pixel(&self) -> usize {
        Self::BYTES_PER_PIXEL
    }
    fn quantise(&self, rgb888: &[u8], output: &mut [u8]) {
        quantise_rgb888(rgb888, output);
    }
}

/// Quantise a single RGB888 pixel to RGB565, returned as a u16 packed
/// as `RRRRRGGGGGGBBBBB` (R in bits 15..11, G in bits 10..5, B in
/// bits 4..0). Spec § 9.2.1 canonical conversion.
///
/// Bit-truncation: `r5 = r8 >> 3`, `g6 = g8 >> 2`, `b5 = b8 >> 3`.
/// Integer-only by construction. Const-fn so callers can quantise at
/// compile time when input is known statically.
#[inline]
#[must_use]
pub const fn quantise_pixel_rgb565(r: u8, g: u8, b: u8) -> u16 {
    let r5 = (r >> 3) as u16;
    let g6 = (g >> 2) as u16;
    let b5 = (b >> 3) as u16;
    (r5 << 11) | (g6 << 5) | b5
}

/// Quantise a flat RGB888 buffer (3 bytes per pixel, R-G-B order) into a
/// flat RGB565 little-endian byte buffer (2 bytes per pixel). On-disk
/// byte order is LE within each 16-bit pixel (spec § 9.2), matching the
/// rest of the spec's endianness rule.
///
/// # Panics
///
/// - if `input.len()` is not a multiple of 3.
/// - if `output.len()` does not equal `input.len() / 3 * 2`.
pub fn quantise_rgb888_to_rgb565(input: &[u8], output: &mut [u8]) {
    assert!(
        input.len().is_multiple_of(3),
        "RGB888 input length ({}) must be a multiple of 3",
        input.len(),
    );
    let pixels = input.len() / 3;
    assert!(
        output.len() == pixels * 2,
        "output length ({}) must equal input pixels ({pixels}) × 2",
        output.len(),
    );
    for (out_pair, rgb) in output.chunks_exact_mut(2).zip(input.chunks_exact(3)) {
        let pixel = quantise_pixel_rgb565(rgb[0], rgb[1], rgb[2]);
        let bytes = pixel.to_le_bytes();
        out_pair[0] = bytes[0];
        out_pair[1] = bytes[1];
    }
}

/// The RGB565 quantiser. RGB888 → RGB565 by bit-truncation, 2 bytes
/// per pixel little-endian on disk.
///
/// Zero-sized: construct via `Rgb565` or `Rgb565::default()`. The trait
/// impl delegates to [`quantise_rgb888_to_rgb565`].
#[derive(Debug, Clone, Copy, Default)]
pub struct Rgb565;

impl Rgb565 {
    /// Quantiser version. Bumped on any output-byte change.
    pub const VERSION: u32 = 1;
    /// Pixel-format enum byte for RGB565 (matches the format's
    /// `PixelFormat::Rgb565 = 2`).
    pub const PIXEL_FORMAT: u8 = 2;
    /// Two output bytes per pixel.
    pub const BYTES_PER_PIXEL: usize = 2;
}

impl Quantiser for Rgb565 {
    fn version(&self) -> u32 {
        Self::VERSION
    }
    fn pixel_format(&self) -> u8 {
        Self::PIXEL_FORMAT
    }
    fn bytes_per_pixel(&self) -> usize {
        Self::BYTES_PER_PIXEL
    }
    fn quantise(&self, rgb888: &[u8], output: &mut [u8]) {
        quantise_rgb888_to_rgb565(rgb888, output);
    }
}

#[cfg(test)]
mod tests {
    use super::{Abgr2222, QUANTISER_VERSION, Quantiser, quantise_pixel, quantise_rgb888};

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

    // --- Quantiser trait tests --------------------------------------

    #[test]
    fn abgr2222_trait_constants_are_consistent() {
        let q = Abgr2222;
        assert_eq!(q.version(), Abgr2222::VERSION);
        assert_eq!(q.pixel_format(), Abgr2222::PIXEL_FORMAT);
        assert_eq!(q.bytes_per_pixel(), Abgr2222::BYTES_PER_PIXEL);
        // QUANTISER_VERSION is the back-compat alias for Abgr2222::VERSION.
        assert_eq!(QUANTISER_VERSION, Abgr2222::VERSION);
    }

    #[test]
    fn abgr2222_pixel_format_byte_is_one() {
        // The pixel_format enum byte for ABGR2222 is 1 (see format::types).
        assert_eq!(Abgr2222::PIXEL_FORMAT, 1);
    }

    #[test]
    fn abgr2222_trait_matches_free_function_byte_for_byte() {
        // Same input through both paths must produce identical output —
        // the trait is purely a wrapper over the free function.
        let input: [u8; 48] = [
            255, 0, 0, 0, 255, 0, 0, 0, 255, 255, 255, 255, 128, 0, 0, 0, 128, 0, 0, 0, 128, 128,
            128, 128, 42, 42, 42, 43, 43, 43, 85, 85, 85, 127, 127, 127, 170, 170, 170, 212, 212,
            212, 213, 213, 213, 255, 128, 0,
        ];
        let mut via_free = [0_u8; 16];
        let mut via_trait = [0_u8; 16];
        quantise_rgb888(&input, &mut via_free);
        Abgr2222.quantise(&input, &mut via_trait);
        assert_eq!(via_free, via_trait);
    }

    #[test]
    fn abgr2222_works_through_dyn_dispatch() {
        // Verify the trait is dyn-compatible — important for future
        // code paths that pick a quantiser at runtime based on a
        // pixel_format byte.
        let q: &dyn Quantiser = &Abgr2222;
        assert_eq!(q.version(), Abgr2222::VERSION);
        assert_eq!(q.pixel_format(), 1);
        assert_eq!(q.bytes_per_pixel(), 1);

        let input = [255_u8, 0, 0, 0, 255, 0];
        let mut output = [0_u8; 2];
        q.quantise(&input, &mut output);
        // (255,0,0) → A=3, B=0, G=0, R=3 → 0b1100_0011
        // (0,255,0) → A=3, B=0, G=3, R=0 → 0b1100_1100
        assert_eq!(output, [0b1100_0011, 0b1100_1100]);
    }

    // --- RGB565 quantiser tests --------------------------------------

    #[test]
    fn rgb565_trait_constants_are_consistent() {
        let q = super::Rgb565;
        assert_eq!(q.version(), super::Rgb565::VERSION);
        assert_eq!(q.pixel_format(), super::Rgb565::PIXEL_FORMAT);
        assert_eq!(q.bytes_per_pixel(), super::Rgb565::BYTES_PER_PIXEL);
        assert_eq!(super::Rgb565::PIXEL_FORMAT, 2);
        assert_eq!(super::Rgb565::BYTES_PER_PIXEL, 2);
    }

    #[test]
    fn rgb565_pixel_is_bit_truncation() {
        use super::quantise_pixel_rgb565;
        // (255, 0, 0) → r5=31, g6=0, b5=0 → 0xF800
        assert_eq!(quantise_pixel_rgb565(255, 0, 0), 0xF800);
        // (0, 255, 0) → r5=0, g6=63, b5=0 → 0x07E0
        assert_eq!(quantise_pixel_rgb565(0, 255, 0), 0x07E0);
        // (0, 0, 255) → r5=0, g6=0, b5=31 → 0x001F
        assert_eq!(quantise_pixel_rgb565(0, 0, 255), 0x001F);
        // (255, 255, 255) → 0xFFFF
        assert_eq!(quantise_pixel_rgb565(255, 255, 255), 0xFFFF);
    }

    /// Spec § 14.7 determinism gate: the canonical RGB888 → RGB565
    /// truncation applied to the § 14.4 input MUST produce the
    /// listed bytes. Any change here is an `Rgb565::VERSION` bump.
    #[test]
    fn rgb565_determinism_spec_14_7() {
        let input: [u8; 48] = [
            // Row 0: pure R, G, B, white
            255, 0, 0, 0, 255, 0, 0, 0, 255, 255, 255, 255,
            // Row 1: half-intensity primaries + 50% grey
            128, 0, 0, 0, 128, 0, 0, 0, 128, 128, 128, 128,
            // Row 2: per-channel greys at quantiser boundaries
            42, 42, 42, 43, 43, 43, 85, 85, 85, 127, 127, 127,
            // Row 3: per-channel greys above the high boundary + a
            // three-distinct-channel pixel
            170, 170, 170, 212, 212, 212, 213, 213, 213, 255, 128, 0,
        ];
        let mut output = [0_u8; 32];
        super::quantise_rgb888_to_rgb565(&input, &mut output);

        // Spec § 14.7 expected output, LE on disk.
        let expected: [u8; 32] = [
            0x00, 0xF8, 0xE0, 0x07, 0x1F, 0x00, 0xFF, 0xFF,
            0x00, 0x80, 0x00, 0x04, 0x10, 0x00, 0x10, 0x84,
            0x45, 0x29, 0x45, 0x29, 0xAA, 0x52, 0xEF, 0x7B,
            0x55, 0xAD, 0xBA, 0xD6, 0xBA, 0xD6, 0x00, 0xFC,
        ];
        assert_eq!(
            output, expected,
            "RGB565 output drifted from spec § 14.7; bump Rgb565::VERSION if intentional",
        );
    }

    #[test]
    #[should_panic(expected = "must be a multiple of 3")]
    fn rgb565_buffer_rejects_non_multiple_of_3_input() {
        let input = [0_u8; 4];
        let mut output = [0_u8; 2];
        super::quantise_rgb888_to_rgb565(&input, &mut output);
    }

    #[test]
    #[should_panic(expected = "must equal input pixels")]
    fn rgb565_buffer_rejects_output_size_mismatch() {
        let input = [0_u8; 6]; // 2 pixels, so output must be 4 bytes
        let mut output = [0_u8; 2]; // wrong
        super::quantise_rgb888_to_rgb565(&input, &mut output);
    }
}
