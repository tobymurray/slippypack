# 2026-08-07 — Watch cartography, panel colorimetry, and compression re-measure

**Subject.** The map-delivery-workflow evaluation (`MAP_DELIVERY_WORKFLOW.md`, repo root).
**Repos read:** `slippypack` @ `1f9132d`, `una-sdk` @ `59210889`, `rawtiles` @ `38d4d26`
(spec document **v0.6**, wire `(1,0)`, Status: Provisional — cloned read-only into a
scratch directory).
**Nothing was posted to GitHub.** `gh` was used once, read-only, to resolve the `rawtiles`
default branch. No tiles were fetched from anyone's tile server. Every raster byte
analysed here came from the committed `athens.rawtiles` fixture or was generated locally.

Eight experiments. **Two of my own hypotheses were falsified** (E2 and E5); both are
recorded, because in each case the falsification changed a recommendation.

---

## The input that reframed the problem

The panel is a **Sharp LS012B7DD06A**. Its device specification (Sharp
`LCP-2619063C`, REV 01-Dec-20 — full text extract committed here as
`LS012B7DD06A-datasheet-text.txt`) settles the two things
`MAP_DELIVERY_PROMPT.md` § 3 listed as "Unknown, worth establishing":

| Datasheet, verbatim | Where | Consequence |
|---|---|---|
| "This TFT 64-Color LCD module is a **reflective** active-matrix with slightly transmissive **memory** liquid crystal display module" | § 1 Applicable | Transflective MIP. Sunlight-legible; needs the frontlight in the dark |
| "**1 pixel has RGB each 2bit, the pixel can display 64 colors.**" | § 2 Overview | ABGR2222 is not a software compromise — it is the **panel's native format**. Quantising to 64 is exact, not lossy |
| "Area gradation of each RGB have 2 dot" | § 6 figure | The 4 levels/channel are **spatial area modulation** ⇒ output is linear in reflectance, **not** sRGB-gamma |
| Contrast ratio CR: min 20, **typ 25** | Table 7-1 | The whole tonal range is 25:1. There is no deep black |
| Reflectivity ratio R: min 7.0, **typ 8.4 %** | Table 7-1 | Panel "white" reflects 8.4 %. White paper is ~90 % |
| NTSC ratio **18 %** | Table 7-1 | ≈ 25 % of sRGB area. Saturated colours render as pastels |
| Hold mode (no update) **11 µW** typ; 30 Hz update **500 µW** typ | Table 6-7-1 | A static map is free to keep on screen. Only panning costs power |

The SDK instantiates exactly this part: `una-sdk/Docs/architecture-deep-dive.md:383`
(`mpLcd = new LS012B7DD06A();`), consistent with the 240×240 / 8 bpp / ABGR2222
framebuffer at `una-sdk/Docs/TouchGFX-Port-Architecture.md:24,395`.

**No network radio beyond BLE appears anywhere in the SDK's hardware discovery**
(`architecture-deep-dive.md:375-395` probes LCD, backlight, BLE, GPS and nothing else),
which closes candidate workflow W-H.

---

## E1 — What are the 64 colours actually worth? (`scripts/panel.py`, `tone.py`, `contrast.py`)

**Method.** Build a colorimetric model of the panel from the datasheet's measured
primaries. Solve the three primary luminances that reproduce the measured white
(`xyY_to_XYZ` + a 3×3 solve), add the black floor implied by CR = 25, then evaluate all
64 codes in CIELAB and CIEDE2000. Assumption, stated: the four area-modulated levels are
equal steps 0, ⅓, ⅔, 1 — see "Open" below.

**Log.**

```
Luminance split R:G:B = 0.166 : 0.668 : 0.166   (sRGB/BT.709 = 0.213 : 0.715 : 0.072)

Neutral ramp:  (0,0,0) L*=23.67   (1,1,1) L*=66.52   (2,2,2) L*=86.01   (3,3,3) L*=100.00
                        +42.8              +19.5              +14.0

L* spans 23.7 .. 100.0            codes with L* < 40:  3 of 64
                                  codes with L* < 50:  6 of 64
                                  codes with L* > 80: 30 of 64

pairwise dE2000 over 2016 pairs: min 5.24, median 28.54, max 66.01
max mutually-separable subset:  dE>=5 -> 64,  dE>=8 -> 37,  dE>=10 -> 29
```

**Verdict — CONFIRMED.** Four findings, in order of consequence:

1. **The palette is top-heavy to the point of deformity.** Only **3 of 64 codes** are
   darker than L\* 40, and only one of those three is neutral (`0xC0`). The other two
   (`0xC1` warm, `0xD0` cool, both L\* 36.6) are strongly chromatic. **There is no
   neutral dark grey between L\* 23.7 and L\* 66.5.** Any design that wants a black road
   *and* a dark-grey casing cannot have both — the second dark must differ by hue.
2. **Green is the lightness channel.** It carries 66.8 % of luminance across only four
   levels, so `g` alone sets the tier: g=0 → L\* 23.7–66.4, g=3 → L\* 86.1–100.0. The
   useful mental model is **g = lightness (4 steps), r and b = hue (16 per step)**.
3. **Blue is 2.3× more luminous, relative to the others, than it is in sRGB.** A colour
   picked as "dark blue" on a monitor arrives on this panel noticeably lighter.
4. All 64 codes are mutually distinguishable in careful side-by-side viewing
   (min ΔE2000 = 5.24) — but only **29** survive a ΔE ≥ 10 separation, which is the
   defensible threshold for *adjacent area fills read at a glance, at arm's length, on a
   1.19″ panel*. **Design against ~29, not 64.** The ΔE ≥ 10 threshold is `PLAUSIBLE`
   (CIEDE2000 is calibrated for large patches under reference viewing, not for 1–2 px
   features on a wrist); a supervised legibility trial on hardware would settle it.

Contrast budget (reflectance ratios, from `contrast.py`):

| pair | CR |
|---|---|
| ink `0xC0` vs paper `0xFF` | **25.0 : 1** |
| second ink `0xC1`/`0xD0` vs paper | 10.7 : 1 |
| second ink vs ink | 2.3 : 1 |
| water `0xF0` vs paper | 5.0 : 1 |
| **park green `0xDD` vs paper** | **1.3 : 1** |

Green fills separate from paper almost entirely by hue, and hue is the weak axis at
18 % NTSC. **Do not encode anything a runner must notice as green-vs-paper.**

---

## E2 — Does the reference basemap use any of that range? (`scripts/codes.py`, `athens_look.py`)

**Falsified hypothesis (recorded — it changed a recommendation).** I predicted from the
whole-pack histogram that the Athens pack would be visually near-blank: 53 % of pixels
are `0xFF` and 36 % are `0xEA`, two codes 1.5:1 apart. I was wrong about the *appearance*
— rendered through the panel model (`images/athens_z16_viewport.png`) the map is
perfectly legible, with roads, buildings and labels all clearly present. Structure lives
in the 10 % tail, not the mode.

**What is true, and is worse, is the tonal placement.**

```
share of pixels using the darkest code 0xC0 (L*23.7, 25:1) : 0.00695 %
share with L* < 50 (any real "ink")                        : 0.00699 %
share with L* > 80 (all "paper")                           : 99.91675 %
distinct codes used, whole pack                            : 27 of 64
```

**Verdict — CONFIRMED.** The reference basemap places **99.92 % of its pixels above
L\* 80** and uses the panel's maximum-contrast code for **0.007 %** of them. Roads are
`0xEF` (L\* 97.9) against a `0xFF` (L\* 100) background — a **1.02 : 1** feature-to-ground
contrast — and label text is `0xD5` (L\* 66.5), **2.8 : 1**. On a display capable of
25 : 1 the shipped reference cartography exercises roughly 3 : 1.

Most of this is the style, not the quantiser. It is what happens when a style tuned for a
bright emissive sRGB screen — osm-carto encodes hierarchy in *hue* (yellow roads, pink
buildings, green parks) and keeps lightness nearly constant — is transplanted onto a
reflective panel where hue is the weak axis and lightness is the strong one. **osm-carto
is close to a worst-case style for this hardware**, and that is the measured case for
palette-first.

**Amended after E8:** the quantiser is a *second, independent* contributor, stacked on top
of the style. Spec § 9.1.1's canonical thresholds assume the quanta display as
{0, 85, 170, 255}; on this panel they display as ≈ {56, 162, 215, 255}, so the quantiser
pushes 93.2 % of colours a level too light — in the same direction the style already
erred. See E8.

---

## E3 — Compression, re-measured on 687 real tiles (`scripts/athens_compress.py`)

`RAWTILES_SPEC_ADEQUACY.md` § 5 measured n = 12 tiles of one style and flagged the result
`PLAUSIBLE`. Re-run against all **687** tiles of `athens.rawtiles` (45,023,232 raw tile
bytes, 256 px, ABGR2222), using a spec-§ 9.11 pixel-level RLE implementation.

| scheme | bytes | % of raw | × |
|---|---:|---:|---:|
| None (as shipped) | 45,023,232 | 100.0 % | 1.00 |
| **spec RLE (§ 9.11)** | **3,460,688** | **7.7 %** | **13.01** |
| deflate-9, 32 KiB window | 1,407,051 | 3.1 % | 32.00 |

| zoom | tiles | raw MiB | RLE % | deflate % |
|---:|---:|---:|---:|---:|
| 12 | 6 | 0.38 | 30.3 % | 12.5 % |
| 13 | 16 | 1.00 | 19.6 % | 8.5 % |
| 14 | 42 | 2.62 | 14.4 % | 6.2 % |
| 15 | 143 | 8.94 | 9.0 % | 3.8 % |
| 16 | 480 | 30.00 | **6.0 %** | 2.4 % |

**Verdict — CONFIRMED, and it corrects the number the whole size budget was built on.**
`MAP_DELIVERY_PROMPT.md` § 3 carries "spec RLE measured 32.4 % ⇒ ~14.6 MiB for Athens".
The real figure for that pack is **7.7 % ⇒ 3.3 MiB**, a 4.4× error in the pessimistic
direction. The 32.4 % came from 12 dense low-zoom Stanley Park tiles.

Two structural corrections follow:

- **The RLE ratio is strongly zoom-dependent** (30.3 % at z12 → 6.0 % at z16). Higher
  zooms multiply tile *count* by 4× but the bytes per tile fall, so **adding a zoom level
  costs ~2.2×, not 4×**. z16 holds 70 % of Athens' raw bytes but only 52 % of its
  compressed bytes.
- **The case for adding deflate/LZ4 to the format is much weaker than the adequacy audit
  implied.** At the corrected baseline the gap between shipped RLE and deflate is 2.5×,
  not 12×, and it buys nothing that changes a workflow decision. See the change list in
  `MAP_DELIVERY_WORKFLOW.md` § 8.

---

## E4 — Palette-first vs quantise-after, and the render pipeline (`scripts/scene.py`, `x12.py`)

**Method.** One synthetic scene — road hierarchy with casing, lake, river, woodland,
contours, buildings, a dashed trail, three haloed labels — rendered four ways at 240×240
and measured for code count, spec-RLE ratio and ink usage.
**Limitation, stated up front:** the geometry is synthetic, not real OSM. It was built to
exercise the real cases, and the *palette* conclusions transfer, but **the byte sizes here
must not be quoted as product numbers** — E3's real-content ratios are the ones to use.

| arm | codes | RLE | % raw | × | pixels < L\* 50 |
|---|---:|---:|---:|---:|---:|
| **B palette-first, no AA** | **11** | 8,683 | **15.1 %** | 6.63 | **5.17 %** |
| A1 quantise-after, 1× + AA | 18 | 8,515 | 14.8 % | 6.76 | 0.98 % |
| A2 quantise-after, 2× → downsample | 30 | 13,152 | 22.8 % | 4.38 | 0.79 % |
| A3 quantise-after, 1× + ordered dither | 28 | 50,409 | **87.5 %** | 1.14 | 0.89 % |

Images: `images/x12_arms.png` (2× nearest, for colour), `images/x12_arms_1to1.png`
(1:1, for line weight — judge stroke decisions on this one).

**Verdict — CONFIRMED, three separate decisions settled:**

1. **Palette-first costs nothing in bytes and buys 5.3× the ink range.** 15.1 % vs 14.8 %
   on identical geometry is inside the noise of a single scene; ink usage goes 0.98 % →
   5.17 %. The § 5 hypothesis holds, and the reason it holds is E1: the win comes from
   deliberately spending the three dark codes, which no quantiser will do for you because
   the source style never asked for them.
2. **Do not render at 2× and downsample.** It inflates the code count 11 → 30 (downsampling
   manufactures intermediate colours the palette was designed to avoid), costs **1.5× the
   bytes**, and on a panel with no intermediate shades the "apparent depth" it buys on an
   emissive display does not materialise.
3. **Do not dither. Ever.** 87.5 % of raw — **5.8× the palette-first size** — and the
   cross-hatch is plainly visible at 1:1 in `x12_arms.png` panel 4. This confirms the
   § 5 prediction with a number.

---

## E5 — Can a normal anti-aliasing renderer do palette-first? (`scripts/x2b.py`)

**Falsified hypothesis (recorded — it changed a recommendation).** I concluded from E7
below that palette-first requires a rasteriser that can draw with anti-aliasing *off*, and
that since MapLibre always anti-aliases lines and text, PLAN.md Phase 2 would need a
custom rasteriser. **That is wrong, and the fix is trivial.**

**Method.** Render the same scene in the panel-palette colours *with* anti-aliasing (what
MapLibre would emit), then quantise two ways: nearest-of-64, versus **snapping to the N
colours the style actually declares**.

| arm | codes | RLE | pixels on a palette slot |
|---|---:|---:|---:|
| B palette-first, no AA (reference) | 11 | 15.1 % | 100.00 % |
| C palette colours + AA, nearest-of-64 | 18 | 15.3 % | 99.50 % |
| **D palette colours + AA, snap-to-slots** | **11** | **15.3 %** | **100.00 %** |

**Verdict — CONFIRMED.** Anti-aliasing perturbs only **0.50 %** of pixels (the edges), and
**quantising to nearest-of-N-declared-slots instead of nearest-of-64 recovers the
palette-first result exactly** — same code count, same LUT behaviour, +0.2 % bytes. This
is roughly ten lines in the quantiser and it removes "write a custom aliased rasteriser"
from the Phase 2 risk list. MapLibre's `fill-antialias: false` helps for polygons but is
not needed once snap-to-slots is in.

---

## E6 — Does a blit-time palette LUT restyle a pack? (`scripts/restyle.py`, `x7.py`)

`RAWTILES_SPEC_ADEQUACY.md` `U8` says a reader is free to tone-map; `MAP_DELIVERY_PROMPT.md`
§ 5 proposes a 64-entry LUT applied at blit time as a nearly-free differentiation
mechanism. Tested in simulation, on two different kinds of pack content.

**Run 1 — on the real Athens pack.** Map its 27 codes to day / night / high-contrast
palettes. Result: `images/restyle_compare.png`. **It fails.** Labels disintegrate into
scattered black speckle and roads largely vanish.

**Why it fails, which is the useful part:** the Athens raster was produced by an
anti-aliased renderer, so a semantic class does not own a code — text's dark core lands on
`0xD5` while its anti-aliased skirt lands on `0xEA`, `0xEF` and others shared with
unrelated features. A LUT is a *per-code* operation, so remapping a code moves the core
and the skirt inconsistently and the feature falls apart.

**Run 2 — on palette-first content** (arm B, one code per class, aliased). Result:
`images/x7_lut_variants.png` — four variants from **one** unmodified pack:

| variant | codes | mean L\* | pixels < L\* 50 |
|---|---:|---:|---:|
| DAY (as authored) | 11 | 88.33 | 5.17 % |
| NIGHT | 8 | 40.10 | 77.43 % |
| HIGH CONTRAST | 5 | 90.94 | 5.17 % |
| TRAIL (urban clutter dropped, path + contours promoted) | 7 | 91.32 | 5.46 % |

Text stays crisp in all four. An inversion stress test (`images/x2b_lut_survives.png`)
holds for arms B, C and D.

**Verdict — CONFIRMED, with a precondition that is the finding.** The LUT mechanism works,
and it is as cheap as § 5 claims — but **only on packs rendered palette-first with one
code per feature class.** It cannot restyle an anti-aliased pack. This is an independent
second argument for palette-first, arrived at from a different direction than E4's, and
together they make the render pipeline non-optional.

Two caveats not settled here: this is a **simulation**, not a device measurement — the
per-frame cost of the LUT indirection on the real `blitCopy` path is unmeasured (X7 in
the charter remains open, see `MAP_DELIVERY_WORKFLOW.md` § 9); and NIGHT is workable only
because an inverted map needs *one* dark tone for its ground and many light ones for its
features, which is the shape the panel happens to have. A dark-background design needing
several dark tones is not available on this hardware.

---

## E7 — Region and archive arithmetic (`scripts/x5.py`)

Exact Web Mercator tile counts, costed at E3's **measured per-zoom** RLE ratios, at
`tile_dim = 128`.

| region | z12–16 | z10–17 |
|---|---:|---:|
| Saturday run, 8×6 km (Athens ON) | 687 t / **0.8 MiB** | 2,481 t / 2.2 MiB |
| City, 25×20 km (Vancouver) | 2,432 t / **2.9 MiB** | 9,444 t / 8.0 MiB |
| National park (Lake District) | 23,510 t / 27.1 MiB | 92,842 t / 77.3 MiB |
| Metro region, 90 km | 24,832 t / 28.6 MiB | 98,567 t / 82.0 MiB |
| Small country (Switzerland) | 584,765 t / 669 MiB | 2.34 M t / 1.89 GiB |
| England + Wales | 3.24 M t / 3.6 GiB | 12.97 M t / 10.5 GiB |

Global archive, watch-tuned ABGR2222 + spec RLE, `tile_dim = 128`, land fraction 0.148:

```
z0-15 : 377 GB compressed   (3,472 GB raw)
+ z16 : +625 GB             -> 1,002 GB total
```

**Verdict — CONFIRMED (arithmetic; the ratios it consumes are E3-measured).** The
decisive number: **the entire world, watch-tuned, at z0–16, is ~1 TB.** The same
coverage as PNG basemap tiles is tens of TB. Watch-tuned ABGR2222 + RLE is what makes a
global archive a small object rather than a large one.

Monthly hosting, whole planet stored, 2 packs × 8 MB downloaded per owner per month:

| provider | 100 owners | 10,000 | 100,000 |
|---|---:|---:|---:|
| Backblaze B2 ($0.006/GB-mo, $0 egress) | $6 | $6 | $6 |
| Cloudflare R2 ($0.015/GB-mo, $0 egress) | $15 | $15 | $15 |
| AWS S3 + CloudFront ($0.023/GB-mo, $0.085/GB egress) | $23 | $36 | $156 |

Storage prices retrieved 2026-08-07; they are list rates and exclude per-operation
charges, which are immaterial at these object counts. **On a zero-egress provider the
bill does not move between 100 and 100,000 owners** — the archive dominates and it is
fixed. The recurring cost that actually scales is *re-rendering*, not serving: at an
assumed 50 tiles/s/core for a flat style at 128 px, a full-planet z0–16 render is
≈ 4,400 core-hours ≈ **$150–300 per rebuild** on spot compute (`PLAUSIBLE` — the
throughput figure is an estimate, not a measurement; a one-hour benchmark on one region
would settle it).

---

## Cut experiments

- **X4 (browser render throughput)** — cut. It decides W-C, which the compliance work
  demoted to runner-up before throughput mattered. Re-run it only if the recommendation's
  hosted arm is rejected. What it needs: MapLibre GL JS rendering an Athens-sized region
  to canvas, measuring tiles/s and peak memory, plus whether large-canvas chunking fixes
  cross-tile label placement.
- **X6 (transfer, end to end)** — cut for the device half; **not runnable without the
  watch and the UNA mobile app.** `una-sdk/Docs/deploy.md:44` establishes only that a
  mobile app exists and matches `.uapp` versions by `APP_ID`; it documents no file-transfer
  capability at all, and no BLE throughput figure appears anywhere in the SDK. This is the
  single largest open question in the recommendation and it is a question for UNA, not an
  experiment — see `MAP_DELIVERY_WORKFLOW.md` § 9, risk R2.
- **X7 (blit-time LUT on device)** — half done. The mechanism is proven in simulation
  (E6); the per-frame cost on the real `blitCopy` path is not measured. Needs the
  AthensRun PoC on hardware.

## Open — what would settle the assumptions this bundle rests on

1. **The four area-gradation levels are assumed to be equal steps 0, ⅓, ⅔, 1.** The
   datasheet states area gradation with two dots per channel but does not table the
   reflectance of each level. If the two sub-dots are area-weighted 1:2 the steps are as
   assumed; if they are equal-area the panel has 3 real levels and a redundant code.
   **Settled by:** a photometer (or a calibrated camera and a grey card) reading the four
   levels of each channel on hardware. This shifts every L\* in E1 but not the shape of
   the conclusions — green still dominates luminance, the ramp is still top-heavy.
2. **ΔE ≥ 10 as the at-a-glance separation threshold** (E1) — reasoned, not measured.
3. **50 tiles/s/core render throughput** (E7) — estimated, not measured.
4. **E4's byte sizes are synthetic geometry.** Re-measure on real rendered tiles before
   quoting. E3's ratios are the real-content ones.

---

## E8 — Does spec § 9.1.1's canonical quantiser suit this panel? (`scripts/quantiser.py`)

Added after the main run, when the recommendation's format-fit was assessed against
`rawtiles` @ `38d4d26`.

**Method.** § 9.1.1 states the 2-bit quanta are "displayed as `{0, 85, 170, 255}` (8-bit
equivalents)" and places thresholds at 42/127/212 accordingly. E1 established the panel's
four levels are area-modulated, hence linear in *reflectance*. Convert each level's
modelled reflectance back to its sRGB-equivalent code and compare.

**Log.**

```
quantum 0: L*= 23.67  Y/Yw=0.0400  sRGB-equivalent  56.3   spec assumes   0
quantum 1: L*= 66.52  Y/Yw=0.3600  sRGB-equivalent 161.7   spec assumes  85
quantum 2: L*= 86.01  Y/Yw=0.6800  sRGB-equivalent 215.1   spec assumes 170
quantum 3: L*=100.00  Y/Yw=1.0000  sRGB-equivalent 255.0   spec assumes 255

Panel-correct thresholds : [109. 188. 235.]
Spec 9.1.1 thresholds    : [ 42. 127. 212.]

grey values where the two disagree: 151 of 256 (59 %)
  input  43-109: spec picks 1, panel-correct is 0   (spec is too LIGHT)
  input 128-188: spec picks 2, panel-correct is 1   (spec is too LIGHT)
  input 213-235: spec picks 3, panel-correct is 2   (spec is too LIGHT)

random RGB888 colours quantised differently: 93.2 %

mean dE2000 to the intended grey, over the 0-255 axis:
  spec 9.1.1 quantiser :  16.24   worst  44.36
  panel-aware          :   6.65   worst  18.60
```

**Verdict — CONFIRMED.** The canonical quantiser is **correct for a bit-replicating
display path** — a framebuffer whose 2-bit value is expanded to 8 bits and shown on a
normal sRGB display, which is a real and common target and plausibly the one § 9.2's
ST77xx/PineTime note has in mind. It is **wrong for the LS012B7DD06A**, whose area
gradation makes reflectance linear in the code value. The error is systematic and always
in the same direction: **too light**. Mean fidelity loss on the grey axis is **2.4×**.

This is a second, independent cause of E2's washed-out reference basemap, stacked on top
of the style choice: osm-carto puts everything in the top of the range, and then the
canonical quantiser pushes it further up.

**Why it does not block the recommendation:** palette-first declares its colours as
palette slots and snaps to the nearest declared slot (E5), so the canonical RGB888 path is
never invoked on a colour it can get wrong. The defect bites naive-quantisation workflows
only — which is what the Athens pack is.

**The architectural point.** A display's transfer function is a property of the *target
panel*, not of the writer. § 9.1.1 welds one into the canonical quantiser and § A.3 ties
it to `quantiser_version`, which makes the format's identity contract quietly
device-specific for a format that explicitly courts several devices. The machinery to fix
this already exists — `quantiser_version` is per-pixel-format and versioned — so the change
is additive: keep `1` for bit-replication targets, add `2` for linear-reflectance targets.

**Caveat.** This inherits E1's stated assumption that the four area-gradation levels are
equal steps 0, ⅓, ⅔, 1. If the sub-dots are weighted differently the exact thresholds move;
the *direction* of the error does not, because any area-modulated ramp is linear in light
while § 9.1.1's is linear in sRGB code. A photometer reading of the four levels settles the
exact numbers.
