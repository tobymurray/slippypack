//! Validation for `--attribution` strings before they're emitted as the
//! payload of an `ATTR` extension section per the rawtiles spec § 7.3.
//!
//! The spec is strict about which codepoints may appear inside an
//! attribution string: LF is reserved for separating per-source
//! attributions, and a handful of other control / line-break codepoints
//! are forbidden outright. This module rejects any input that would
//! produce a non-conformant payload, with distinct error variants so
//! tests (and a future structured-error CLI output) can identify which
//! rule fired.

/// Why a candidate attribution string was rejected. Each variant maps
/// to one spec rule from § 7.3 so callers can render a precise
/// diagnostic.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum AttrError {
    /// Spec § 7.3: "Payload length MUST NOT be zero. A pack with zero
    /// sources MUST omit the ATTR section." The CLI omits ATTR when
    /// `--attribution` is absent; an explicitly empty value is an
    /// error so the user knows to drop the flag rather than passing
    /// `""`.
    Empty,
    /// Contains LF (0x0A). LF is reserved for separating attribution
    /// strings across active sources. Phase 1 builds are single-source,
    /// so any LF in input would falsely advertise a phantom second
    /// source.
    ContainsLf,
    /// Contains CR (0x0D). Spec forbids CRLF and bare CR.
    ContainsCr,
    /// Contains a C0 control character other than LF
    /// (range 0x01..=0x1F except 0x0A) or DEL (0x7F).
    ContainsControl { byte: u8 },
    /// Contains a Unicode line-break codepoint outside the C0 range:
    /// U+0085 (NEL), U+2028 (LS), or U+2029 (PS).
    ContainsUnicodeLineBreak { codepoint: u32 },
}

impl core::fmt::Display for AttrError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match *self {
            Self::Empty => f.write_str(
                "attribution must not be empty; omit --attribution to produce a pack \
                 without an ATTR section",
            ),
            Self::ContainsLf => f.write_str(
                "attribution must not contain LF (0x0A); LF is reserved for separating \
                 per-source attribution strings in multi-source packs, which Phase 1 \
                 does not yet support",
            ),
            Self::ContainsCr => f.write_str(
                "attribution must not contain CR (0x0D); spec forbids CRLF and bare CR",
            ),
            Self::ContainsControl { byte } => write!(
                f,
                "attribution must not contain ASCII control byte {byte:#04x}",
            ),
            Self::ContainsUnicodeLineBreak { codepoint } => write!(
                f,
                "attribution must not contain Unicode line-break codepoint U+{codepoint:04X}",
            ),
        }
    }
}

impl std::error::Error for AttrError {}

/// Reject inputs whose UTF-8 bytes would not form a conformant ATTR
/// payload per the rawtiles spec § 7.3. Returns `Ok(())` if the string
/// is safe to emit as the payload of a single-source ATTR section.
///
/// # Errors
///
/// See [`AttrError`] for the rules enforced. The variants are distinct
/// so a caller can render a targeted diagnostic for each spec rule.
pub fn validate_attribution_string(s: &str) -> Result<(), AttrError> {
    if s.is_empty() {
        return Err(AttrError::Empty);
    }
    for ch in s.chars() {
        match ch {
            '\n' => return Err(AttrError::ContainsLf),
            '\r' => return Err(AttrError::ContainsCr),
            '\u{0085}' | '\u{2028}' | '\u{2029}' => {
                return Err(AttrError::ContainsUnicodeLineBreak {
                    codepoint: ch as u32,
                });
            }
            c if (c as u32) < 0x20 || (c as u32) == 0x7F => {
                // ASCII C0 control range (excluding LF, already handled
                // above) plus DEL. ch is always ASCII here so the cast
                // to u8 is lossless.
                return Err(AttrError::ContainsControl { byte: c as u8 });
            }
            _ => {}
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{AttrError, validate_attribution_string};

    #[test]
    fn accepts_osm_attribution() {
        validate_attribution_string("\u{00a9} OpenStreetMap contributors").unwrap();
    }

    #[test]
    fn accepts_compound_attribution_with_semicolon_separator() {
        // OpenTopoMap-style compound credit. Phase 1 packs one source
        // so internal separators belong inside one ATTR string.
        validate_attribution_string(
            "Karte: \u{00a9} OpenTopoMap (CC-BY-SA); Daten: \u{00a9} OSM-Mitwirkende, SRTM",
        )
        .unwrap();
    }

    #[test]
    fn rejects_empty() {
        assert_eq!(validate_attribution_string(""), Err(AttrError::Empty));
    }

    #[test]
    fn rejects_single_lf() {
        assert_eq!(
            validate_attribution_string("\u{00a9} OSM\n\u{00a9} SRTM"),
            Err(AttrError::ContainsLf),
        );
    }

    #[test]
    fn rejects_crlf() {
        assert_eq!(
            validate_attribution_string("\u{00a9} OSM\r\nsecond line"),
            Err(AttrError::ContainsCr),
        );
    }

    #[test]
    fn rejects_bare_cr() {
        assert_eq!(
            validate_attribution_string("\u{00a9} OSM\rmore"),
            Err(AttrError::ContainsCr),
        );
    }

    #[test]
    fn rejects_c0_control() {
        assert_eq!(
            validate_attribution_string("hello\x07bell"),
            Err(AttrError::ContainsControl { byte: 0x07 }),
        );
    }

    #[test]
    fn rejects_del() {
        assert_eq!(
            validate_attribution_string("hello\x7fworld"),
            Err(AttrError::ContainsControl { byte: 0x7f }),
        );
    }

    #[test]
    fn rejects_nel() {
        assert_eq!(
            validate_attribution_string("hello\u{0085}world"),
            Err(AttrError::ContainsUnicodeLineBreak { codepoint: 0x0085 }),
        );
    }

    #[test]
    fn rejects_line_separator() {
        assert_eq!(
            validate_attribution_string("hello\u{2028}world"),
            Err(AttrError::ContainsUnicodeLineBreak { codepoint: 0x2028 }),
        );
    }

    #[test]
    fn rejects_paragraph_separator() {
        assert_eq!(
            validate_attribution_string("hello\u{2029}world"),
            Err(AttrError::ContainsUnicodeLineBreak { codepoint: 0x2029 }),
        );
    }

    #[test]
    fn accepts_non_ascii_utf8() {
        // CJK ideographs — multi-byte UTF-8 but no forbidden codepoints.
        validate_attribution_string("\u{65e5}\u{672c}\u{8a9e}").unwrap();
    }

    #[test]
    fn rejects_tab() {
        // HTAB (0x09) is in the C0 range and not exempted, so spec
        // § 7.3 requires us to reject it.
        assert_eq!(
            validate_attribution_string("hello\tworld"),
            Err(AttrError::ContainsControl { byte: 0x09 }),
        );
    }
}
