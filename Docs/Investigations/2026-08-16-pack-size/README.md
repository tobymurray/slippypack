# 2026-08-16 — Pack size: the codec, the pyramid, and the premise

**Subject.** A 25 km region at z11–16 builds to **65 MB** (62 MiB). That is large enough that the
question was put plainly: is there a realistic path to smaller packs, *even if the spec
has to change*, and is there a compression option that does not cost the watch more than
it is worth?

**Standing.** `MAP_CARTOGRAPHY_SPEC.md` and the `.rawtiles` spec are **this project's own
inventions**, written to ground a design before anything existed to measure. Nothing here
treats them as fixed. Where a measurement contradicts a spec decision, the spec is what
moves — and three of them are contradicted below, one of them by the renderer that
already shipped.

**Repos read:** `slippypack` @ `6488424`, `watch-apps` @ `fdb9df6`.

**Measured on:** `big.rawtiles`, built by the deployed page on 2026-08-16 — 18,169 tiles,
256 px, z11–16, bbox `-76.26755,44.40142,-75.63645,44.85058` (rural eastern Ontario,
~50 × 50 km). **One pack, one landscape.** Dense urban tiles have more edges and will
compress worse; the ratios should survive, the absolute MB will not.

**Everything here runs from `scripts/`.** `tiledump.rs` decodes a pack to one raw
ABGR2222 file per tile (it is the only part that needs the workspace);
`measure-compression.py` and `measure-decode-cost.py` measure candidate encodings over
those files; `vector-size.mjs` reads the same region out of the Protomaps archive the
raster build renders from. Raw output is in `data/`.

```sh
# tiledump.rs + tiledump.Cargo.toml build as their own tiny crate against
# slippypack-core; it is the only step that needs the workspace.
cargo run --release --bin tiledump -- big.rawtiles /tmp/tiles   # writes /tmp/tiles/*.raw
python3 scripts/measure-decode-cost.py /tmp                     # data/codecs-and-zoom.txt
python3 scripts/measure-compression.py /tmp 1200                # data/encodings.txt
python3 scripts/measure-variants.py /tmp                        # data/variants.txt
(cd ../../../www && node ../Docs/Investigations/2026-08-16-pack-size/scripts/vector-size.mjs)
                                                                # data/vector-tiles.txt
```

`measure-compression.py` needs Python 3.14 (`compression.zstd`) and Pillow;
`measure-variants.py` shells out to `lz4`. Sample sizes differ per script — 1,212 tiles,
826 and 404 — which is why the same row differs by ~2% between tables (RLE8 reads 3,497
B/tile in one and 3,576 in another). Ratios are what carry.

Every candidate is measured **per tile**, because the watch reads one tile at a time. An
encoding that only wins across the whole file is not a candidate for this format; it is a
different format.

---

## Verdict

**Yes, and the cheap win is not the interesting one.** Swapping RLE8 for deflate is 2.5×
for about 3 KB of decoder. But the codec is not where the size is: **the pyramid is**, and
underneath the pyramid is the premise — that a watch cannot render, so it must be shipped
pixels. That premise is what costs an order of magnitude, and the evidence that it is
still true is weaker than it was when the spec was written.

| | 25 km pack | vs today | what it costs the watch |
|---|---|---|---|
| RLE8 — today | **65 MB** | 1.0× | ~nothing; a few bytes of decoder state |
| deflate, 4 KB window | **26 MB** | 2.5× smaller | ~3 KB code + 4 KB window |
| zstd -19 | 20 MB | 3.2× smaller | ~60 KB code — 20× the decoder for 22% more |
| deflate + z16 only over a 5 km core | **12 MB** | 5.4× smaller | nothing; it is a product decision |
| the same region as **vector** (z14 MVT) | **3.1 MB** | **21× smaller** | a renderer it does not have |

---

## F1 — RLE8 is the weakest decision in the format, and it was never priced

RLE8 leaves **3,576 B** of a 65,536 B tile (5.5%). Deflate leaves **1,445 B** (2.2%) for
the same pixels. The spec chose RLE8 for a real reason — § 9.11 gives it a *canonical*
encoding, so two independent writers produce byte-identical packs — but the price of that
guarantee was never written down. It is **2.5× the size of every pack ever built**.

| codec | B/tile | vs RLE8 | 25 km pack | decoder |
|---|---|---|---|---|
| RLE8 (today) | 3,576 | 1.00× | 65 MB | a few bytes of state |
| LZ4 -12 | 2,151 | 0.60× | 39 MB | ~1 KB, no window |
| deflate -6, 4 KB window | 1,666 | 0.47× | 30 MB | ~3 KB + 4 KB |
| **deflate -9, 4 KB window** | **1,445** | **0.40×** | **26 MB** | **~3 KB + 4 KB** |
| zstd -9 | 1,461 | 0.42× | 27 MB | ~60 KB + window |
| zstd -19 | 1,096 | 0.31× | 20 MB | ~60 KB + window |

**The reserved codec slots are the wrong two.** The format reserves `QOI = 2` and
`LZ4 = 3` and does not reserve deflate. LZ4 measured **worse than deflate on the axis that
matters** (2,151 B/tile against 1,445) while saving ~2 KB of decoder — and QOI is designed
for RGB photographs, so on 8-bit palette indices it would have to expand to RGB first,
which is a different pixel format, not a different codec. Two slots were reserved on
intuition and the measurement supports neither.

**zstd is not worth it here.** It wins 22% over deflate for 20× the decoder code, and its
`-19` encode cost is 33 s per 826 tiles — on an 18,169-tile build that is ~12 minutes
added to a 10-minute build. Deflate -9 adds ~14%.

## F2 — A 4 KB window is as good as 32 KB, which is what makes deflate viable on a watch

| window | B/tile |
|---|---|
| 32 KB | 1,421 |
| **4 KB** | **1,401** |

**1.4% apart.** A tile's redundancy is row-to-row — 256 bytes apart at `tile_dim` 256 — so
a 4 KB window already spans sixteen rows of context and there is almost nothing further
back worth referencing. This is the finding that decides the recommendation: `inflate`
with a 4 KB window is ~3 KB of code and 4 KB of RAM, against a GUI budget that today
cannot afford a *second* 64 KiB tile slot (F6).

Two things that sound clever and are not, both measured:

- **PNG-style row filtering made it worse** (1,617 B/tile against 1,445). Filters model
  continuous tone; these are palette indices, where `index − index` is meaningless.
- **Repacking to 4 bits per pixel** — only 11 palette codes are ever used, so 3.46 bits
  would do — is *worse* after entropy coding (zstd: 1,142 against 1,089). Nibble packing
  destroys the byte alignment the match finder works on. It costs 2× the raw size to
  save nothing.

zstd dictionaries also lost (1,160 B/tile with a 4 KB dictionary against 1,089 without):
64 KiB tiles are large enough to carry their own context.

## F3 — The pyramid is the cost, and the deepest level is always most of it

| zoom | tiles | deflate | share |
|---|---|---|---|
| 11 | 20 | 0.2 MB | 0.7% |
| 12 | 72 | 0.3 MB | 1.3% |
| 13 | 240 | 1.0 MB | 3.9% |
| 14 | 900 | 3.0 MB | 11.5% |
| 15 | 3,481 | 6.8 MB | 26.5% |
| **16** | **13,456** | **14.5 MB** | **56.1%** |

Each level costs 4× the one above, so **the deepest level is ~56% of any pack and the
deepest two are ~83%** — independent of region, because it is just the geometry of a
quadtree. Adding z17 to this pack would add ~58 MB to a 26 MB pack.

Which means the cheapest large win in the whole investigation requires no format change at
all: **the deepest zoom does not have to cover the whole region.** Keeping z16 only within
5 km of the centre (4% of the area) while z11–15 cover everything takes the deflate pack
from 25.9 MB to **11.9 MB**. Per-zoom tile sets are already sparse, so a pack like this is
expressible today; nothing in the format has to change, only what the builder chooses to
put in it. Note the deeper levels also compress *better* per tile (1,080 B at z16 against
8,651 B at z11) — they are emptier — so the pyramid's cost is entirely tile count.

## F4 — The same region as vector is 3.1 MB, and it does not have a deepest level

Read straight from `data.source.coop/protomaps/openstreetmap/v4.pmtiles`, the archive the
raster build already renders from — so this is like-for-like on source data:

| vector source | tiles over the region | mean | total |
|---|---|---|---|
| z12 MVT | 72 | 18.3 KB | 1.3 MB |
| z13 MVT | 240 | 7.9 KB | 1.9 MB |
| **z14 MVT** | **900** | **3.3 KB** | **3.1 MB** |

**3.1 MB against 26 MB, and the comparison understates it**, because z14 geometry
over-zooms: it serves z15, z16, z17 and beyond by scaling, where the raster pyramid must
store each level separately at 4× the last. Raster cost scales as `4^zmax × area`; vector
scales with feature count, i.e. with area alone. That is the entire structural difference,
and no codec closes it.

A watch-specific vector format would be smaller again — the MVT above carries attributes,
names and layers this cartography never draws — but 3.1 MB is the honest measured number
and the tighter figure would be a guess.

## F5 — Whole-file CRC-32 makes size a startup cost, not just a storage cost

`MapManager` verifies each pack by reading it end to end, sustaining **2.9 MB/s on
hardware** (measured 2026-08-13: 160.5 MiB of packs in 56.6 s). So:

| pack | verification before it can draw |
|---|---|
| 65 MB (today) | **~22 s** |
| 26 MB (deflate) | ~9 s |
| 12 MB (deflate + core-only z16) | ~4 s |
| 3 MB (vector) | ~1 s |

The trailing CRC-32 is a whole-file integrity check, so **the entire pack must be read
before any of it is trusted** — and trust gates rendering. Size is therefore paid twice:
once in flash, once in seconds of `verifying map` before the first tile appears. A
per-tile or per-block digest would let a pack draw immediately and verify lazily, and
would localise corruption to the tile that has it instead of condemning the file. This is
a spec change worth making independently of anything else here.

## F6 — `tile_dim`: the spec was right, the renderer disagreed, and the renderer won

The spec (§ 7) specifies **128**, on a RAM argument: a 256 px ABGR2222 tile is 64 KiB, and
a 240 px viewport can straddle four of them = 256 KiB, against a ~600 K GUI budget.
`MapKit` hardcodes **256** (`MapMath::TILE_DIM = 256`, `TILE_SHIFT = 8`) and `PackCatalog`
silently rejects anything else as *"no map for here"*.

The renderer's own measurements support the spec, not the renderer:

- `TileCache::SLOTS = 1`, and that is a measurement, not a choice: 2 slots overflow
  `.bss` by 33,884 B, 4 slots by 165,044 B. One 64 KiB tile is all the RAM there is.
- At 128 px the same 64 KiB holds **four** tiles, and the worst-case mosaic for a 240 px
  viewport is 9 tiles = 144 KiB against 256 KiB at 256 px.
- Decode is 6–9 ms per 256 px tile; a 128 px tile is a quarter of the pixels.

So the spec's § 7 reasoning survives contact with the hardware and the implementation does
not — but the implementation is what ships, so every pack this project builds for a real
watch is 256. This is the clearest case in the format of **a decision made in a document
losing to a decision made in code**, and it should be resolved in one direction or the
other rather than left as a picker in the browser UI (which is where it currently lives).

## F7 — ABGR2222 at one byte per pixel is the premise that survives

Worth stating plainly because it is the one place the spec's instinct is vindicated by the
code: `MapTileView` calls
`HAL::lcd().blitCopy(pixels, Bitmap::ABGR2222, source, blitRect, 255, false)`. The decoded
tile is handed to the display HAL **with no conversion at all**. A 4-bit indexed on-disk
format would be free to exist — expansion would happen in the same decode loop that
already runs — but the *RAM* cost of a cached tile is fixed at `tile_dim²` bytes by the
blit path, and no on-disk cleverness changes it. Which is another way of saying the only
real lever on tile RAM is `tile_dim`, i.e. F6.

(For the record: 11 palette codes carry ~3.46 bits of entropy per pixel, and RLE8 already
gets the file down to 0.44 bits per pixel, deflate to 0.18. The pixels are not where the
waste is.)

## F8 — The determinism guarantee is expensive, and it is protecting less than it looks

Canonical RLE8 exists so two writers produce byte-identical packs. Deflate has no
canonical output: two encoders, or one encoder at two levels, produce different bytes for
identical pixels. But note what actually depends on it:

- **`pack_uuid` does not.** Identity derives from the descriptor and the *pre-compression*
  content hash (`Source::Style::content_hash` is the rendered RGB888 stream, I-011). Two
  packs with the same pixels and different deflate encoders derive the **same UUID**.
- **Same-writer reproducibility does not.** The CLI and the browser each produce stable
  bytes for stable inputs; `verify-e2e`'s run-twice-and-`cmp` check still passes.
- Only **cross-writer byte-identity** breaks — and both current writers are Rust calling
  the same crate, so it survives in practice and fails only for a hypothetical third
  implementation.

So the spec can keep everything anyone actually relies on by saying "any deflate stream
that decodes to the declared pixels is valid" and dropping byte-identity to a
*recommendation* with a named encoder and level. Holding the stronger guarantee costs
2.5× on every pack, forever, to protect a writer that does not exist.

## F9 — 5% of tiles are duplicates of another tile

923 of 18,169 tiles (5.1%) are byte-identical to another tile; the most repeated one
appears **463 times** (uniform water or landuse). The index is `(offset, length)` per
tile, so two entries can point at one blob and the reader as written does not object —
`validate_tile_lengths` only constrains uncompressed tiles. Whether the *spec* permits
overlapping tile blobs needs checking in the `rawtiles` repo, which was not read here. A
writer-side change, worth ~5% on this landscape and considerably more on coastal or
heavily-forested regions.

---

## What a vector map on this watch would actually look like

Not hypothetical in the way it sounds. Three of the four hard parts already exist.

**The blit path does not change.** A vector renderer's job is to fill a 256 × 256
ABGR2222 buffer — which is exactly what `TileCache` already hands `blitCopy`. Swapping
"decode RLE8 into this buffer" for "rasterise geometry into this buffer" leaves
`MapTileView`, `MapSession`, `PackSelection`, `TraceBuffer` and the whole mosaic/scroll
path untouched. The seam is one function wide.

**A Rust renderer on this watch is already proven.** `RustGuiPoc` draws its GUI with
`embedded-graphics` into the ABGR2222 framebuffer through the SDK's `CustomGUI` entry
point — `no_std`, on the supported app path, no TouchGFX. Polygon fill and thick-line
stroke against a byte-per-pixel target is the same class of work.

**The data is small and the parser is small.** MVT is protobuf varints and zigzag deltas;
a reader that keeps only geometry for a handful of layer classes should be a few KB of code —
smaller than the zstd decoder rejected in F1.

**What is genuinely hard, in order:**

1. **Labels.** Everything above is geometry. Text needs glyphs, shaping, collision
   detection and placement — the part MapLibre spends most of its complexity on. The
   honest first version draws no labels at all, which for a 240 px watch face during an
   activity may be the right product anyway.
2. **Rasterisation cost per tile, which is unmeasured.** RLE8 decode is 6–9 ms. A scanline
   fill of a z14 tile's polygons plus stroked roads will be some multiple of that, and if
   it lands past ~50 ms it starts eating the 1 Hz fix cadence with only one cache slot to
   absorb it. **This is the number that decides the whole idea and it does not exist yet.**
   It is also cheap to get: rasterise a fixed z14 tile on-device and time it.
3. **The style has to live somewhere.** Today the style is applied in the browser and
   baked into pixels; on-device rendering means the watch owns colour and width rules, so
   they either compile into the app or ride in the pack.
4. **It deletes this project's premise.** `slippypack` exists to turn a style into pixels.
   A vector pack means the browser's job becomes clipping, simplifying and re-encoding
   geometry — the same identity, ordering and determinism machinery, but a different
   payload. `slippypack-core`'s builder, identity and container survive; the quantiser and
   the whole render pipeline do not.

**What it would buy:** ~20× smaller for the same coverage (3.1 MB against 65 MB), zoom
levels that cost nothing, ~1 s of CRC verification instead of ~22 s, and rotation and
smooth zoom becoming possible rather than impossible. That is the Garmin-shaped answer:
Garmin ships vectors and renders on-device, which is why its coverage-per-megabyte is not
in the same category. No codec gets there; only a renderer does.

---

## Recommendations, in order of return per unit of work

1. **A detail radius — the deepest zoom over a smaller core.** ~2× smaller. No spec
   change, no watch change, builder-side only. Do this first regardless of everything
   else.
2. **Deflate as a codec, 4 KB window.** 2.5× smaller for ~3 KB of watch code;
   `miniz_oxide` and `flate2` are already in `Cargo.lock` via `image`. Requires: a new
   `Compression` value, an `inflate` in `Container.hpp`, and the F8 decision about what
   determinism the spec is really claiming. Combined with (1): **65 MB → ~12 MB.**
3. **Per-tile or per-block integrity instead of a whole-file CRC.** Turns ~22 s of
   `verifying map` into ~0, and makes corruption local. Spec change, no size change.
4. **Deduplicate identical tile blobs.** ~5% here, writer-only, pending a spec check.
5. **Measure on-device rasterisation of one vector tile.** The cheapest experiment in this
   document and the only one that can change the product's shape. Everything about vector
   maps on this watch is gated on a number nobody has.
6. **Do not adopt zstd.** 22% over deflate for 20× the decoder and double the build time.
7. **Resolve `tile_dim` in one place.** The measurements favour 128; the shipped renderer
   requires 256. A format whose two implementations disagree about its most basic
   parameter has a bug in its process, not just in one of them.
