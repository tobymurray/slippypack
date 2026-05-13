//! slippypack-core — shared library for the `.upack` writer pipeline.
//!
//! See `PLAN.md` at the repo root for the full design. The eventual module
//! layout (per PLAN.md § The load-bearing observation: shared Rust core):
//!
//! - `decode`     — PNG / JPEG → RGB888 via the `image` crate.
//! - `quantise`   — RGB888 → ABGR2222 (integer-only, cross-platform deterministic).
//! - `format`     — `.upack` `TileWriter` trait + `UpackWriter` implementation.
//! - `reader`     — `.upack` parser (round-trip tests; future "open existing pack").
//! - `projection` — Web Mercator and Local Linear math.
//! - `identity`   — UUIDv5 derivation from the canonical source descriptor,
//!                  source-mtime / Last-Modified accumulator for `build_timestamp`.
//!
//! None of these modules exist yet — this crate is part of the workspace
//! skeleton committed before Phase 0 starts. `cargo check --workspace`
//! should pass against this stub.
