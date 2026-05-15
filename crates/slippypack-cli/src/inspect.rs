//! `slippypack inspect <pack>` — human-readable dump of a `.rawtiles`
//! pack's header, per-zoom tile counts, and extension sections.
//!
//! Phase 1 `PoC` scaffolding: lets a user verify a freshly-built pack
//! parses, has the expected tile count, and carries any extension
//! sections (`ATTR`, eventually `NAME` / `SRCD` / etc.) without having
//! to write a Rust program against `RawtilesReader`. Output format is
//! human-readable lines today; a `--format json` flag can land later
//! when a programmatic consumer needs it.

use std::path::PathBuf;

use slippypack_core::format::{
    ExtensionSection, ParsedHeader, RawtilesReader, ReaderError, TAG_ATTR, TAG_SRCD,
};

/// CLI-supplied arguments for `inspect`.
pub struct InspectArgs {
    /// Path to the `.rawtiles` pack to inspect.
    pub path: PathBuf,
}

/// Errors `inspect` can surface to the CLI.
#[derive(Debug)]
#[non_exhaustive]
pub enum InspectError {
    /// File I/O failure when opening or reading the pack.
    Io(std::io::Error),
    /// Pack didn't parse as a valid `.rawtiles` file. The reader's
    /// error describes which invariant failed.
    Reader(ReaderError),
}

impl core::fmt::Display for InspectError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Io(e) => write!(f, "I/O error: {e}"),
            Self::Reader(e) => write!(f, "pack did not parse: {e:?}"),
        }
    }
}

impl std::error::Error for InspectError {}

impl From<std::io::Error> for InspectError {
    fn from(e: std::io::Error) -> Self {
        Self::Io(e)
    }
}

impl From<ReaderError> for InspectError {
    fn from(e: ReaderError) -> Self {
        Self::Reader(e)
    }
}

/// Read the pack at `args.path`, parse via [`RawtilesReader`], and
/// write a human-readable summary to `out`.
///
/// # Errors
///
/// - [`InspectError::Io`] if the file can't be read.
/// - [`InspectError::Reader`] if the bytes aren't a valid pack.
pub fn run_inspect<W: std::io::Write>(
    args: &InspectArgs,
    out: &mut W,
) -> Result<(), InspectError> {
    let bytes = std::fs::read(&args.path)?;
    let reader = RawtilesReader::open(&bytes)?;
    write_summary(out, &args.path, bytes.len(), &reader)?;
    Ok(())
}

fn write_summary<W: std::io::Write>(
    out: &mut W,
    path: &std::path::Path,
    file_size: usize,
    reader: &RawtilesReader<'_>,
) -> std::io::Result<()> {
    let header = reader.header();
    let meta = reader.metadata();

    writeln!(out, "file: {}", path.display())?;
    writeln!(out, "size: {file_size} bytes")?;
    writeln!(
        out,
        "format_version: {}.{}",
        header.format_version.major, header.format_version.minor,
    )?;
    writeln!(out, "pack_uuid: {}", format_uuid(&meta.pack_uuid))?;
    if let Some(s) = meta.supersedes_uuid {
        writeln!(out, "supersedes_uuid: {}", format_uuid(&s))?;
    }
    if let Some(p) = meta.parent_uuid {
        writeln!(out, "parent_uuid: {}", format_uuid(&p))?;
    }
    writeln!(out, "pixel_format: {:?}", meta.pixel_format)?;
    writeln!(out, "projection: {:?}", meta.projection)?;
    writeln!(out, "tile_addressing_scheme: {:?}", meta.tile_addressing_scheme)?;
    writeln!(out, "tile_axis_convention: {:?}", meta.tile_axis_convention)?;
    writeln!(out, "tile_dim_px: {}", meta.tile_dim_px)?;
    writeln!(out, "zoom_range: {}-{}", meta.zoom_range.0, meta.zoom_range.1)?;
    writeln!(
        out,
        "bbox: minLon={} minLat={} maxLon={} maxLat={}",
        format_micro(meta.bbox.min_lon_micro),
        format_micro(meta.bbox.min_lat_micro),
        format_micro(meta.bbox.max_lon_micro),
        format_micro(meta.bbox.max_lat_micro),
    )?;
    writeln!(out, "build_timestamp: {}", meta.build_timestamp)?;
    writeln!(out, "tile_count: {}", reader.tile_count())?;
    write_per_zoom_counts(out, header)?;
    write_extensions(out, reader.extensions())?;
    Ok(())
}

/// Lines for the zooms that actually contain tiles. Empty zooms are
/// elided to keep the output tight — for a typical 1-3 zoom pack you'd
/// otherwise wade through 21 lines of "zoom N: 0 tiles".
fn write_per_zoom_counts<W: std::io::Write>(
    out: &mut W,
    header: &ParsedHeader,
) -> std::io::Result<()> {
    for (z, entry) in header.derived.zoom_offsets.iter().enumerate() {
        if entry.count > 0 {
            writeln!(out, "  zoom {z}: {} tile(s)", entry.count)?;
        }
    }
    Ok(())
}

fn write_extensions<W: std::io::Write>(
    out: &mut W,
    extensions: &[ExtensionSection],
) -> std::io::Result<()> {
    writeln!(out, "extensions: {} section(s)", extensions.len())?;
    for ext in extensions {
        let tag_str = String::from_utf8_lossy(&ext.tag);
        write!(
            out,
            "  {} ({} byte payload)",
            tag_str,
            ext.payload.len(),
        )?;
        // Inline a decoded preview for the tags we know how to render
        // — ATTR / SRCD are free-form UTF-8; NAME has a length-prefixed
        // BCP-47 tag plus the display name (defer); PLET is binary
        // palette bytes (defer); unknown tags get a hex-dump preview.
        // NAME (length-prefixed BCP-47 + display name) and PLET
        // (binary palette bytes) have their own decoders; tag+length
        // is enough for now. ATTR / SRCD are free-form UTF-8 — render
        // the payload so the operator can eyeball it.
        if ext.tag == TAG_ATTR || ext.tag == TAG_SRCD {
            if let Ok(s) = core::str::from_utf8(&ext.payload) {
                writeln!(out, ": {s:?}")?;
            } else {
                writeln!(out, ": <not valid UTF-8>")?;
            }
        } else {
            writeln!(out)?;
        }
    }
    Ok(())
}

/// Format a 16-byte UUID as the conventional `8-4-4-4-12` hex string.
fn format_uuid(bytes: &[u8; 16]) -> String {
    use core::fmt::Write as _;
    let mut hex = String::with_capacity(32);
    for b in bytes {
        write!(hex, "{b:02x}").expect("writing to String never fails");
    }
    format!(
        "{}-{}-{}-{}-{}",
        &hex[0..8],
        &hex[8..12],
        &hex[12..16],
        &hex[16..20],
        &hex[20..32],
    )
}

/// Convert a microdegree integer back to a decimal-degree string with
/// 6 fractional digits (the spec's pinned representation precision).
fn format_micro(micro: i32) -> String {
    let sign = if micro < 0 { "-" } else { "" };
    let abs = micro.unsigned_abs();
    let whole = abs / 1_000_000;
    let frac = abs % 1_000_000;
    format!("{sign}{whole}.{frac:06}")
}

#[cfg(test)]
mod tests {
    use super::{format_micro, format_uuid};

    #[test]
    fn format_uuid_matches_canonical_hyphenation() {
        let bytes = [
            0xde, 0xad, 0xbe, 0xef, 0xca, 0xfe, 0xba, 0xbe, 0x12, 0x34, 0x56, 0x78, 0x90, 0xab,
            0xcd, 0xef,
        ];
        assert_eq!(
            format_uuid(&bytes),
            "deadbeef-cafe-babe-1234-567890abcdef",
        );
    }

    #[test]
    fn format_micro_renders_positive_value() {
        assert_eq!(format_micro(49_290_000), "49.290000");
    }

    #[test]
    fn format_micro_renders_negative_value() {
        assert_eq!(format_micro(-123_155_000), "-123.155000");
    }

    #[test]
    fn format_micro_pads_short_fractional() {
        assert_eq!(format_micro(1_000), "0.001000");
    }

    #[test]
    fn format_micro_zero() {
        assert_eq!(format_micro(0), "0.000000");
    }
}
