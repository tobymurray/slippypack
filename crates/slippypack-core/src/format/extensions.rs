//! Extension-section framing and tag constants.
//!
//! Each section is `[tag (4 B FourCC), len (4 B u32 LE), payload (len B)]`
//! with the next section's tag starting at a 4-byte-aligned offset
//! (i.e., 0–3 zero bytes of padding follow the payload).
//!
//! Tag constants for the reserved upper-case ASCII tags (defined by the
//! rawtiles spec at <https://github.com/tobymurray/rawtiles>); lower-case
//! ASCII tags are application-private.

/// Header size of one extension section (tag + length): 8 bytes.
pub const SECTION_HEADER_SIZE: usize = 8;

/// Reserved upper-case ASCII tag for **pack display name**. Payload
/// layout per the rawtiles spec:
///
/// ```text
/// uint8 tag_length | bcp47_tag (tag_length bytes, UTF-8) | name (rest, UTF-8)
/// ```
///
/// `tag_length` may be `0` to mean "no locale specified" (the
/// unlocalized default name). Multiple `NAME` sections may appear in a
/// pack, one per locale; readers pick by locale match (BCP-47
/// lookup-rules apply) or fall back to the `tag_length=0` section.
///
/// Use [`build_name_payload`] / [`parse_name_payload`] rather than
/// hand-encoding payloads.
pub const TAG_NAME: [u8; 4] = *b"NAME";

/// Reserved upper-case ASCII tag for **source description** (UTF-8,
/// free-form provenance text — "OSM 2026-04 Geofabrik Italy extract,
/// MapLibre watch-tuned style v2").
pub const TAG_SRCD: [u8; 4] = *b"SRCD";

/// Reserved upper-case ASCII tag for **attribution text** required by
/// source licenses. Per the rawtiles spec's `ATTR` definition: UTF-8,
/// newline-separated attribution strings, one per active source.
pub const TAG_ATTR: [u8; 4] = *b"ATTR";

/// Reserved upper-case ASCII tag for **palette** (when `pixel_format`
/// is an indexed value). Tightly-packed ABGR2222 bytes, one per
/// palette entry.
pub const TAG_PLET: [u8; 4] = *b"PLET";

/// Reserved upper-case ASCII tag for **affine matrix** (required when
/// `projection = LocalLinear`). Payload: 6 little-endian floats — the
/// 2×3 affine taking image-pixel coords to lat/lon.
pub const TAG_AFFN: [u8; 4] = *b"AFFN";

/// A single extension section: a 4-byte tag plus a payload buffer.
///
/// Tags are not validated for case beyond what the spec mandates (UPPER
/// = reserved for SDK use, `lower` = application-private). Slippypack
/// callers can construct any tag; the spec only constrains what readers
/// do with them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExtensionSection {
    pub tag: [u8; 4],
    pub payload: Vec<u8>,
}

/// Errors when parsing extension-section bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum ExtensionError {
    /// Slice ended mid-section (couldn't read the 8-byte header or the
    /// declared payload length).
    TruncatedSection,
    /// A section's declared length would extend past the end of the
    /// input buffer.
    LengthExceedsBuffer { declared: u32, available: usize },
    /// The padding bytes between sections weren't zero. Strict parsers
    /// reject; lenient parsers may treat as a warning. The current
    /// reader is strict.
    NonZeroPadding,
}

impl core::fmt::Display for ExtensionError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match *self {
            Self::TruncatedSection => f.write_str("extension section truncated"),
            Self::LengthExceedsBuffer {
                declared,
                available,
            } => write!(
                f,
                "extension declared length {declared} exceeds remaining buffer ({available})",
            ),
            Self::NonZeroPadding => f.write_str("non-zero alignment padding between sections"),
        }
    }
}

impl core::error::Error for ExtensionError {}

/// Serialize one extension section into a flat byte vector including
/// alignment padding. Length: `8 + payload.len() + pad`, where `pad`
/// is 0-3 bytes of zero to round up to a 4-byte boundary.
///
/// # Panics
///
/// Panics if `section.payload.len()` exceeds `u32::MAX`. Realistic
/// extension payloads are far smaller (the largest expected v1 payload
/// is the `ATTR` attribution text for a multi-source layered build,
/// well under 64 KB).
#[must_use]
pub fn write_extension_section(section: &ExtensionSection) -> Vec<u8> {
    let payload_len = section.payload.len();
    let padded_payload_len = payload_len.next_multiple_of(4);
    let mut buf = Vec::with_capacity(SECTION_HEADER_SIZE + padded_payload_len);
    buf.extend_from_slice(&section.tag);
    let length_u32 = u32::try_from(payload_len).expect("extension payload exceeds u32 length");
    buf.extend_from_slice(&length_u32.to_le_bytes());
    buf.extend_from_slice(&section.payload);
    // Zero-pad to 4-byte alignment.
    buf.resize(SECTION_HEADER_SIZE + padded_payload_len, 0);
    buf
}

/// Serialize a sequence of extension sections back-to-back, each padded
/// to 4-byte alignment. Convenience wrapper around
/// [`write_extension_section`].
#[must_use]
pub fn write_extension_sections(sections: &[ExtensionSection]) -> Vec<u8> {
    let mut out = Vec::new();
    for section in sections {
        out.extend(write_extension_section(section));
    }
    out
}

/// Parse a flat byte buffer into a sequence of [`ExtensionSection`].
/// The buffer must consist of zero or more concatenated section
/// records (each `[tag, length, payload, alignment-padding]`) and end
/// exactly on a section boundary (no trailing garbage).
///
/// # Errors
///
/// See [`ExtensionError`].
///
/// # Panics
///
/// Does not panic in practice — the function validates input length
/// before each fixed-width field access, so the internal
/// `.try_into()` calls on slices have guaranteed-correct lengths.
pub fn read_extension_sections(mut input: &[u8]) -> Result<Vec<ExtensionSection>, ExtensionError> {
    let mut out = Vec::new();
    while !input.is_empty() {
        if input.len() < SECTION_HEADER_SIZE {
            return Err(ExtensionError::TruncatedSection);
        }
        let mut tag = [0_u8; 4];
        tag.copy_from_slice(&input[0..4]);
        let length = u32::from_le_bytes(input[4..8].try_into().expect("4 bytes"));
        let length_us = length as usize;
        let padded = length_us.next_multiple_of(4);
        let total = SECTION_HEADER_SIZE + padded;
        if input.len() < SECTION_HEADER_SIZE + length_us {
            return Err(ExtensionError::LengthExceedsBuffer {
                declared: length,
                available: input.len().saturating_sub(SECTION_HEADER_SIZE),
            });
        }
        if input.len() < total {
            return Err(ExtensionError::TruncatedSection);
        }
        let payload = input[SECTION_HEADER_SIZE..SECTION_HEADER_SIZE + length_us].to_vec();
        // Strict padding check.
        for &b in &input[SECTION_HEADER_SIZE + length_us..total] {
            if b != 0 {
                return Err(ExtensionError::NonZeroPadding);
            }
        }
        out.push(ExtensionSection { tag, payload });
        input = &input[total..];
    }
    Ok(out)
}

// ---- NAME extension payload codec ----------------------------------

/// Errors when building or parsing a [`TAG_NAME`] payload.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum NameSectionError {
    /// The BCP-47 tag's UTF-8 byte length doesn't fit in the `uint8`
    /// length prefix (> 255 bytes). Realistic tags are well under 50
    /// bytes (e.g. `en-Latn-GB-boont-x-private` is 28); the limit
    /// surfaces only for malformed input.
    TagTooLong { len: usize },
    /// Payload was zero bytes long (no `tag_length` prefix at all).
    PayloadEmpty,
    /// `tag_length` byte declared more bytes than the payload contains.
    TagExceedsPayload { declared: usize, available: usize },
    /// The declared tag bytes or the name bytes weren't valid UTF-8.
    InvalidUtf8,
}

impl core::fmt::Display for NameSectionError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match *self {
            Self::TagTooLong { len } => {
                write!(f, "BCP-47 tag is {len} bytes; must fit in uint8 (≤ 255)")
            }
            Self::PayloadEmpty => f.write_str("NAME payload is empty (missing tag_length byte)"),
            Self::TagExceedsPayload {
                declared,
                available,
            } => write!(
                f,
                "NAME tag_length {declared} exceeds remaining payload ({available} bytes)",
            ),
            Self::InvalidUtf8 => f.write_str("NAME tag or name bytes are not valid UTF-8"),
        }
    }
}

impl core::error::Error for NameSectionError {}

/// Build a [`TAG_NAME`] section payload from a BCP-47 language tag and
/// a pack display name.
///
/// Pass `bcp47_tag = ""` for the unlocalized default name (the section
/// then encodes a single `0x00` length byte followed by the name).
///
/// # Errors
///
/// - [`NameSectionError::TagTooLong`] if the tag's UTF-8 byte length
///   exceeds 255.
pub fn build_name_payload(bcp47_tag: &str, name: &str) -> Result<Vec<u8>, NameSectionError> {
    let tag_bytes = bcp47_tag.as_bytes();
    let tag_len = tag_bytes.len();
    let tag_len_u8 =
        u8::try_from(tag_len).map_err(|_| NameSectionError::TagTooLong { len: tag_len })?;
    let mut out = Vec::with_capacity(1 + tag_len + name.len());
    out.push(tag_len_u8);
    out.extend_from_slice(tag_bytes);
    out.extend_from_slice(name.as_bytes());
    Ok(out)
}

/// Parse a [`TAG_NAME`] section payload into `(bcp47_tag, name)`.
/// `bcp47_tag` is `""` (zero-length) for an unlocalized default name.
///
/// # Errors
///
/// - [`NameSectionError::PayloadEmpty`] if the payload is zero bytes.
/// - [`NameSectionError::TagExceedsPayload`] if the declared
///   `tag_length` exceeds the remaining bytes.
/// - [`NameSectionError::InvalidUtf8`] if either the tag bytes or the
///   name bytes aren't valid UTF-8.
pub fn parse_name_payload(payload: &[u8]) -> Result<(&str, &str), NameSectionError> {
    let (&tag_len_byte, rest) = payload
        .split_first()
        .ok_or(NameSectionError::PayloadEmpty)?;
    let tag_len = tag_len_byte as usize;
    if rest.len() < tag_len {
        return Err(NameSectionError::TagExceedsPayload {
            declared: tag_len,
            available: rest.len(),
        });
    }
    let (tag_bytes, name_bytes) = rest.split_at(tag_len);
    let tag = core::str::from_utf8(tag_bytes).map_err(|_| NameSectionError::InvalidUtf8)?;
    let name = core::str::from_utf8(name_bytes).map_err(|_| NameSectionError::InvalidUtf8)?;
    Ok((tag, name))
}

#[cfg(test)]
mod tests {
    use super::{
        ExtensionError, ExtensionSection, NameSectionError, SECTION_HEADER_SIZE, TAG_AFFN,
        TAG_ATTR, TAG_NAME, TAG_PLET, TAG_SRCD, build_name_payload, parse_name_payload,
        read_extension_sections, write_extension_section, write_extension_sections,
    };

    #[test]
    fn reserved_tags_are_ascii_upper() {
        for tag in [TAG_NAME, TAG_SRCD, TAG_ATTR, TAG_PLET, TAG_AFFN] {
            for b in tag {
                assert!(
                    b.is_ascii_uppercase(),
                    "tag {tag:?} has non-uppercase byte {b:#04x}",
                );
            }
        }
    }

    #[test]
    fn empty_payload_section_is_8_bytes() {
        let s = ExtensionSection {
            tag: TAG_NAME,
            payload: vec![],
        };
        let buf = write_extension_section(&s);
        assert_eq!(buf.len(), SECTION_HEADER_SIZE);
        assert_eq!(&buf[0..4], &TAG_NAME);
        assert_eq!(&buf[4..8], &[0, 0, 0, 0]);
    }

    #[test]
    fn payload_length_is_u32_little_endian() {
        let s = ExtensionSection {
            tag: TAG_SRCD,
            payload: b"hello".to_vec(),
        };
        let buf = write_extension_section(&s);
        assert_eq!(&buf[4..8], &[5, 0, 0, 0], "length = 5 LE");
    }

    #[test]
    fn payload_is_inlined_after_header() {
        let s = ExtensionSection {
            tag: TAG_ATTR,
            payload: b"hello".to_vec(),
        };
        let buf = write_extension_section(&s);
        assert_eq!(&buf[8..13], b"hello");
    }

    #[test]
    fn payload_is_zero_padded_to_4_byte_alignment() {
        // payload.len() == 5 → padded to 8.
        let s = ExtensionSection {
            tag: TAG_ATTR,
            payload: b"hello".to_vec(),
        };
        let buf = write_extension_section(&s);
        assert_eq!(buf.len(), 8 + 8);
        assert_eq!(&buf[13..16], &[0, 0, 0]);

        // payload.len() == 4 → no padding.
        let s = ExtensionSection {
            tag: TAG_ATTR,
            payload: b"4444".to_vec(),
        };
        let buf = write_extension_section(&s);
        assert_eq!(buf.len(), 8 + 4);
    }

    #[test]
    fn write_then_read_round_trips_single_section() {
        let s = ExtensionSection {
            tag: TAG_NAME,
            payload: b"Local trails".to_vec(),
        };
        let buf = write_extension_section(&s);
        let parsed = read_extension_sections(&buf).unwrap();
        assert_eq!(parsed.len(), 1);
        assert_eq!(parsed[0], s);
    }

    #[test]
    fn multiple_sections_round_trip() {
        let sections = vec![
            ExtensionSection {
                tag: TAG_NAME,
                payload: b"My pack".to_vec(),
            },
            ExtensionSection {
                tag: TAG_SRCD,
                payload: b"OSM 2026-04".to_vec(),
            },
            ExtensionSection {
                tag: TAG_ATTR,
                payload: b"\xc2\xa9 OpenStreetMap contributors\n\xc2\xa9 MapTiler".to_vec(),
            },
        ];
        let buf = write_extension_sections(&sections);
        let parsed = read_extension_sections(&buf).unwrap();
        assert_eq!(parsed, sections);
    }

    #[test]
    fn empty_buffer_yields_no_sections() {
        let parsed = read_extension_sections(&[]).unwrap();
        assert!(parsed.is_empty());
    }

    #[test]
    fn read_rejects_truncated_header() {
        // 7 bytes — not enough for an 8-byte section header.
        let buf = [0_u8; 7];
        assert_eq!(
            read_extension_sections(&buf),
            Err(ExtensionError::TruncatedSection),
        );
    }

    #[test]
    fn read_rejects_length_exceeds_buffer() {
        // Header claims payload len 100; only 8 bytes total available.
        let mut buf = [0_u8; 8];
        buf[0..4].copy_from_slice(&TAG_NAME);
        buf[4..8].copy_from_slice(&100_u32.to_le_bytes());
        assert!(matches!(
            read_extension_sections(&buf),
            Err(ExtensionError::LengthExceedsBuffer { declared: 100, .. }),
        ));
    }

    #[test]
    fn read_rejects_non_zero_padding() {
        // 5-byte payload → 3 bytes of padding expected to be zero.
        let mut buf = vec![0_u8; 16];
        buf[0..4].copy_from_slice(&TAG_NAME);
        buf[4..8].copy_from_slice(&5_u32.to_le_bytes());
        buf[8..13].copy_from_slice(b"hello");
        buf[13] = 0xFF; // non-zero padding
        assert_eq!(
            read_extension_sections(&buf),
            Err(ExtensionError::NonZeroPadding),
        );
    }

    #[test]
    fn sections_pack_back_to_back_with_alignment_only_between() {
        let a = ExtensionSection {
            tag: TAG_NAME,
            payload: b"a".to_vec(), // 1 byte payload → 3 bytes pad → next at offset 12
        };
        let b = ExtensionSection {
            tag: TAG_SRCD,
            payload: b"bb".to_vec(), // 2 byte payload → 2 bytes pad
        };
        let buf = write_extension_sections(&[a.clone(), b.clone()]);
        assert_eq!(buf.len(), 12 + 12, "two 12-byte aligned sections");
        // Second section starts at offset 12.
        assert_eq!(&buf[12..16], &TAG_SRCD);
        let parsed = read_extension_sections(&buf).unwrap();
        assert_eq!(parsed, vec![a, b]);
    }

    #[test]
    fn empty_section_at_start_is_handled() {
        // An empty-payload section followed by a non-empty one.
        let sections = vec![
            ExtensionSection {
                tag: TAG_PLET,
                payload: vec![],
            },
            ExtensionSection {
                tag: TAG_NAME,
                payload: b"x".to_vec(),
            },
        ];
        let buf = write_extension_sections(&sections);
        let parsed = read_extension_sections(&buf).unwrap();
        assert_eq!(parsed, sections);
    }

    #[test]
    fn ascii_lowercase_tags_are_application_private_and_round_trip() {
        // Lowercase tags are application-private per spec; the format
        // module doesn't enforce case rules.
        let custom = ExtensionSection {
            tag: *b"trls",
            payload: b"trail name data".to_vec(),
        };
        let buf = write_extension_section(&custom);
        let parsed = read_extension_sections(&buf).unwrap();
        assert_eq!(parsed[0], custom);
    }

    // ---- NAME payload codec tests --------------------------------

    #[test]
    fn build_name_payload_round_trip_simple() {
        let payload = build_name_payload("en", "Alpine Trails").unwrap();
        // Layout: [0x02, 'e', 'n', 'A', 'l', 'p', 'i', 'n', 'e', ' ',
        //          'T', 'r', 'a', 'i', 'l', 's']
        assert_eq!(payload[0], 0x02);
        assert_eq!(&payload[1..3], b"en");
        assert_eq!(&payload[3..], b"Alpine Trails");

        let (tag, name) = parse_name_payload(&payload).unwrap();
        assert_eq!(tag, "en");
        assert_eq!(name, "Alpine Trails");
    }

    #[test]
    fn build_name_payload_empty_tag_for_unlocalized_name() {
        let payload = build_name_payload("", "Trails").unwrap();
        assert_eq!(payload[0], 0x00);
        assert_eq!(&payload[1..], b"Trails");

        let (tag, name) = parse_name_payload(&payload).unwrap();
        assert_eq!(tag, "");
        assert_eq!(name, "Trails");
    }

    #[test]
    fn build_name_payload_full_bcp47() {
        // A realistic full BCP-47 tag with extensions.
        let payload = build_name_payload("en-Latn-GB-boont-x-private", "Pack").unwrap();
        let (tag, name) = parse_name_payload(&payload).unwrap();
        assert_eq!(tag, "en-Latn-GB-boont-x-private");
        assert_eq!(name, "Pack");
    }

    #[test]
    fn build_name_payload_utf8_name() {
        // Non-ASCII name. BCP-47 tags themselves are ASCII per RFC 5646.
        let payload = build_name_payload("ja", "日本のトレイル").unwrap();
        let (tag, name) = parse_name_payload(&payload).unwrap();
        assert_eq!(tag, "ja");
        assert_eq!(name, "日本のトレイル");
    }

    #[test]
    fn build_name_payload_max_length_tag() {
        // Tag at the 255-byte limit (1-byte length prefix maxes out).
        let tag = "a".repeat(255);
        let payload = build_name_payload(&tag, "Pack").unwrap();
        assert_eq!(payload[0], 255);
        assert_eq!(payload.len(), 1 + 255 + 4);
        let (parsed_tag, parsed_name) = parse_name_payload(&payload).unwrap();
        assert_eq!(parsed_tag.len(), 255);
        assert_eq!(parsed_name, "Pack");
    }

    #[test]
    fn build_name_payload_rejects_tag_too_long() {
        let tag = "a".repeat(256);
        let err = build_name_payload(&tag, "Pack").unwrap_err();
        assert!(matches!(err, NameSectionError::TagTooLong { len: 256 }));
    }

    #[test]
    fn parse_name_payload_rejects_empty() {
        let err = parse_name_payload(&[]).unwrap_err();
        assert!(matches!(err, NameSectionError::PayloadEmpty));
    }

    #[test]
    fn parse_name_payload_rejects_tag_length_overflow() {
        // tag_length = 10 but only 2 bytes follow.
        let payload = [10_u8, b'e', b'n'];
        let err = parse_name_payload(&payload).unwrap_err();
        assert!(matches!(
            err,
            NameSectionError::TagExceedsPayload {
                declared: 10,
                available: 2
            }
        ));
    }

    #[test]
    fn parse_name_payload_rejects_invalid_utf8_in_tag() {
        // tag_length = 2, then two invalid UTF-8 bytes.
        let payload = [2_u8, 0xff, 0xfe, b'x'];
        let err = parse_name_payload(&payload).unwrap_err();
        assert!(matches!(err, NameSectionError::InvalidUtf8));
    }

    #[test]
    fn parse_name_payload_rejects_invalid_utf8_in_name() {
        // tag_length = 0, then invalid UTF-8 in the name bytes.
        let payload = [0_u8, 0xff, 0xfe];
        let err = parse_name_payload(&payload).unwrap_err();
        assert!(matches!(err, NameSectionError::InvalidUtf8));
    }

    #[test]
    fn name_payload_fits_into_extension_section_end_to_end() {
        // End-to-end: build a NAME payload, wrap it in an ExtensionSection,
        // serialize + parse back, and re-decode the payload.
        let original_payload = build_name_payload("en", "Hello").unwrap();
        let section = ExtensionSection {
            tag: TAG_NAME,
            payload: original_payload.clone(),
        };
        let bytes = write_extension_section(&section);
        let parsed = read_extension_sections(&bytes).unwrap();
        assert_eq!(parsed.len(), 1);
        assert_eq!(parsed[0].tag, TAG_NAME);
        assert_eq!(parsed[0].payload, original_payload);
        let (tag, name) = parse_name_payload(&parsed[0].payload).unwrap();
        assert_eq!(tag, "en");
        assert_eq!(name, "Hello");
    }
}
