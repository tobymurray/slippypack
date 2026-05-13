# slippypack

Build offline `.upack` map packs.

> **Status:** plan + workspace skeleton. The Cargo workspace compiles (`cargo check --workspace`) but no functional code has shipped yet. Phase 0 (the core library) is the next deliverable.

See [PLAN.md](PLAN.md) for the design and phasing.

## What this is

A Rust toolkit for building offline tile packs in the `.upack` format. One core library, two front-ends — a native CLI and a browser PWA — both writing byte-identical packs. The format spec lives in the sibling `una-sdk` watch firmware project; slippypack is the canonical writer.

## Repository layout

| Path | Role |
|---|---|
| `crates/slippypack-core/` | Shared library: `.upack` writer + reader, ABGR2222 quantiser, projection math, UUIDv5 identity derivation |
| `crates/slippypack-cli/` | Native CLI binary (`slippypack make ...`) |
| `crates/slippypack-web/` | WASM front-end glue (base module loaded by every PWA build) |
| `crates/slippypack-web-mbtiles/` | MBTiles reader; lazy-loaded WASM module |
| `crates/slippypack-web-pmtiles/` | PMTiles reader; lazy-loaded WASM module |
| `www/` | TypeScript shell + assets for the PWA (lands in Phase 4) |
| `PLAN.md` | Design, phasing, and spec-level details |

## Building

```sh
cargo check --workspace
```

The skeleton compiles; no functional code has landed yet. Each crate's `src/` contains a placeholder pointing at the PLAN.md section that defines its eventual contents.

## Status

See [PLAN.md § Phasing](PLAN.md#phasing). The first user-facing milestone is Phase 1's first slice (`--source synthetic` and `--source https://.../{z}/{x}/{y}.png`), landing ~2.5–3 weeks after Phase 0 work starts.
