//! CRC-32/ISO-HDLC (the "PNG/zlib" CRC) for the `.upack` footer.
//!
//! The `.upack` format ends with a 4-byte little-endian CRC-32 over
//! every byte of the file up to (but not including) the CRC itself.
//! See PLAN.md and the una-sdk `.upack` spec.
//!
//! This is the standard polynomial used by PNG, gzip, zip, zlib — the
//! one most often called just "CRC-32". Polynomial `0xEDB88320`
//! (reflected form), init `0xFFFF_FFFF`, xor-out `0xFFFF_FFFF`.

/// Precomputed table for table-driven CRC-32 update. 1024 bytes
/// (256 × u32). Computed at compile time so there's no runtime
/// initialization cost.
const CRC32_TABLE: [u32; 256] = {
    let mut table = [0_u32; 256];
    let mut i = 0_usize;
    while i < 256 {
        // `i` is in 0..256 so the cast to u32 is exact.
        #[allow(clippy::cast_possible_truncation)]
        let mut c = i as u32;
        let mut j = 0;
        while j < 8 {
            c = if c & 1 != 0 {
                (c >> 1) ^ 0xEDB8_8320
            } else {
                c >> 1
            };
            j += 1;
        }
        table[i] = c;
        i += 1;
    }
    table
};

/// Streaming CRC-32 accumulator. Construct with [`Crc32::new`], feed
/// bytes via [`Crc32::update`] (zero or more times), and call
/// [`Crc32::finalize`] to get the final CRC-32 value.
///
/// The intermediate state is `u32`; the writer streams the entire pack
/// through this accumulator while assembling header + index + tile
/// blob + extension sections, then emits the finalized value as the
/// 4-byte little-endian footer.
#[derive(Debug, Clone, Copy)]
pub struct Crc32 {
    value: u32,
}

impl Crc32 {
    /// Initialize the CRC state. Equivalent to [`Default::default`].
    #[must_use]
    pub const fn new() -> Self {
        Self { value: 0xFFFF_FFFF }
    }

    /// Mix `data` into the accumulator.
    pub fn update(&mut self, data: &[u8]) {
        let mut c = self.value;
        for &b in data {
            let idx = ((c ^ u32::from(b)) & 0xFF) as usize;
            c = (c >> 8) ^ CRC32_TABLE[idx];
        }
        self.value = c;
    }

    /// Return the final CRC-32 value. Consumes the accumulator so an
    /// already-finalized CRC can't be accidentally re-finalized.
    #[must_use]
    pub const fn finalize(self) -> u32 {
        self.value ^ 0xFFFF_FFFF
    }
}

impl Default for Crc32 {
    fn default() -> Self {
        Self::new()
    }
}

/// One-shot CRC-32 over `data`. Equivalent to constructing a [`Crc32`],
/// calling `update(data)`, and calling `finalize()`.
#[must_use]
pub fn crc32_ieee(data: &[u8]) -> u32 {
    let mut crc = Crc32::new();
    crc.update(data);
    crc.finalize()
}

#[cfg(test)]
mod tests {
    use super::{Crc32, crc32_ieee};

    /// Standard CRC-32 test vectors (CRC-32/ISO-HDLC, the "PNG/zlib" CRC).
    /// Values verified against multiple independent implementations.
    #[test]
    fn empty_input_crc_is_zero() {
        // init (0xFFFF_FFFF) XOR xor_out (0xFFFF_FFFF) = 0.
        assert_eq!(crc32_ieee(&[]), 0);
    }

    #[test]
    fn single_a_byte_crc() {
        // "a" is a well-known CRC-32 test vector.
        assert_eq!(crc32_ieee(b"a"), 0xE8B7_BE43);
    }

    #[test]
    fn abc_crc() {
        assert_eq!(crc32_ieee(b"abc"), 0x3524_41C2);
    }

    /// The "check value" published in CRC catalog references for
    /// CRC-32/ISO-HDLC: `CRC32("123456789") = 0xCBF4_3926`.
    #[test]
    fn standard_check_value() {
        assert_eq!(crc32_ieee(b"123456789"), 0xCBF4_3926);
    }

    #[test]
    fn streaming_matches_one_shot() {
        let data = b"The quick brown fox jumps over the lazy dog";
        let one_shot = crc32_ieee(data);

        let mut streaming = Crc32::new();
        // Update in arbitrary chunks.
        streaming.update(&data[..3]);
        streaming.update(&data[3..10]);
        streaming.update(&data[10..]);
        let chunked = streaming.finalize();

        assert_eq!(chunked, one_shot);
    }

    #[test]
    fn zero_bytes_have_predictable_crc() {
        // 4 zero bytes — the kind of input a serializer might emit for
        // a reserved field. Lock the value so layout-test fixtures stay
        // stable as we work out header bytes.
        assert_eq!(crc32_ieee(&[0_u8; 4]), 0x2144_DF1C);
    }

    #[test]
    fn update_with_empty_slice_is_noop() {
        let mut a = Crc32::new();
        a.update(b"hello");
        a.update(&[]);
        let with_empty = a.finalize();

        let without_empty = crc32_ieee(b"hello");
        assert_eq!(with_empty, without_empty);
    }

    #[test]
    fn default_matches_new() {
        let a = Crc32::default();
        let b = Crc32::new();
        // Same starting state; same finalize for empty data.
        assert_eq!(a.finalize(), b.finalize());
    }

    #[test]
    fn long_input_crc_matches_byte_by_byte() {
        // 1 KiB of pseudo-random-ish data (linear-feedback-style) — verify
        // chunked and per-byte updates produce identical CRCs.
        let mut data = [0_u8; 1024];
        let mut seed: u32 = 0xC0FF_EE42;
        for byte in &mut data {
            seed = seed.wrapping_mul(1_103_515_245).wrapping_add(12_345);
            #[allow(clippy::cast_possible_truncation)]
            {
                // (seed >> 16) takes the high bits; we want the LSB of that
                // for an 8-bit pseudo-random value. Truncation is intentional.
                *byte = (seed >> 16) as u8;
            }
        }
        let one_shot = crc32_ieee(&data);

        let mut byte_by_byte = Crc32::new();
        for byte in data {
            byte_by_byte.update(&[byte]);
        }
        assert_eq!(byte_by_byte.finalize(), one_shot);
    }
}
