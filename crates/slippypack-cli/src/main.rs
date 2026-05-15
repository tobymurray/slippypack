//! `slippypack` — native CLI for building offline `.rawtiles` tile packs.
//!
//! Phase 1 first slice: `make --source synthetic --out <path>` builds a
//! pack from the embedded synthetic fixture and writes it via an
//! atomic `.rawtiles.partial` → rename. Other source kinds (URL templates,
//! MBTiles, PMTiles, `dir://`) land in Phase 1.x; vector sources
//! (`pbf`, `style`) land in Phase 2.

use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use clap::{Parser, Subcommand};

mod attribution;
mod build;
mod debug;
mod inspect;
mod sources;

use attribution::validate_attribution_string;
use build::{BboxDeg, BuildError, BuildOptions, build};
use debug::{DebugUuidArgs, DebugUuidFormat, run_debug_uuid};
use inspect::{InspectArgs, InspectError, run_inspect};

#[derive(Parser)]
#[command(
    name = "slippypack",
    version,
    about = "Build offline .rawtiles map packs"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Build a `.rawtiles` from a tile source.
    Make(MakeArgs),
    /// Print a human-readable summary of a `.rawtiles` pack: header
    /// metadata, per-zoom tile counts, and extension sections.
    Inspect(InspectCliArgs),
    /// Diagnostic subcommands.
    #[command(subcommand)]
    Debug(DebugCommand),
}

#[derive(clap::Args)]
struct InspectCliArgs {
    /// Path to the `.rawtiles` pack to inspect.
    pack: PathBuf,
}

#[derive(Subcommand)]
enum DebugCommand {
    /// Print the UUIDv5 `pack_uuid` (or canonical descriptor bytes
    /// with `--bytes`) for a given source/bbox/zoom — without doing
    /// any tile fetching, decoding, or writing.
    Uuid(DebugUuidCliArgs),
}

#[derive(clap::Args)]
struct DebugUuidCliArgs {
    /// Tile source. Same shape as `make --source`.
    #[arg(long)]
    source: String,

    /// Bounding box in decimal degrees. Required for URL-template /
    /// MBTiles / PMTiles / `dir://` sources; ignored for `synthetic`.
    #[arg(long, value_parser = parse_bbox, allow_hyphen_values = true)]
    bbox: Option<BboxDeg>,

    /// Zoom range. Same shape as `make --zoom`.
    #[arg(long, value_parser = parse_zoom)]
    zoom: Option<(u8, u8)>,

    /// Auth header (`"Name: value"`) — only the kind is recorded in
    /// the descriptor, so the actual value here doesn't affect the
    /// emitted UUID, but its presence does. Repeatable.
    #[arg(long = "auth-header")]
    auth_header: Vec<String>,

    /// Auth query (`"key=value"`) — same descriptor semantics as
    /// `--auth-header`. Repeatable.
    #[arg(long = "auth-query")]
    auth_query: Vec<String>,

    /// Emit the canonical descriptor bytes (UTF-8 JSON, no trailing
    /// newline) instead of the derived UUIDv5. Useful for piping into
    /// `sha1sum`, `xxd`, or a third-party UUIDv5 implementation for
    /// independent verification.
    #[arg(long)]
    bytes: bool,
}

#[derive(clap::Args)]
struct MakeArgs {
    /// Tile source. Phase 1 first slice supports only `synthetic`.
    /// Phase 1.x adds HTTPS URL templates, MBTiles, PMTiles, and `dir://`.
    #[arg(long)]
    source: String,

    /// Output pack path. The CLI writes to `<out>.partial` first and
    /// atomically renames on success — partial files are removed on
    /// abort.
    #[arg(long)]
    out: PathBuf,

    /// Bounding box in decimal degrees: `minLon,minLat,maxLon,maxLat`.
    /// Required for URL-template / MBTiles / PMTiles / `dir://` sources.
    /// Ignored for `synthetic` (which has a fixed world-bbox).
    #[arg(long, value_parser = parse_bbox, allow_hyphen_values = true)]
    bbox: Option<BboxDeg>,

    /// Zoom range: single zoom (e.g. `8`) or inclusive range (e.g. `6-12`).
    /// Required for URL-template / MBTiles / PMTiles / `dir://` sources.
    /// Ignored for `synthetic` (which is fixed at z=2).
    #[arg(long, value_parser = parse_zoom)]
    zoom: Option<(u8, u8)>,

    /// HTTP header to add to every URL-template request, in
    /// `"Name: value"` form. Repeatable. Example:
    /// `--auth-header "Authorization: Bearer $TOKEN"`. The header name
    /// is recorded in the pack's canonical descriptor as
    /// `auth_kinds: ["header"]`; the value is NOT part of the
    /// descriptor (per identity.rs).
    #[arg(long = "auth-header")]
    auth_header: Vec<String>,

    /// Query parameter to append to every URL-template request, in
    /// `"key=value"` form. Repeatable. Example:
    /// `--auth-query "key=YOUR_MAPTILER_KEY"`. The key kind is
    /// recorded in `auth_kinds: ["query"]`; the value is NOT part of
    /// the descriptor.
    #[arg(long = "auth-query")]
    auth_query: Vec<String>,

    /// CI override: pin `build_timestamp` to a fixed value (seconds
    /// since Unix epoch). Production builds derive this from input
    /// `Last-Modified` headers / file mtimes; the synthetic source
    /// has no freshness info so its default is `0`.
    #[arg(long)]
    timestamp: Option<u64>,

    /// CI override: pin `pack_uuid` to a fixed value (hex form, with
    /// or without hyphens). Production builds derive this via UUIDv5
    /// over the canonical source descriptor.
    #[arg(long = "pack-uuid")]
    pack_uuid: Option<String>,

    /// Override the per-host request-rate limit (requests per second).
    /// Applies to every host fetched in this run. Default behavior:
    /// `tile.openstreetmap.org` and its subdomains are capped at 2
    /// req/sec (per the OSM tile usage policy); other hosts get 4
    /// req/sec. Raise this for hosts where you have a paid quota.
    #[arg(long = "rate-per-sec")]
    rate_per_sec: Option<f64>,

    /// Attribution string for the source's data, embedded as an ATTR
    /// extension section per the rawtiles spec § 7.3. UTF-8, no LF (LFs
    /// are reserved for separating per-source strings in multi-source
    /// packs, which Phase 1 doesn't support; use ';' or another separator
    /// for compound attribution within one source). May not be empty.
    ///
    /// For OSM-derived sources the OSM license requires "© OpenStreetMap
    /// contributors". Source-specific renderers (`OpenTopoMap`, `CyclOSM`,
    /// etc.) typically require additional credits — concatenate them.
    #[arg(long, value_parser = parse_attribution)]
    attribution: Option<String>,
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    let cancel = install_cancel_handler();
    match cli.command {
        Command::Make(args) => match run_make(args, cancel) {
            Ok(()) => ExitCode::SUCCESS,
            Err(BuildError::Cancelled) => {
                eprintln!("cancelled");
                // 130 = 128 + SIGINT (2), the conventional shell exit
                // code for a Ctrl-C-terminated process.
                ExitCode::from(130)
            }
            Err(e) => {
                eprintln!("error: {e}");
                ExitCode::FAILURE
            }
        },
        Command::Inspect(args) => match run_inspect_cli(&args) {
            Ok(()) => ExitCode::SUCCESS,
            Err(e) => {
                eprintln!("error: {e}");
                ExitCode::FAILURE
            }
        },
        Command::Debug(DebugCommand::Uuid(args)) => match run_debug_uuid_cli(&args) {
            Ok(()) => ExitCode::SUCCESS,
            Err(e) => {
                eprintln!("error: {e}");
                ExitCode::FAILURE
            }
        },
    }
}

fn run_inspect_cli(args: &InspectCliArgs) -> Result<(), InspectError> {
    let inspect_args = InspectArgs {
        path: args.pack.clone(),
    };
    let stdout = std::io::stdout();
    let mut handle = stdout.lock();
    run_inspect(&inspect_args, &mut handle)
}

fn run_debug_uuid_cli(args: &DebugUuidCliArgs) -> Result<(), BuildError> {
    let format = if args.bytes {
        DebugUuidFormat::Bytes
    } else {
        DebugUuidFormat::Uuid
    };
    let auth_headers = args
        .auth_header
        .iter()
        .map(|s| sources::url_template::AuthHeader::parse(s))
        .collect::<Result<Vec<_>, _>>()
        .map_err(BuildError::UrlTemplate)?;
    let auth_query = args
        .auth_query
        .iter()
        .map(|s| sources::url_template::AuthQuery::parse(s))
        .collect::<Result<Vec<_>, _>>()
        .map_err(BuildError::UrlTemplate)?;
    let debug_args = DebugUuidArgs {
        source: args.source.clone(),
        bbox: args.bbox,
        zoom_range: args.zoom,
        auth_headers,
        auth_query,
        format,
    };
    let stdout = std::io::stdout();
    let mut handle = stdout.lock();
    run_debug_uuid(&debug_args, &mut handle)
}

/// Install a SIGINT / Ctrl-C handler that flips the returned
/// `AtomicBool` to `true`. The build loop polls this between tile
/// operations and returns [`BuildError::Cancelled`] when set.
///
/// Calling `ctrlc::set_handler` twice in one process panics, so this
/// function must run exactly once per `main`. We return a guarded Arc
/// rather than a global static so unit tests (which drive `build()`
/// directly) can construct their own tokens without touching the
/// process-wide handler.
fn install_cancel_handler() -> Arc<AtomicBool> {
    let cancel = Arc::new(AtomicBool::new(false));
    let handler_cancel = Arc::clone(&cancel);
    let _ = ctrlc::set_handler(move || {
        handler_cancel.store(true, Ordering::Relaxed);
    });
    cancel
}

fn run_make(args: MakeArgs, cancel: Arc<AtomicBool>) -> Result<(), BuildError> {
    let pack_uuid_override = match args.pack_uuid {
        Some(s) => Some(parse_pack_uuid(&s)?),
        None => None,
    };
    let auth_headers = args
        .auth_header
        .iter()
        .map(|s| sources::url_template::AuthHeader::parse(s))
        .collect::<Result<Vec<_>, _>>()
        .map_err(BuildError::UrlTemplate)?;
    let auth_query = args
        .auth_query
        .iter()
        .map(|s| sources::url_template::AuthQuery::parse(s))
        .collect::<Result<Vec<_>, _>>()
        .map_err(BuildError::UrlTemplate)?;
    let rate_override = match args.rate_per_sec {
        Some(r) => Some(
            sources::rate_limit::RatePerSec::from_req_per_sec(r).ok_or_else(|| {
                BuildError::UrlTemplate(
                    sources::url_template::UrlTemplateError::InvalidRate(r.to_string()),
                )
            })?,
        ),
        None => None,
    };
    let opts = BuildOptions {
        source: args.source,
        out: args.out,
        bbox: args.bbox,
        zoom_range: args.zoom,
        auth_headers,
        auth_query,
        rate_override,
        timestamp_override: args.timestamp,
        pack_uuid_override,
        attribution: args.attribution,
        cancel: Some(cancel),
    };
    build(&opts)
}

fn parse_attribution(s: &str) -> Result<String, String> {
    validate_attribution_string(s).map_err(|e| e.to_string())?;
    Ok(s.to_string())
}

fn parse_bbox(s: &str) -> Result<BboxDeg, String> {
    let parts: Vec<&str> = s.split(',').collect();
    if parts.len() != 4 {
        return Err(format!(
            "expected 'minLon,minLat,maxLon,maxLat' (4 comma-separated values), got {} part(s)",
            parts.len(),
        ));
    }
    let parse_one = |idx: usize, name: &str| -> Result<f64, String> {
        parts[idx]
            .trim()
            .parse::<f64>()
            .map_err(|e| format!("invalid {name}: {e}"))
    };
    let min_lon = parse_one(0, "minLon")?;
    let min_lat = parse_one(1, "minLat")?;
    let max_lon = parse_one(2, "maxLon")?;
    let max_lat = parse_one(3, "maxLat")?;
    if !(-180.0..=180.0).contains(&min_lon) || !(-180.0..=180.0).contains(&max_lon) {
        return Err(format!(
            "longitudes must be in [-180, 180]; got minLon={min_lon}, maxLon={max_lon}",
        ));
    }
    if !(-90.0..=90.0).contains(&min_lat) || !(-90.0..=90.0).contains(&max_lat) {
        return Err(format!(
            "latitudes must be in [-90, 90]; got minLat={min_lat}, maxLat={max_lat}",
        ));
    }
    if min_lon >= max_lon {
        return Err(format!("minLon {min_lon} must be < maxLon {max_lon}"));
    }
    if min_lat >= max_lat {
        return Err(format!("minLat {min_lat} must be < maxLat {max_lat}"));
    }
    Ok(BboxDeg {
        min_lon,
        min_lat,
        max_lon,
        max_lat,
    })
}

fn parse_zoom(s: &str) -> Result<(u8, u8), String> {
    let parse_z = |part: &str, name: &str| -> Result<u8, String> {
        let z: u8 = part
            .trim()
            .parse()
            .map_err(|e| format!("invalid {name}: {e}"))?;
        // 23 is ZOOM_OFFSETS_COUNT - 1 (the spec's max addressable zoom).
        if z > 23 {
            return Err(format!("{name} must be in [0, 23]; got {z}"));
        }
        Ok(z)
    };
    if let Some((a, b)) = s.split_once('-') {
        let min = parse_z(a, "min zoom")?;
        let max = parse_z(b, "max zoom")?;
        if min > max {
            return Err(format!("min zoom {min} must be ≤ max zoom {max}"));
        }
        Ok((min, max))
    } else {
        let z = parse_z(s, "zoom")?;
        Ok((z, z))
    }
}

fn parse_pack_uuid(s: &str) -> Result<[u8; 16], BuildError> {
    // Accept either "0123...ef" (32 hex chars) or
    // "01234567-89ab-...-...-...". Strip hyphens, then hex-decode.
    let stripped: String = s.chars().filter(|c| *c != '-').collect();
    if stripped.len() != 32 {
        return Err(BuildError::InvalidPackUuid(s.to_string()));
    }
    let mut out = [0_u8; 16];
    for (i, byte) in out.iter_mut().enumerate() {
        let nibble_hi = hex_nibble(stripped.as_bytes()[i * 2])
            .ok_or_else(|| BuildError::InvalidPackUuid(s.to_string()))?;
        let nibble_lo = hex_nibble(stripped.as_bytes()[i * 2 + 1])
            .ok_or_else(|| BuildError::InvalidPackUuid(s.to_string()))?;
        *byte = (nibble_hi << 4) | nibble_lo;
    }
    if out == [0_u8; 16] {
        // Spec invariant: pack_uuid must be non-zero.
        return Err(BuildError::InvalidPackUuid(s.to_string()));
    }
    Ok(out)
}

fn hex_nibble(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::{parse_bbox, parse_pack_uuid, parse_zoom};

    #[test]
    fn parse_uuid_with_hyphens() {
        let s = "4e72f962-6632-4538-8e0a-7eab63350f3f";
        let parsed = parse_pack_uuid(s).unwrap();
        assert_eq!(
            parsed,
            [
                0x4e, 0x72, 0xf9, 0x62, 0x66, 0x32, 0x45, 0x38, 0x8e, 0x0a, 0x7e, 0xab, 0x63, 0x35,
                0x0f, 0x3f
            ],
        );
    }

    #[test]
    fn parse_uuid_without_hyphens() {
        let s = "4e72f962663245388e0a7eab63350f3f";
        let parsed = parse_pack_uuid(s).unwrap();
        assert_eq!(parsed[0], 0x4e);
        assert_eq!(parsed[15], 0x3f);
    }

    #[test]
    fn parse_uppercase_hex() {
        let parsed = parse_pack_uuid("ABCDEF0123456789ABCDEF0123456789").unwrap();
        assert_eq!(parsed[0], 0xab);
        assert_eq!(parsed[15], 0x89);
    }

    #[test]
    fn parse_rejects_short_input() {
        assert!(parse_pack_uuid("deadbeef").is_err());
    }

    #[test]
    fn parse_rejects_long_input() {
        assert!(parse_pack_uuid(&"a".repeat(33)).is_err());
    }

    #[test]
    fn parse_rejects_non_hex_chars() {
        assert!(parse_pack_uuid("zzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzz").is_err());
    }

    #[test]
    fn parse_rejects_all_zero() {
        assert!(parse_pack_uuid("00000000000000000000000000000000").is_err());
        assert!(parse_pack_uuid("00000000-0000-0000-0000-000000000000").is_err());
    }

    #[test]
    fn parse_bbox_accepts_normal_form() {
        let b = parse_bbox("-0.15,51.49,-0.10,51.52").unwrap();
        assert!((b.min_lon - -0.15).abs() < 1e-12);
        assert!((b.min_lat - 51.49).abs() < 1e-12);
        assert!((b.max_lon - -0.10).abs() < 1e-12);
        assert!((b.max_lat - 51.52).abs() < 1e-12);
    }

    #[test]
    fn parse_bbox_tolerates_whitespace_around_components() {
        let b = parse_bbox(" -1.0 , -2.0 , 3.0 , 4.0 ").unwrap();
        assert!((b.min_lon - -1.0).abs() < 1e-12);
        assert!((b.max_lat - 4.0).abs() < 1e-12);
    }

    #[test]
    fn parse_bbox_rejects_wrong_part_count() {
        assert!(parse_bbox("0,0,0").is_err());
        assert!(parse_bbox("0,0,0,0,0").is_err());
    }

    #[test]
    fn parse_bbox_rejects_out_of_range() {
        assert!(parse_bbox("-181,0,1,1").is_err()); // lon < -180
        assert!(parse_bbox("0,-91,1,1").is_err()); // lat < -90
    }

    #[test]
    fn parse_bbox_rejects_inverted_corners() {
        assert!(parse_bbox("10,0,5,1").is_err()); // minLon > maxLon
        assert!(parse_bbox("0,10,1,5").is_err()); // minLat > maxLat
    }

    #[test]
    fn parse_zoom_single() {
        assert_eq!(parse_zoom("8").unwrap(), (8, 8));
    }

    #[test]
    fn parse_zoom_range() {
        assert_eq!(parse_zoom("6-12").unwrap(), (6, 12));
    }

    #[test]
    fn parse_zoom_zero_to_zero() {
        assert_eq!(parse_zoom("0").unwrap(), (0, 0));
        assert_eq!(parse_zoom("0-0").unwrap(), (0, 0));
    }

    #[test]
    fn parse_zoom_rejects_inverted_range() {
        assert!(parse_zoom("12-6").is_err());
    }

    #[test]
    fn parse_zoom_rejects_above_max() {
        assert!(parse_zoom("24").is_err());
        assert!(parse_zoom("0-24").is_err());
        // z=23 is the max allowed (ZOOM_OFFSETS_COUNT - 1).
        assert_eq!(parse_zoom("23").unwrap(), (23, 23));
        assert_eq!(parse_zoom("0-23").unwrap(), (0, 23));
    }

    #[test]
    fn parse_zoom_rejects_garbage() {
        assert!(parse_zoom("abc").is_err());
        assert!(parse_zoom("3-x").is_err());
    }
}
