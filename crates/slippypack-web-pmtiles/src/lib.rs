//! slippypack-web-pmtiles — PMTiles reader, lazy-loaded WASM module.
//!
//! See `PLAN.md` § Phase 6 — Source picker. Module size budget: ≤ 300 KB
//! gzipped. The TS Worker dynamically imports this module only when the
//! user picks the PMTiles source kind. The PMTiles upstream crate's
//! documented backends are all native, so a custom `OpfsAsyncBackend`
//! implementing `pmtiles::AsyncBackend` lives here.
//!
//! Workspace skeleton only — no functional code has landed.
