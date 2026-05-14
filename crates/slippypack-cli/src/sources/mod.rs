//! Raster tile sources for the CLI's `make` subcommand.
//!
//! Each source produces raw PNG/JPEG bytes for a given `(z, x, y)`
//! coordinate; the CLI pipeline then runs decode → quantise → format
//! to assemble the pack.
//!
//! Phase 1 first slice ships only the [`synthetic`] source. The
//! `url_template` source (HTTPS URL templates) and the `mbtiles`,
//! `pmtiles`, `dir` sources land in Phase 1.x.

pub mod synthetic;
