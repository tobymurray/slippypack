//! Diagnostic subcommands grouped under `slippypack debug ...`.
//!
//! Phase 1.x first deliverable: `slippypack debug uuid` — emits either
//! the derived UUIDv5 `pack_uuid` (default) or the canonical descriptor
//! bytes (`--bytes`) for a given source/bbox/zoom combination, without
//! actually building a pack. Useful for verifying that two CLI
//! invocations would produce the same `pack_uuid`, or for sharing the
//! exact descriptor that fed a given pack with someone else for
//! independent UUIDv5 verification.

use slippypack_core::format::{Compression, PixelFormat};
use slippypack_core::identity::{canonical_descriptor_bytes, derive_pack_uuid};

use crate::build::{BboxDeg, BuildError, BuildOptions, descriptor_for};

/// Output shape for `debug uuid`.
#[derive(Debug, Clone, Copy, Default)]
pub enum DebugUuidFormat {
    /// Default: print the derived UUIDv5 as a hyphenated lowercase
    /// string followed by a newline.
    #[default]
    Uuid,
    /// Print the canonical descriptor bytes verbatim (no trailing
    /// newline). Pipe to `sha1sum` (with the namespace prepended) to
    /// independently verify the UUID; or to `xxd`/`jq` to inspect.
    Bytes,
}

/// Arguments to `slippypack debug uuid` (the shape mirrors `MakeArgs`'s
/// source/bbox/zoom subset, plus `--auth-header`/`--auth-query` since
/// those shape the descriptor's `auth_kinds`, plus `--pixel-format`
/// since that's also in the descriptor).
#[derive(Debug, Clone)]
pub struct DebugUuidArgs {
    pub source: String,
    pub bbox: Option<BboxDeg>,
    pub zoom_range: Option<(u8, u8)>,
    pub auth_headers: Vec<crate::sources::url_template::AuthHeader>,
    pub auth_query: Vec<crate::sources::url_template::AuthQuery>,
    pub pixel_format: PixelFormat,
    /// Tile edge length in pixels. In the canonical descriptor, so it changes
    /// the derived UUID exactly as `pixel_format` does.
    pub tile_dim_px: u16,
    pub format: DebugUuidFormat,
}

impl Default for DebugUuidArgs {
    fn default() -> Self {
        Self {
            source: String::new(),
            bbox: None,
            zoom_range: None,
            auth_headers: Vec::new(),
            auth_query: Vec::new(),
            pixel_format: PixelFormat::Abgr2222,
            tile_dim_px: 256,
            format: DebugUuidFormat::default(),
        }
    }
}

/// Compute the descriptor for `args` and write either the derived UUID
/// or the canonical bytes to `out`.
///
/// # Errors
///
/// Surfaces any descriptor-construction error (unknown source, bad URL
/// template, missing bbox/zoom).
pub fn run_debug_uuid<W: std::io::Write>(
    args: &DebugUuidArgs,
    out: &mut W,
) -> Result<(), BuildError> {
    let opts = BuildOptions {
        source: args.source.clone(),
        out: std::path::PathBuf::new(),
        bbox: args.bbox,
        zoom_range: args.zoom_range,
        auth_headers: args.auth_headers.clone(),
        auth_query: args.auth_query.clone(),
        rate_override: None,
        timestamp_override: None,
        pack_uuid_override: None,
        attribution: None,
        pixel_format: args.pixel_format,
        tile_dim_px: args.tile_dim_px,
        compression: Compression::None,
        cancel: None,
    };
    let descriptor = descriptor_for(&opts)?;
    match args.format {
        DebugUuidFormat::Uuid => {
            let uuid = derive_pack_uuid(&descriptor);
            writeln!(out, "{uuid}").map_err(BuildError::Io)?;
        }
        DebugUuidFormat::Bytes => {
            let bytes = canonical_descriptor_bytes(&descriptor);
            out.write_all(&bytes).map_err(BuildError::Io)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use slippypack_core::format::PixelFormat;

    use super::{DebugUuidArgs, DebugUuidFormat, run_debug_uuid};
    use crate::build::{BboxDeg, BuildError};

    #[test]
    fn synthetic_uuid_is_deterministic() {
        let args = DebugUuidArgs {
            source: "synthetic".to_string(),
            tile_dim_px: 256,
            bbox: None,
            zoom_range: None,
            auth_headers: Vec::new(),
            auth_query: Vec::new(),
            format: DebugUuidFormat::Uuid,            pixel_format: PixelFormat::Abgr2222,
        };
        let mut buf = Vec::new();
        run_debug_uuid(&args, &mut buf).unwrap();
        let uuid_a = String::from_utf8(buf).unwrap();

        let mut buf = Vec::new();
        run_debug_uuid(&args, &mut buf).unwrap();
        let uuid_b = String::from_utf8(buf).unwrap();

        assert_eq!(uuid_a, uuid_b);
        // UUID + newline = 37 chars.
        assert_eq!(uuid_a.len(), 37);
        assert!(uuid_a.ends_with('\n'));
    }

    #[test]
    fn synthetic_uuid_is_uuidv5_shape() {
        let args = DebugUuidArgs {
            source: "synthetic".to_string(),
            tile_dim_px: 256,
            bbox: None,
            zoom_range: None,
            auth_headers: Vec::new(),
            auth_query: Vec::new(),
            format: DebugUuidFormat::Uuid,            pixel_format: PixelFormat::Abgr2222,
        };
        let mut buf = Vec::new();
        run_debug_uuid(&args, &mut buf).unwrap();
        let s = String::from_utf8(buf).unwrap();
        // RFC 4122 hyphenated form: 8-4-4-4-12 with hex; the version
        // nibble for UUIDv5 is the first nibble of the third group.
        let trimmed = s.trim_end_matches('\n');
        let parts: Vec<&str> = trimmed.split('-').collect();
        assert_eq!(parts.len(), 5);
        assert_eq!(parts[0].len(), 8);
        assert_eq!(parts[1].len(), 4);
        assert_eq!(parts[2].len(), 4);
        assert_eq!(parts[3].len(), 4);
        assert_eq!(parts[4].len(), 12);
        assert!(parts[2].starts_with('5'), "version nibble is 5: {trimmed}");
    }

    #[test]
    fn bytes_output_starts_with_canonical_json() {
        let args = DebugUuidArgs {
            source: "synthetic".to_string(),
            tile_dim_px: 256,
            bbox: None,
            zoom_range: None,
            auth_headers: Vec::new(),
            auth_query: Vec::new(),
            format: DebugUuidFormat::Bytes,            pixel_format: PixelFormat::Abgr2222,
        };
        let mut buf = Vec::new();
        run_debug_uuid(&args, &mut buf).unwrap();
        // Canonical bytes always start with the affn key (lex-first;
        // `null` for non-LocalLinear packs like synthetic).
        assert!(
            buf.starts_with(b"{\"affn\":null,\"bbox\":"),
            "expected canonical JSON; got: {:?}",
            String::from_utf8_lossy(&buf),
        );
        // And end with the zoom_range key, with closing brace.
        assert!(
            buf.ends_with(b"}"),
            "expected closing brace; got: {:?}",
            String::from_utf8_lossy(&buf),
        );
    }

    #[test]
    fn url_template_uuid_changes_when_bbox_changes() {
        let make = |bbox: BboxDeg| {
            let mut buf = Vec::new();
            let args = DebugUuidArgs {
                source: "https://example.com/{z}/{x}/{y}.png".to_string(),
                bbox: Some(bbox),
                zoom_range: Some((10, 12)),
                auth_headers: Vec::new(),
                auth_query: Vec::new(),
                format: DebugUuidFormat::Uuid,
                pixel_format: PixelFormat::Abgr2222,
                tile_dim_px: 256,
            };
            run_debug_uuid(&args, &mut buf).unwrap();
            String::from_utf8(buf).unwrap()
        };
        let a = make(BboxDeg {
            min_lon: -0.15,
            min_lat: 51.49,
            max_lon: -0.10,
            max_lat: 51.52,
        });
        let b = make(BboxDeg {
            min_lon: -0.20,
            min_lat: 51.49,
            max_lon: -0.10,
            max_lat: 51.52,
        });
        assert_ne!(a, b);
    }

    #[test]
    fn missing_bbox_for_url_template_errors() {
        let args = DebugUuidArgs {
            source: "https://example.com/{z}/{x}/{y}.png".to_string(),
            bbox: None,
            zoom_range: Some((10, 12)),
            auth_headers: Vec::new(),
            auth_query: Vec::new(),
            format: DebugUuidFormat::Uuid,
            pixel_format: PixelFormat::Abgr2222,
            tile_dim_px: 256,
        };
        let mut buf = Vec::new();
        let err = run_debug_uuid(&args, &mut buf).unwrap_err();
        assert!(matches!(err, BuildError::MissingBbox), "got {err:?}");
    }

    #[test]
    fn url_template_uuid_changes_when_auth_header_added() {
        // Auth values stay out of the descriptor (per identity.rs's
        // I-009-style rule) but auth *kinds* are part of it, so toggling
        // --auth-header on must change the UUID.
        let bbox = BboxDeg {
            min_lon: -0.15,
            min_lat: 51.49,
            max_lon: -0.10,
            max_lat: 51.52,
        };
        let without = {
            let args = DebugUuidArgs {
                source: "https://example.com/{z}/{x}/{y}.png".to_string(),
                bbox: Some(bbox),
                zoom_range: Some((10, 12)),
                auth_headers: Vec::new(),
                auth_query: Vec::new(),
                format: DebugUuidFormat::Uuid,
                pixel_format: PixelFormat::Abgr2222,
                tile_dim_px: 256,
            };
            let mut buf = Vec::new();
            run_debug_uuid(&args, &mut buf).unwrap();
            String::from_utf8(buf).unwrap()
        };
        let with = {
            let args = DebugUuidArgs {
                source: "https://example.com/{z}/{x}/{y}.png".to_string(),
                bbox: Some(bbox),
                zoom_range: Some((10, 12)),
                auth_headers: vec![crate::sources::url_template::AuthHeader {
                    name: "Authorization".to_string(),
                    value: "Bearer xyz".to_string(),
                }],
                auth_query: Vec::new(),
                format: DebugUuidFormat::Uuid,
                pixel_format: PixelFormat::Abgr2222,
                tile_dim_px: 256,
            };
            let mut buf = Vec::new();
            run_debug_uuid(&args, &mut buf).unwrap();
            String::from_utf8(buf).unwrap()
        };
        assert_ne!(without, with);
    }

    #[test]
    fn url_template_uuid_does_not_depend_on_auth_value() {
        // Two invocations with the same auth *kind* but different auth
        // *values* must produce the same UUID — values stay out of the
        // descriptor.
        let bbox = BboxDeg {
            min_lon: -0.15,
            min_lat: 51.49,
            max_lon: -0.10,
            max_lat: 51.52,
        };
        let make = |value: &str| {
            let args = DebugUuidArgs {
                source: "https://example.com/{z}/{x}/{y}.png".to_string(),
                bbox: Some(bbox),
                zoom_range: Some((10, 12)),
                auth_headers: vec![crate::sources::url_template::AuthHeader {
                    name: "Authorization".to_string(),
                    value: value.to_string(),
                }],
                auth_query: Vec::new(),
                format: DebugUuidFormat::Uuid,
                pixel_format: PixelFormat::Abgr2222,
                tile_dim_px: 256,
            };
            let mut buf = Vec::new();
            run_debug_uuid(&args, &mut buf).unwrap();
            String::from_utf8(buf).unwrap()
        };
        assert_eq!(make("Bearer one"), make("Bearer two"));
    }

    #[test]
    fn unknown_source_kind_errors() {
        let args = DebugUuidArgs {
            source: "not-a-real-kind".to_string(),
            tile_dim_px: 256,
            bbox: None,
            zoom_range: None,
            auth_headers: Vec::new(),
            auth_query: Vec::new(),
            format: DebugUuidFormat::Uuid,            pixel_format: PixelFormat::Abgr2222,
        };
        let mut buf = Vec::new();
        let err = run_debug_uuid(&args, &mut buf).unwrap_err();
        assert!(
            matches!(err, BuildError::UnknownSourceKind(_)),
            "got {err:?}"
        );
    }
}
