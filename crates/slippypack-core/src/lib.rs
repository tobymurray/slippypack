//! slippypack-core — shared library for the `.upack` writer pipeline.
//!
//! See `PLAN.md` at the repo root for the full design. The eventual module
//! layout (per PLAN.md § The load-bearing observation: shared Rust core):
//!
//! - [`decode`] — PNG / JPEG → RGB888 via the `image` crate. **Landed.**
//! - [`quantise`] — RGB888 → ABGR2222 (integer-only, cross-platform deterministic). **Landed.**
//! - `format` — `.upack` `TileWriter` trait + `UpackWriter` implementation. (Phase 0)
//! - `reader` — `.upack` parser (round-trip tests; future "open existing pack"). (Phase 0)
//! - [`projection`] — Web Mercator (Local Linear lands later in Phase 0). **Mercator landed.**
//! - [`identity`] — UUIDv5 derivation from the canonical source descriptor.
//!   The source-mtime / Last-Modified accumulator for `build_timestamp` lives
//!   per-front-end since it's I/O-shaped. **Canonical descriptor + derivation landed.**

pub mod decode;
pub mod identity;
pub mod projection;
pub mod quantise;
