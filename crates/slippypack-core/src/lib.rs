//! slippypack-core — shared library for the `.upack` writer pipeline.
//!
//! See `PLAN.md` at the repo root for the full design. The eventual module
//! layout (per PLAN.md § The load-bearing observation: shared Rust core):
//!
//! - [`decode`] — PNG / JPEG → RGB888 via the `image` crate. **Landed.**
//! - [`quantise`] — RGB888 → ABGR2222 (integer-only, cross-platform deterministic). **Landed.**
//! - [`format`] — `.upack` byte-layout primitives (header / tile index /
//!   extensions / CRC32). The `TileWriter` trait + `UpackWriter` land in a
//!   follow-up slice. **Primitives landed.**
//! - [`projection`] — Web Mercator (Local Linear lands later in Phase 0). **Mercator landed.**
//! - [`identity`] — UUIDv5 derivation from the canonical source descriptor.
//!   The source-mtime / Last-Modified accumulator for `build_timestamp` lives
//!   per-front-end since it's I/O-shaped. **Canonical descriptor + derivation landed.**

pub mod decode;
pub mod format;
pub mod identity;
pub mod projection;
pub mod quantise;
