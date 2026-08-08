# Prompt: bring the `rawtiles` spec in line with what the hardware and the compliant supply chain actually require

You are updating the **`rawtiles` format specification** (`github.com/tobymurray/rawtiles`,
`spec/rawtiles.md`) and everything downstream of it. Six changes are on the table. Each one
was found by measurement or by reading the spec against a real workflow; each is stated
below with its evidence, its options, and the decision you must make. **The decisions are
yours — this prompt gives you the constraints, not the answers.**

The window matters. § 13.1 says the v0.x phase permits breaking changes that "MAY redefine
wire-format semantics under the same `format_version = (1, 0)` on-disk bytes" and that
"any such change invalidates previously-derived `pack_uuid`s and previously-emitted packs."
**That permission expires at v1.0.** Three of the six changes below are descriptor or
quantiser changes that are cheap now and expensive later. Do not defer them on
compatibility grounds that do not yet apply.

---

## 0. Ground rules

- **Never post anything to GitHub** — no PRs, comments, or issues, on any repo. `gh` is
  read-only. Deliverables are local commits on a branch, pushed to `origin` only. Plain
  push, never force.
- **Verify, don't trust this prompt.** Every claim below was checked on 2026-08-07 against
  `rawtiles` @ `38d4d26`. Re-verify anything load-bearing before you act on it. Cite
  `file:line` for code, `§` for spec text, and pin the commit you read.
- Label findings **`CONFIRMED`** (traced to text, code, or experiment) or **`PLAUSIBLE`**
  (reasoned). For each `PLAUSIBLE`, say what would settle it.
- **Do not bulk-fetch tiles from anyone's tile server.** Use `--source synthetic`, the
  committed `athens.rawtiles` fixture, the conformance corpus, or locally-generated data.
  See `MAP_COMPLIANCE_APPENDIX.md` for why this is a hard rule and not politeness.
- Evidence settled by experiment goes in `Docs/Investigations/<date>-<slug>/` in the repo
  it belongs to — hypothesis, method, log, verdict. **Failures included; they are usually
  the useful part.**
- Commits terse, mostly *why*. Branch off `main`.

---

## 1. Settled — do not re-litigate

- **`ABGR2222` is correct and stays.** It is the native pixel format of the watch's panel
  (Sharp LS012B7DD06A: *"1 pixel has RGB each 2bit, the pixel can display 64 colors"*),
  not a compression compromise. RGB565 is for other targets; it is not an upgrade here.
- **`RLE` is adequate.** Measured on all 687 real tiles of `athens.rawtiles`: **7.7 % of
  raw** aggregate, 6.0 % at z16. The earlier 32.4 % figure came from n = 12 dense low-zoom
  tiles. **The reserved `QOI` / `LZ4` codepoints and any deflate proposal are out of scope
  for this pass** — at a 7.7 % baseline the remaining headroom changes no decision.
  `RAWTILES_SPEC_ADEQUACY.md` `M8` should be revisited on this evidence, but not here.
- **The 4 GiB cap stays.** Nothing in the delivery workflow ships packs above ~30 MiB; the
  planet archive is PMTiles, not rawtiles, and that seam is correct.
- **North-up only, no rotation, no alpha in v1.** `blitCopy` cannot rotate; the GPS trace
  is app-drawn.

---

## 2. Where the truth lives

| Thing | Where |
|---|---|
| The spec | `github.com/tobymurray/rawtiles` @ `38d4d26` — `spec/rawtiles.md`, **document v0.6**, wire `(1,0)`, Status: Provisional |
| Conformance corpus | same repo, `spec/conformance/` — `manifest.json` (pins `spec_version` **and a SHA-256 per fixture**), 17 golden + negative fixtures, `generators/`, `verify.py` |
| Reference reader | same repo, `reference-reader/` |
| The evidence for changes 1–4 | `slippypack/Docs/Investigations/2026-08-07-watch-cartography/` — E1 (panel colorimetry), E3 (compression), E5 (palette snapping), E8 (quantiser) |
| The workflow these serve | `slippypack/MAP_DELIVERY_WORKFLOW.md` § 5.3–5.4, `MAP_CARTOGRAPHY_SPEC.md`, `MAP_COMPLIANCE_APPENDIX.md` |
| The prior needs audit | `una-sdk/RAWTILES_SPEC_ADEQUACY.md` — `U12`, `U13`, `M1`, `M8` are referenced below |
| Second writer | `slippypack/crates/slippypack-core/` (writer, quantiser, identity) |
| Second reader | `una-sdk` branch `feat/rawtiles-container` — vendored streaming Container |
| Third reader | `slippypack/spec-validator-cpp/` |

---

## 3. The changes

### C0 — § 9.1.1's canonical quantiser encodes a display model, and it is the wrong one for the watch

**Evidence (`CONFIRMED`, E8).** § 9.1.1 states the 2-bit quanta are *"displayed as
`{0, 85, 170, 255}` (8-bit equivalents)"* and places thresholds at 42/127/212 accordingly.
That is exactly right for a framebuffer whose 2-bit value is **bit-replicated** to 8 bits
and shown on a normal sRGB display — plausibly the ST77xx/PineTime target § 9.2 has in
mind. It is wrong for the Sharp LS012B7DD06A, whose *area gradation* ("Area gradation of
each RGB have 2 dot") makes reflectance linear in the code value. Modelled from the
datasheet's measured primaries, the panel's levels display as ≈ `{56, 162, 215, 255}`;
panel-correct thresholds are ≈ 109/188/235.

Measured consequence: **93.2 %** of RGB888 colours quantise to a different code than the
panel-correct choice, always **too light**. Mean ΔE2000 on the grey axis **16.24 vs 6.65**.

**⚠ The trap you must not fall into.** Those numbers rest on a stated assumption (E1): that
the four area-gradation levels are equal steps 0, ⅓, ⅔, 1. The datasheet states the
*mechanism* but does not table the per-level reflectance. **Do not write `{56, 162, 215,
255}` or `109/188/235` into normative spec text.** If you bake unverified constants into a
spec that claims byte-identical cross-writer reproducibility, you have moved the bug rather
than fixed it. The fix must be *structural*.

**The real defect is architectural:** a display's transfer function is a property of the
**target panel**, not of the writer. § 9.1.1 welds one into the canonical quantiser and
§ A.3 binds it to `quantiser_version`, which makes the identity contract quietly
device-specific — in a format that explicitly courts several devices (§ 9.2).

**Options. Choose one and justify it.**

- **(a) A second canonical quantiser.** Keep `quantiser_version = 1` for bit-replication
  targets; add `= 2` for linear-reflectance targets. Simple, uses machinery that already
  exists. Cost: the spec now has to say *which* linear-reflectance mapping, which drags the
  unverified constants back in.
- **(b) A palette-snapping quantiser.** `quantiser_version = 2` means "the writer declares
  N palette entries; every pixel snaps to the nearest declared entry." This is what the
  recommended cartography pipeline needs anyway (E5: it recovers exact palette-first output
  from an ordinary anti-aliasing renderer at +0.2 % bytes), and it **sidesteps the transfer
  function entirely** — the style author chose the codes, so no display model is implied.
  Cost: **the declared palette must enter the § A.3 descriptor** or reproducibility breaks.
  Decide the palette's canonical serialisation.
- **(c) Separate the concerns.** Keep § 9.1.1 as one named mapping among several, and make
  the *choice* of mapping an explicit descriptor key rather than an implied property of
  `pixel_format`.

**My reading, which you should test rather than adopt:** (b) is the strongest, because it
is the only option that removes a display model from the writer instead of adding a second
one — and it is the one the compliant pipeline needs regardless. (a) is the cheapest. (c)
is the most honest but the largest.

Whatever you choose: **§ 9.1's sentence "displayed as {0, 85, 170, 255} (8-bit
equivalents)" must be corrected or qualified**, because as written it asserts a display
property the format cannot know.

### C1 — `ATTR` and `NAME` are outside the `pack_uuid` descriptor

**Evidence (`CONFIRMED`, `U12` + § A.3's exhaustive key list).** § A.3's top-level keys do
not include any extension payload. So **stripping attribution from a pack leaves its
`pack_uuid` unchanged** — an attribution-stripped pack is indistinguishable by identity
from a compliant one.

**Why this is not merely tidiness.** Packs built from OpenFreeMap or Protomaps are ODbL
**Produced Works**; ODbL § 4.3 requires a notice "reasonably calculated" to reach anyone
who views the work, and the pack's `ATTR` section is how that notice travels. A format
designed to distribute licence-bearing content should not have an identity function that is
blind to the licence text. See `MAP_COMPLIANCE_APPENDIX.md` § 4.

**Decide:** whether `NAME` (multi-locale, § 7.4) belongs in the descriptor alongside `ATTR`,
or whether display name is legitimately identity-neutral. Argue it either way, but say which
and why. `SRCD` is already excluded from reproducible packs by § 7.3 — leave it excluded.

### C2 — no descriptor shape for a vector source rendered through a style

**Evidence (`CONFIRMED`, § A.4).** § A.4 classifies `pmtiles` and `mbtiles` as **raster**
sources, and for the one vector kind (`pbf`) states: *"v1 does not specify PBF-to-pixel
rendering (reserved for a future minor)."* Style sources are excluded from `sources`
entirely.

**Why it matters now.** The compliance work (`MAP_COMPLIANCE_APPENDIX.md` § 6) establishes
that **every source permitting bulk offline derivation is vector or raw data** — the
ready-to-quantise raster services either prohibit it (OSM tile CDN, MapTiler Cloud) or gate
it behind a commercial agreement. So "vector PMTiles → MapLibre style → raster" is not one
workflow among several; it is the *only* compliant one. And it has no home in v1, which
means the recommended pipeline **cannot claim § 14.1 cross-writer reproducibility**.

**The encouraging part, which you should check before designing anything larger:** § A.4
already defines a raster source's `content_hash` as *"the SHA-256 of the writer's
**pre-quantisation RGB888 byte stream** … after the writer's decode/resample/alpha-handling
pipeline has run."* That is **already the correct hash for a rendered vector source** — it
pins the rendered pixels, which is exactly the deterministic surface. Plausibly this change
is a `vector: true` discriminator on the existing `pmtiles`/`mbtiles` shapes plus the
`style_hash` key that already exists, and not a new source kind at all.

Establish whether that is true. If it is, say so plainly and make the small change. If it
is not, the larger design is yours.

### C3 — `M1`: `compression` and `supersedes_uuid` are outside the descriptor

**Evidence (`CONFIRMED`, `RAWTILES_SPEC_ADEQUACY.md` E1).** Two packs from the same source
descriptor with different `--compression` share a `pack_uuid` while differing 6.9× in size
with no tile bytes in common. And because `supersedes_uuid` is absent from the descriptor, a
pack whose only difference from *X* is that it supersedes *X* derives *X*'s UUID —
**self-supersedence**.

**Honest scoping.** This defect *cannot bite the map-delivery workflow*: every pack it ships
uses `RLE`, and a monthly re-render differs in `content_hash` anyway, so it never produces a
pack differing only in these fields. I withdrew it from that workflow's critical path. **It
is still a real defect in a format that promises byte-identical dedup**, and this pass is the
right place to fix it because it is a descriptor change and the v0.x window is open.

Fix both. Note that adding `supersedes_uuid` to the descriptor is self-referential — derive
carefully and add a fixture that pins the resolution.

### C4 — `ATTR` ordering is undefined for style-rendered packs

**Evidence (`CONFIRMED`).** § 7.3 requires `ATTR` strings be *"ordered to match the
canonical `sources` array order defined in Appendix A.4"*. § A.4 says *"Style sources MUST
NOT appear in `sources`."* When a single pack owes attribution to both the tileset producer
(*"OpenFreeMap © OpenMapTiles"*, *"basemap © Protomaps"*) and the underlying data
(*"Map data from OpenStreetMap (ODbL)"*) through **one** source entry, the ordering rule has
nothing to order — the strings are multiple, the source is one.

Define the ordering. This interacts with C1 (if `ATTR` enters the descriptor, its byte order
must be canonical) and C2 (whose source shape it is).

### C5 — `parent_uuid`, conditional

**Evidence (`CONFIRMED`, § 4.5).** *"Reserved in v1 for future pack-compositing support; the
only legal v1 value is all-zero."* So a pack sliced from a larger archive cannot declare
which archive it came from, and two slices of the same archive have no formal relationship.

**This is speculative and it is the one change you may reasonably decline.** It is wanted by
a catalog feature ("this pack covers ground you already have") in a product that does not
exist yet. **Do not design for it on my say-so.** Either (i) find a present-tense need and
specify it, or (ii) leave § 4.5 exactly as it is and record why. A reserved field with a
clear reservation is not a defect.

### C6 — `U13` verifiable prefix: **blocked, do not do this pass**

**Evidence.** § 10 puts a CRC-32 at the footer only, so a partial pack cannot be validated
and a resumable chunked transfer has no integrity story.

**Why it is blocked, not deferred.** Whether this matters depends entirely on an unmeasured
number: the throughput of the watch's BLE File Transfer Service write path. Prior work
(`una-sdk` branch `investigate/ble-large-file-transfer`) confirmed the FTS `writeHandler`
exists, confirmed a large-blob upload family (`0x20`/`0x21`/`0x22`), and **retracted an
earlier 64 KB ceiling** — `offset`/`total`/`chunklen` are genuine 32-bit fields. But the
observed *read* path is a 128-byte-per-round-trip loop, which is ~2.5–17 kB/s: **12 minutes
to 3.3 hours for a 3–29 MiB pack.** If the write path pipelines, the same packs move in
1–8 minutes and USB stops being the only channel.

**Do not design a per-block integrity scheme until that number exists.** It determines chunk
size, whether resume is needed at all, and therefore the shape of the fix. If you want to
unblock it, go measure it — the tooling is already in that branch's `prototype/`.

---

## 4. Blast radius — what else must change

A descriptor or quantiser change is not a one-file edit. Enumerate and handle all of it, or
say explicitly what you left and why.

**In the `rawtiles` repo:**
- `spec/rawtiles.md` — the § edits themselves, plus § 13's version history and the Status line.
- `spec/conformance/manifest.json` — `spec_version`, **and the `pack_sha256` / `hashes_sha256`
  of every fixture your change perturbs.** C0 invalidates § 14.4's quantiser test vector;
  C1/C2/C3 invalidate every UUID-bearing fixture (`golden-canonical-uuid` at minimum).
- `spec/conformance/generators/` — regenerate, do not hand-edit hashes.
- `spec/conformance/CHECKLIST.md`, `verify.py`.
- `reference-reader/` — any changed reader rule.
- **New fixtures** for every new rule. A spec change with no negative fixture is untested.

**Outside the `rawtiles` repo** (do not edit these here; produce a written change list with
each item's blocking relationship, as `MAP_DELIVERY_WORKFLOW.md` § 5.3 does):
- `slippypack/crates/slippypack-core/` — `quantise` (C0), `identity.rs` (C1/C2/C3).
- `slippypack/spec-validator-cpp/`.
- `una-sdk` branch `feat/rawtiles-container` — the vendored streaming reader.
- `slippypack`'s docs, which still reference **spec 0.5**, and `DECISIONS.md:468,487,509`,
  which cite a file **`spec/rawtiles-v1.0-rc1.md` that does not exist on `origin/main`**.
  Pin one version vocabulary while you are here.

---

## 5. Premise traps

- **The v0.x window is open — use it.** § 13.1 permits breaking `pack_uuid` changes now.
  Do not invent forward-compat machinery for a compatibility promise that has not been made.
- **`{56, 162, 215, 255}` is modelled, not measured.** See C0's trap. A photometer reading
  of the four levels per channel on real hardware settles it; until then no constant derived
  from it belongs in normative text.
- **The Athens pack is evidence, not a template.** Its source violates the OSM tile usage
  policy. Do not add fixtures built from it or from any live tile server.
- **`pack_uuid` is not currently a byte-identity.** That is the thing C1/C3 exist to fix;
  don't accidentally reason from the promise while fixing the defect.
- **Do not widen scope into compression.** § 1 settles it. `M8`'s recommendation predates
  the corrected 7.7 % measurement.
- **Reproducibility is the product here.** Every change is to a function whose entire
  purpose is that two independent writers agree byte-for-byte. Anything you add must be
  canonically serialisable, integer-or-string, and JCS-stable (§ A.3's invariants: ASCII
  keys, integers within ±(2⁵³−1), no floats).

---

## 6. Deliverables

1. **The spec edits**, on a branch off `main` in the `rawtiles` repo, pushed to `origin`.
   Bump the document version and write the § 13 history entry. Each change gets a
   one-paragraph rationale in the changelog naming the evidence.
2. **A regenerated, passing conformance corpus**, with new fixtures for every new rule and
   at least one negative fixture per new reader rejection.
3. **`RAWTILES_SPEC_UPDATE.md`** at the `rawtiles` repo root, untracked: the decisions you
   made, the options you rejected and why, and the `CONFIRMED`/`PLAUSIBLE` label on each.
   **C0's and C5's reasoning are the ones worth writing carefully** — one because the
   obvious fix is the wrong one, the other because declining is a legitimate outcome.
4. **A downstream change list** for `slippypack`, `spec-validator-cpp`, and the una-sdk
   Container, each item with its blocking relationship.
5. **An investigation bundle** for anything you settle by experiment.

**What "done" looks like:** someone who has never seen this project reads the spec diff and
can (a) tell what changed and why, from the changelog alone; (b) run `verify.py` green
against a corpus that actually exercises the new rules; and (c) find, for every normative
number you introduced, either a measurement or an explicit statement that it is assumed and
what would settle it. No constant in normative text whose provenance is a model.
