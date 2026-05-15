# slippypack

Build offline `.rawtiles` map packs.

> **Status:** in flight. `slippypack-core` (format writer + reader, decode, quantise, Web Mercator projection, UUIDv5 identity) and the first slice of `slippypack-cli` (`--source synthetic` + URL templates, `make` and `debug uuid` subcommands, SIGINT/atomic write) have landed. The `slippypack-web*` crates are skeleton stubs awaiting Phase 4. ~280 tests passing.

See [PLAN.md](PLAN.md) for the design and phasing.

## What this is

A Rust toolkit for building offline tile packs in the `.rawtiles` format. One core library, two front-ends — a native CLI and a browser PWA — both writing byte-identical packs. The `.rawtiles` byte format is defined by the standalone [rawtiles spec](https://github.com/tobymurray/rawtiles); slippypack is one writer against that contract.

## Repository layout

| Path | Role |
|---|---|
| `crates/slippypack-core/` | Shared library: `.rawtiles` writer + reader, ABGR2222 quantiser, projection math, UUIDv5 identity derivation |
| `crates/slippypack-cli/` | Native CLI binary (`slippypack make ...`) |
| `crates/slippypack-web/` | WASM front-end glue (base module loaded by every PWA build) |
| `crates/slippypack-web-mbtiles/` | MBTiles reader; lazy-loaded WASM module |
| `crates/slippypack-web-pmtiles/` | PMTiles reader; lazy-loaded WASM module |
| `www/` | TypeScript shell + assets for the PWA (lands in Phase 4) |
| `PLAN.md` | Design, phasing, and slippypack-specific details |

The `.rawtiles` byte-level specification lives in its own repository at [github.com/tobymurray/rawtiles](https://github.com/tobymurray/rawtiles).

## Building

```sh
cargo check --workspace
cargo test --workspace
```

The CLI is buildable today:

```sh
cargo run -p slippypack-cli -- make --source synthetic --out test.rawtiles
```

The `slippypack-web*` crates are skeleton stubs — their `src/lib.rs` files contain only a module-level docstring pointing at the PLAN.md section that defines their eventual contents.

## Status

See [PLAN.md § Phasing](PLAN.md#phasing). Phase 0 + the first slice of Phase 1 + the `debug uuid` helper from Phase 1.x have landed. Next on the CLI track: Phase 1.x's remaining source kinds (`dir`, `mbtiles`, `pmtiles`) and multi-source layering. The PWA track (Phase 4 onward) starts when Phase 0 is needed in the browser.
