# Compliance appendix — every source considered

Companion to `MAP_DELIVERY_WORKFLOW.md`. All clauses retrieved **2026-08-07** and quoted
verbatim; raw HTML of each page is committed at
`Docs/Investigations/2026-08-07-watch-cartography/terms/` so the quotes can be checked
against what was actually served.

**I am not a lawyer and this appendix is not legal advice.** It distinguishes throughout
between *"the terms say X"* — which is quotable and which is what this document does —
and *"we would be fine doing Y"*, which is not mine to conclude. § 5 lists, precisely, what
needs actual legal review, so it can be asked once.

Verdicts: **PERMITS** / **PROHIBITS** / **AMBIGUOUS** / **BLOCKED-PENDING-PERMISSION**.
Where terms are ambiguous I have taken the conservative reading and said so.

---

## 1. The gate

The product's use case is a single sentence: **fetch or derive map content ahead of time,
store it in a redistributable file, and put that file on a device that is offline.**
Every row below is judged against that sentence and nothing else. A source that is
excellent for live viewing and forbids offline derivation scores zero here, however good
its cartography.

---

## 2. Raster tile services

### 2.1 OpenStreetMap Foundation tile CDN — `tile.openstreetmap.org`

**URL:** https://operations.osmfoundation.org/policies/tiles/ · retrieved 2026-08-07
**Verdict: PROHIBITS.** Not marginal, not a rate-limit question — the policy names this
exact product.

> **4. Prohibited: bulk downloading ("scraping") and offline use**
>
> **Bulk downloading** is any pre-emptive fetching of tiles other than those a user is
> actively viewing. This includes, but isn't limited to:
> - "Pre-seeding" large areas or multiple zoom levels in advance.
> - Building tile archives (e.g. `.zip`, `.mbtiles`) for later distribution.
> - Automated scans across wide bounding boxes, especially at high zoom (z≥14).
> - Headless bots that pan/zoom the map to force rendering.
>
> Offline use is not permitted on `tile.openstreetmap.org`.
> Features such as "Download city/country for offline use" or "Save area for later"
> **rely on prefetch/bulk downloading** and are therefore prohibited.

and, under "You must not":

> Bulk download ("scrape") tiles or offer prefetch features.

and the enforcement note:

> Prefetch/offline patterns place disproportionate load on community-funded servers and
> will be **blocked without notice**. Repeated violations may lead to longer-term or
> network-level blocks.

The policy also states the remedy, which is worth quoting because it is the shape of the
recommendation:

> **Note:** If you require offline maps, use **self-hosted** tiles or a **provider that
> explicitly allows offline/prefetching**. **Vector tiles** are often more suitable for
> this use-case.

**Consequences, stated plainly.**
- `Docs/Investigations/2026-08-06-athens-pack/athens.rawtiles` was built by fetching 687
  tiles from this CDN. It is **evidence, not a template**, and its build recipe must not
  ship as a default or an example.
- slippypack's per-host 2 req/s cap for `tile.openstreetmap.org` limits *load*. The
  prohibited thing here is the *purpose*, not the rate; the cap does not cure it.
- The athens-pack README's characterisation — "a one-off ~690-tile fetch, not a bulk
  pipeline" — does not survive the policy text: § 4 defines bulk downloading as *any*
  pre-emptive fetching of tiles the user is not actively viewing, and explicitly includes
  building archives for later distribution. **Recommend correcting that README** (spin-off
  S1).

### 2.2 MapTiler Cloud — `PLAN.md`'s headline recommended source

**URL:** https://www.maptiler.com/terms/cloud/ · retrieved 2026-08-07
**Verdict: PROHIBITS.**

> **Export Usage.** Unless otherwise agreed in writing with MapTiler, it is prohibited to
> export map content for usage outside the Service.

> **Bulk Downloading.** Unless otherwise agreed in writing with MapTiler, it is prohibited
> to batch or excessive bulk download of map tiles or otherwise use the Service beyond the
> limits of these Terms.

> **Server-side Proxy & Caching.** It is prohibited to store, save, and/or redistribute
> any map content from a server-side cache or temporary storage or by using a screenshot
> or other static image, instead of directly accessing via our APIs.

There is one carve-out, and it does not reach:

> **End-user Device Caching.** Results of requests can be stored in a temporary personal
> cache (browser cache, mobile app cache, etc.) for use by a single end-user only.

A `.rawtiles` pack is not a *temporary* cache and copying it to a watch is *export outside
the Service*. Conservative reading: **PROHIBITS**, for every candidate workflow.

**This is a direct hit on `PLAN.md`.** Its first-run flow recommends MapTiler's free tier
("100K tile requests/month — comfortably covers a small country") as the default source
for a tool whose only function is building offline archives. That recommendation must be
removed, not merely re-scoped (spin-off S2).

### 2.3 MapTiler Server / Data — the licensed offline route

**URL:** https://www.maptiler.com/terms/server-data/ · retrieved 2026-08-07
**Verdict: BLOCKED-PENDING-PERMISSION** for this product.

Permitted without a custom agreement:

> **Non-commercial Derivative Work.** To produce derivative vector datasets for
> non-commercial purposes or for OpenStreetMap only.

Requiring a custom agreement:

> **Commercial Derivative Work.** To produce derivative work, including but not limited
> to printing, video production, or exports using the map data.

> **Serve Third-party Entities.** To bundle and redistribute the Product in a B2B context
> … This includes reselling, redistributing, renting, leasing, lending, selling or
> sublicensing the Product or any part thereof.

And a clause that independently blocks the hosted workflows:

> **Host On Public Cloud.** It is prohibited to host and serve the Map Content from a
> public cloud. The public cloud is defined as computing services offered by third-party
> providers over the public Internet.

So: a commercial watch product distributing derived packs needs a custom agreement, and
even with one it could not serve the archive from R2/S3/B2 without that also being agreed.
The mitigation is real and acceptable — **ask MapTiler for a quote** — but nothing here
should be assumed. Not priced in this report: MapTiler does not publish Server/Data
pricing for this shape of use; it is a sales conversation.

### 2.4 Stadia Maps — the one commercial raster source with an explicit offline carve-out

**URL:** https://stadiamaps.com/terms-of-service/ · retrieved 2026-08-07
**Verdict: AMBIGUOUS** — permits a strictly client-side BYO workflow, prohibits the hosted
ones. This is the most interesting row in the table and the prompt's expectation that
Stadia would "land similarly" to the others is **not** what the text says.

Prohibited, with an express exception:

> bulk downloading of data, **except for the purpose of caching small amounts of data for
> offline use in a mobile application, not to exceed 100MB cached at a time per device**;

> proxying or caching access to our Services in any way, **except for the above exception
> of limited downloads for offline use in a mobile application**, use of the cacheable
> static maps endpoint …, or standard client-side caching (server-side caching is
> prohibited) …

Also prohibited, and this is what closes the hosted arms:

> creating derivative databases by systematically extracting, reutilizing, or compiling
> substantial portions of data from our Services;

**Reading.** A workflow in which the owner's own browser fetches ≤ 100 MB and writes a pack
for the owner's own device plausibly sits inside the carve-out. A workflow in which *we*
render or fetch centrally and distribute packs to many owners does not — that is
server-side caching and systematic compilation. Two things are genuinely unclear and I am
not going to guess at them: whether a `.rawtiles` file on a watch is "cached … per device
in a mobile application", and whether the 100 MB ceiling is per-device-at-a-time (implying
packs must be evicted) or cumulative. **→ legal review item L3.**

### 2.5 Thunderforest

**URL:** https://www.thunderforest.com/terms/ · retrieved 2026-08-07
**Verdict: PROHIBITS on free tiers; PERMITS-WITH-PLAN above.**

> **Bulk Downloading.** Absolutely no bulk-downloading scraping, pre-downloading,
> pre-caching or anything similar without an appropriate plan.
> If you wish to make bulk downloads, please upgrade your account to the Small Business
> plan or higher.
> … Please get in touch if you'd like to discuss bulk downloading. We're very friendly if
> you approach us for a chat.

and separately:

> Tiles may be cached in-browser and on-device for offline use.

An explicit, documented, priced route to permission. Worth a conversation if a
pre-rendered raster source is ever wanted; **not needed by the recommendation**, which
renders its own tiles.

### 2.6 Carto, Mapbox, Esri, Google

**Not assessed in depth. None is a candidate**, for the same structural reason: all four
are API-metered services whose terms are built around live requests, and none advertises
a bulk-offline-derivation product at consumer scale. If one is ever proposed, its terms
must be read before any prototype fetches a tile — the OSM and MapTiler rows above show
how easily a plan gets built on a source that forbids its only use case.

---

## 3. Bulk-downloadable sources — where the supply actually is

### 3.1 OpenFreeMap

**URL:** https://openfreemap.org/ · retrieved 2026-08-07 · **Verdict: PERMITS**

> Using our **public instance** is completely free: there are no limits on the number of
> map views or requests. There's no registration, no user database, no API keys, and no
> cookies.

> We also provide **weekly full planet downloads** both in Btrfs and MBTiles formats.

> **Is commercial usage allowed?** Yes.

Required attribution, verbatim from the site's Attribution section:

> OpenFreeMap © OpenMapTiles Data from OpenStreetMap

**Conservative note.** "No limits on requests" is permissive but is written about serving
a live map. Nothing on the page addresses systematically prefetching the public instance
to build offline archives. **Use the planet downloads** — a channel explicitly provided
for bulk — rather than reading "no limits" as consent to bulk-pull the CDN. That is both
the safe reading and the polite one, and it costs nothing because the recommendation
wants a local copy anyway.

### 3.2 Protomaps basemaps

**URL:** https://docs.protomaps.com/basemaps/downloads · retrieved 2026-08-07
**Verdict: PERMITS**

> The **Protomaps Basemap** is a general purpose vector base map … It's available as a
> single PMTiles archive, distributed as an **Open Database License Produced Work**
> (OpenStreetMap attribution required)
>
> A full planet file is roughly **120 gigabytes**, including zoom levels from 0 to 15.
>
> Please note that **URLs may change** and hotlinking to these downloads are discouraged.
> Instead, you should **copy the tileset to your own Cloud Storage**.

Daily full-planet builds with BLAKE3 hashes; `pmtiles extract` produces regional subsets.
The guidance to copy to your own storage is exactly what the recommended workflow does.

### 3.3 Raw OSM data — Geofabrik / BBBike extracts

**Verdict: PERMITS.** ODbL; bulk download is the intended and advertised channel.
Maximum control, maximum work — you own a rendering pipeline and a schema.
Relevant as the fallback if OpenFreeMap and Protomaps both became unavailable; not the
recommended primary because Planetiler-schema tiles are a solved, maintained problem.

### 3.4 Overture Maps

**URL:** https://docs.overturemaps.org/attribution/ · retrieved 2026-08-07
**Verdict: PERMITS, with per-theme attribution obligations.**

> Some of the data sources we use in Overture datasets require their own attribution,
> according to their licenses. For example, our Explore tool displays several datasets
> that are made wholly or in part from OpenStreetMap data, so we added the following line
> of attribution to adhere to the requirements of the Open Database License:
> **© OpenStreetMap contributors, Overture Maps Foundation**.

The addresses theme in particular carries a per-country patchwork (G-NAF has its own EULA;
Austria is CC BY 4.0; and so on). **Not recommended for v1**: the attribution surface is
per-theme and per-country, which is a poor fit for a pack format with one `ATTR` string
and a watch with a 240×240 screen. Revisit if a specific Overture theme is ever needed.

### 3.5 OpenMapTiles

**Verdict: AMBIGUOUS — check the tileset licence separately from the schema licence.**
The *schema* is open; the pre-generated *tilesets* sold by OpenMapTiles/MapTiler are a
commercial product governed by § 2.3's terms. OpenFreeMap uses the unmodified OpenMapTiles
*schema* while generating its own tiles with Planetiler, which is why § 3.1 is clean and
this row is not. **Do not source tilesets here**; sourcing the schema is fine.

### 3.6 Terrain and the open raster estate

Public-domain or open-licensed, bulk download is the intended channel, and — importantly
— **no tile-server policy problem exists at all**:

| source | licence posture | use |
|---|---|---|
| Copernicus DEM (GLO-30) | free and open, attribution required | hillshade, contours |
| USGS 3DEP | US public domain | hillshade, contours (US) |
| Natural Earth | public domain | z0–6 context |
| NOAA ENC | US public domain | water/marine |
| national open data (OS Open Zoomstack, NRCan, …) | per-country open licences | optional local enrichment |

Copernicus' licence page did not render to static HTML on retrieval (JS-only) and is
therefore **not quoted here**; its exact terms and attribution string must be read before
a terrain layer ships. **→ legal review item L4.** This is not on the critical path:
§ 6 of the cartography spec defers terrain past v1.

---

## 4. ODbL obligations that travel with the pack

The pack is the thing being distributed, so the obligations land on the pack and on the
watch, not only on a website.

**Produced Work vs Derivative Database.** ODbL 1.0 § 4.3:

> **4.3 Notice for using output (Contents).** Creating and Using a Produced Work does not
> require the notice in Section 4.2. However, if you Publicly Use a Produced Work, You
> must include a notice associated with the Produced Work reasonably calculated to make
> any Person that uses, views, accesses, interacts with, or is otherwise exposed to the
> Produced Work aware that Content was obtained from the Database, Derivative Database,
> or the Database as part of a Collective Database, and that it is available under this
> License.

(https://opendatacommons.org/licenses/odbl/1-0/ · retrieved 2026-08-07)

A `.rawtiles` pack is **rendered raster pixels** — the standard reading is that this is a
Produced Work, not a Derivative Database, and Protomaps distributes its own vector
basemap on exactly that basis (§ 3.2). The practical difference is large: a Produced Work
carries the § 4.3 *notice* obligation but does **not** drag § 4.4's share-alike onto the
pack, so we would not owe anyone the source database. **I am not qualified to certify
that conclusion for a commercial product → legal review item L1.**

**What the watch must display.** The OSMF Attribution Guidelines
(https://osmfoundation.org/wiki/Licence/Attribution_Guidelines · retrieved 2026-08-07)
give a safe harbour that fits a 240×240 panel far better than "put it in the corner":

> For a browsable map (e.g., embedded in a web page or application), the credit should
> typically appear in a corner of the map. … Alternatively, the attribution may be placed
> adjacent to the map or **on a splash screen or pop-up shown when a user starts the app,
> device, website, etc.**

> You may use a mechanism to fade/collapse the attribution under certain conditions:
> … automatically **after five seconds**. This also applies to splash screens or pop-ups.

> If the attribution has been collapsed, the user must still be able to find the licence
> information if they look for it, for example from an '(i)' button in the corner of the
> map or an '**About**' option in a menu.

> **If attribution is presented to the user upon application startup, it does not need to
> be presented to the user every time the user looks at or interacts with the
> application.**

> If a splash screen or other method of attribution cannot link to the licence
> information, the link must be provided in an easily locatable part of the … application
> (e.g. in the menu under "Data licences").

On acceptable wording:

> The historical forms of attribution "© OpenStreetMap contributors" or
> "© OpenStreetMap" are acceptable.

> … if you have rendered OSM data to your own design, you may wish to use "**Map data from
> OpenStreetMap.**"

**Therefore, the watch obligation is concrete and cheap:** show the pack's `ATTR` string
on map-app startup for ≥ 5 seconds, and keep the full licence text reachable from an About
screen. No permanent on-map overlay is required, which matters when the map is 240×240.

**Exact attribution strings a derived pack must carry.**

| pipeline | `ATTR` string |
|---|---|
| Protomaps basemap → our renderer | `Map data from OpenStreetMap (ODbL) · basemap © Protomaps` |
| OpenFreeMap planet → our renderer | `OpenFreeMap © OpenMapTiles · Map data from OpenStreetMap (ODbL)` |
| Geofabrik / raw OSM → our renderer | `Map data from OpenStreetMap (ODbL)` |
| any of the above + Copernicus DEM | append ` · elevation © Copernicus` (exact wording pending L4) |

The About screen must additionally carry `openstreetmap.org/copyright` as printable text,
since the watch cannot hyperlink.

**A defect that lands here.** `RAWTILES_SPEC_ADEQUACY.md` `U12` records that `ATTR` bytes
are outside the `pack_uuid` descriptor (§ A.3), so **stripping attribution from a pack
leaves its identity unchanged** — an attribution-stripped pack is indistinguishable from
a compliant one by UUID. That is a licence-compliance hole in a format used to distribute
ODbL Produced Works, and it is item C1 on the change list in
`MAP_DELIVERY_WORKFLOW.md` § 8.

---

## 5. What needs actual legal review

Precise, so it can be asked once.

- **L1 — Produced Work status.** Confirm that a `.rawtiles` pack of rendered raster tiles
  derived from OSM-based vector tiles is a *Produced Work* under ODbL 1.0 § 4.3 and not a
  *Derivative Database* under § 4.4, and that distributing it therefore carries the notice
  obligation but not share-alike. Include the case where the pack is distributed
  commercially and bundled with a device.
- **L2 — Attribution sufficiency on a 240×240 panel.** Confirm that a ≥ 5 s startup
  display of the `ATTR` string plus an About screen carrying `openstreetmap.org/copyright`
  as plain text satisfies § 4.3's "reasonably calculated" standard, given the OSMF safe
  harbour quoted in § 4 above and the absence of hyperlinking on the device.
- **L3 — Stadia Maps carve-out.** Does a `.rawtiles` pack written by the owner's own
  browser and copied to the owner's own watch fall inside "caching small amounts of data
  for offline use in a mobile application, not to exceed 100MB cached at a time per
  device"? Is the 100 MB ceiling per-device-at-a-time or cumulative? Only relevant if the
  runner-up workflow is chosen.
- **L4 — Copernicus DEM.** Read and record the current licence and required attribution
  string before any terrain layer ships.
- **L5 — Chained attribution.** Confirm the § 4 table's strings are sufficient where our
  pack is derived from a Produced Work (Protomaps' PMTiles) that is itself derived from
  OSM — i.e. that we owe both the upstream producer and OSM, and that the strings name
  both correctly.

None of L1–L5 blocks starting work. L1 and L2 should be settled before the first pack is
distributed to anyone outside the project.

---

## 6. Summary table

| source | URL | verdict | usable for |
|---|---|---|---|
| OSM tile CDN | operations.osmfoundation.org/policies/tiles/ | **PROHIBITS** | nothing in this product |
| MapTiler Cloud | maptiler.com/terms/cloud/ | **PROHIBITS** | nothing in this product |
| MapTiler Server/Data | maptiler.com/terms/server-data/ | **BLOCKED-PENDING-PERMISSION** | needs custom agreement + cloud-hosting waiver |
| Stadia Maps | stadiamaps.com/terms-of-service/ | **AMBIGUOUS** | client-side BYO only (≤100 MB/device); not hosted |
| Thunderforest | thunderforest.com/terms/ | PROHIBITS free / **PERMITS with plan** | priced route, not needed |
| Carto / Mapbox / Esri / Google | — | not assessed | not candidates |
| **OpenFreeMap** (planet downloads) | openfreemap.org | **PERMITS** | ✅ primary vector source |
| **Protomaps basemaps** | docs.protomaps.com/basemaps/downloads | **PERMITS** | ✅ primary vector source |
| Geofabrik / BBBike (raw OSM) | ODbL | **PERMITS** | ✅ fallback |
| Overture Maps | docs.overturemaps.org/attribution/ | PERMITS, per-theme obligations | deferred |
| OpenMapTiles tilesets | — | **AMBIGUOUS** | schema yes, tilesets no |
| Copernicus DEM / USGS 3DEP / Natural Earth | various | PERMITS (L4 pending) | terrain, post-v1 |

**The structural conclusion, stated plainly because I do agree with it:** every source
that permits bulk offline derivation is **vector** or raw data; the sources that ship
ready-to-quantise raster either forbid it outright or gate it behind a commercial
agreement. I looked for a counter-example — a compliant pre-rendered raster source good
enough for a 64-colour panel — and did not find one. Stadia is the closest and its
carve-out reaches only the client-side workflow.

So **"we render our own tiles" is the price of admission.** The convenient part is that
the cartography evidence arrives at the same place from a different direction: § 2 of
`MAP_CARTOGRAPHY_SPEC.md` shows that quantising someone else's raster is *also* the wrong
way to make this map, because it spends 0.007 % of the panel's ink. The compliance
constraint and the quality constraint want the same thing.
