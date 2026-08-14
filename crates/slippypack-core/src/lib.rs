//! slippypack-core — shared library for the `.rawtiles` writer pipeline.
//!
//! See `PLAN.md` at the repo root for the full design. The eventual module
//! layout (per PLAN.md § The load-bearing observation: shared Rust core):
//!
//! - [`decode`] — PNG / JPEG → RGB888 via the `image` crate. **Landed.**
//! - [`quantise`] — RGB888 → ABGR2222 (integer-only, cross-platform deterministic). **Landed.**
//! - [`format`] — `.rawtiles` byte-layout primitives (header / tile index /
//!   extensions / CRC32) **plus** the `TileWriter` trait and concrete
//!   `RawtilesWriter` implementation. **Primitives + writer landed.**
//! - [`projection`] — Web Mercator (Local Linear lands later in Phase 0). **Mercator landed.**
//! - [`identity`] — UUIDv5 derivation from the canonical source descriptor.
//!   The source-mtime / Last-Modified accumulator for `build_timestamp` lives
//!   per-front-end since it's I/O-shaped. **Canonical descriptor + derivation landed.**

// `extern crate alloc;` makes the `alloc` crate's types (Box, Vec, etc) available
// under the `alloc::*` path even though slippypack-core is currently std-compiled.
// Modules that target the eventual `no_std + alloc` switch use `alloc::*` paths
// today so the transition is mechanical when it lands.
extern crate alloc;

pub mod builder;
pub mod decode;
pub mod format;
pub mod identity;
pub mod projection;
pub mod quantise;
