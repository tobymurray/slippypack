# How a map gets onto a UNA watch

**Date:** 2026-08-07 · **Status:** recommendation, for decision
**Evidence:** `Docs/Investigations/2026-08-07-watch-cartography/` (E1–E7)
**Companions:** `MAP_COMPLIANCE_APPENDIX.md` · `MAP_CARTOGRAPHY_SPEC.md`

Nothing was posted to GitHub. No tiles were fetched from any tile server during this
investigation; every raster byte analysed came from the committed `athens.rawtiles`
fixture or was generated locally.

---

## 1. Verdict

**Recommended: a pre-rendered global archive on static object storage, sliced into packs
in the browser, transferred by USB mass storage — with a catalog of ready-made regional
packs sitting in front of it for the first-run case.** (Candidate W-E + W-F, USB
transfer; W-G added if and when UNA's mobile app can carry files.)

Why this and not the others, in one paragraph each:

- **We must render our own tiles.** Every source that permits building offline archives is
  vector or raw data; every ready-to-quantise raster source either forbids it outright
  (OSM tile CDN, MapTiler Cloud) or gates it behind a commercial agreement (MapTiler
  Server/Data, Thunderforest). See `MAP_COMPLIANCE_APPENDIX.md` § 6. I looked for a
  counter-example and did not find one.
- **We should render our own tiles anyway.** The shipped reference basemap places 99.92 %
  of its pixels above L\* 80 and draws roads at **1.02 : 1** on a panel capable of
  **25 : 1** (E2). Palette-first rendering uses 5.3× more of the ink range at no cost in
  bytes (E4). The compliance constraint and the quality constraint want the same thing.
- **Render it once, centrally, not on every owner's machine.** The world, watch-tuned, is
  **~1 TB** (E7) — small, because ABGR2222 + spec RLE is 13× smaller than the raw pack and
  vastly smaller than a PNG planet. Storing it costs **$15/month on Cloudflare R2 and does
  not move between 100 and 100,000 owners**, because zero-egress object storage makes the
  bill a function of the archive, not the audience.
- **Slicing beats rendering, for the owner.** With the archive pre-rendered, a "build" is a
  range-read and a copy: instant, byte-identical for everyone, and curated style variants
  become tractable. Rendering per-request (W-D) does the same work repeatedly; rendering in
  the browser (W-C) does it slowly, on unproven throughput, with quality varying by device.
- **Central rendering removes PLAN.md Phase 2's three biggest risks.** Phase 2 is 4–6 weeks
  with a live API risk, and its named hazards are Windows being untested upstream,
  `cargo install` failing without a C++ toolchain, and cross-OS CI. **All three vanish when
  the renderer runs on one Linux box that we own** and never ships to a user.

**Runner-up: W-C — the PWA renders its own tiles from a vector source in the browser.**
Same compliance posture, zero hosting, no planet-render pipeline to operate.

**The specific condition under which the runner-up wins:** if the project cannot own
recurring infrastructure — no budget line for $15–300/month, or no one to re-render when
OSM moves, or a decision that Brilliant Made should not host map content at all. That is a
policy question, not a technical one. If it is answered "no hosting", W-C is the right
fallback and the cartography, format and transfer work in this report all still apply
unchanged; only the slicing step moves from a range-read to a live render, and charter
experiment **X4 (browser render throughput) must be run before committing** — it was cut
here because compliance demoted W-C before throughput mattered.

**One thing is genuinely unresolved and it is not a technical risk I can retire:** whether
UNA's mobile app can transfer arbitrary files to the watch, and at what throughput. See
risk **R2**. It does not change the recommendation — USB-MSC works today
(`una-sdk/Docs/deploy.md`) — but it decides whether the product is phone-first or
desktop-first, which is a large difference in reach.

---

## 2. The owner's path

Written for the case the brief names: *"I'm running the Y trail on Saturday and I've never
used this watch before."* Software that **exists today** is marked ✅; software that **must
be built** is marked 🔨 with its owner.

### 2.1 First run — the catalog path (target: under 4 minutes)

1. Owner opens the map app on the watch. It has no packs, so it shows a screen with a
   short URL and a QR code. 🔨 *watch app — pack-discovery + empty state (una-sdk)*
2. Owner opens the URL on a laptop. The page geolocates (browser permission) and offers
   **"Maps near you"** — a handful of ready-made regional packs, each with a size and a
   coverage thumbnail. 🔨 *web — catalog page*
3. Owner clicks the nearest one. A `.rawtiles` file downloads. Typical sizes, at E3's
   measured compression: **city 25 × 20 km = 2.9 MiB; national park = 27 MiB; metro region
   90 km = 29 MiB** (z12–16, `tile_dim` 128). Nothing is built — this is a static file.
4. Owner plugs the watch in by USB. It appears as mass storage. ✅ *works today —
   `una-sdk/Docs/deploy.md`*
5. Owner drags the file into `Apps/<mapapp>/maps/`. 🔨 *the page shows this step as an
   animation, because a non-technical owner will not read a path*
6. **Owner ejects the drive safely, then unplugs.** ✅ *documented* — and the app verifies
   the pack's footer CRC on first open, which at measured device throughput (27.8 MiB/s,
   `RAWTILES_SPEC_ADEQUACY.md` E3) is **~1 s for a 29 MiB pack**.
7. Map app now lists the pack. Attribution shows for 5 seconds on first open — which is
   what the OSMF safe harbour requires and no more (`MAP_COMPLIANCE_APPENDIX.md` § 4).
   🔨 *watch app — attribution display*

### 2.2 The specific-trail path (target: under 6 minutes)

Same as above, except at step 2 the owner searches or drags a box on a map:

2. Owner types "Y trail" or drags a rectangle. The page shows the pack size live as the
   box changes. 🔨 *web — region picker; this is PLAN.md Phase 5, which survives*
3. Owner clicks Build. The browser issues HTTP range reads against the global archive,
   pulls only the tiles inside the box, and writes a `.rawtiles` locally.
   🔨 *`slippypack-web` — archive range-reader + writer (new); the writer half is
   `slippypack-core`, which exists ✅*
   **No server compute. The archive is static files.** For an 8 × 6 km region that is 687
   tiles ≈ **0.8 MiB** and a few seconds.
4. …then steps 4–7 above, unchanged.

### 2.3 At the trailhead, no signal

Nothing in the path above happens at the trailhead. Packs are on the watch, the panel holds
its image at 11 µW, and the map works with the radio off. **The failure mode that matters
is "I forgot to load a map"**, and the mitigation is the catalog: a single "my region" pack
of 25–30 MiB covers a metro area at z12–16 and should be loaded on day one, not per-run.
The watch app should say so when it is empty.

### 2.4 If the mobile app can carry files (pending R2)

Steps 4–6 collapse into: open the phone app, tap the downloaded pack, tap Send. This is
strictly better and it is what makes the product phone-first. It also **imposes a size
ceiling that changes the catalog design**: a 3 MiB city pack over BLE at a realistic
30–100 kB/s is 30–100 s, which is fine; a 29 MiB metro pack is **5–16 minutes**, which is
not. If the companion path lands, the catalog should default to city-sized packs and offer
region-sized ones as a USB-only option. Note `U13` — the format has no verifiable prefix,
so a chunked transfer has no integrity story until change **C2** below lands; a 29 MiB BLE
transfer that cannot resume is a workflow-killer.

---

## 3. Candidate scoring

Legal compliance is a **gate**: fail it and the row scores zero regardless of everything
else. Quotes for every verdict are in `MAP_COMPLIANCE_APPENDIX.md`.

| | legal | reach | time to first map | quality ceiling | coverage @ size | infra 100 / 10k / 100k | to build (risk) | maintenance | trailhead |
|---|---|---|---|---|---|---|---|---|---|
| **W-A** BYO + CLI | ⚠️ *user absorbs it* | developers only | ~1 h | high | any | $0 | small ✅ mostly exists | user's | fine |
| **W-B** BYO + PWA | ⚠️ *user absorbs it* | anyone with a compliant source — i.e. almost nobody | ~20 min | high | any | $0 | PLAN Ph 4–9 (med) | user's | fine |
| **W-C** PWA renders own tiles | ✅ | anyone with a browser | 5–15 min, unproven | **high** | limited by browser RAM | $0–15 | Ph 4–9 + browser renderer (**high, X4 uncut**) | ours, low | fine |
| **W-D** hosted build service | ✅ | anyone | 2–5 min | **high** | any | ~$15 + compute; scales with requests | server + renderer (med) | ours | fine |
| **W-E** global archive, sliced | ✅ | anyone | **1–3 min** | **high** | any | **$15 / $15 / $15** | render pipeline + slicer (med-high) | ours, monthly | fine |
| **W-F** catalog of pre-built packs | ✅ | anyone | **<1 min** | **high** | fixed regions | **$6–15 flat** | catalog page only (**low**) | ours, monthly | fine |
| **W-G** companion-app transfer | ✅ | phone owners | — *(transfer only)* | — | ≤ ~5 MiB practical | $0 | **unknown — see R2** | UNA's | fine |
| **W-H** on-watch download | — | — | — | — | — | — | **closed** | — | — |

**W-A / W-B fail the gate in practice, not in theory.** They are legal only if the *user*
brings a compliant source, which means the user absorbs the compliance question. For a
consumer product that is not a neutral design choice: the plausible thing a non-technical
owner types into a URL-template box is `tile.openstreetmap.org`, which the OSM policy
prohibits by name. Shipping a box that invites that is a defect. If W-A survives at all it
is as a **developer tool with no default source and a warning**, which is roughly what it
is today.

**W-H is closed.** The SDK's hardware discovery probes LCD, backlight, BLE and GPS and
nothing else (`una-sdk/Docs/architecture-deep-dive.md:375-395`); no radio beyond BLE
appears anywhere. On-watch download is not conceivable on this hardware.

**W-E and W-F are the same system.** A catalog is a pre-sliced cache of popular regions in
front of the archive. Building W-F first is the cheapest path to a working product, and it
upgrades to W-E without rework. **That is the recommended sequencing** — see § 7.

**W-D is dominated by W-E.** Same compliance posture, same quality, but it re-renders the
same ground repeatedly and puts a server on the critical path. The only thing it buys is
not pre-rendering the planet — and pre-rendering the planet costs ~$150–300 a run (E7).

**Cost assumptions, stated.** Archive = 1,002 GB (z0–16, land fraction 0.148, `tile_dim`
128, E3-measured per-zoom RLE ratios). Owner traffic = 2 packs × 8 MiB per owner per month.
Storage list rates retrieved 2026-08-07: Backblaze B2 $0.006/GB-mo, Cloudflare R2
$0.015/GB-mo, both zero egress; AWS S3 $0.023/GB-mo + CloudFront $0.085/GB. Per-operation
charges are immaterial at these object counts. **The recurring cost that scales is
re-rendering, not serving:** ≈4,400 core-hours per full-planet rebuild at an assumed
50 tiles/s/core ⇒ **$150–300 per rebuild** on spot compute. That throughput is `PLAUSIBLE`,
not measured; a one-hour benchmark on one region settles it.

---

## 4. What the evidence changed

Five things in the brief's own premises did not survive measurement. Each is load-bearing.

| premise as stated | what measurement says |
|---|---|
| "Spec RLE measured 32.4 % ⇒ ~14.6 MiB for Athens" | **7.7 % ⇒ 3.3 MiB** on all 687 real tiles (E3). The 32.4 % came from n=12 dense low-zoom tiles. A 4.4× error, pessimistic |
| "Higher zooms multiply size by 4× per level" | **≈2.2×.** Tile count goes 4× but bytes/tile fall with zoom (30.3 % at z12 → 6.0 % at z16) (E3) |
| "64 colours is a palette design problem" | True, and the reason is sharper than expected: **only 3 of 64 codes are below L\* 40**, one of them neutral. There is no neutral dark grey between L\* 23.7 and L\* 66.5 (E1) |
| "Palette differentiation is nearly free" | True **only on palette-first packs**. A blit-time LUT cannot restyle an anti-aliased pack — labels disintegrate (E6). The precondition is the finding |
| Phase 2 may need a custom aliased rasteriser | No. **Snap-to-declared-slots instead of nearest-of-64** recovers the palette-first result from an ordinary anti-aliasing renderer: same code count, +0.2 % bytes, ~10 lines (E5) |

And one input that was listed as unknown and is now settled: the panel is a **Sharp
LS012B7DD06A** — transflective memory-in-pixel, **64 colours native** (so ABGR2222 is the
hardware, not a compromise), 25:1 contrast, 18 % NTSC, and it **holds its image at 11 µW**.
Datasheet quotes in the investigation README.

---

## 5. Implementation plan against what exists

### 5.1 slippypack phases

| phase | fate | why |
|---|---|---|
| 0 — core | ✅ **landed, survives** | writer/reader/quantiser/projection/identity all still needed |
| 1 — CLI `synthetic` + URL template | **survives, demoted** | keep `synthetic`; the URL-template path must lose its OSM-shaped default (S2) |
| 1.x — `dir` / `mbtiles` / `pmtiles` sources | **promoted to critical path** | `pmtiles` is now how we read the Protomaps planet; it was optional, it is now the front door |
| **2 — vector rendering** | **promoted to critical path, risk reduced** | no longer optional (compliance). But it runs **server-side on Linux only**, which deletes the Windows-untested, C++-toolchain and cross-OS-CI risks named in PLAN.md § Phase 2 |
| 3 — GeoTIFF | **deferred** | terrain is post-v1 (`MAP_CARTOGRAPHY_SPEC.md` § 6) |
| 4 — PWA WASM bootstrap | **survives** | needed by the slicer |
| 5 — region picker + presets | **survives** | this is § 2.2 step 2 |
| 6 — source picker | **cut** | there is no compliant source for an owner to pick. Replaced by: no picker, one archive |
| 7 — PWA shell | **survives** | |
| 8 — OPFS streaming | **survives, descoped** | needed for 30 MiB packs, not for the multi-GB packs PLAN.md contemplated — those exceed the format's 4 GiB cap anyway |
| 9 — custom regions | **survives** | |
| 10 — hand-drawn packs | unchanged | orthogonal |
| 11 — CI / deploy | **survives** | |

**New work not in PLAN.md:** the planet render pipeline (Protomaps PMTiles → watch style →
128 px ABGR2222 → per-zoom RLE → archive layout); the archive range-reader; the catalog
page; the palette-snapping quantiser mode (small, E5).

**Cut from PLAN.md's premise:** its "BYO tile sources / zero infrastructure / no
project-hosted services" framing. The owner suspended it for this question and it does not
survive on merit — BYO fails because there is no compliant source a non-technical owner can
bring, and "zero infrastructure" turns out to cost $15/month to abandon.

### 5.2 What the watch app / SDK must provide

| capability | status |
|---|---|
| pack discovery — scan `maps/`, list packs, show coverage | 🔨 new |
| pack picker UI, and an empty state that tells a new owner what to do | 🔨 new |
| attribution display — `ATTR` for ≥5 s on map open; About screen with `openstreetmap.org/copyright` as plain text | 🔨 new; requirement quoted in `MAP_COMPLIANCE_APPENDIX.md` § 4 |
| 64-entry palette LUT applied at blit time, with day/night/high-contrast/trail variants | 🔨 new; mechanism proven in simulation (E6), **per-frame cost unmeasured** |
| footer-CRC verification on first open after copy | partly ✅ (reader validates); needs UI |
| serialise USB-MSC writes against BLE sync | ⚠️ documented corruption mode; needs enforcement |

### 5.3 Change list routed to `rawtiles`

Spec pinned at **`38d4d26`, document v0.6, wire `(1,0)`, Status: Provisional** — the file
is `spec/rawtiles.md` on `origin/main`.

The version drift the brief flagged is real and worse than "one revision stale": the most
recent slippypack commit message (`1f9132d`) references **spec 0.5**, `DECISIONS.md`
references a file **`spec/rawtiles-v1.0-rc1.md`** (at `:468`, `:487`, `:509`) which **does
not exist on `rawtiles` `origin/main`**, and the una-sdk investigations reference 0.6 and
0.7. Only 0.6 is on `origin`. Every `§` quoted in this report is from `38d4d26`.

| # | change | blocking relationship |
|---|---|---|
| **C0** | **§ 9.1.1's canonical quantiser encodes a display model this panel contradicts.** It states the quanta are "displayed as {0, 85, 170, 255}"; measured on the LS012B7DD06A they are ≈ **{56, 162, 215, 255}**, because area gradation makes the levels linear in *reflectance*, not in sRGB code. **93.2 % of RGB888 colours quantise to a different code than the panel-correct choice, always too light; mean ΔE2000 on the grey axis 16.24 vs 6.65** (E8). Add `quantiser_version = 2` for linear-reflectance targets | **Blocks nothing in the recommendation** — palette-first snaps to declared slots and never invokes the canonical path. **Blocks any naive-quantisation workflow**, and it is a second, independent cause of E2's washed-out reference basemap |
| **C1** | Bring `ATTR` (and `NAME`) into the § A.3 `pack_uuid` descriptor | **Blocks nothing technically; blocks a licence claim.** Today an attribution-stripped pack keeps its identity (`U12`), which is a compliance hole in a format used to distribute ODbL Produced Works |
| **C2** | A verifiable prefix — per-block integrity so a partial pack can be validated (`U13`) | **Blocks W-G only.** Resumable chunked BLE transfer has no integrity story without it. USB-MSC does not need it |
| **C3** | **A descriptor shape for a vector source rendered through a style.** § A.4 classifies `pmtiles`/`mbtiles` as *raster* sources, and for the one vector kind (`pbf`) states "v1 does not specify PBF-to-pixel rendering (reserved for a future minor)" | **Blocks the recommendation's reproducibility claim, not its function.** See § 5.4 |
| ~~C4~~ | ~~deflate/LZ4 codepoint~~ | **Withdrawn.** E3's corrected 7.7 % baseline leaves a 2.5× gap to deflate, not 12×, and it changes no workflow decision. `RAWTILES_SPEC_ADEQUACY.md` `M8` should be revisited on the same evidence |
| ~~C5~~ | ~~`compression` + `supersedes_uuid` into the descriptor (`M1`)~~ | **Withdrawn for this workflow.** M1 is a real defect but it cannot bite here: every pack we ship uses `RLE`, and a monthly re-render differs in `content_hash` anyway, so it never produces a pack differing *only* in `compression` or `supersedes_uuid`. Worth fixing for the format; not on this critical path |
| **C6** | Alpha-bearing pixel format for overlay packs (`U7`) | Blocks nothing in v1 — the GPS trace is app-drawn, and terrain is deferred |

**Nothing on this list blocks starting.** C1 should land before the first pack goes to
anyone outside the project; C0 before anyone builds a pack by naive quantisation.

### 5.4 Does the format withstand the workflow?

Mostly yes, and in two places better than expected. Assessed against `38d4d26`.

**Holds up:**

- **`ABGR2222` is exactly right, and not by luck.** It is the panel's native format
  (datasheet § 2), and § 9.2's ST77xx/PineTime/Bangle.js note shows the format was
  designed with more than one target in view. The bit order in § 9.1 and the § 14.4 test
  vector check out against the model built here.
- **`style_hash` and `quantiser_version` are already in the § A.3 descriptor.** A format
  whose identity contract anticipates that *rendering* and *quantisation* are part of what
  makes a pack is precisely what a palette-first, style-driven pipeline needs. This is the
  single most encouraging thing in the spec for this recommendation.
- **`tile_dim_px` is a free u16** (§ 4.7), so the move to 128 px is legal today.
  **slippypack has no `--tile-dim` flag** — a tool gap, not a format gap.
- **Slicing is cheap by construction.** § 5.2's sort order plus § 4.12's `zoom_offsets`
  plus § 6.1's 4-byte alignment mean a regional slice is a filtered concatenation in
  `(z,x,y)` order with recomputed offsets. No re-encoding.
- **`supersedes_uuid` (§ 4.4) carries the monthly re-render** cleanly.
- **The ATTR rules (§ 7.3) are strict in the right way** — NFC, LF-separated, no trailing
  LF, control characters rejected by both writer and reader. Good discipline for a field
  that carries a licence obligation.
- **Open cost is a non-issue at these sizes.** 27.8 MiB/s measured ⇒ ~1 s of eager
  validation for the largest catalog pack.

**Does not hold up:**

- **C0, above** — the canonical quantiser is calibrated for a bit-replicating display path.
  The deeper point is architectural: a display's transfer function is a property of the
  *target panel*, not of the writer, so welding one into the canonical quantiser makes the
  format's identity contract quietly device-specific.
- **C3, above — the recommended pipeline has no home in v1.** Vector PMTiles → MapLibre
  style → raster is exactly what compliance forces (`MAP_COMPLIANCE_APPENDIX.md` § 6), and
  § A.4 has no shape for it. The encouraging part: the fix looks small, because § A.4
  already defines a raster source's `content_hash` as "the SHA-256 of the writer's
  **pre-quantisation RGB888 byte stream** … after the writer's decode/resample/alpha-handling
  pipeline has run". That is *already* the right hash for a rendered vector source — it
  pins the rendered pixels, which is the deterministic surface. Plausibly this is a
  `vector: true` flag on the existing `pmtiles`/`mbtiles` shapes plus the `style_hash` key
  that already exists, not a new kind.
- **`parent_uuid` is reserved-must-be-zero** (§ 4.5), so a slice cannot declare which
  archive it came from. The catalog would like to say "this pack covers ground you already
  have". Low priority; § 4.5's stated purpose ("future pack-compositing") is adjacent.
- **ATTR ordering is underspecified for style-rendered packs.** § 7.3 requires the strings
  be "ordered to match the canonical `sources` array order", and § A.4 says "Style sources
  MUST NOT appear in `sources`". When attribution is owed to both the tileset producer
  (Protomaps) and the data (OSM) through one source entry, the ordering rule has nothing
  to order.
- **The archive cannot itself be rawtiles.** 1 TB against a 4 GiB cap (§ 3). That is the
  correct outcome, not a defect — rawtiles is the *delivery* format and PMTiles is the
  *storage* format, and the seam between them is clean — but it should be stated so nobody
  tries to make one format do both.

---

## 6. Ranked risks

| | risk | severity | mitigation |
|---|---|---|---|
| **R1** | **Phase 2's renderer is 4–6 weeks with a live API risk** — `maplibre_native` exposes `render_static(...)`, not per-tile `renderTile(z,x,y)` | high | Unchanged from PLAN.md, but **narrowed**: server-side-only means Linux-only, killing the Windows/toolchain/CI risks. Week-1 spike and its three exit conditions still apply |
| **R2** | **Unknown whether UNA's mobile app can transfer arbitrary files, and at what BLE throughput.** `deploy.md:44` establishes only that it matches `.uapp` versions by `APP_ID` | high | **Ask UNA.** This is a question, not an experiment. It decides phone-first vs desktop-first, and the catalog's default pack size |
| **R3** | **The blit-time LUT's per-frame cost is unmeasured** (E6 is simulation) | medium | Charter X7 on the AthensRun PoC. If it is too slow, variants become separate packs — N× storage and transfer |
| **R4** | Planet re-render cadence and cost is estimated, not measured (~$150–300/run at an assumed 50 tiles/s/core) | medium | One-hour benchmark on one region before committing to a cadence |
| **R5** | **Baked labels freeze the locale at build time.** "Any UNA owner means global" — a Kraków owner wanting Polish and one wanting transliteration need different packs | medium | Accepted for v1 (`MAP_CARTOGRAPHY_SPEC.md` § 5); revisit when a second locale is required. This is the weakest part of the recommendation |
| **R6** | USB-MSC writes concurrent with BLE sync corrupt the exFAT volume — documented | medium | Serialise; verify after copy; the transfer UI must say "eject safely" and mean it |
| **R7** | Line weights, the ΔE ≥ 10 threshold, and the assumed area-gradation steps are all judgements or estimates | medium | A hardware legibility trial settles all three at once |
| **R8** | Protomaps download URLs "may change"; OpenFreeMap is one person's project with no SLA | low | We copy to our own storage — which is what both projects tell you to do. Two independent sources, either sufficient |
| **R9** | The 4 GiB pack cap bites at country scale (`RAWTILES_SPEC_ADEQUACY.md` E2) | low | Nothing in this design ships country packs; the largest catalog entry is ~30 MiB |

---

## 7. Sequencing

The cheapest path to a working product, each step shippable:

1. **Palette + style** (`MAP_CARTOGRAPHY_SPEC.md` §§ 3–4) and the palette-snapping
   quantiser mode (E5, ~10 lines). No infrastructure.
2. **Phase 2 renderer, Linux-only, server-side.** The long pole. Week 1 is the spike.
3. **Render one region** — the Athens bbox is the obvious A/B against the existing pack —
   and put it beside `images/x12_arms_1to1.png`. This is the first honest quality check.
4. **W-F: catalog.** Render ~50 popular regions, publish as static files with a browsable
   index. **This is a working product for most owners** and needs no slicer.
5. **X7 on hardware** — LUT cost. Decides whether variants are free.
6. **W-E: archive + browser slicer.** Render the planet, publish, build the range-reader.
7. **W-G** if UNA's answer to R2 permits it, gated on change C2.

Steps 1–4 are the v1. Steps 5–7 are what make it good.

---

## 8. Spin-offs — real, out of scope here

| # | item | target |
|---|---|---|
| **S1** | `Docs/Investigations/2026-08-06-athens-pack/README.md` characterises its build as "a one-off ~690-tile fetch, not a bulk pipeline". The OSM policy defines bulk downloading as *any* pre-emptive fetching and names archive-building explicitly. Correct the README so the fixture is not read as a sanctioned recipe | `una-sdk`, branch `docs/athens-pack-compliance-note` |
| **S2** | `PLAN.md` recommends MapTiler Cloud's free tier as the default source; its terms prohibit the tool's only use case. Remove the recommendation; remove any OSM-shaped default from the URL-template path; add a compliance note to `slippypack make --source` help | `slippypack`, branch `fix/source-compliance-defaults` |
| **S3** | Spec-reference drift: commit `1f9132d` says spec 0.5, `DECISIONS.md:468,487,509` cite `spec/rawtiles-v1.0-rc1.md` (no such file on `rawtiles` `origin/main`), una-sdk cites 0.6 and 0.7. `origin/main` is `spec/rawtiles.md` @ v0.6. Pin one | `slippypack`, branch `docs/spec-0.6-refresh` |
| **S4** | slippypack emits the *requested* bbox verbatim rather than § 4.9's canonical tile-coverage bbox (noted in the athens-pack README) | `slippypack`, branch `fix/canonical-bbox` |
| **S5** | `Docs/architecture-deep-dive.md`'s "256 KB app RAM" Mermaid label is stale; real budgets are 500 K service / 600 K GUI (`cmake/una-app.cmake:217,:281`) | `una-sdk`, branch `docs/fix-ram-budget-label` |
| **S6** | The panel is a Sharp LS012B7DD06A — transflective MIP, 64 colours native, 25:1, 11 µW hold. None of this is in the SDK docs, and all of it constrains UI design well beyond maps | `una-sdk`, branch `docs/panel-characteristics` |

---

## 9. Charter experiments — what was run, what was cut

| | status |
|---|---|
| **X1** palette + style spike | ✅ run (E1, E4). **Benchmark against Garmin/Coros/Suunto/Bangle.js NOT done** — see below |
| **X2** quantise fidelity vs size | ✅ run (E4, E5). 1× wins; no downsample; never dither |
| **X3** compression re-measure | ✅ run (E3) on 687 real tiles. 32.4 % → **7.7 %** |
| **X4** browser render throughput | ❌ **cut.** Decides W-C, which compliance demoted to runner-up. Must be run before choosing the runner-up |
| **X5** archive arithmetic and cost | ✅ run (E7) |
| **X6** transfer end-to-end | ❌ **cut** — not runnable without the watch and an answer to R2 |
| **X7** blit-time LUT on device | ⚠️ **half.** Mechanism proven in simulation (E6); per-frame cost unmeasured |

**The one claim this report deliberately does not make.** The brief asks for "the best map
on any watch at this price" and says a claim of class-leading with no comparison image is
not a finding. **I did not produce that comparison and I do not make that claim.** Getting
it means photographing a Garmin, a Coros and a Bangle.js showing the same place at the same
scale in the same light — an afternoon with three watches, and it should happen before any
marketing claim. What the evidence does support: this design uses **5.3× more of the
panel's contrast range** than the current reference basemap, which draws its roads at
1.02 : 1 on a 25 : 1 display.
