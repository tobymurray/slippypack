//! Canonical source descriptor + UUIDv5 `pack_uuid` derivation.
//!
//! See `PLAN.md` § Canonical source descriptor for the wire-level rules
//! this module enforces:
//!
//! - UTF-8 JSON object, no whitespace, keys sorted lexicographically by
//!   codepoint, no trailing newline.
//! - Integers in decimal, no leading zeros, no `+`/`.0`.
//! - Coordinates as integer microdegrees (lat/lon × 10⁶, banker's rounding).
//! - File-content hashes as lowercase hex SHA-256.
//! - `pack_uuid = UUIDv5(slippypack_namespace, canonical_descriptor_bytes)`.
//!
//! The serializer is hand-rolled (not `serde_json`) for three reasons:
//! one canonical form (no surprises from `serde_json`'s defaults), zero
//! dependency surface beyond `uuid`, and the schema is fixed so a hand-
//! rolled implementation is small and easy to audit.

use uuid::Uuid;

/// Permanent slippypack namespace UUID, used as the seed for every
/// UUIDv5 `pack_uuid` derivation.
///
/// Generated via `uuidgen` on 2026-05-13. **Never changes across
/// slippypack versions** — changing this value would alter every
/// `pack_uuid` ever produced and break the "did the watch already
/// receive this pack?" companion check.
pub const SLIPPYPACK_NAMESPACE: Uuid =
    Uuid::from_bytes(*b"\x4e\x72\xf9\x62\x66\x32\x45\x38\x8e\x0a\x7e\xab\x63\x35\x0f\x3f");

/// `.upack` format version. v1 is (1, 0); spec bumps will change this.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct FormatVersion {
    pub major: u8,
    pub minor: u8,
}

/// Zoom range `[min, max]` inclusive on both ends.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ZoomRange {
    pub min: u8,
    pub max: u8,
}

/// Bounding box in integer microdegrees (lat/lon × 10⁶, banker's rounding).
///
/// See PLAN.md § Canonical source descriptor → "Numeric input precision"
/// for the rationale: the CLI/TOML accept decimal-degree floats; this
/// module's callers convert to microdegrees before constructing the
/// descriptor. Two inputs differing by less than 10⁻⁶ degrees collapse to
/// the same descriptor (and therefore the same `pack_uuid`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct BoundingBox {
    pub min_lon_micro: i32,
    pub min_lat_micro: i32,
    pub max_lon_micro: i32,
    pub max_lat_micro: i32,
}

/// Authentication mechanism used by a URL-template source. Auth *values*
/// (API keys, etc.) are deliberately NOT in the descriptor — see PLAN.md.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum AuthKind {
    /// HTTP header-based auth (e.g. `Authorization: Bearer ...`).
    /// Sorts first because "header" < "query" lexicographically.
    Header,
    Query,
}

/// A single source entry in the canonical descriptor's `sources` array.
///
/// Variants are declared in alphabetical kind-name order so the derived
/// `Ord` implementation matches the canonical sort rule (per PLAN.md):
/// `dir < geotiff < mbtiles < pbf < pmtiles < style < synthetic < url`.
///
/// **Per-variant field order also matters**: derived `Ord` compares
/// fields in declaration order, which determines tie-breaking when two
/// sources share `(zoom_min, zoom_max, kind)`. The first field is the
/// "identity" per PLAN.md § Canonical source descriptor.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Source {
    Dir {
        content_hash: [u8; 32],
        zoom_min: u8,
        zoom_max: u8,
    },
    Geotiff {
        content_hash: [u8; 32],
        zoom_min: u8,
        zoom_max: u8,
    },
    Mbtiles {
        content_hash: [u8; 32],
        zoom_min: u8,
        zoom_max: u8,
    },
    Pbf {
        content_hash: [u8; 32],
        zoom_min: u8,
        zoom_max: u8,
    },
    Pmtiles {
        content_hash: [u8; 32],
        zoom_min: u8,
        zoom_max: u8,
    },
    Style {
        /// SHA-256 of the MapLibre Style Spec JSON file. The style file
        /// IS this source's data (a `style:///path/to/style.json` source
        /// renders tiles directly from the style's embedded `sources`),
        /// so the style content is part of the source identity.
        ///
        /// Distinct from `PackDescriptor::style_hash` at the top level,
        /// which is the SHA-256 of the `--style` flag applied to a
        /// separate vector source (e.g. a PBF) at render time.
        content_hash: [u8; 32],
        zoom_min: u8,
        zoom_max: u8,
    },
    Synthetic {
        fixture_version: u32,
    },
    Url {
        template: String,
        auth_kinds: Vec<AuthKind>,
        zoom_min: u8,
        zoom_max: u8,
    },
}

impl Source {
    /// The source's `zoom_min` for sort purposes. `Synthetic` has no zoom
    /// range and sorts as `zoom_min = 0`.
    #[must_use]
    pub fn zoom_min(&self) -> u8 {
        match self {
            Self::Dir { zoom_min, .. }
            | Self::Geotiff { zoom_min, .. }
            | Self::Mbtiles { zoom_min, .. }
            | Self::Pbf { zoom_min, .. }
            | Self::Pmtiles { zoom_min, .. }
            | Self::Style { zoom_min, .. }
            | Self::Url { zoom_min, .. } => *zoom_min,
            Self::Synthetic { .. } => 0,
        }
    }

    /// The source's `zoom_max` for sort purposes. `Synthetic` has no zoom
    /// range and sorts as `zoom_max = 0`.
    #[must_use]
    pub fn zoom_max(&self) -> u8 {
        match self {
            Self::Dir { zoom_max, .. }
            | Self::Geotiff { zoom_max, .. }
            | Self::Mbtiles { zoom_max, .. }
            | Self::Pbf { zoom_max, .. }
            | Self::Pmtiles { zoom_max, .. }
            | Self::Style { zoom_max, .. }
            | Self::Url { zoom_max, .. } => *zoom_max,
            Self::Synthetic { .. } => 0,
        }
    }
}

/// The fields that go into the `pack_uuid` derivation.
///
/// See PLAN.md § Canonical source descriptor for the full schema. The
/// top-level key set is **fixed**; new keys are a format-version bump
/// (which becomes part of the descriptor itself, so old packs and new
/// packs get distinct `pack_uuid` values cleanly).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackDescriptor {
    pub bbox: BoundingBox,
    pub format_version: FormatVersion,
    pub pixel_format: u8,
    pub projection: u8,
    pub quantiser_version: u32,
    /// Caller-supplied source list. Order doesn't matter — the canonical
    /// serializer sorts by `(zoom_min, zoom_max, derived Source Ord)`
    /// before emitting bytes.
    pub sources: Vec<Source>,
    /// SHA-256 of the `--style` JSON (or the `style://` source's content)
    /// when a renderer-style is in play; `None` for non-renderer builds.
    pub style_hash: Option<[u8; 32]>,
    pub tile_addressing_scheme: u8,
    pub tile_axis_convention: u8,
    pub tile_dim_px: u16,
    pub zoom_range: ZoomRange,
}

/// Serialize the descriptor to its canonical byte form.
///
/// The output is UTF-8 JSON with:
/// - top-level keys in lexicographic codepoint order,
/// - no whitespace anywhere,
/// - no trailing newline,
/// - integers in decimal with no leading zeros and no `+`/`.0`,
/// - file-content hashes as lowercase hex strings,
/// - `sources` sorted per [`Source`]'s declared `Ord` after a `(zoom_min,
///   zoom_max)` major key,
/// - `auth_kinds` (inside URL source entries) sorted and deduplicated.
#[must_use]
pub fn canonical_descriptor_bytes(d: &PackDescriptor) -> Vec<u8> {
    let mut buf = String::new();
    buf.push('{');

    buf.push_str("\"bbox\":");
    write_bbox(&d.bbox, &mut buf);

    buf.push_str(",\"format_version\":");
    write_format_version(d.format_version, &mut buf);

    buf.push_str(",\"pixel_format\":");
    write_uint(u64::from(d.pixel_format), &mut buf);

    buf.push_str(",\"projection\":");
    write_uint(u64::from(d.projection), &mut buf);

    buf.push_str(",\"quantiser_version\":");
    write_uint(u64::from(d.quantiser_version), &mut buf);

    buf.push_str(",\"sources\":");
    write_sources(&d.sources, &mut buf);

    buf.push_str(",\"style_hash\":");
    match &d.style_hash {
        Some(hash) => write_hex_string(hash, &mut buf),
        None => buf.push_str("null"),
    }

    buf.push_str(",\"tile_addressing_scheme\":");
    write_uint(u64::from(d.tile_addressing_scheme), &mut buf);

    buf.push_str(",\"tile_axis_convention\":");
    write_uint(u64::from(d.tile_axis_convention), &mut buf);

    buf.push_str(",\"tile_dim_px\":");
    write_uint(u64::from(d.tile_dim_px), &mut buf);

    buf.push_str(",\"zoom_range\":");
    write_zoom_range(d.zoom_range, &mut buf);

    buf.push('}');
    buf.into_bytes()
}

/// Derive `pack_uuid` from the descriptor:
/// `UUIDv5(SLIPPYPACK_NAMESPACE, canonical_descriptor_bytes(descriptor))`.
#[must_use]
pub fn derive_pack_uuid(d: &PackDescriptor) -> Uuid {
    let bytes = canonical_descriptor_bytes(d);
    Uuid::new_v5(&SLIPPYPACK_NAMESPACE, &bytes)
}

// ---- Internal byte-level writers ------------------------------------

fn write_uint(n: u64, buf: &mut String) {
    // `u64::to_string` produces canonical decimal (no leading zeros,
    // no `+`, no `.0`) — matches PLAN.md's integer rules.
    buf.push_str(&n.to_string());
}

fn write_sint(n: i64, buf: &mut String) {
    buf.push_str(&n.to_string());
}

fn write_bbox(b: &BoundingBox, buf: &mut String) {
    buf.push('[');
    write_sint(i64::from(b.min_lon_micro), buf);
    buf.push(',');
    write_sint(i64::from(b.min_lat_micro), buf);
    buf.push(',');
    write_sint(i64::from(b.max_lon_micro), buf);
    buf.push(',');
    write_sint(i64::from(b.max_lat_micro), buf);
    buf.push(']');
}

fn write_format_version(v: FormatVersion, buf: &mut String) {
    buf.push('[');
    write_uint(u64::from(v.major), buf);
    buf.push(',');
    write_uint(u64::from(v.minor), buf);
    buf.push(']');
}

fn write_zoom_range(z: ZoomRange, buf: &mut String) {
    buf.push('[');
    write_uint(u64::from(z.min), buf);
    buf.push(',');
    write_uint(u64::from(z.max), buf);
    buf.push(']');
}

const HEX_DIGITS: &[u8; 16] = b"0123456789abcdef";

fn write_hex_string(bytes: &[u8], buf: &mut String) {
    buf.push('"');
    for b in bytes {
        buf.push(HEX_DIGITS[(b >> 4) as usize] as char);
        buf.push(HEX_DIGITS[(b & 0x0f) as usize] as char);
    }
    buf.push('"');
}

fn write_json_string(s: &str, buf: &mut String) {
    buf.push('"');
    for c in s.chars() {
        match c {
            '"' => buf.push_str("\\\""),
            '\\' => buf.push_str("\\\\"),
            // All control chars get the \u00XX form for one canonical
            // representation (no mix of \n / \t shortcuts).
            c if (c as u32) < 0x20 => {
                use core::fmt::Write;
                // Buffer is local; `write!` to a String never fails.
                write!(buf, "\\u{:04x}", c as u32).unwrap();
            }
            c => buf.push(c),
        }
    }
    buf.push('"');
}

fn write_sources(sources: &[Source], buf: &mut String) {
    // Sort by (zoom_min, zoom_max, derived Source Ord). Derived Ord on
    // Source compares variant index first (alphabetical kind order by
    // construction) then per-variant fields, the first of which is the
    // "identity" per PLAN.md.
    let mut sorted: Vec<&Source> = sources.iter().collect();
    sorted.sort_by(|a, b| {
        let zoom_a = (a.zoom_min(), a.zoom_max());
        let zoom_b = (b.zoom_min(), b.zoom_max());
        zoom_a.cmp(&zoom_b).then_with(|| a.cmp(b))
    });

    buf.push('[');
    for (i, s) in sorted.iter().enumerate() {
        if i > 0 {
            buf.push(',');
        }
        write_source(s, buf);
    }
    buf.push(']');
}

fn write_source(s: &Source, buf: &mut String) {
    match s {
        // File-backed kinds all share the same shape:
        // {content_hash, kind, zoom_max, zoom_min} in lex-sorted key order.
        Source::Dir {
            content_hash,
            zoom_min,
            zoom_max,
        } => write_file_kind(buf, "dir", content_hash, *zoom_min, *zoom_max),
        Source::Geotiff {
            content_hash,
            zoom_min,
            zoom_max,
        } => write_file_kind(buf, "geotiff", content_hash, *zoom_min, *zoom_max),
        Source::Mbtiles {
            content_hash,
            zoom_min,
            zoom_max,
        } => write_file_kind(buf, "mbtiles", content_hash, *zoom_min, *zoom_max),
        Source::Pbf {
            content_hash,
            zoom_min,
            zoom_max,
        } => write_file_kind(buf, "pbf", content_hash, *zoom_min, *zoom_max),
        Source::Pmtiles {
            content_hash,
            zoom_min,
            zoom_max,
        } => write_file_kind(buf, "pmtiles", content_hash, *zoom_min, *zoom_max),
        Source::Style {
            content_hash,
            zoom_min,
            zoom_max,
        } => write_file_kind(buf, "style", content_hash, *zoom_min, *zoom_max),
        Source::Synthetic { fixture_version } => {
            // Keys: fixture_version, kind
            buf.push_str("{\"fixture_version\":");
            write_uint(u64::from(*fixture_version), buf);
            buf.push_str(",\"kind\":\"synthetic\"}");
        }
        Source::Url {
            template,
            auth_kinds,
            zoom_min,
            zoom_max,
        } => {
            // Keys: auth_kinds, kind, template, zoom_max, zoom_min
            buf.push_str("{\"auth_kinds\":");
            write_auth_kinds(auth_kinds, buf);
            buf.push_str(",\"kind\":\"url\",\"template\":");
            write_json_string(template, buf);
            buf.push_str(",\"zoom_max\":");
            write_uint(u64::from(*zoom_max), buf);
            buf.push_str(",\"zoom_min\":");
            write_uint(u64::from(*zoom_min), buf);
            buf.push('}');
        }
    }
}

fn write_file_kind(
    buf: &mut String,
    kind_name: &str,
    content_hash: &[u8; 32],
    zoom_min: u8,
    zoom_max: u8,
) {
    // Keys in lex order: content_hash, kind, zoom_max, zoom_min.
    buf.push_str("{\"content_hash\":");
    write_hex_string(content_hash, buf);
    buf.push_str(",\"kind\":\"");
    buf.push_str(kind_name);
    buf.push_str("\",\"zoom_max\":");
    write_uint(u64::from(zoom_max), buf);
    buf.push_str(",\"zoom_min\":");
    write_uint(u64::from(zoom_min), buf);
    buf.push('}');
}

fn write_auth_kinds(auth_kinds: &[AuthKind], buf: &mut String) {
    // Sort + dedup defensively. Callers that pass unsorted or duplicate
    // values still get canonical output.
    let mut sorted: Vec<AuthKind> = auth_kinds.to_vec();
    sorted.sort();
    sorted.dedup();

    buf.push('[');
    for (i, k) in sorted.iter().enumerate() {
        if i > 0 {
            buf.push(',');
        }
        match k {
            AuthKind::Header => buf.push_str("\"header\""),
            AuthKind::Query => buf.push_str("\"query\""),
        }
    }
    buf.push(']');
}

#[cfg(test)]
mod tests {
    use super::{
        AuthKind, BoundingBox, FormatVersion, PackDescriptor, SLIPPYPACK_NAMESPACE, Source,
        ZoomRange, canonical_descriptor_bytes, derive_pack_uuid,
    };
    use uuid::Uuid;

    /// A minimal-but-realistic descriptor used as the base across tests.
    /// Most tests start from this and tweak one field.
    fn baseline_descriptor() -> PackDescriptor {
        PackDescriptor {
            bbox: BoundingBox {
                min_lon_micro: -180_000_000,
                min_lat_micro: -85_000_000,
                max_lon_micro: 180_000_000,
                max_lat_micro: 85_000_000,
            },
            format_version: FormatVersion { major: 1, minor: 0 },
            pixel_format: 1,
            projection: 1,
            quantiser_version: 1,
            sources: vec![Source::Url {
                template: "https://tile.openstreetmap.org/{z}/{x}/{y}.png".to_string(),
                auth_kinds: vec![],
                zoom_min: 6,
                zoom_max: 12,
            }],
            style_hash: None,
            tile_addressing_scheme: 1,
            tile_axis_convention: 1,
            tile_dim_px: 128,
            zoom_range: ZoomRange { min: 6, max: 12 },
        }
    }

    #[test]
    fn namespace_uuid_is_committed_value() {
        // Pin the namespace bytes; changing this value invalidates every
        // pack_uuid ever derived by slippypack.
        assert_eq!(
            SLIPPYPACK_NAMESPACE.to_string(),
            "4e72f962-6632-4538-8e0a-7eab63350f3f",
        );
    }

    #[test]
    fn baseline_canonical_bytes_match_committed_string() {
        let d = baseline_descriptor();
        let bytes = canonical_descriptor_bytes(&d);
        let expected = concat!(
            r#"{"bbox":[-180000000,-85000000,180000000,85000000]"#,
            r#","format_version":[1,0]"#,
            r#","pixel_format":1"#,
            r#","projection":1"#,
            r#","quantiser_version":1"#,
            r#","sources":[{"auth_kinds":[]"#,
            r#","kind":"url","template":"https://tile.openstreetmap.org/{z}/{x}/{y}.png""#,
            r#","zoom_max":12,"zoom_min":6}]"#,
            r#","style_hash":null"#,
            r#","tile_addressing_scheme":1"#,
            r#","tile_axis_convention":1"#,
            r#","tile_dim_px":128"#,
            r#","zoom_range":[6,12]}"#,
        );
        assert_eq!(
            core::str::from_utf8(&bytes).unwrap(),
            expected,
            "canonical bytes drifted",
        );
    }

    #[test]
    fn top_level_keys_are_in_lexicographic_order() {
        let d = baseline_descriptor();
        let s = String::from_utf8(canonical_descriptor_bytes(&d)).unwrap();
        // Walk the top-level keys and assert they appear in lex order.
        let top_level_keys = [
            "bbox",
            "format_version",
            "pixel_format",
            "projection",
            "quantiser_version",
            "sources",
            "style_hash",
            "tile_addressing_scheme",
            "tile_axis_convention",
            "tile_dim_px",
            "zoom_range",
        ];
        let mut search_from = 0;
        for key in top_level_keys {
            let pattern = format!("\"{key}\":");
            let pos = s[search_from..]
                .find(&pattern)
                .unwrap_or_else(|| panic!("missing key {key}"));
            search_from += pos + pattern.len();
        }
    }

    #[test]
    fn no_whitespace_in_canonical_bytes() {
        let d = baseline_descriptor();
        let bytes = canonical_descriptor_bytes(&d);
        for &b in &bytes {
            assert!(
                !b.is_ascii_whitespace(),
                "found whitespace byte {b:#04x} in canonical output",
            );
        }
    }

    #[test]
    fn no_trailing_newline_in_canonical_bytes() {
        let d = baseline_descriptor();
        let bytes = canonical_descriptor_bytes(&d);
        assert_ne!(bytes.last(), Some(&b'\n'), "canonical bytes ended with \\n");
    }

    #[test]
    fn empty_sources_serialise_to_empty_array() {
        let mut d = baseline_descriptor();
        d.sources.clear();
        let bytes = canonical_descriptor_bytes(&d);
        let s = core::str::from_utf8(&bytes).unwrap();
        assert!(s.contains("\"sources\":[]"), "got: {s}");
    }

    #[test]
    fn style_hash_some_serialises_as_lowercase_hex() {
        let mut d = baseline_descriptor();
        let mut hash = [0_u8; 32];
        for (i, b) in hash.iter_mut().enumerate() {
            *b = u8::try_from(i).unwrap();
        }
        d.style_hash = Some(hash);
        let bytes = canonical_descriptor_bytes(&d);
        let s = core::str::from_utf8(&bytes).unwrap();
        assert!(
            s.contains(
                "\"style_hash\":\"000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f\""
            ),
            "got: {s}",
        );
    }

    #[test]
    fn style_hash_none_serialises_as_null() {
        let d = baseline_descriptor();
        let bytes = canonical_descriptor_bytes(&d);
        let s = core::str::from_utf8(&bytes).unwrap();
        assert!(s.contains("\"style_hash\":null"), "got: {s}");
    }

    #[test]
    fn sources_sort_by_zoom_range_first_then_kind() {
        // Construct two URL sources with deliberately reversed declaration
        // order and check the canonical output sorts them by zoom range.
        let mut d = baseline_descriptor();
        d.sources = vec![
            Source::Url {
                template: "https://b/{z}/{x}/{y}.png".to_string(),
                auth_kinds: vec![],
                zoom_min: 10,
                zoom_max: 14,
            },
            Source::Url {
                template: "https://a/{z}/{x}/{y}.png".to_string(),
                auth_kinds: vec![],
                zoom_min: 4,
                zoom_max: 8,
            },
        ];
        let bytes = canonical_descriptor_bytes(&d);
        let s = core::str::from_utf8(&bytes).unwrap();
        let pos_a = s.find("https://a/").unwrap();
        let pos_b = s.find("https://b/").unwrap();
        assert!(
            pos_a < pos_b,
            "lower-zoom source should appear first; got order: {s}",
        );
    }

    #[test]
    fn sources_with_same_zoom_sort_by_kind() {
        // Two sources, same zoom range, different kinds. Alphabetical
        // kind order is: dir, geotiff, mbtiles, pbf, pmtiles, style,
        // synthetic, url.
        let mut d = baseline_descriptor();
        d.sources = vec![
            Source::Url {
                template: "https://x/{z}/{x}/{y}.png".to_string(),
                auth_kinds: vec![],
                zoom_min: 6,
                zoom_max: 12,
            },
            Source::Mbtiles {
                content_hash: [0xab; 32],
                zoom_min: 6,
                zoom_max: 12,
            },
        ];
        let bytes = canonical_descriptor_bytes(&d);
        let s = core::str::from_utf8(&bytes).unwrap();
        let pos_mbtiles = s.find("\"kind\":\"mbtiles\"").unwrap();
        let pos_url = s.find("\"kind\":\"url\"").unwrap();
        assert!(
            pos_mbtiles < pos_url,
            "mbtiles should sort before url; got: {s}",
        );
    }

    #[test]
    fn auth_kinds_serialise_sorted_and_deduplicated() {
        let mut d = baseline_descriptor();
        d.sources = vec![Source::Url {
            template: "https://example.com/{z}/{x}/{y}.png".to_string(),
            // Caller passed unsorted, with a duplicate. Serializer must fix.
            auth_kinds: vec![AuthKind::Query, AuthKind::Header, AuthKind::Query],
            zoom_min: 6,
            zoom_max: 12,
        }];
        let bytes = canonical_descriptor_bytes(&d);
        let s = core::str::from_utf8(&bytes).unwrap();
        assert!(
            s.contains("\"auth_kinds\":[\"header\",\"query\"]"),
            "got: {s}",
        );
    }

    #[test]
    fn empty_auth_kinds_serialise_as_empty_array() {
        let d = baseline_descriptor();
        let bytes = canonical_descriptor_bytes(&d);
        let s = core::str::from_utf8(&bytes).unwrap();
        assert!(s.contains("\"auth_kinds\":[]"), "got: {s}");
    }

    #[test]
    fn url_template_with_quote_and_backslash_is_escaped() {
        let mut d = baseline_descriptor();
        d.sources = vec![Source::Url {
            template: r#"https://example.com/"weird"\path/{z}/{x}/{y}.png"#.to_string(),
            auth_kinds: vec![],
            zoom_min: 6,
            zoom_max: 12,
        }];
        let bytes = canonical_descriptor_bytes(&d);
        let s = core::str::from_utf8(&bytes).unwrap();
        assert!(
            s.contains(r#""template":"https://example.com/\"weird\"\\path/{z}/{x}/{y}.png""#),
            "got: {s}",
        );
    }

    #[test]
    fn url_template_with_control_char_uses_u_escape() {
        let mut d = baseline_descriptor();
        // U+0009 (HT tab) is a control char. The serializer must emit it as
        // the six-character sequence "\\u0009" (backslash, u, 0, 0, 0, 9)
        // rather than the raw tab byte.
        d.sources = vec![Source::Url {
            template: "https://example.com/\t{z}/{x}/{y}.png".to_string(),
            auth_kinds: vec![],
            zoom_min: 6,
            zoom_max: 12,
        }];
        let bytes = canonical_descriptor_bytes(&d);
        let s = core::str::from_utf8(&bytes).unwrap();
        // Use a non-raw string for the expected substring so the literal
        // backslash is unambiguous: \\u0009 in source means the 6 chars
        // `	` in the assertion target.
        let expected_substring = "\"template\":\"https://example.com/\\u0009{z}/{x}/{y}.png\"";
        assert!(s.contains(expected_substring), "got: {s}");
        // Negative check: the raw tab byte must not appear in the output.
        assert!(
            !bytes.contains(&0x09),
            "raw tab byte leaked into canonical output",
        );
    }

    #[test]
    fn file_kind_sources_serialise_with_hex_hash() {
        let mut d = baseline_descriptor();
        let mut hash = [0_u8; 32];
        hash[0] = 0xde;
        hash[1] = 0xad;
        hash[2] = 0xbe;
        hash[3] = 0xef;
        d.sources = vec![Source::Mbtiles {
            content_hash: hash,
            zoom_min: 6,
            zoom_max: 12,
        }];
        let bytes = canonical_descriptor_bytes(&d);
        let s = core::str::from_utf8(&bytes).unwrap();
        assert!(
            s.contains(
                "\"content_hash\":\"deadbeef000000000000000000000000000000000000000000000000000000\
                 00\""
            ),
            "got: {s}",
        );
        assert!(s.contains("\"kind\":\"mbtiles\""), "got: {s}");
    }

    #[test]
    fn synthetic_source_serialises_with_fixture_version() {
        let mut d = baseline_descriptor();
        d.sources = vec![Source::Synthetic { fixture_version: 1 }];
        let bytes = canonical_descriptor_bytes(&d);
        let s = core::str::from_utf8(&bytes).unwrap();
        assert!(
            s.contains("\"sources\":[{\"fixture_version\":1,\"kind\":\"synthetic\"}]"),
            "got: {s}",
        );
    }

    #[test]
    fn style_source_serialises_with_content_hash() {
        let mut d = baseline_descriptor();
        let mut hash = [0_u8; 32];
        hash[0] = 0xab;
        hash[1] = 0xcd;
        d.sources = vec![Source::Style {
            content_hash: hash,
            zoom_min: 6,
            zoom_max: 12,
        }];
        let bytes = canonical_descriptor_bytes(&d);
        let s = core::str::from_utf8(&bytes).unwrap();
        assert!(
            s.contains(
                "\"sources\":[{\"content_hash\":\"abcd00000000000000000000000000000000000000000000000000000000000\
                 0\",\"kind\":\"style\",\"zoom_max\":12,\"zoom_min\":6}]"
            ),
            "got: {s}",
        );
    }

    #[test]
    fn style_source_with_different_content_hash_changes_pack_uuid() {
        // Two Style sources with identical zoom ranges but different
        // style JSONs MUST NOT collide on pack_uuid. This is the bug
        // closed by adding content_hash to Source::Style.
        let mut d_a = baseline_descriptor();
        let mut hash_a = [0_u8; 32];
        hash_a[0] = 0x11;
        d_a.sources = vec![Source::Style {
            content_hash: hash_a,
            zoom_min: 6,
            zoom_max: 12,
        }];

        let mut d_b = baseline_descriptor();
        let mut hash_b = [0_u8; 32];
        hash_b[0] = 0x22;
        d_b.sources = vec![Source::Style {
            content_hash: hash_b,
            zoom_min: 6,
            zoom_max: 12,
        }];

        assert_ne!(
            derive_pack_uuid(&d_a),
            derive_pack_uuid(&d_b),
            "Style sources with different content_hash must produce different pack_uuids",
        );
    }

    #[test]
    fn negative_bbox_coords_serialise_with_minus_sign() {
        let mut d = baseline_descriptor();
        d.bbox = BoundingBox {
            min_lon_micro: -123_456_789,
            min_lat_micro: -34_567_890,
            max_lon_micro: 0,
            max_lat_micro: 0,
        };
        let bytes = canonical_descriptor_bytes(&d);
        let s = core::str::from_utf8(&bytes).unwrap();
        assert!(
            s.contains("\"bbox\":[-123456789,-34567890,0,0]"),
            "got: {s}"
        );
    }

    #[test]
    fn source_input_order_does_not_change_canonical_bytes() {
        // Two equivalent descriptors with sources in different orders
        // should produce identical canonical bytes (and therefore the
        // same pack_uuid).
        let mk = |sources| PackDescriptor {
            sources,
            ..baseline_descriptor()
        };
        let s1 = Source::Mbtiles {
            content_hash: [0x11; 32],
            zoom_min: 4,
            zoom_max: 8,
        };
        let s2 = Source::Url {
            template: "https://b/{z}/{x}/{y}.png".to_string(),
            auth_kinds: vec![],
            zoom_min: 10,
            zoom_max: 14,
        };
        let d1 = mk(vec![s1.clone(), s2.clone()]);
        let d2 = mk(vec![s2, s1]);
        assert_eq!(
            canonical_descriptor_bytes(&d1),
            canonical_descriptor_bytes(&d2),
        );
        assert_eq!(derive_pack_uuid(&d1), derive_pack_uuid(&d2));
    }

    /// Determinism gate: the baseline descriptor's `pack_uuid` is locked
    /// to a specific UUID. Any drift here is a hash-output change and
    /// demands either a `quantiser_version` / `format_version` / etc.
    /// bump (depending on what changed) or a fix to the canonicalizer.
    ///
    /// The expected UUID was computed once by calling `derive_pack_uuid`
    /// on the baseline descriptor and committing the result. Two parties
    /// can verify independently by feeding the committed canonical bytes
    /// into any conformant UUIDv5 implementation with the committed
    /// namespace.
    #[test]
    fn determinism_baseline_pack_uuid_is_committed() {
        let d = baseline_descriptor();
        let uuid = derive_pack_uuid(&d);
        let expected =
            Uuid::parse_str("53077f67-522e-5cb0-b2b5-ffddba17d0db").expect("valid UUID literal");
        assert_eq!(
            uuid, expected,
            "baseline pack_uuid drifted; canonical bytes or namespace changed",
        );
    }

    #[test]
    fn descriptor_equality_implies_pack_uuid_equality() {
        let d1 = baseline_descriptor();
        let d2 = baseline_descriptor();
        assert_eq!(d1, d2);
        assert_eq!(derive_pack_uuid(&d1), derive_pack_uuid(&d2));
    }

    #[test]
    fn quantiser_version_change_changes_pack_uuid() {
        let d1 = baseline_descriptor();
        let mut d2 = baseline_descriptor();
        d2.quantiser_version = 2;
        assert_ne!(
            derive_pack_uuid(&d1),
            derive_pack_uuid(&d2),
            "bumping quantiser_version must change pack_uuid",
        );
    }

    #[test]
    fn bbox_change_changes_pack_uuid() {
        let d1 = baseline_descriptor();
        let mut d2 = baseline_descriptor();
        d2.bbox.min_lon_micro += 1;
        assert_ne!(
            derive_pack_uuid(&d1),
            derive_pack_uuid(&d2),
            "shifting bbox must change pack_uuid",
        );
    }

    #[test]
    fn derived_uuid_is_uuidv5() {
        let d = baseline_descriptor();
        let uuid = derive_pack_uuid(&d);
        // Version is in bits 48..52 of the UUID (per RFC 4122). For
        // UUIDv5, the value is 5.
        assert_eq!(uuid.get_version_num(), 5);
    }
}
