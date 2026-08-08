# Prompt: Decide how a map gets from "I want one" to "it's on my watch" — for every UNA owner

You are choosing the **end-to-end workflow** that lets an ordinary UNA watch owner pick an
area, get a `.rawtiles` map pack for it, and load it onto the watch — and choosing the
**cartography** that pack contains. Two deliverables, one decision, because they constrain
each other: the tile source you may legally use determines whether you control the style,
and the style determines pack size, which determines which transfer channels work.

The bar is not "a workflow exists." The bar is **the best map on any watch at this
price**, on a 240×240 panel with 64 available colours, for someone running, hiking, or
riding — and a path to it that a non-technical owner completes without reading a README.

Two things are already true and shape everything: the on-device format is settled
(`.rawtiles`, blit-ready raster; see § 1), and **the tile source everything has been
prototyped against is not legally usable for this product** (§ 4 — read it before you
design anything). The second fact is why this evaluation exists now rather than after the
PWA ships.

---

## 0. Ground rules (hard constraints)

- **Never post anything to GitHub** — no PRs, comments, or issues, on any repo. `gh` is
  read-only. Your deliverable is local markdown plus investigation bundles.
- **Verify, don't trust this prompt.** Every fact below was checked on 2026-08-07;
  re-verify anything load-bearing. Cite `file:line` for claims about code, spec `§` for
  claims about the format, and URL + retrieval date for claims about anyone's terms.
  Label findings `CONFIRMED` (traced to text, code, or experiment) or `PLAUSIBLE`
  (reasoned) — and for each `PLAUSIBLE`, say what would settle it.
- **Terms of use are a gate, not a factor.** A workflow that violates a source's terms
  scores zero regardless of how good the map is. Quote the governing clause **verbatim**
  with its URL and retrieval date. Where terms are ambiguous, say so explicitly and take
  the conservative reading. Never let a workflow's legality rest on your inference about
  what a provider "probably means" — if that's the only support, the workflow is
  `BLOCKED-PENDING-PERMISSION`, and the mitigation is "ask them," which is a real and
  acceptable recommendation.
- **Do not bulk-fetch tiles from anyone's server during this investigation.** This
  includes "just a few hundred to test." If a source's terms prohibit pre-emptive
  fetching, they prohibit your experiment too. Use `--source synthetic`, the committed
  Athens pack, locally-hosted data you downloaded through a channel intended for bulk
  download, or a self-run renderer.
- **Cost every hosted option in dollars.** Storage, egress, compute, per-month at 100 /
  10,000 / 100,000 owners. A hosted workflow with an unstated cost is not a candidate.
  State the assumption behind every number.
- **You are not a lawyer and the report must not pretend otherwise.** Distinguish "the
  terms say X" (quotable) from "we would be fine doing Y" (not yours to conclude). Flag
  anything that needs actual legal review as such, precisely, so it can be asked once.
- Evidence convention: anything settled by experiment goes in
  `Docs/Investigations/<date>-<slug>/` in the repo it belongs to (hypothesis / method /
  log / screenshots / verdict — failures included, they are usually the useful part).
- Commits terse, mostly *why*. Branch off `main`, push to `origin` only, plain push
  never force.

## 1. Settled — do not re-litigate

These were decided with evidence. Re-opening them is out of scope; contradicting them
requires new evidence, not a new opinion.

- **`.rawtiles` is the on-device format.** `RAWTILES_MAP_EVALUATION.md` +
  `Docs/Investigations/2026-08-05-rawtiles-map-evaluation/` (branch `rawtiles-next-step`
  in `una-sdk`) settled rawtiles-vs-PMTiles/MBTiles/vector for blit-ready raster on this
  hardware. The prior-art sweep in that bundle's `findings/ecosystem.md` is thorough —
  read it rather than redoing it.
- **Vector rendering on the watch is out.** Demonstrated possible, consistently not
  chosen; every practical system pre-rasterises somewhere (`findings/ecosystem.md` § 7).
- **The app-side map screen works.** `poc/athensrun` (in `una-sdk`) is a running app with
  a Garmin-style live map: viewport math, tile cache, trace overlay, a vendored C++
  reader. Read what it learned; do not rebuild it.
- **USB mass storage works today** (`una-sdk/Docs/deploy.md`). It is a real transfer
  channel, not a hypothetical one.
- **Wire-format changes are findings, not actions.** If the best workflow needs something
  the format lacks (a compression codepoint, an alpha-bearing pixel format, per-block
  integrity), that's a change list routed to the `rawtiles` repo with the workflow's
  dependency on it stated. Don't edit the spec here.

## 2. Reopened — the constraints you are allowed to break

`slippypack/PLAN.md` was written under constraints the owner has now explicitly
suspended for this question. Treat these as **open, not settled**:

- **"No project-hosted services. Zero infrastructure cost. BYO tile sources."**
  (`PLAN.md` § "The three axes this plan optimises", § "Out of scope".) Suspended. You
  may propose hosted software — a website that builds packs, a rendered global archive, a
  catalog of pre-built packs, anything. If BYO survives on merit, say *why* it wins, not
  that the plan said so.
- **"No in-browser vector rendering. The PWA stays raster-only."** (`PLAN.md` § "The PWA
  — a strict subset".) Reopened, and § 4 is the reason: if the compliant sources are
  *vector* tile sets, then "fetch pre-rendered raster tiles" describes a workflow with no
  legal supply. Browser-side rasterisation is exactly what Bangle.js's map loader does
  (`findings/ecosystem.md` § 4), and it puts the style under our control, which § 5 says
  is the whole ballgame.
- **`PLAN.md`'s phasing and estimates.** Phases 4–9 describe a PWA built around BYO
  raster sources. If that premise fails, so does the phasing. Say which phases survive,
  which are cut, and which are new work — don't inherit the schedule.

## 3. Where the truth lives (read before deciding)

| Thing | Where | What it tells you |
|---|---|---|
| The pack writer | `slippypack` (this repo) — `PLAN.md` (design record), `DECISIONS.md` (~1000 numbered decisions with rationale), `README.md` (what actually works) | Today: `slippypack make` with `--source synthetic` and HTTPS URL templates; `--pixel-format abgr2222\|rgb565`; `--compression none\|rle8`; per-host rate limiting; atomic write. `dir`/`mbtiles`/`pmtiles` sources, the vector renderer, and the whole PWA are **unbuilt** |
| The vector-render risk | `PLAN.md` § "Phase 2 — CLI vector rendering" | 4–6 weeks, and the `maplibre_native` Rust binding exposes `render_static(...)`, **not** a per-tile `renderTile(z,x,y)` API. Read the three exit conditions before assuming "we'll just render our own tiles" is cheap |
| The format | `github.com/tobymurray/rawtiles` | Normative spec + conformance corpus (golden + negative fixtures). **Pin the version before quoting `§`s** — see § 8's premise traps |
| The format's known gaps | `una-sdk/RAWTILES_SPEC_ADEQUACY.md` (69 KB; read § 1 verdict, § 3 needs matrix, § 5 compression, § 9 ranked gaps) | The needs-space audit. Directly relevant: `M1` (identity is unsound as a byte-dedup key), `U13` (no verifiable prefix ⇒ resumable transfer must invent its own integrity), `U7` (no alpha ⇒ no overlay packs), `U8` (day/night is a reader-side tone-map), `U6` (4 GiB cap), § 5 (the compression ladder points at the wrong rung) |
| The reference basemap | `una-sdk/Docs/Investigations/2026-08-06-athens-pack/` | A real 45,037,308-byte pack: 10×8 km, z12–16, 687 tiles, ABGR2222, `compression = None`. **Built from `tile.openstreetmap.org` — see § 4 before treating it as a template** |
| The consuming app | `una-sdk` branch `poc/athensrun` (`Examples/Apps/AthensRun/`), plus `spike/rawtiles-device-proof` and `feat/rawtiles-container` | Viewport math, tile cache, trace overlay, the vendored reader, and the device measurements below |
| App distribution | `una-sdk/Docs/deploy.md` | `https://apps.unawatch.com` is a real app portal (sign up, App ID, upload a zip of `.uapp` + `icon.png` + `config.json`, Release). It references a **mobile app** that matches `.uapp` versions — **find out what that mobile app can and cannot transfer** (§ 6, W-G) |

**Device ground truth (measured — do not re-derive):**

- Display **240×240, 8 bpp, ABGR2222** — `una-sdk/Docs/TouchGFX-Port-Architecture.md:24,395`.
  2 bits per channel ⇒ **64 colours, 4 levels per channel**, and spec § 9.1 pins alpha
  fully opaque in v1. This is the single most consequential constraint in the document.
- `LCD::blitCopy(ptr, Bitmap::ABGR2222, …)` renders correctly at negative-X/-Y overhang
  on hardware. **No rotation** ⇒ north-up only; track-up is not available.
- GUI process RAM budget **600 K**, service **500 K** — `cmake/una-app.cmake:281,:217`.
- 64 KiB tile read **6–9 ms**; first filesystem touch after app start **≈ 113 ms**;
  open + eager-validate a 787,008-byte pack **≈ 27 ms (27.8 MiB/s)** ⇒ a 45 MB pack CRCs
  in ~1.6 s, the 4 GiB cap in ~147 s.
- Storage: 2 MB internal NOR + multi-GB eMMC behind exFAT. Apps see a **sandbox-relative**
  filesystem (no absolute volume path resolves); `skMaxPathLen = 256`; no mmap.
- **Documented corruption mode:** USB-MSC writes concurrent with BLE sync corrupt the
  exFAT volume. Any transfer design must serialise against BLE sync and verify after copy.
- **Unknown, worth establishing:** the panel technology (nothing in the docs says
  transflective / AMOLED / MIP) and whether the watch has any network radio beyond BLE.
  Both change the answer — the first for sunlight legibility, the second for whether
  on-watch download is even conceivable. Find out; if you can't, say so and scope around it.

**Size arithmetic to start from** (verify, then extend): the Athens pack is 43 MiB at
`compression = None`. Spec RLE measured **32.4%** of raw ⇒ ~14.6 MiB for the same area.
Small-window deflate measured **13.0%** ⇒ ~5.6 MiB — but *deflate is not in the format*
(§ 8 trap). And that 13.0% is n = 12 tiles of one style; `RAWTILES_SPEC_ADEQUACY.md` § 5
states the ABGR2222 span runs ~13% (flat vector styles) to ~75–80% (dithered satellite).
**Your style choices move pack size by ~6×.** Treat compression ratio as an *output* of
the cartography decision, not an input.

## 4. The compliance question — read this before designing anything

The prototypes were built by fetching `https://tile.openstreetmap.org/{z}/{x}/{y}.png`.
Three findings, all retrieved 2026-08-07, that you must re-verify and then build on:

1. **The OSM tile usage policy prohibits exactly this workflow.**
   `https://operations.osmfoundation.org/policies/tiles/` defines bulk downloading as
   "any pre-emptive fetching of tiles other than those a user is actively viewing," names
   **building offline archives** among the prohibited patterns, and states such traffic
   "will be blocked without notice." **This is not a rate-limit question.** slippypack's
   built-in 2 req/s cap for `tile.openstreetmap.org` limits *load*; it does not make
   prefetching-for-offline-storage permitted. Consequence: the Athens pack is not a
   template, and slippypack's URL-template path pointed at OSM's tile CDN is not a
   shippable default. Re-read the current policy text and quote it.
2. **MapTiler Cloud — `PLAN.md`'s headline recommended source — prohibits it too.**
   Its Cloud terms (`https://www.maptiler.com/terms/cloud/`) prohibit batch/bulk
   download of tiles and prohibit storing or redistributing map content from a cache
   instead of accessing the API directly, absent a written agreement. So `PLAN.md`'s
   first-run flow ("MapTiler — free tier: 100K tile requests/month — comfortably covers a
   small country") recommends a source whose terms forbid the product's only use case.
   MapTiler's **Server / Data** products (`https://www.maptiler.com/terms/server-data/`)
   are the licensed offline route — price them. Expect Stadia, Thunderforest, Carto,
   Mapbox, Esri, and Google to land similarly; check each, don't assume.
3. **There are compliant bulk sources, and they are vector.**
   - **OpenFreeMap** (`https://openfreemap.org/`) — no API key, no registration, no
     stated request limits, commercial use allowed, MIT-licensed software, and **weekly
     full-planet downloads** (MBTiles and Btrfs). Attribution: "OpenFreeMap ©
     OpenMapTiles Data from OpenStreetMap."
   - **Protomaps basemaps** (`https://docs.protomaps.com/basemaps/downloads`) — daily
     full-planet PMTiles builds (~120 GB, z0–15), ODbL as a Produced Work, `pmtiles
     extract` for regional subsets, and explicit guidance to copy tilesets to your own
     storage rather than hotlink.
   - **Raw OSM data** — Geofabrik / BBBike extracts, ODbL, bulk download is the intended
     channel. Maximum control, maximum work (you own a rendering pipeline).
   - Also assess: **Overture Maps**, **OpenMapTiles**' own pre-generated packages (check
     the tileset licence separately from the schema licence), and the open raster / terrain
     estate that is legally *free* and cartographically valuable — **Copernicus DEM**,
     **USGS 3DEP**, Natural Earth, NOAA ENC, NAIP, Sentinel-2, and national open-data sets
     (OS Open Zoomstack, NRCan, etc.). Hillshade and contours are a large legibility win
     for trail use and carry no tile-server policy problem at all.

**The structural consequence, which you should state plainly if you agree with it:** the
sources that permit bulk offline derivation are vector; the sources that ship
ready-to-quantise raster mostly forbid it. So **"we render our own tiles" is not a
nice-to-have, it's the price of admission** — which is convenient, because § 5 argues it's
also the only way to hit the quality bar. Test that reasoning; if you find a compliant
pre-rendered raster source good enough for a 64-colour panel, that changes the answer.

**ODbL obligations to pin down** (they travel with the pack, not just the website): what
attribution the pack must carry and where the watch must display it; whether distributing
derived tile packs makes them a Produced Work or a Derivative Database, and what that
implies for offering source data; and how the spec's `ATTR` section satisfies "reasonably
calculated" attribution on a 240×240 screen. `RAWTILES_SPEC_ADEQUACY.md` `U12` has a head
start — including the defect that `ATTR` bytes are outside the `pack_uuid` descriptor, so
an attribution-stripped pack keeps its identity.

## 5. The cartography bar — where "class leading" is won or lost

This is the least-explored and highest-upside part of the problem, and it will not be
served by picking a MapLibre style off the shelf and quantising it. Treat it as a design
deliverable with its own evidence.

**The core fact: 64 colours is a palette design problem, not a quantisation problem.**
Four levels per channel means every colour in the output is one of 64 fixed values.
Naively quantising an osm-carto-style raster into that space collapses the greens and
greys into each other and turns casing into noise. The alternative — **design the 64-entry
palette first, then constrain the style to emit only palette colours** — makes
quantisation exact and lossless, and puts every one of the 64 slots under deliberate
control. Establish which approach wins with images, not argument.

Questions to settle, each with a visual artefact:

- **The palette.** What are the 64 entries actually spent on? How many go to terrain
  ramps, water, road hierarchy, text and its halo? What's the minimum contrast ratio
  between adjacent map features that survives at 240×240 in daylight?
- **Line weight and casing without anti-aliasing.** At 240×240 a 1 px road is a hairline
  and there are no intermediate shades to soften it. Minimum legible stroke, casing
  strategy, halo strategy. This is where Garmin's watch cartography is visibly good —
  study it.
- **Render scale.** Render at 1× vs render at 2× and downsample before quantising, vs
  ordered dithering. Each trades apparent depth against RLE ratio, and dithering is
  catastrophic for compression (it destroys the run structure RLE and LZ77 both need).
  Measure both axes together; the size number belongs next to the picture.
- **Labels: in the raster or drawn by the app?** Baked labels are free at runtime but burn
  palette slots on text antialiasing, can't stay upright under any future rotation, and
  bloat every zoom. App-drawn labels keep the raster palette clean and let text scale —
  but need a data channel the format doesn't have, which is a spec finding and a real cost.
  The app already draws a GPS trace over the blit (`poc/athensrun`), so the drawing
  machinery exists. Decide with a mock of both.
- **Terrain.** Hillshade and contours from public-domain DEMs, at what palette cost and
  what size cost. For trail and hiking this is likely the largest single legibility gain
  available; for urban running it may be noise.
- **Zoom ladder.** Which zooms a watch actually needs, and what each shows. z12–16 is what
  the Athens pack used; justify or change it. Higher zooms multiply size by 4× per level.
- **Benchmarks.** Put your renders side by side at 1:1 with Garmin's watch basemap, Coros,
  Suunto, and Bangle.js's 3 bpp openstmap. State plainly where yours is better and where
  it isn't. A claim of "class leading" with no comparison image is not a finding.

**Activity differentiation — and the cheap lever hiding in the format.** The ask is
"potentially differentiated between activity if necessary." Split it into two mechanisms,
because they cost wildly different amounts:

- **Palette differentiation is nearly free.** ABGR2222 has 64 possible values, so a
  **64-entry lookup table applied at blit time** can restyle any pack — day/night,
  high-contrast, per-activity colourways — with no extra pack, no extra storage, no extra
  transfer, and one indirection per pixel. `RAWTILES_SPEC_ADEQUACY.md` `U8` notes the
  reader is free to tone-map; nobody has proved it on device. **Prove it in the AthensRun
  PoC and measure the blit cost.** If it holds, day/night and activity colourways are an
  app feature, and the pack stays single-purpose.
- **Feature-selection differentiation is expensive.** A trail map that shows contours and
  a road map that shows crossings are *different rasters*. That means N packs (N× storage,
  N× transfer, N× build time) — or one style carefully designed to serve every activity at
  once. Judge honestly which activities genuinely need their own raster (candidates: road
  running / urban, trail + hiking, cycling, ski, water) and which are satisfied by a
  palette swap plus app-drawn overlays. **The answer might be one excellent pack.** Say so
  if it is.

## 6. Candidate workflows

**Generate your own list first**, from the owner's point of view ("I'm running the Y
trail on Saturday and I've never used this watch before"), and only then merge with this
seed list so it doesn't anchor you. For each survivor, write the owner's path as a
numbered sequence of concrete actions, and name at every step which software exists today
versus must be built.

- **W-A — BYO source + local CLI.** Today's shipped path. Reach: developers only. Legality
  depends entirely on which source the user brings — which means the *user* absorbs the
  compliance question, which is worth an explicit judgement.
- **W-B — BYO source + browser PWA.** `PLAN.md` Phases 4–9. Zero infra. § 4 attacks its
  premise: what compliant source does a non-technical user bring?
- **W-C — PWA that renders its own tiles.** MapLibre GL JS renders vector tiles (public
  OpenFreeMap instance, or PMTiles we host) to a canvas; WASM quantises and packs. Style
  under our control; no raster-tile ToS problem if the vector source permits it. Open
  questions: throughput, and whether rendering large canvases with overlap (rather than
  256 px tiles) fixes cross-tile label placement and line continuity.
- **W-D — Hosted build service.** Owner picks an area on a website; we render and slice
  server-side and return a `.rawtiles`. Best quality control and the simplest UX; costs
  compute and egress; needs the compliance story to be airtight because *we* are the one
  doing the fetching at scale.
- **W-E — Precomputed global archive, sliced on demand.** Render the world once into
  watch-tuned tiles, store on object storage, and make a "build" a slice rather than a
  render — server-side, or client-side via HTTP range reads against an index. Instant
  builds, byte-identical results, and curated style variants become tractable. Cost the
  render (CPU-hours), the storage, and the egress; note that zero-egress object storage
  changes this arithmetic a lot. Watch-tuned ABGR2222 + compression is *far* smaller than
  a PNG planet, so run the numbers before dismissing it.
- **W-F — Catalog of pre-built packs.** No build step at all: publish city / park /
  region packs as static files with a browsable index, owner downloads the nearest one.
  Simplest possible thing that could work; least flexible. Worth scoring precisely because
  it may beat everything on time-to-first-map.
- **W-G — Companion-app transfer.** `Docs/deploy.md` references a UNA mobile app that
  matches `.uapp` versions. Find out whether it can move arbitrary files, and at what
  throughput. Then do the arithmetic: a 5 MB pack and a 45 MB pack over BLE are different
  products. Note `U13` — the format has no verifiable prefix, so resumable chunked
  transfer has no integrity story today; a 45 MB BLE transfer that can't resume is a
  workflow-killer, and that's a spec finding.
- **W-H — On-watch download.** Only if a radio beyond BLE exists. Check; if not, close it
  with one line.

Combinations are likely the real answer (e.g. hosted archive + browser slicer + USB
sideload, with a companion-app path for small packs). Score combinations, not just atoms.

**Score every candidate on:** legal compliance (gate — pass/fail, with the quote);
reach (what fraction of owners can actually complete it, and what they need to own);
time from "I want a map" to "it's on my watch"; cartographic quality ceiling; coverage
and zoom reach at acceptable pack size; infra cost at three scales; software that must be
built (and its risk — note `PLAN.md` Phase 2's renderer is 4–6 weeks with a live API
risk); maintenance burden (who re-renders when OSM changes); and failure modes, including
what happens at a trailhead with no signal.

## 7. Experiments charter

Each is a `Docs/Investigations/` bundle; each changes a real decision. Cut what you must
and say what you cut — a cut experiment with a reason beats a faked one.

- **X1 — Palette + style spike.** Design a candidate 64-entry palette; render one real
  area (urban and trail) at 240×240 through a candidate substrate; place side by side with
  Garmin / Coros / Bangle.js at 1:1. Deliverable: PNGs, the palette as hex + ABGR2222
  bytes, and a verdict on palette-first vs quantise-after.
- **X2 — Quantise fidelity vs size.** 1× / 2×-downsample / dithered, each measured for
  visual verdict *and* RLE ratio. Decides the render pipeline and half the size budget.
- **X3 — Compression re-measure on real content.** `RAWTILES_SPEC_ADEQUACY.md` § 5's
  13.0% is n = 12 tiles of one style and is flagged `PLAUSIBLE`, not `CONFIRMED`. Re-run
  `compress.py` (in `una-sdk/Docs/Investigations/2026-08-06-rawtiles-spec-adequacy/`)
  against your actual candidate style, a contour-heavy topo, and a satellite source, at
  128 px and 256 px. Decides whether the workflow's zoom targets require a spec change.
- **X4 — Browser render throughput.** Can a browser render + quantise + pack an
  Athens-sized region in a tolerable time? Measure tiles/s and peak memory; test whether
  large-canvas chunking fixes seams and labels. Decides W-C.
- **X5 — Global archive arithmetic and cost.** Tile counts, bytes after your measured
  compression ratio, render CPU-hours, storage and egress per month at three scales.
  Decides W-E/W-F.
- **X6 — Transfer, end to end.** USB-MSC copy + verify time for 5 / 15 / 45 MB;
  companion-app capability and BLE throughput if it can carry files; the integrity story
  (`U13`) for whichever channel wins. Include the BLE-sync/USB-MSC corruption interlock.
- **X7 — Blit-time palette LUT on device.** Prove the 64-entry restyle in the AthensRun
  PoC and measure the per-frame cost. If it works, it changes how many packs the product
  needs, which changes every size and transfer number above.

## 8. Known premise traps

Each of these has already cost someone time.

- **The Athens pack is evidence, not a template.** Its source violates § 4.1.
- **"256 KB app RAM" is a stale Mermaid label** in `Docs/architecture-deep-dive.md`. Real
  budgets: 500 K service / 600 K GUI (`cmake/una-app.cmake:217,:281`).
- **Spec version drift.** slippypack's docs reference spec 0.5; the una-sdk investigations
  reference 0.6 and 0.7; `DECISIONS.md` has an entry rebranding the document `v1.0-rc1`.
  Pin the actual HEAD of the `rawtiles` repo before you quote a single `§`.
- **Deflate is not in the format.** § 5's best compression number is measured, not
  specified — the format defines `None` and pixel-RLE, and reserves QOI and LZ4. Any size
  budget built on deflate carries a spec dependency; say so where you use it.
- **RGB565 is not an upgrade here.** The watch framebuffer is ABGR2222; RGB565 doubles
  pack size and buys nothing on this device (it exists for other targets). Don't reach for
  it to "improve quality" — the quality lever is the palette and the style, not the depth.
- **Tile count is not pack size.** Compression is per-tile and content-dependent across a
  ~6× range. Never quote a size without the style and the codec that produced it.
- **`pack_uuid` is not a byte-identity.** `M1` in the adequacy audit: two packs from the
  same source descriptor shared a UUID while differing 6.9× in size with no tile bytes in
  common. Any dedup, catalog, or "you already have this" feature that assumes otherwise is
  building on a known defect — design around it and cite it.
- **North-up only.** `blitCopy` can't rotate. Don't design a track-up experience.
- **"Any UNA owner" means global.** The workflow has to work for someone in Jakarta or
  Kraków, not just Ontario: worldwide coverage, non-Latin labels, and an owner who never
  reads documentation. A design that only demonstrably works for one town in Ontario has
  answered a smaller question than the one asked.

## 9. Deliverables

1. **`MAP_DELIVERY_WORKFLOW.md`** (slippypack repo root, untracked). Verdict first: the
   **one recommended workflow**, the runner-up, and the specific condition under which the
   runner-up wins. Then the scored candidate matrix, the owner's numbered path through the
   recommendation, and the ranked risk list.
2. **Compliance appendix.** Every source considered as a row: name, URL, retrieval date,
   verbatim governing clause, verdict (`PERMITS` / `PROHIBITS` / `AMBIGUOUS` /
   `BLOCKED-PENDING-PERMISSION`), the exact attribution string a derived pack must carry,
   and any obligation that lands on the watch UI rather than the website. Plus a short,
   precise list of what needs actual legal review.
3. **Cartography spec.** The 64-entry palette (hex + ABGR2222 byte per slot), per-zoom
   feature and label rules, line weights and casing, the render-and-quantise pipeline, the
   terrain decision, and the activity-variant mechanism (palette LUT vs separate raster,
   per activity). Precise enough that two people implementing from it produce the same map.
   With comparison images.
4. **Implementation plan mapped onto what exists.** Which `slippypack` phases survive,
   which are cut, which are new; what the watch app or SDK must provide (pack discovery,
   palette LUT, pack picker, attribution display); what infrastructure exists and what it
   costs per month; and the change list routed to the `rawtiles` repo with each item's
   blocking relationship to the workflow stated.
5. **Spin-off list** — anything real but out of scope, with its target repo and branch.
6. **Investigation bundles** for everything settled by experiment.

**What "done" looks like:** someone who has never seen this project reads the report and
can (a) follow an ordinary watch owner's path from "I want a map of Saturday's run" to
"the map is on my watch," with the software that exists and the software that must be
built named at every step and costed; (b) satisfy themselves that every source in the
pipeline permits what we do with it, because the clause is quoted rather than
characterised; and (c) look at an image and agree — or disagree with specifics — that this
is the best map on a watch at this price. No claim carrying only vibes.
