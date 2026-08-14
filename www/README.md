# www — the browser front-end

Renders a region into a `.rawtiles` pack, in the browser, with no server
and nothing uploaded. This is PLAN.md Phase 4's "bare-minimum browser
harness", built out far enough to produce a real pack.

**What it is not, yet:** there is no region picker (Phase 5), no source
picker (cut — there is one archive), no PWA shell or offline launch
(Phase 7), and no OPFS streaming (Phase 8, needed before metro-sized
packs). The region is a bbox you type in.

## Running it

```sh
npm install
npm run build:wasm        # needs the wasm32 target + wasm-bindgen-cli 0.2.127
npm run vendor            # copies the runtime deps out of node_modules/
npm run serve             # then open http://localhost:8080
```

`pkg/` and `vendor/` are both gitignored build products.

## Deploying it

```sh
npm run build:site        # assembles dist/ — the whole site, ~1.4 MB
```

`dist/` is static and self-contained: no server, no bundler, nothing built
at serve time. `.github/workflows/pages.yml` runs exactly these scripts
and publishes it to GitHub Pages on a push to `main`.

**Why `vendor/` exists.** The page loads a handful of files out of
`node_modules/`, which is neither in the repo nor deployable — importing
from it works locally and 404s everywhere else. `scripts/vendor.sh`
copies them into `vendor/`, so the dev layout and the deployed layout are
the same layout.

It copies more than the obvious entrypoints, and the reason is worth
knowing: `maplibre-gl.mjs` imports `maplibre-gl-shared.mjs`, and MapLibre
also spawns `maplibre-gl-worker.mjs` as a **Worker**. A missing worker
does not throw — tile parsing simply never happens, the map never fires
`idle`, and the build hangs with no error at all. So `vendor.sh` scans
the vendored files for every `./*.mjs` reference rather than only static
imports, and fails if one is unaccounted for.

## Checking it

`verify-e2e.mjs` drives the page in a real Firefox, builds a pack, and
writes it out so the Rust CLI can be the thing that decides whether it is
valid:

```sh
npm run verify
cargo run -p slippypack-cli -- inspect ../target/browser.rawtiles
```

It reports the footer CRC-32 as well, because that is what MapManager
verifies on the watch — better to learn it here than over USB. Run it
twice and `cmp` the outputs: the build is deterministic, so the two files
should be byte-identical.

## How it works, and the two things that are load-bearing

```
Protomaps planet PMTiles  (HTTP range reads, no server of ours)
  └─ MapLibre GL JS + watch-style.json    ← renders 16×16 tile BLOCKS
       └─ canvas readback, sliced to 128 px tiles
            └─ slippypack-web (WASM)      ← quantise, RLE, container, UUID
                 └─ .rawtiles → save to the watch over USB
```

**Render in blocks, not per tile.** One map render per pack tile takes
105.7 s for a trail-sized pack and misses the usability criterion;
16 × 16 blocks sliced afterwards take 9.2 s for the same pack. The
per-tile cost is fixed overhead (~41 ms/render) and does not improve with
caching, so this is not a tuning knob — it is the difference between the
product working and not. Measured in
`Docs/Investigations/2026-08-14-x4-browser-render/`.

**Feed tiles in ascending `(z, x, y)`, x-major.** The pack's source
`content_hash` is a hash of the tile *stream*, so the order is part of
the pack's identity, and the writer sorts by `(z, x, y)` — which makes
**x** the major axis. Buffering a block-*row* is the intuitive thing and
it is wrong; `render.js` buffers a block-*column*. The WASM builder
rejects out-of-order tiles rather than quietly producing a pack with a
different `content_hash` (DECISIONS.md B-002), which is how this bug was
caught the first time.

## Nothing here decides what a pack contains

Quantisation, compression, hashing, container layout and `pack_uuid`
derivation are all in `slippypack-core`, shared with the CLI, so the two
front-ends cannot drift (DECISIONS.md B-001). This directory renders
pixels and marshals them across the WASM boundary.

## Attribution is not optional

`watch-style.json` renders OpenStreetMap data via Protomaps' basemap.
The pack carries an `ATTR` extension section, and the page shows the
attribution too. The watch must display it for at least 5 s on map open
— see `MAP_COMPLIANCE_APPENDIX.md` § 4. That part is still 🔨 on the
watch side.
