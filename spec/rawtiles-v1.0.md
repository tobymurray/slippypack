# rawtiles format specification — version 1.0

**Status:** Stable for slippypack 0.1.0 / una-sdk MapTrack Phase 1.
**Date:** 2026-05-14.

This document defines the `.rawtiles` binary file format. It is the byte-level contract between writers (e.g. [slippypack](https://github.com/tobymurray/slippypack)) and readers (e.g. una-sdk's watch firmware `TilePack`, or any future device-side consumer). Conforming implementations on either side need only this document.

The format's design home is this slippypack repository. The format name **rawtiles** is independent of any specific producer or consumer. The una-sdk watch firmware is one reader; future consumers (other watches, embedded displays, kiosks, e-readers) implement the same spec.

## Scope and audience

- **Writers** (slippypack, third-party builders) need every section.
- **Readers** (firmware, validators, debug tools) need §§ 4–13.
- **Appendix A** is normative only for writers that need to produce byte-identical `.rawtiles` files to slippypack for the same logical inputs. Writers without that goal MAY pick any non-zero `pack_uuid`.

## 1. Conventions

- The key words **MUST**, **MUST NOT**, **SHOULD**, **MAY** are to be interpreted as in RFC 2119 / RFC 8174.
- All multi-byte integers are little-endian.
- All struct fields are tightly packed; no implicit padding between fields within the header or within a tile-index entry.
- Lengths and offsets are byte counts measured from the start of the file (byte 0) unless otherwise stated.
- "Conforming reader" / "conforming writer" mean implementations satisfying §§ 11 and 12 respectively.

## 2. Terminology

- **Pack** — one `.rawtiles` file.
- **Tile** — an addressed byte blob, identified by `(z, x, y)` for quadtree packs or by virtue of being the single image in a single-image pack.
- **Section** — one of the five top-level regions of a pack: *header*, *tile index*, *tile blob*, *extensions*, *footer*.
- **Reserved value** — a byte or tag value whose semantics are deliberately undefined in this version. Writers MUST NOT emit reserved values; readers MUST reject packs that contain them.

## 3. File structure

A `.rawtiles` file consists of five sections in fixed order:

```
+---------------------------------+ offset 0
|  Header                         |  fixed 290 bytes (§ 4)
+---------------------------------+ 290
|  Tile index                     |  20 × tile_count bytes (§ 5)
+---------------------------------+ 290 + 20 × tile_count
|  0–3 zero padding bytes         |  to 4-byte alignment
+---------------------------------+ tile_blob_start (4-aligned)
|  Tile blob                      |  per-tile bytes, each 4-aligned (§ 6)
+---------------------------------+ extensions_offset (4-aligned)
|  Extension sections             |  zero or more TLV sections (§ 7)
+---------------------------------+ file_size − 4
|  CRC-32 footer                  |  4 bytes (§ 10)
+---------------------------------+ file_size
```

A pack is at most **4 GiB** in total size. All on-disk offsets (`index_offset`, `extensions_offset`, `zoom_offsets[].offset`, tile-index `offset`) are u32 LE. A writer that would produce a larger pack MUST fail with a "pack too large" error rather than overflow.

## 4. Header (offset 0, 290 bytes)

| Offset | Size | Field | Notes |
|------:|----:|---|---|
| 0 | 4 | `magic` | ASCII `RAWT` (`0x52 0x41 0x57 0x54`) |
| 4 | 1 | `format_version_major` | u8; `1` in this version |
| 5 | 1 | `format_version_minor` | u8; `0` in this version |
| 6 | 16 | `pack_uuid` | non-zero, opaque |
| 22 | 16 | `supersedes_uuid` | all-zero = none |
| 38 | 16 | `parent_uuid` | reserved; MUST be all-zero in v1 |
| 54 | 1 | `pixel_format` | enum, § 8.1 |
| 55 | 1 | `projection` | enum, § 8.2 |
| 56 | 1 | `tile_addressing_scheme` | enum, § 8.3 |
| 57 | 1 | `tile_axis_convention` | enum, § 8.4 |
| 58 | 2 | `tile_dim_px` | u16; non-zero |
| 60 | 1 | `zoom_min` | u8; ≤ `zoom_max` |
| 61 | 1 | `zoom_max` | u8; < 24 |
| 62 | 4 | `bbox.min_lon` | i32 microdegrees |
| 66 | 4 | `bbox.min_lat` | i32 microdegrees |
| 70 | 4 | `bbox.max_lon` | i32 microdegrees |
| 74 | 4 | `bbox.max_lat` | i32 microdegrees |
| 78 | 8 | `build_timestamp` | u64; Unix epoch seconds; 0 = "no freshness info" |
| 86 | 4 | `tile_count` | u32; total entries in the tile index |
| 90 | 4 | `index_offset` | u32; byte offset of tile-index start |
| 94 | 192 | `zoom_offsets[24]` | per-zoom directory (§ 4.12) |
| 286 | 4 | `extensions_offset` | u32; byte offset of first extension section |
| **290** | | **end of header** | |

### 4.1 `magic`

The four ASCII bytes `RAWT`. Readers MUST reject any file whose first four bytes are not this sequence.

### 4.2 `format_version`

A `(major, minor)` pair. This specification defines `(1, 0)`.

- Readers MUST reject any pack whose `major ≠ 1`.
- Readers MUST accept packs with `major = 1, minor > 0`. The fixed-size header layout is frozen per major version; minor bumps add extension tags or enum values, which a reader handles per §§ 7.2 and 8.

### 4.3 `pack_uuid`

16 bytes, opaque to readers from the format's perspective.

- MUST NOT be all-zero; writers MUST validate and readers MUST reject.
- Writers MAY pick any non-zero 16-byte value. Slippypack derives it per Appendix A so that two builds with the same logical inputs produce identical `pack_uuid`s.
- `pack_uuid` is an identity field, not an integrity check. Integrity is the CRC (§ 10).

### 4.4 `supersedes_uuid`

16 bytes. The all-zero value is the sentinel for *"this pack supersedes no other"*. A non-zero value advertises that this pack replaces a previous pack with that UUID; readers MAY use the field to drive cache eviction or deduplication.

### 4.5 `parent_uuid`

16 bytes. Reserved in v1 for future pack-compositing support.

- v1 writers MUST set this to all-zero.
- v1 readers MUST reject packs where this field is not all-zero.

### 4.6 Enum bytes

`pixel_format`, `projection`, `tile_addressing_scheme`, and `tile_axis_convention` are single-byte enums. See § 8 for legal values; readers MUST reject any unknown value.

### 4.7 `tile_dim_px`

u16 little-endian. Pixel side length of one (square) tile.

- MUST be non-zero.
- For `addressing_scheme = Quadtree`, slippypack writes `128`.
- For `addressing_scheme = SingleImage`, slippypack writes a value ≤ 240.

### 4.8 `zoom_min` / `zoom_max`

Inclusive on both ends. `zoom_max ≥ zoom_min`. `zoom_max < 24` (the size of the per-zoom directory, § 4.12).

For `addressing_scheme = SingleImage` the pack has only one logical image and both fields are 0.

### 4.9 `bbox`

Four `i32` little-endian values, in this byte order: `min_lon`, `min_lat`, `max_lon`, `max_lat`.

- Units: integer microdegrees (= decimal degrees × 10⁶).
- Range: `lon ∈ [−180_000_000, 180_000_000]`; `lat ∈ [−90_000_000, 90_000_000]`. For `projection = WebMercator` the latitude range is further restricted by the Mercator pole limit (~±85.051129°, i.e. ±85_051_129 microdegrees); readers MAY use this to validate but MUST NOT reject packs solely on the basis of latitudes slightly outside that range.
- `min_lon ≤ max_lon`, `min_lat ≤ max_lat`.

### 4.10 `build_timestamp`

u64 little-endian; seconds since the Unix epoch (1970-01-01T00:00:00Z).

The value SHOULD represent the freshness of the underlying source data (e.g. most recent source `mtime` or HTTP `Last-Modified`), not the wall-clock build time. This makes byte-identical reproducible builds possible.

The value `0` is the sentinel for *"no freshness information available."* (slippypack uses this for the synthetic source kind, which has no real-world data.)

### 4.11 `tile_count` and `index_offset`

- `tile_count` (u32): total number of entries in the tile index across all zooms.
- `index_offset` (u32): byte offset where the first tile-index entry begins.

v1 writers MUST place the tile index immediately after the header, so `index_offset = 290`. Readers MUST accept any value `≥ 290` that points inside the file.

### 4.12 `zoom_offsets[24]`

A fixed-size directory of 24 entries, one per zoom level `z ∈ [0, 23]`. Each entry is 8 bytes:

| Field | Type | Notes |
|---|---|---|
| `offset` | u32 LE | byte offset of the first tile-index entry at this zoom |
| `count` | u32 LE | number of tile-index entries at this zoom |

For zooms with no tiles, both fields MUST be `0`. For zooms with tiles, `offset` is the byte offset of the first tile-index entry at that zoom (computed as `index_offset + 20 × cumulative_count_of_lower_zooms`), and `count` equals the number of entries walked at that zoom in the index.

The 24-slot fixed size accommodates zooms 0 through 23 inclusive. Zoom 22 is the deepest level publicly served by OSM and Google Maps as of writing; zoom 23 is one slot of headroom.

### 4.13 `extensions_offset`

u32 little-endian. Byte offset where the first extension section begins.

- MUST be `≥` the end of the tile blob.
- MUST be `≤ file_size − 4` (the CRC footer occupies the last 4 bytes).
- For packs with no extension sections, `extensions_offset` points at the CRC footer (i.e. `= file_size − 4`).

## 5. Tile index

A contiguous array of 20-byte entries starting at `index_offset`, holding `tile_count` entries.

### 5.1 Entry layout (20 bytes)

| Offset | Size | Field | Notes |
|---|---|---|---|
| 0 | 1 | `z` | u8; tile zoom level |
| 1 | 1 | `compression` | enum, § 8.5 |
| 2 | 1 | `flags` | u8; reserved in v1, MUST be 0 |
| 3 | 1 | `reserved` | MUST be 0 |
| 4 | 4 | `x` | u32 LE; tile column |
| 8 | 4 | `y` | u32 LE; tile row (interpreted per `tile_axis_convention`) |
| 12 | 4 | `offset` | u32 LE; byte offset of the tile bytes |
| 16 | 4 | `length` | u32 LE; tile-bytes length |
| **20** | | **end of entry** | |

### 5.2 Constraints

A conforming pack satisfies all of:

- Entries are sorted ascending by `(z, x, y)`.
- `z < 24` for every entry.
- `compression` is a value supported by the writer's `format_version` per § 8.5. (v1: only `0 = None`.)
- `flags = 0` and `reserved = 0` for every entry in v1. Readers MUST reject non-zero values.
- `offset` is 4-byte aligned and lies within the tile blob.
- `offset + length ≤ extensions_offset`.
- No two entries share the same `(z, x, y)` triple.

### 5.3 Tile lookup

A reader looking up the bytes for `(z, x, y)` SHOULD:

1. Read `zoom_offsets[z]`. If `count == 0`, the tile is absent.
2. Binary-search the `count` entries starting at `offset` for the `(x, y)` key. The within-zoom ordering by `(x, y)` guarantees a well-defined ordering.
3. If found, read `length` bytes at the entry's `offset` from the file.

## 6. Tile blob

The tile blob is the contiguous region from the (padded) end of the tile index to `extensions_offset`. It contains the raw tile bytes referenced by each index entry's `(offset, length)`.

### 6.1 Alignment

- The blob's start offset MUST be 4-byte aligned. Writers achieve this by emitting 0–3 zero bytes after the tile index.
- Each tile MUST start at a 4-byte-aligned offset.
- Each tile MUST be followed by 0–3 zero bytes of padding so that the next tile (or `extensions_offset`) is 4-byte aligned. Padding bytes are not part of the tile and are NOT counted in the entry's `length`.

### 6.2 Content

The byte content of a tile is determined by `pixel_format` (§ 8.1) and `compression` (§ 8.5).

For v1 with `pixel_format = ABGR2222` and `compression = None`, every tile is exactly `tile_dim_px × tile_dim_px` bytes (= 16,384 bytes for the standard 128² watch tile).

## 7. Extension sections

Extension sections begin at `extensions_offset` and continue until the CRC footer. A pack MAY contain zero or more sections.

### 7.1 Section framing

Each section is laid out as:

| Offset within section | Size | Field |
|---:|---:|---|
| 0 | 4 | `tag` (FourCC, ASCII) |
| 4 | 4 | `length` (u32 LE) |
| 8 | `length` | `payload` |
| 8 + `length` | 0–3 | zero padding to 4-byte boundary |

The next section begins at the 4-byte-aligned offset following the previous section's padding.

`length` is the payload length in bytes; it does NOT include the 8-byte section header or trailing padding.

### 7.2 Tag naming and reader behavior on unknown tags

The four-byte tag is compared verbatim. Case is normative for forward-compatibility behavior:

- **Upper-case ASCII first byte** (`A–Z`): the tag is **SDK-reserved** ("critical" in PNG terms). Allocated by spec versions. Readers MUST reject any pack containing an upper-case tag they do not recognise.
- **Lower-case ASCII first byte** (`a–z`): the tag is **application-private** ("ancillary"). Writers MAY emit any such tag for their own purposes; readers MUST accept the pack and MAY ignore unknown lower-case tags.

Tag bytes 2–4 MAY be any printable ASCII; their case has no normative meaning.

### 7.3 Reserved tags (v1)

| Tag | Meaning | Payload |
|---|---|---|
| `NAME` | Pack display name | Length-prefixed BCP-47 tag + UTF-8 name; see § 7.4. Multiple `NAME` sections MAY appear (one per locale). |
| `SRCD` | Source description | Free-form UTF-8 provenance text (e.g. *"OSM 2026-04 Geofabrik Italy extract, MapLibre watch-tuned style v2"*). |
| `ATTR` | Attribution | UTF-8; newline-separated attribution strings, one per active source, no trailing newline. |
| `PLET` | Palette | Packed pixel-format bytes (one per palette entry). Required when `pixel_format` is an indexed format; reserved for future use in v1. |
| `AFFN` | Affine matrix | 48 bytes: six little-endian IEEE-754 `f64` values `(a, b, c, d, e, f)` defining the 2×3 affine `[a b c; d e f]` that maps image-pixel coordinates `(u, v)` to geographic coordinates `(lon, lat)` in decimal degrees: `lon = a·u + b·v + c`, `lat = d·u + e·v + f`. Required when `projection = LocalLinear`. |

Conditional requirements:

- `AFFN` MUST be present when `projection = LocalLinear`. Readers MUST reject LocalLinear packs without `AFFN`.
- `PLET` MUST be present when `pixel_format` is an indexed format (none in v1; reserved).

### 7.4 `NAME` payload layout

The `NAME` section's payload is length-prefixed, not delimiter-separated:

| Offset within payload | Size | Field |
|---:|---:|---|
| 0 | 1 | `tag_length` (u8) — number of bytes in the BCP-47 language tag |
| 1 | `tag_length` | `bcp47_tag` — BCP-47 language tag bytes, UTF-8 (ASCII in practice; RFC 5646 tags are ASCII-only) |
| 1 + `tag_length` | — | `name` — UTF-8 pack name, occupies the remainder of the payload |

Rules:

- `tag_length` MAY be `0`, indicating "no locale specified". A pack with multiple `NAME` sections SHOULD include exactly one section with `tag_length = 0` as the unlocalized fallback name.
- `bcp47_tag` MUST be a syntactically valid BCP-47 language tag per RFC 5646. Readers SHOULD NOT validate the semantic correctness of the tag (e.g. whether a region subtag is registered) — that's the writer's responsibility.
- `name` MUST be valid UTF-8 and SHOULD NOT be empty.
- The total payload length is `1 + tag_length + name.len()`; the section header's `length` field carries this total.

Readers selecting a `NAME` section for display:

1. If multiple sections are present, prefer the one whose `bcp47_tag` best matches the device locale per BCP-47 lookup rules (RFC 4647 § 3.4).
2. Fall back to the `tag_length = 0` section if no locale matches.
3. If no fallback section exists, readers MAY pick any of the available `NAME` sections; the choice is implementation-defined.

**Rationale for length-prefixing over delimiter-separation**: BCP-47 tags don't contain tabs, so a tab-delimited form would also work for v1; but length-prefixing is binary-clean (no need for readers to scan for an in-band delimiter), aligns with the rest of the format's length-prefix conventions (extension sections, tile-index entries), and is robust against any future tag-syntax expansion. Names containing tab characters (allowed under "free-form UTF-8") would break a tab-delimited form silently.

## 8. Enumerations

In every enum, readers MUST reject any unknown value encountered in the header or tile index. Forward-compatible additions arrive via spec minor-version bumps (§ 13), not by injecting unknown values into v1 packs.

### 8.1 `pixel_format` (header byte 54)

| Value | Name | Status |
|---:|---|---|
| 0 | reserved | reader MUST reject |
| 1 | `ABGR2222` | v1 |
| 2 | reserved (`L4` indexed) | reader MUST reject |
| 3 | reserved (`L2` indexed) | reader MUST reject |
| 4 | reserved (`BW`) | reader MUST reject |
| 5–255 | reserved | reader MUST reject |

### 8.2 `projection` (header byte 55)

| Value | Name | Status |
|---:|---|---|
| 0 | reserved | reader MUST reject |
| 1 | `WebMercator` | v1 |
| 2 | reserved (equirectangular) | reader MUST reject |
| 3 | `LocalLinear` | v1 (single-image hand-drawn packs) |
| 4–255 | reserved | reader MUST reject |

### 8.3 `tile_addressing_scheme` (header byte 56)

| Value | Name | Status |
|---:|---|---|
| 0 | reserved | reader MUST reject |
| 1 | `Quadtree` | v1 |
| 2 | `SingleImage` | v1 |
| 3–255 | reserved | reader MUST reject |

`projection` and `tile_addressing_scheme` are not independently combinable; see § 8.6 for the legal pair table.

### 8.4 `tile_axis_convention` (header byte 57)

| Value | Name | Status |
|---:|---|---|
| 0 | reserved | reader MUST reject |
| 1 | `XYZ` | v1 (slippy-map default; Y increases southward) |
| 2 | `TMS` | v1 (`gdal2tiles --profile mercator` default; Y increases northward) |
| 3–255 | reserved | reader MUST reject |

Meaningful only when `addressing_scheme = Quadtree`. For `SingleImage`, readers MUST accept the byte at face value (treating any of `1` or `2` as valid) and SHOULD ignore it for rendering.

### 8.5 `compression` (tile-index byte 1)

| Value | Name | Status |
|---:|---|---|
| 0 | `None` | v1 |
| 1 | reserved (`LZ4`) | reader MUST reject |
| 2 | reserved (`QOI`) | reader MUST reject |
| 3–255 | reserved | reader MUST reject |

### 8.6 Legal enum combinations and structural constraints

Not every combination of `projection` × `tile_addressing_scheme` is meaningful. v1 defines exactly two legal pairs; readers MUST reject all others.

| `projection` | `tile_addressing_scheme` | Legal in v1 | Description |
|---|---|:---:|---|
| `WebMercator` (1) | `Quadtree` (1) | ✅ | The standard slippy-map case: pyramidal tiles at zooms `[zoom_min, zoom_max]`, indexed by `(z, x, y)`. |
| `WebMercator` (1) | `SingleImage` (2) | ❌ — MUST reject | Undefined. A "single Mercator image" has no canonical bounds. |
| `LocalLinear` (3) | `Quadtree` (1) | ❌ — MUST reject | Undefined. Local-linear coordinates have no canonical pyramidal subdivision. |
| `LocalLinear` (3) | `SingleImage` (2) | ✅ | One image with a corner-to-lat/lon affine (`AFFN`). For hand-drawn maps and similar uses. |

Readers MUST verify this pairing against the header bytes at offsets 55 and 56 before doing any further parsing.

**SingleImage tile-index constraint.** When `tile_addressing_scheme = SingleImage`:

- `tile_count` MUST be exactly `1`.
- The lone index entry's `z` MUST be `0`. (Its `x` and `y` are unconstrained by the spec but conventionally both `0`.)
- `zoom_min` and `zoom_max` in the header MUST both be `0`.
- `zoom_offsets[0]` is the only non-zero directory entry; `zoom_offsets[1..24]` MUST be all-zero.

Readers MUST reject `SingleImage` packs that violate any of these. v1.0 deliberately does NOT support tiled (multi-image) `SingleImage` packs — future tiled forms get a new `tile_addressing_scheme` enum value via a minor-version bump (§ 13), not an ambiguous reinterpretation of `SingleImage = 2`.

## 9. Pixel formats

### 9.1 `ABGR2222`

Each pixel is one byte. Bit layout, MSB to LSB:

```
bit:   7  6   5  4   3  2   1  0
       └─A─┘  └─B─┘  └─G─┘  └─R─┘
```

- Each channel is 2 bits, encoding quanta `{0, 1, 2, 3}` displayed as `{0, 85, 170, 255}` (8-bit equivalents).
- Writers MUST set `A = 3` (fully opaque) for every pixel in v1 packs. v1 readers MUST treat any pixel as opaque regardless of the alpha bits.

#### 9.1.1 Canonical quantisation from RGB888

Slippypack's canonical quantisation maps each 8-bit channel to a 2-bit quantum via thresholds at the midpoints between displayed levels:

| Input range | Output quantum | Displayed level |
|---:|---:|---:|
| 0 – 42 | 0 | 0 |
| 43 – 127 | 1 | 85 |
| 128 – 212 | 2 | 170 |
| 213 – 255 | 3 | 255 |

The quantisation is integer-only by construction: any conforming implementation MUST produce byte-identical output for the same input across architectures, languages, and platforms.

This quantisation is identified by `quantiser_version = 1` in Appendix A's descriptor. Any byte-output change to the quantisation requires a `quantiser_version` bump.

Conformance test vectors are in § 14.4.

## 10. Footer (CRC)

The last 4 bytes of the file are a u32 little-endian CRC-32 value.

- **Algorithm:** CRC-32/ISO-HDLC (the "PNG/zlib" CRC).
  - Polynomial: `0xEDB88320` (reflected form of `0x04C11DB7`).
  - Initial value: `0xFFFFFFFF`.
  - Input reflected, output reflected, XOR-out `0xFFFFFFFF`.
  - Check value for ASCII `"123456789"`: `0xCBF43926`.
- **Scope:** every byte from offset 0 up to (but not including) the CRC's own 4 bytes.

Readers MUST verify the CRC at open time and reject the pack on mismatch.

## 11. Reader requirements

A conforming v1 reader MUST:

1. Reject any file shorter than 398 bytes (header + CRC footer).
2. Reject any file whose first 4 bytes are not `RAWT`.
3. Reject any pack whose `format_version_major ≠ 1`.
4. Accept packs with `format_version_minor > 0`, applying §§ 7.2 and 8 to any extension tags or enum values they contain.
5. Reject `pack_uuid` equal to all-zero.
6. Reject `parent_uuid` not equal to all-zero.
7. Reject any unknown `pixel_format`, `projection`, `tile_addressing_scheme`, `tile_axis_convention`, or `compression` byte (§ 8).
8. Reject any tile-index entry with non-zero `flags` or non-zero `reserved`.
9. Reject any tile-index entry whose `offset` is not 4-byte aligned, lies before the tile blob, or whose `offset + length` exceeds `extensions_offset`.
10. Reject the pack if `zoom_offsets[z].count` does not equal the actual count of tile-index entries at zoom `z` for any `z`.
11. Reject any pack containing an unknown extension tag whose first byte is upper-case ASCII (`A–Z`).
12. Accept and MAY ignore any unknown extension tag whose first byte is lower-case ASCII.
13. Reject `projection = LocalLinear` packs that do not contain an `AFFN` extension.
14. Verify the CRC-32 footer and reject the pack on mismatch.

A conforming v1 reader SHOULD:

15. Use byte-wise (`memcpy`-style) extraction when reading multi-byte fields. The format is byte-oriented; no multi-byte field is guaranteed to be naturally aligned in the file. Native pointer-cast reads fault on strict-alignment platforms (notably some Cortex-M configurations).
16. Validate that `index_offset ≥ 290` and that `extensions_offset ≥ index_offset + 20 × tile_count + tile_blob_size`.

## 12. Writer requirements

A conforming v1 writer MUST:

1. Emit exactly the bytes defined by §§ 4–10 for the inputs.
2. Choose `pack_uuid` as a non-zero 16-byte value (or derive it per Appendix A).
3. Set `parent_uuid` to all-zero.
4. Place the tile index immediately after the header (`index_offset = 290`).
5. Sort the tile index ascending by `(z, x, y)`.
6. Reject duplicate `(z, x, y)` tile inputs at write time.
7. Pad the tile index to a 4-byte boundary before the tile blob.
8. Place each tile at a 4-byte-aligned offset and pad with 0–3 zero bytes between tiles.
9. Place each extension section starting at a 4-byte-aligned offset; pad each payload to a 4-byte boundary with zero bytes.
10. Populate `zoom_offsets[z] = (0, 0)` for every zoom `z` with no tiles, and `(byte_offset_of_first_entry_at_z, count_at_z)` otherwise.
11. Emit an `AFFN` extension when `projection = LocalLinear`.
12. Compute the CRC-32 over every preceding byte and emit it as the file's last 4 bytes.

A conforming v1 writer SHOULD:

13. Set `build_timestamp` to the most-recent source-data freshness time (mtime / `Last-Modified`), not wall-clock build time, when reproducibility is a goal.
14. Use only ASCII printable bytes in extension tags.

A conforming v1 writer MUST NOT:

15. Emit an upper-case extension tag not defined in this spec (§ 7.3).
16. Emit a non-zero `flags` or `reserved` byte in any tile-index entry.

## 13. Versioning

### 13.1 Semantics

- **Major bump** (e.g. `1.0 → 2.0`): incompatible change. Header layout, tile-index layout, CRC scope, or pixel-format encoding may change. v1 readers MUST reject v2 packs.
- **Minor bump** (e.g. `1.0 → 1.1`): additive change. The header layout is frozen per major version; minor bumps allocate new extension tags, new enum values, or relax existing constraints. A v1.0 reader MUST accept v1.x packs, but the per-§ 7.2 / § 8 rules cause it to reject any v1.x pack that uses newly-allocated SDK-reserved values it doesn't know.

### 13.2 Adding new SDK-reserved extension tags

New upper-case tags are allocated by minor-version bumps. Writers MAY emit them in any v1.x pack with `x ≥` the minor that introduced the tag. Readers built against an earlier minor will reject such packs (per § 7.2), which is the intended forward-compatible behavior.

### 13.3 Adding new enum values

New `pixel_format`, `projection`, `addressing_scheme`, `axis_convention`, or `compression` values are allocated by minor-version bumps. Readers built against earlier minors reject packs that use them.

### 13.4 Adding new application-private extension tags

Lower-case tags can be allocated at any time by any writer without a version bump. Readers MUST tolerate unknown lower-case tags.

## 14. Conformance

### 14.1 Round-trip property

For every conforming pack, applying a conforming reader followed by a conforming writer (with the same input metadata) MUST produce a byte-identical pack. This is the format's lossless claim.

### 14.2 Cross-implementation gate

The slippypack repository ships an independent C++ validator at `spec-validator-cpp/`. The validator re-derives parsing from this specification without reusing slippypack's Rust source. CI runs the validator against every committed golden fixture. Third-party readers SHOULD pass the same validator against the same fixtures.

### 14.3 Golden fixtures

Slippypack commits a corpus of test fixtures exercising every interesting v1 layout shape (smallest non-empty pack; multi-zoom directory; multi-source `ATTR`; single-image `AFFN`; …). Their `pack_uuid`s and CRCs are pinned; any drift is a bug or a deliberate `quantiser_version` / `format_version` bump.

### 14.4 ABGR2222 quantiser test vector

A conforming writer applying the canonical quantisation of § 9.1.1 to this 16-pixel RGB888 input MUST produce the listed output. Mismatch indicates either a quantiser bug or a `quantiser_version` divergence.

Input (48 bytes, RGB888, 16 pixels):

```
255,  0,  0      0,255,  0      0,  0,255    255,255,255
128,  0,  0      0,128,  0      0,  0,128    128,128,128
 42, 42, 42     43, 43, 43     85, 85, 85    127,127,127
170,170,170    212,212,212    213,213,213    255,128,  0
```

Output (16 bytes, ABGR2222):

```
0xC3, 0xCC, 0xF0, 0xFF,
0xC2, 0xC8, 0xE0, 0xEA,
0xC0, 0xD5, 0xD5, 0xD5,
0xEA, 0xEA, 0xFF, 0xCB
```

### 14.5 CRC-32 check value

For the ASCII input `"123456789"`, the CRC-32/ISO-HDLC algorithm of § 10 produces `0xCBF43926`. Conforming implementations MUST match this value.

## 15. File extension and MIME type

- **File extension:** `.rawtiles`
- **MIME type:** `application/vnd.rawtiles` (proposed; not registered with IANA).

---

## Appendix A — Canonical `pack_uuid` derivation

This appendix defines slippypack's `pack_uuid` derivation. It is normative for writers that need to produce byte-identical packs to slippypack for the same logical inputs. Writers without that goal MAY choose any non-zero 16-byte value for `pack_uuid`.

### A.1 Namespace

The slippypack UUID namespace is the constant:

```
RAWTILES_NAMESPACE = 4e72f962-6632-4538-8e0a-7eab63350f3f
```

This value MUST NOT vary across writer versions. Changing it would invalidate every `pack_uuid` ever produced and break the "did the watch already receive this pack?" deduplication check.

### A.2 Derivation

```
pack_uuid = UUIDv5(RAWTILES_NAMESPACE, canonical_descriptor_bytes)
```

where `canonical_descriptor_bytes` is defined in § A.3 and UUIDv5 is the SHA-1-based name-based UUID per RFC 4122 § 4.3.

### A.3 Canonical source descriptor

`canonical_descriptor_bytes` is the UTF-8 encoding of a JSON object with these strict canonicalisation rules:

- No whitespace anywhere.
- Top-level keys in lexicographic codepoint order.
- No trailing newline.
- Integers in decimal; no leading zeros; no `+`/`.0`.
- File-content hashes as lowercase hex SHA-256.
- String escapes: `"` → `\"`, `\` → `\\`, any control character (codepoint < `0x20`) → `\uXXXX` (four lowercase hex digits). No other escape forms (`\n`, `\t`, `\/`, etc.) appear.
- All numeric coordinates are integer microdegrees (= decimal degrees × 10⁶) using banker's rounding (half-to-even). Inputs differing by less than 10⁻⁶ degrees produce identical descriptors and identical `pack_uuid`s.

Top-level keys, in lex order:

| Key | Type | Source |
|---|---|---|
| `bbox` | `[i64, i64, i64, i64]` | `[min_lon_µ°, min_lat_µ°, max_lon_µ°, max_lat_µ°]` |
| `format_version` | `[u8, u8]` | from § 4.2 |
| `pixel_format` | int | from § 8.1 |
| `projection` | int | from § 8.2 |
| `quantiser_version` | int | `1` for v1's `ABGR2222` quantiser; bumped on any byte-output change |
| `sources` | array | one object per active source, ordered per § A.4 |
| `style_hash` | hex string or `null` | SHA-256 of the MapLibre style JSON when a renderer-style is in play; `null` otherwise |
| `tile_addressing_scheme` | int | from § 8.3 |
| `tile_axis_convention` | int | from § 8.4 |
| `tile_dim_px` | int | from § 4.7 |
| `zoom_range` | `[u8, u8]` | `[zoom_min, zoom_max]` from § 4.8 |

When `projection = LocalLinear`, an additional top-level key `affn` (sorted into lex position) carries the six affine coefficients as integer microunits.

### A.4 `sources` ordering and per-kind shape

The `sources` array is sorted ascending by `(zoom_min, zoom_max, derived_source_order)`. The derived order compares the source's `kind` name lexicographically (`dir < geotiff < image < mbtiles < pbf < pmtiles < style < synthetic < url`), then the source's *identity* (URL template for `url`; content hash for file-backed kinds; `fixture_version` for `synthetic`).

Per-kind entry shapes (keys in lex order within each object):

- **File-backed kinds** (`dir`, `geotiff`, `mbtiles`, `pbf`, `pmtiles`, `style`):

  ```
  {"content_hash":"<sha256-hex>","kind":"<kind>","zoom_max":<int>,"zoom_min":<int>}
  ```

  For `style`, the `content_hash` is the SHA-256 of the style JSON.

- **`synthetic`** (built-in fixture):

  ```
  {"fixture_version":<int>,"kind":"synthetic"}
  ```

- **`url`** (URL template):

  ```
  {"auth_kinds":[…],"kind":"url","template":"<url>","zoom_max":<int>,"zoom_min":<int>}
  ```

  `auth_kinds` is a sorted, deduplicated array drawn from `"header"` and `"query"`. Authentication *values* (API keys, tokens) MUST NOT appear in the descriptor — only the *kinds* of authentication in use. This keeps `pack_uuid` stable across credential rotations.

- **`image`** (LocalLinear hand-drawn):

  ```
  {"content_hash":"<sha256-hex>","kind":"image"}
  ```

### A.5 Worked example

Baseline descriptor for a single-source pack of OSM tiles, z=6–12, world-scale bbox:

```json
{"bbox":[-180000000,-85000000,180000000,85000000],"format_version":[1,0],"pixel_format":1,"projection":1,"quantiser_version":1,"sources":[{"auth_kinds":[],"kind":"url","template":"https://tile.openstreetmap.org/{z}/{x}/{y}.png","zoom_max":12,"zoom_min":6}],"style_hash":null,"tile_addressing_scheme":1,"tile_axis_convention":1,"tile_dim_px":128,"zoom_range":[6,12]}
```

Derived `pack_uuid`:

```
53077f67-522e-5cb0-b2b5-ffddba17d0db
```

Two writers can independently verify by feeding the canonical bytes above into any conformant UUIDv5 implementation with the namespace from § A.1.

---

## Appendix B — Reserved values

Allocations awaiting future minor versions. Implementations MUST NOT emit these values; readers MUST reject any pack that contains them.

| Field | Reserved values | Intent |
|---|---|---|
| `pixel_format` | `2` | `L4` indexed |
| `pixel_format` | `3` | `L2` indexed |
| `pixel_format` | `4` | `BW` (1-bit) |
| `projection` | `2` | equirectangular |
| `compression` | `1` | `LZ4` |
| `compression` | `2` | `QOI` |
| Extension tags (upper-case) | every 4-byte ASCII sequence whose first byte is `A–Z` and is not listed in § 7.3 | future SDK extensions |

All other byte values for the listed enums are unallocated; future spec versions will assign them.

---

## Appendix C — Change history

| Version | Date | Notes |
|---|---|---|
| 1.0 | 2026-05-14 | Initial publication. Frozen at slippypack 0.1.0 and una-sdk MapTrack Phase 1. |
