//! slippypack-core — shared library for the `.upack` writer pipeline.
//!
//! See `PLAN.md` at the repo root for the full design. The eventual module
//! layout (per PLAN.md § The load-bearing observation: shared Rust core):
//!
//! - `decode` — PNG / JPEG → RGB888 via the `image` crate. (Phase 0)
//! - [`quantise`] — RGB888 → ABGR2222 (integer-only, cross-platform deterministic). **Landed.**
//! - `format` — `.upack` `TileWriter` trait + `UpackWriter` implementation. (Phase 0)
//! - `reader` — `.upack` parser (round-trip tests; future "open existing pack"). (Phase 0)
//! - `projection` — Web Mercator and Local Linear math. (Phase 0)
//! - `identity` — UUIDv5 derivation from the canonical source descriptor;
//!   source-mtime / Last-Modified accumulator for `build_timestamp`. (Phase 0)

pub mod quantise;
