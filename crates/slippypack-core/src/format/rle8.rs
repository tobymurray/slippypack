//! Byte-level run-length encoding for `compression = 1` per rawtiles
//! spec § 9.11.
//!
//! The encoded stream is a sequence of one-byte-prefixed blocks:
//!
//! ```text
//! H (1 byte) + payload
//! ```
//!
//! - `H ∈ [0x00, 0x7F]`: literal run. Payload is `(H + 1)` literal bytes.
//! - `H ∈ [0x80, 0xFF]`: repeat run. Payload is one byte; decoder emits
//!   it `((H & 0x7F) + 1)` times.
//!
//! Both run kinds encode 1-128 output bytes. The encoded stream has no
//! terminator — the decoder stops once the expected uncompressed byte
//! count has been produced. Decoder state is a one-byte register plus
//! the output cursor; the decoder is row-streamable.
//!
//! [`encode`] produces the canonical encoding (§ 9.11's encoder rules:
//! greedy, repeats only for runs of length ≥ 3, literal runs split at
//! the 128-byte cap, repeats split at the 128-byte cap). Two writers
//! applied to the same input MUST produce byte-identical encoded
//! streams; the round-trip property of spec § 14.1 depends on it.

use alloc::vec::Vec;

/// Cap on the per-block run length. Header byte encodes `length − 1`,
/// so the maximum representable length per block is 128.
const MAX_RUN: usize = 128;

/// Threshold at which the canonical encoder prefers a repeat run over
/// a literal one. Runs of exactly this length and above emit a repeat
/// header; shorter runs flow into literal accumulation.
const REPEAT_THRESHOLD: usize = 3;

/// Canonical RLE8-encode `uncompressed` into a freshly-allocated `Vec`.
///
/// The encoding is deterministic and lossless. `encode(decode(x)) == x`
/// for any `x` valid `encode` output; `decode(encode(y))` recovers `y`
/// for any byte sequence `y`. Cross-writer reproducibility holds: two
/// implementations of this function MUST produce byte-identical
/// encoded streams for the same input.
///
/// Worst-case expansion is ≈ 0.78% (one header byte per 128 input
/// bytes in a purely literal stream).
#[must_use]
pub fn encode(uncompressed: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(uncompressed.len() + uncompressed.len() / 128 + 1);
    let mut i = 0;
    let n = uncompressed.len();

    while i < n {
        // Count the longest repeat starting at i, capped at MAX_RUN.
        let mut run = 1;
        while i + run < n && uncompressed[i + run] == uncompressed[i] && run < MAX_RUN {
            run += 1;
        }

        if run >= REPEAT_THRESHOLD {
            // Repeat run: header `0x80 | (run - 1)`, payload = 1 byte.
            // Cast is safe: run ∈ [REPEAT_THRESHOLD, MAX_RUN] = [3, 128]
            // so run - 1 ∈ [2, 127], fits in u8.
            #[allow(clippy::cast_possible_truncation)]
            let header = 0x80_u8 | ((run - 1) as u8);
            out.push(header);
            out.push(uncompressed[i]);
            i += run;
            continue;
        }

        // Literal accumulation: extend the literal run until either
        // we hit the MAX_RUN cap or the next position starts a
        // repeat-eligible run.
        let lit_start = i;
        i += 1; // we already know the byte at i isn't part of a long repeat
        while i < n {
            if i - lit_start >= MAX_RUN {
                break;
            }
            // Peek for a repeat-eligible run starting at i.
            let mut peek = 1;
            while i + peek < n
                && uncompressed[i + peek] == uncompressed[i]
                && peek < REPEAT_THRESHOLD
            {
                peek += 1;
            }
            if peek >= REPEAT_THRESHOLD {
                // Stop the literal run; the repeat will be emitted
                // on the next outer-loop iteration.
                break;
            }
            i += 1;
        }
        let lit_len = i - lit_start;
        // Cast is safe: lit_len ∈ [1, MAX_RUN] = [1, 128], so
        // lit_len - 1 ∈ [0, 127], fits in u8 with the top bit clear.
        #[allow(clippy::cast_possible_truncation)]
        let header = (lit_len - 1) as u8;
        out.push(header);
        out.extend_from_slice(&uncompressed[lit_start..i]);
    }

    out
}

/// RLE8 decoder errors.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum DecodeError {
    /// Encoded stream ended while the current block expected more
    /// payload bytes (literal-run cut off, or repeat-run missing its
    /// repeat byte).
    UnexpectedEnd,
    /// Decoder produced a different byte count than the caller declared
    /// as expected (`tile_dim_px² × bytes_per_pixel`). Spec § 6.2 says
    /// the decoder MUST produce exactly the declared count; mismatch
    /// is a per-tile decode error.
    OutputLengthMismatch { expected: usize, produced: usize },
}

impl core::fmt::Display for DecodeError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match *self {
            Self::UnexpectedEnd => f.write_str("encoded RLE8 stream ended mid-block"),
            Self::OutputLengthMismatch { expected, produced } => write!(
                f,
                "decoded RLE8 produced {produced} bytes; expected {expected}",
            ),
        }
    }
}

impl core::error::Error for DecodeError {}

/// Decode `encoded` into a freshly-allocated `Vec` of exactly
/// `expected_len` bytes. Returns [`DecodeError`] if the encoded stream
/// is truncated or if the decoded count doesn't match `expected_len`.
///
/// # Errors
///
/// - [`DecodeError::UnexpectedEnd`]: encoded stream cuts off
///   mid-block.
/// - [`DecodeError::OutputLengthMismatch`]: the encoded stream
///   decodes to a different byte count than `expected_len`.
pub fn decode(encoded: &[u8], expected_len: usize) -> Result<Vec<u8>, DecodeError> {
    let mut out = Vec::with_capacity(expected_len);
    let mut i = 0;

    while i < encoded.len() {
        let header = encoded[i];
        i += 1;
        if header < 0x80 {
            // Literal run.
            let len = usize::from(header) + 1;
            let end = i.checked_add(len).ok_or(DecodeError::UnexpectedEnd)?;
            if end > encoded.len() {
                return Err(DecodeError::UnexpectedEnd);
            }
            out.extend_from_slice(&encoded[i..end]);
            i = end;
        } else {
            // Repeat run.
            let len = usize::from(header & 0x7F) + 1;
            if i >= encoded.len() {
                return Err(DecodeError::UnexpectedEnd);
            }
            let byte = encoded[i];
            i += 1;
            // Use a tight loop rather than `vec![byte; len]; extend` to
            // keep the decoder allocation-free past the initial reserve.
            for _ in 0..len {
                out.push(byte);
            }
        }

        if out.len() >= expected_len {
            break;
        }
    }

    if out.len() != expected_len {
        return Err(DecodeError::OutputLengthMismatch {
            expected: expected_len,
            produced: out.len(),
        });
    }
    Ok(out)
}

/// Streaming decoder variant: writes decoded bytes into `output` via the
/// supplied closure, one slice at a time. Intended for the lazy-validating
/// reader profile (spec § 11.1) that wants to feed pixels directly to a
/// row buffer or SPI bus without buffering the whole decoded tile in RAM.
///
/// The closure is invoked with non-empty byte slices in decode order.
/// The total number of bytes passed to the closure across all calls
/// equals `expected_len` on a successful return.
///
/// # Errors
///
/// Same conditions as [`decode`] plus any error returned from the
/// closure (wrapped in `Err::OutputSink`).
pub fn decode_streaming<F, E>(
    encoded: &[u8],
    expected_len: usize,
    mut output: F,
) -> Result<(), StreamingDecodeError<E>>
where
    F: FnMut(&[u8]) -> Result<(), E>,
{
    let mut i = 0;
    let mut produced: usize = 0;
    // Small stack scratch for repeat-run emission. Sized to MAX_RUN
    // so a single repeat block fits in one closure call.
    let mut repeat_buf = [0_u8; MAX_RUN];

    while i < encoded.len() && produced < expected_len {
        let header = encoded[i];
        i += 1;
        if header < 0x80 {
            let len = usize::from(header) + 1;
            let end = i
                .checked_add(len)
                .ok_or(StreamingDecodeError::Decode(DecodeError::UnexpectedEnd))?;
            if end > encoded.len() {
                return Err(StreamingDecodeError::Decode(DecodeError::UnexpectedEnd));
            }
            let to_write = (expected_len - produced).min(len);
            output(&encoded[i..i + to_write]).map_err(StreamingDecodeError::OutputSink)?;
            produced += to_write;
            i = end;
        } else {
            let len = usize::from(header & 0x7F) + 1;
            if i >= encoded.len() {
                return Err(StreamingDecodeError::Decode(DecodeError::UnexpectedEnd));
            }
            let byte = encoded[i];
            i += 1;
            let to_write = (expected_len - produced).min(len);
            repeat_buf[..to_write].fill(byte);
            output(&repeat_buf[..to_write]).map_err(StreamingDecodeError::OutputSink)?;
            produced += to_write;
        }
    }

    if produced != expected_len {
        return Err(StreamingDecodeError::Decode(
            DecodeError::OutputLengthMismatch {
                expected: expected_len,
                produced,
            },
        ));
    }
    Ok(())
}

/// Wraps [`DecodeError`] alongside any caller-supplied output-sink error
/// for [`decode_streaming`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum StreamingDecodeError<E> {
    /// Same shape as [`DecodeError`].
    Decode(DecodeError),
    /// Caller's output closure returned an error.
    OutputSink(E),
}

#[cfg(test)]
mod tests {
    use super::{DecodeError, decode, decode_streaming, encode};
    use alloc::vec;
    use alloc::vec::Vec;

    /// Spec § 14.8 test vectors.
    #[test]
    fn spec_14_8_canonical_encodings() {
        let cases: &[(&[u8], &[u8])] = &[
            (&[0xAA], &[0x00, 0xAA]),
            (&[0xAA, 0xBB], &[0x01, 0xAA, 0xBB]),
            (&[0xAA, 0xAA, 0xAA], &[0x82, 0xAA]),
            (&[0x00; 128], &[0xFF, 0x00]),
            // case 5: 129 zeros → FF 00 00 00
            (&[0_u8; 129], &[0xFF, 0x00, 0x00, 0x00]),
            (&[0xAA, 0xBB, 0xCC, 0xDD, 0xEE], &[0x04, 0xAA, 0xBB, 0xCC, 0xDD, 0xEE]),
            (
                &[0xAA, 0xAA, 0xBB, 0xBB, 0xBB, 0xCC],
                &[0x01, 0xAA, 0xAA, 0x82, 0xBB, 0x00, 0xCC],
            ),
        ];
        for (i, (src, expected)) in cases.iter().enumerate() {
            let got = encode(src);
            assert_eq!(
                &got[..],
                *expected,
                "case {} encode mismatch: got {got:02X?}, expected {expected:02X?}",
                i + 1,
            );
            let round = decode(&got, src.len()).unwrap();
            assert_eq!(&round[..], *src, "case {} round-trip mismatch", i + 1);
        }
    }

    #[test]
    fn round_trips_pseudo_random() {
        // Linear congruential generator for deterministic pseudo-random
        // bytes; no external crate needed.
        let mut state: u64 = 0x1234_5678_9ABC_DEF0;
        let mut src = Vec::with_capacity(10_000);
        for _ in 0..10_000 {
            state = state
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1_442_695_040_888_963_407);
            // Top byte is the most uniform for this LCG.
            src.push((state >> 56) as u8);
        }
        let enc = encode(&src);
        let dec = decode(&enc, src.len()).unwrap();
        assert_eq!(dec, src);
    }

    #[test]
    fn round_trips_long_repeats() {
        // 1000 copies of 0x42 — encoder splits into 128 + 128 + ... + 104
        // repeat blocks.
        let src = vec![0x42_u8; 1000];
        let enc = encode(&src);
        let dec = decode(&enc, src.len()).unwrap();
        assert_eq!(dec, src);
        // Should be ~16 blocks of header+1 = ~32 bytes, way less than
        // the uncompressed 1000.
        assert!(enc.len() < 50, "expected tight encoding, got {} bytes", enc.len());
    }

    #[test]
    fn decode_rejects_truncated_literal() {
        // Header says "5 literal bytes" but only 3 are present.
        let bad = [0x04, 0xAA, 0xBB, 0xCC];
        assert_eq!(decode(&bad, 5), Err(DecodeError::UnexpectedEnd));
    }

    #[test]
    fn decode_rejects_repeat_missing_payload() {
        // Header 0x82 says "3-byte repeat" but no payload byte follows.
        let bad = [0x82];
        assert_eq!(decode(&bad, 3), Err(DecodeError::UnexpectedEnd));
    }

    #[test]
    fn decode_rejects_overlong_output() {
        // Header says 3-byte repeat but caller expects only 2 bytes.
        // The decoder produces 3 and returns OutputLengthMismatch
        // because the loop guard `out.len() >= expected_len` doesn't
        // truncate mid-run; the truncation would mask a malformed pack.
        let enc = [0x82, 0xAA];
        let res = decode(&enc, 2);
        match res {
            Err(DecodeError::OutputLengthMismatch { expected: 2, produced: 3 }) => {}
            other => panic!("expected OutputLengthMismatch{{2,3}}, got {other:?}"),
        }
    }

    #[test]
    fn streaming_decoder_emits_in_decode_order() {
        // 100 literal bytes followed by 50-byte repeat run, then a
        // 5-byte literal — verify streamed bytes match the bulk decode.
        let mut src = Vec::new();
        src.extend(0..100_u8);
        src.extend(core::iter::repeat_n(0xCC_u8, 50));
        src.extend([0xAA, 0xBB, 0xCC, 0xDD, 0xEE]);
        let enc = encode(&src);

        let bulk = decode(&enc, src.len()).unwrap();

        let mut streamed: Vec<u8> = Vec::new();
        decode_streaming::<_, ()>(&enc, src.len(), |chunk| {
            streamed.extend_from_slice(chunk);
            Ok(())
        })
        .unwrap();

        assert_eq!(bulk, streamed);
        assert_eq!(streamed, src);
    }

    #[test]
    fn streaming_decoder_propagates_sink_errors() {
        let src = vec![0xAA_u8, 0xBB, 0xCC];
        let enc = encode(&src);
        let result = decode_streaming::<_, &'static str>(&enc, src.len(), |_| Err("sink failed"));
        match result {
            Err(super::StreamingDecodeError::OutputSink("sink failed")) => {}
            other => panic!("expected OutputSink, got {other:?}"),
        }
    }
}
