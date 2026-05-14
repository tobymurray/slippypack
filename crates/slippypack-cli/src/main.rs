//! `slippypack` — native CLI for building offline `.upack` tile packs.
//!
//! Phase 1 first slice: `make --source synthetic --out <path>` builds a
//! pack from the embedded synthetic fixture and writes it via an
//! atomic `.upack.partial` → rename. Other source kinds (URL templates,
//! MBTiles, PMTiles, `dir://`) land in Phase 1.x; vector sources
//! (`pbf`, `style`) land in Phase 2.

use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, Subcommand};

mod build;
mod sources;

use build::{BuildError, BuildOptions, build};

#[derive(Parser)]
#[command(name = "slippypack", version, about = "Build offline .upack map packs")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Build a `.upack` from a tile source.
    Make(MakeArgs),
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
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    match cli.command {
        Command::Make(args) => match run_make(args) {
            Ok(()) => ExitCode::SUCCESS,
            Err(e) => {
                eprintln!("error: {e}");
                ExitCode::FAILURE
            }
        },
    }
}

fn run_make(args: MakeArgs) -> Result<(), BuildError> {
    let pack_uuid_override = match args.pack_uuid {
        Some(s) => Some(parse_pack_uuid(&s)?),
        None => None,
    };
    let opts = BuildOptions {
        source: args.source,
        out: args.out,
        timestamp_override: args.timestamp,
        pack_uuid_override,
    };
    build(&opts)
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
    use super::parse_pack_uuid;

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
}
