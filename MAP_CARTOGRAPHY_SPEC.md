# Cartography spec — the UNA watch basemap

Companion to `MAP_DELIVERY_WORKFLOW.md`. Evidence:
`Docs/Investigations/2026-08-07-watch-cartography/` (experiments E1–E7, images, scripts).

The target is precise: **240 × 240, 8 bpp ABGR2222, Sharp LS012B7DD06A reflective
memory-in-pixel panel, 64 colours native, 25:1 contrast, 18 % NTSC gamut, no rotation.**

This document is meant to be implementable: two people working from it should produce the
same map. Where I am specifying a judgement rather than a measurement, it says so.

---

## 1. The five facts that determine every decision below

From the Sharp device specification `LCP-2619063C` Table 7-1 and § 2, and from the
colorimetric model built on it (E1):

1. **ABGR2222 is the panel's native format**, not a compression choice — "1 pixel has RGB
   each 2bit, the pixel can display 64 colors." Quantisation to 64 is *exact*. There is no
   higher-fidelity option on this hardware, which is why RGB565 is not an upgrade: it
   doubles pack size and the panel discards the extra bits.
2. **The tonal range is L\* 23.7 → 100, not 0 → 100.** Reflectance runs 0.336 % to 8.4 %
   (CR 25:1). There is no deep black and there never will be.
3. **Only 3 of 64 codes are darker than L\* 40**, and only one of them (`0xC0`) is neutral.
   The other two are strongly chromatic. **There is no neutral dark grey between L\* 23.7
   and L\* 66.5.**
4. **Green is the lightness channel** — 66.8 % of luminance in 4 steps. `g` sets the tier;
   `r` and `b` shift hue within it.
5. **The panel holds its image for 11 µW.** A static map is free; only redraw costs power.

Everything that follows is a consequence of 2, 3 and 4.

---

## 2. Palette-first, and why quantise-after loses

Measured, on the shipped reference basemap (E2):

> `athens.rawtiles` places **99.92 % of its pixels above L\* 80**, and uses the panel's
> maximum-contrast code for **0.007 %** of them. Its roads are drawn at **1.02 : 1**
> against the background; its labels at **2.8 : 1**. The panel can do **25 : 1**.

That is not a quantiser defect. osm-carto encodes hierarchy in *hue* and holds lightness
nearly constant — correct for a bright emissive sRGB screen, close to worst-case for a
reflective panel where hue is the weak axis (18 % NTSC) and lightness is the strong one.

Measured, on identical geometry (E4): rendering **palette-first** — designing the 64-entry
palette, then constraining the style to emit only palette colours — uses **5.3× more of
the ink range** (5.17 % vs 0.98 % of pixels below L\* 50) at **no cost in bytes**
(15.1 % vs 14.8 % spec-RLE).

Independently (E6): a blit-time palette LUT **cannot restyle an anti-aliased pack** —
label text disintegrates into speckle, because a semantic class does not own a code once
anti-aliasing smears it across several. It restyles a palette-first pack cleanly. So
palette-first is also the precondition for day/night/activity variants being free.

**Verdict: palette-first. Both arguments — contrast and restylability — point the same
way, and so does the compliance constraint, which forces us to own the renderer anyway.**

Side by side: `Docs/Investigations/2026-08-07-watch-cartography/images/x12_arms.png`
(colour, 2× nearest) and `x12_arms_1to1.png` (1:1 — judge stroke weights on this one).

---

## 3. The palette

64 entries exist; **14 slots are spent**, drawn from the ~29 codes that are mutually
separable at ΔE2000 ≥ 10 (E1). The remaining 50 codes are deliberately unused — they are
the headroom for terrain ramps (§ 6) and future overlays, and leaving them empty is what
keeps RLE runs long.

`preview` is the sRGB rendering of what the eye sees on the panel once adapted (E1's
model), **not** an sRGB colour to paste into a style file. The authoritative value is the
ABGR2222 byte.

| slot | byte | r g b | L\* | a\* | b\* | preview | CR vs paper | role |
|---|---|---|---:|---:|---:|---|---:|---|
| `paper` | `0xFF` | 3 3 3 | 100.00 | 0.00 | 0.00 | `#FFFFFF` | 1.0 | ground; also text halo |
| `landuse` | `0xEE` | 2 3 2 | 95.74 | −6.74 | 5.34 | `#E9F6E8` | 1.1 | built-up wash, one step off paper |
| `wood_lt` | `0xDD` | 1 3 1 | 91.12 | −14.94 | 12.08 | `#D0EDCD` | 1.3 | large forest blocks |
| `building` | `0xEA` | 2 2 2 | 86.01 | 0.00 | 0.00 | `#D7D7D7` | 1.5 | context only |
| `wood` | `0xD8` | 0 2 1 | 77.32 | −22.35 | 5.09 | `#93CAB5` | 1.9 | strongest usable green |
| `water` | `0xF4` | 0 1 3 | 70.40 | −19.37 | −22.60 | `#62B7D5` | 2.8 | lake/sea fill |
| `contour` | `0xC5` | 1 1 0 | 62.24 | 2.93 | 16.00 | `#A5947A` | 4.1 | terrain lines |
| `water_dk` | `0xF0` | 0 0 3 | 51.77 | −12.32 | −40.80 | `#0084C2` | 5.0 | waterway lines, lake edge |
| `trace` | `0xC3` | 3 0 0 | 51.76 | 55.00 | 16.58 | `#D24F62` | 5.0 | **app-drawn** GPS trace |
| `road_minor` | `0xC1` | 1 0 0 | 36.58 | 31.86 | 8.76 | `#874149` | 10.7 | second ink, warm |
| `path` | `0xD0` | 0 0 1 | 36.58 | −6.74 | −24.30 | `#285B7D` | 10.7 | second ink, cool |
| `road_major` | `0xC0` | 0 0 0 | 23.67 | 0.00 | 0.00 | `#383838` | **25.0** | the darkest thing on the map |
| `ink` | `0xC0` | 0 0 0 | 23.67 | 0.00 | 0.00 | `#383838` | **25.0** | label text |
| `halo` | `0xFF` | 3 3 3 | 100.00 | 0.00 | 0.00 | `#FFFFFF` | — | = `paper` |

Full 64-code reference with L\* and byte for every slot:
`images/palette_chart.png`. Regenerate with `scripts/chart.py`.

**Rules the table encodes, which are the actual specification:**

- **R1 — The three dark codes are a budget, and roads and text spend it.** `0xC0` goes to
  the road you are on and to label text. `0xC1` and `0xD0` are the only other inks; they
  differ from `0xC0` by 2.3:1 in luminance, so they are distinguished from it by **hue,
  not weight**. Warm (`0xC1`) = vehicular; cool (`0xD0`) = foot/trail. That pairing is a
  judgement, but the constraint that forces a hue split is measured.
- **R2 — Never encode a must-notice distinction as green-vs-paper.** `wood_lt` against
  `paper` is **1.3 : 1**. Green area fills are ambient context. Anything a runner must
  see is ink or water-blue.
- **R3 — Contrast floor 4:1 for anything a moving user must read**, 1.5:1 for ambient
  fills that only need to not-be-paper. `contour` at 4.1:1 sits exactly on the floor; that
  is deliberate — contours must read without competing with roads.
- **R4 — One code per feature class, no exceptions.** This is what makes § 5's LUT work.
- **R5 — `trace` is never baked into a pack.** It is drawn by the app over the blit, as
  `poc/athensrun` already does. It gets the panel's most saturated code (C\* 57.4) because
  it must win against every basemap colour.

---

## 4. Line weight, casing, and text — no anti-aliasing available

At 240 × 240 on a 30.24 mm panel the pixel pitch is 126 µm, and there are no intermediate
shades to soften a stroke. Every edge is hard. The following are **judgements informed by
the 1:1 renders**, not measurements, and are the part of this spec most likely to change
after a hardware legibility trial.

| element | stroke | casing | note |
|---|---|---|---|
| major road | **4 px** `road_major` | 7 px `halo` (⇒ 1.5 px halo each side) | halo is `paper`, so it reads as a gap, not a line |
| minor road | **2 px** `road_minor` | 5 px `halo` | |
| path / trail | **2 px** `path`, dashed 3 on / 3 off | none | dash is the trail signifier; hue is the confirmation |
| waterway | 2 px `water_dk` | none | |
| contour | **1 px** `contour` | none | 1 px is the floor; see below |
| lake / sea | fill `water` | 1 px `water_dk` edge | |

- **1 px is the absolute minimum and is reserved for contours.** With no anti-aliasing a
  1 px line is a full-contrast hairline that either exists or does not — which is why it
  works for contours (a texture, read collectively) and fails for roads (individual
  features, read singly). Roads start at 2 px.
- **Casing inverts the usual convention.** On a light-background emissive map, roads are
  light with dark casing. Here there is no second dark tone to case with (fact 3), so
  **the road is the dark thing and the casing is a `paper` halo** — it separates the road
  from adjacent fills by a gap rather than by an outline. This is also why casing costs
  nothing in palette slots.
- **Text: 11–12 px bold, `ink`, with a 1 px `halo` ring, rendered aliased.** The halo is
  the same code as `paper`, so it costs no slot and it keeps text legible over `wood`,
  `landuse` and `building` fills. Aliased text at 11 px is blocky but readable at 1:1
  (`images/x12_arms_1to1.png`); anti-aliased text is what broke E6's restyle.

---

## 5. Labels: baked, for v1

`MAP_DELIVERY_PROMPT.md` § 5 asks this be decided with a mock of both. The mock exists for
baked (all E4/E6 images); the app-drawn arm was **not** mocked, so this decision is
`PLAUSIBLE` and is the weakest recommendation in this document.

**Bake labels into the raster for v1.** Reasons:

- The claimed cost of baking — "burns palette slots on text antialiasing" — **does not
  apply once text is aliased**. Aliased text uses `ink` and `halo`, both of which are
  already spent on other things. It costs **zero additional slots**.
- The claimed cost "can't stay upright under any future rotation" is moot: `blitCopy`
  cannot rotate, so the product is north-up only and there is no future rotation to
  protect against.
- App-drawn labels need a data channel the format does not have — a per-tile label list
  with positions, priorities and collision boxes. That is a wire-format addition
  (`RAWTILES_SPEC_ADEQUACY.md` has no matching need; it would be new), plus a label-placement
  engine inside a 600 K GUI budget. Large cost, and it is on the critical path for nothing.
- Baked labels do bloat every zoom, and that is the real cost — but at E3's measured
  ratios the whole Athens region is 0.8 MiB, so there is no size pressure to relieve.

**Revisit when** either (a) a second locale must render from the same pack — baked labels
are frozen at build time, so a Kraków owner who wants Latin transliteration and a local
who wants Polish need two packs today; or (b) label legibility fails a hardware trial and
scalable text becomes necessary. (a) is the likelier trigger and it is a real limitation of
this recommendation for the "any UNA owner means global" requirement — see
`MAP_DELIVERY_WORKFLOW.md` § 9 risk R5.

---

## 6. Terrain — designed in, deferred out of v1

Hillshade and contours carry no tile-server policy problem (Copernicus DEM, USGS 3DEP are
open bulk downloads) and are, for trail use, plausibly the largest single legibility gain
available. They are nonetheless **out of v1**, for a reason that is measured rather than
scheduling-driven:

- **Hillshade is a continuous tone ramp, and this panel has four levels.** A hillshade
  rendered into the available neutral ramp (L\* 23.7 / 66.5 / 86.0 / 100.0 — steps of
  42.8, 19.5, 14.0) will band severely, and the only fix is dithering, which E4 measured
  at **5.8× the pack size** and which is visible at 1:1. A hillshade that avoids both
  banding and dithering needs intermediate tones the hardware does not have.
- **Contours are cheap and work now.** They are 1 px lines in one code (`0xC5`), they cost
  ~4 % of pixels in the E4 scene, and they compress well because they are sparse.

**Therefore: contours in v1.5 (style change only, no new machinery beyond a DEM ingest);
hillshade not before a hardware trial proves a 4-level ramp can carry it.** If it can, the
50 unspent codes are where its ramp lives — a warm 4-step ramp in the `g`-tier structure
(fact 4) is the shape to try.

---

## 7. Zoom ladder

**z12–16, with z11 added for orientation.** The Athens pack's z12–16 choice is justified;
here is the justification it was missing. At latitude 45°:

| zoom | m/px | 240 px spans | use |
|---:|---:|---:|---|
| z11 | 54 | 13 km | "where am I in the region" |
| z12 | 27 | 6.5 km | route overview |
| z13 | 13.5 | 3.2 km | |
| z14 | 6.75 | 1.6 km | default running zoom |
| z15 | 3.38 | 811 m | |
| z16 | 1.69 | 405 m | junction detail |

z17 (203 m across) is below the useful range for someone moving at 3–5 m/s — the map would
be off-screen before it is read. **z16 is the floor.**

The 4×-per-level fear is overstated: E3 measured RLE ratios falling with zoom (30.3 % at
z12 → 6.0 % at z16), so **adding a zoom level costs ≈ 2.2×, not 4×.**

**`tile_dim = 128`, not the Athens pack's 256.** A 256 px ABGR2222 tile is 64 KiB, and a
240 × 240 viewport can straddle four of them = 256 KiB — 43 % of the 600 K GUI budget
(`una-sdk/cmake/una-app.cmake:281`) spent on tile cache alone. At 128 px a tile is 16 KiB
and the worst case is nine tiles = 144 KiB. It also lifts the 4 GiB cap's tile ceiling from
65,516 to 261,824 (`RAWTILES_SPEC_ADEQUACY.md` E2).

---

## 8. Render and quantise pipeline

```
vector tiles (Protomaps PMTiles / OpenFreeMap MBTiles, local copy)
  └─ MapLibre, watch style JSON  ── colours declared as the § 3 palette's sRGB previews
       └─ render at 1×, 128 px tiles, fill-antialias: false where available
            └─ QUANTISE: snap to nearest of the N DECLARED SLOTS  ← not nearest-of-64
                 └─ ABGR2222 buffer, N codes exactly
                      └─ spec § 9.11 pixel RLE
                           └─ .rawtiles
```

Four rules, each measured:

- **Render at 1×.** Not 2×-and-downsample: E4 measured that at 30 codes and 1.5× the bytes,
  and the apparent depth it buys on an emissive display does not exist here.
- **Never dither.** E4: 87.5 % of raw, 5.8× palette-first, visible cross-hatch at 1:1.
- **Snap to declared slots, not to nearest-of-64.** E5: anti-aliasing perturbs 0.50 % of
  pixels; snapping to the N style slots recovers exactly the no-anti-aliasing result —
  same code count, same LUT behaviour, +0.2 % bytes. **This is ~10 lines in the quantiser
  and it removes "write a custom aliased rasteriser" from the Phase 2 risk list.**
- **Compression `RLE`, per spec § 9.11.** E3 measured 7.7 % aggregate on real content
  (6.0 % at z16). Do not build a size budget on deflate — it is not in the format.

`slippypack-core` already has a quantiser; the change is a `--palette <slots>` mode that
snaps to a declared set instead of the full cube.

---

## 9. Activity differentiation — one pack, four LUTs

E6 proved the mechanism in simulation, on palette-first content, from a **single
unmodified pack**: `images/x7_lut_variants.png`.

| variant | mechanism | what changes |
|---|---|---|
| **Day** | identity LUT | as authored |
| **Night** | 64-entry LUT | `paper`→`0xC0`, roads→`0xFF`, fills→`0xC1`/`0xD0`. Inverted maps need *one* dark tone for ground and many light ones for features — which is the shape this panel has |
| **High contrast** | 64-entry LUT | everything collapses to ink / paper / water. 5 codes |
| **Trail** | 64-entry LUT | buildings and landuse → `paper`; `road_minor` → `0xEA` (recedes); `path` → `0xC0` (promoted to primary); contours → `0xC1` |

**The judgement the prompt asks for: one excellent pack, four LUTs. No activity needs its
own raster.** Road-running, trail, hiking and cycling differ in *emphasis*, and emphasis is
exactly what a per-code LUT changes — the Trail variant above demotes roads and promotes
paths without touching a byte of the pack. The two candidates that would genuinely need
different rasters are **ski** (piste geometry absent from the base style) and **water**
(bathymetry, marine marks); both are v2 questions and both would be *additional* packs,
not variants of this one.

Cost of the mechanism: one 64-byte table per variant, one indirection per pixel at blit
time. **The per-frame cost on the real `blitCopy` path is unmeasured** — E6 is a
simulation. That measurement (charter X7) is the last thing standing between this
recommendation and "confirmed"; it needs the AthensRun PoC on hardware.

A dark-background design needing *several* dark tones is not available on this hardware at
all. Night mode works because it needs one.

---

## 10. What this spec does not establish

**I did not produce the 1:1 benchmark against Garmin, Coros, Suunto and Bangle.js, and
therefore I do not claim this is the best map on a watch at this price.** The prompt is
right that such a claim without a comparison image is not a finding, so it is not made.

What is blocking it: obtaining those vendors' basemap renders means either fetching their
copyrighted tile output (which is the same compliance problem this whole report is about)
or photographing the devices, which I cannot do. **What would settle it:** photograph a
Garmin Fenix/Forerunner, a Coros Apex and a Bangle.js 2 showing the same location at the
same scale in the same light, crop to 240 × 240, and place them beside
`images/x12_arms_1to1.png`. That is an afternoon with three watches, and it should happen
before any marketing claim is made.

What *can* be said from evidence: this design uses **5.3× more of the panel's contrast
range** than the current reference basemap (E2, E4), and the current reference basemap
draws its roads at 1.02 : 1. Bangle.js's `openstmap` is 3 bpp — 8 colours against this
panel's 64 — so the comparison there is about palette discipline rather than depth, and
their loader rasterises in the browser, which is the same architectural move recommended
as the runner-up in `MAP_DELIVERY_WORKFLOW.md`.

Also unestablished, and listed so nobody mistakes them for settled:

- The four area-gradation levels are **assumed** to be equal steps (E1 "Open" § 1).
- ΔE ≥ 10 as the at-a-glance separation threshold is reasoned, not measured.
- Line weights in § 4 are judgements from 1:1 renders, not from a hardware trial.
- E4's byte figures come from **synthetic geometry**; E3's real-content ratios are the
  ones to quote.
- The app-drawn-label arm of § 5 was never mocked.
