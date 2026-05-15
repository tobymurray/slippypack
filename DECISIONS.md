# Decisions log

Implementation-level decisions made during slippypack development. [PLAN.md](PLAN.md) documents the project-level design and architectural decisions; this file records the granular choices made during coding that PLAN.md doesn't already pin.

Each entry has a stable ID (never renumbered), a one-line decision, the rationale, a pointer to where the decision is enforced in code, and the commit it landed in.

When a decision is refined or reversed, **edit the entry rather than deleting it** — the history stays auditable. Add a "Superseded by …" line if a new entry replaces an old one.

---

## W — Workspace

### W-001 — Five-crate workspace
Layout: `slippypack-core`, `slippypack-cli`, `slippypack-web`, `slippypack-web-mbtiles`, `slippypack-web-pmtiles`. Matches PLAN.md § Code organisation.
**Manifests:** `Cargo.toml` (workspace `members`).
**Commit:** `16ff518`.

### W-002 — Rust edition 2024, MSRV 1.95
Recent stable; supports the language features we're likely to use across both std and no_std targets. `rust-toolchain.toml` pins the exact toolchain so contributors get reproducible builds.
**Manifests:** `Cargo.toml` (workspace.package), `rust-toolchain.toml`.
**Commit:** `16ff518`.

### W-003 — `Cargo.lock` committed
Standard Rust practice for workspaces that ship a binary (slippypack-cli). The lockfile is the cross-machine build-determinism anchor.
**Manifests:** `.gitignore` (Cargo.lock is NOT in the ignore list).
**Commit:** `16ff518`.

### W-004 — Release profile tuned for binary size
`lto = true`, `codegen-units = 1`, `panic = "abort"`, `strip = "symbols"`. Primarily for the WASM crates (where size is load-bearing per PLAN.md § Phase 4); the CLI inherits and can override per-crate if a benchmark indicates runtime perf matters more.
**Manifests:** `Cargo.toml` `[profile.release]`.
**Commit:** `16ff518`.

### W-005 — `unsafe_code = "forbid"` at workspace level
Nothing in slippypack's scope (format writer, integer math, JSON canonicalization, projection math, async fetch) needs `unsafe`. Forbidding it prevents accidental drift and saves a code-review axis.
**Manifests:** `Cargo.toml` `[workspace.lints.rust]`.
**Commit:** `16ff518`.

### W-006 — `clippy::pedantic = warn` at workspace level
Catches stylistic issues early. `module_name_repetitions = allow` because the workspace expects module names to echo their parent (e.g., `Mercator` in `mercator.rs`, `RawtilesWriter` in `format/mod.rs`).
**Manifests:** `Cargo.toml` `[workspace.lints.clippy]`.
**Commit:** `16ff518`.

### W-007 — `doc-valid-idents` extended in `clippy.toml`
Adds proper-noun format / protocol names that look like Rust identifiers but aren't: `MBTiles`, `PMTiles`, `UUIDv5`, `MapLibre`, `MapTiler`, `MapTrack`. The `..` suffix preserves clippy's default list (`OAuth`, `OpenStreetMap`, `WebAssembly`, etc.). Rust identifiers in our own code (e.g. `TileWriter`) keep their backticks — they're not exempted.
**Manifests:** `clippy.toml`.
**Commit:** `951389d`.

### W-008 — `slippypack-core` kept as `std` for now
PLAN.md commits to `no_std + alloc`. Deferred until the `format` module lands because no current module needs allocations. The switch is one PR (`#![no_std]` + `extern crate alloc` + libm dep) when the first alloc-requiring module ships.
**Manifests:** `crates/slippypack-core/src/lib.rs` (no `#![no_std]` attribute yet).
**Open until:** the `format` module commit.

---

## Q — Quantise module

### Q-001 — ABGR2222 byte layout: `AABBGGRR` from MSB to LSB
Neither PLAN.md nor the una-sdk MapTrack PLAN explicitly pins the bit order of the ABGR2222 byte. Picked the most common ABGR2222 convention: channel order MSB → LSB matching the name (`A` in bits 7–6, `B` in 5–4, `G` in 3–2, `R` in 1–0). Needs cross-verification against the una-sdk `TilePack` reader when MapTrack Phase 2 lands.
**Manifests:** `crates/slippypack-core/src/quantise.rs::quantise_pixel` (`0b1100_0000 | (b2 << 4) | (g2 << 2) | r2`).
**Commit:** `cdd611d`.

### Q-002 — Alpha always = `3` (fully opaque) in v1
v1 packs are opaque map tiles. The top 2 bits of every quantised byte are hard-coded to `11`. If overlay packs ever ship semi-transparent tiles, this becomes an input-dependent value, which would be a `QUANTISER_VERSION` bump.
**Manifests:** `crates/slippypack-core/src/quantise.rs::quantise_pixel` (`0b1100_0000 |` constant).
**Commit:** `cdd611d`.

### Q-003 — Channel thresholds at midpoints `42|43`, `127|128`, `212|213`
Midpoints between the four displayed levels `{0, 85, 170, 255}` are `42.5`, `127.5`, `212.5`. Round-to-nearest puts each integer input in the bucket whose displayed value is closer.
**Manifests:** `crates/slippypack-core/src/quantise.rs::channel_to_2bit`.
**Commit:** `cdd611d`.

### Q-004 — If-chain implementation, not `(v + 42) / 85` integer math
Functionally identical, but the if-chain avoids the `u8` overflow concern (`v + 42 > 255` for `v ≥ 214`) and gives clippy-pedantic-clean code with no cast-related lints to suppress.
**Manifests:** `crates/slippypack-core/src/quantise.rs::channel_to_2bit`.
**Commit:** `cdd611d`.

### Q-005 — `QUANTISER_VERSION: u32 = 1`
`u32` to fit the canonical descriptor's `int` keys. Starts at `1` (no `0` — `0` is conventionally "unset / invalid").
**Manifests:** `crates/slippypack-core/src/quantise.rs::QUANTISER_VERSION`.
**Commit:** `cdd611d`.

### Q-006 — `quantise_pixel` is `const fn`
Enables compile-time quantisation for known palettes (e.g. baking the synthetic fixture into constants). No runtime cost.
**Manifests:** `crates/slippypack-core/src/quantise.rs::quantise_pixel`.
**Commit:** `cdd611d`.

### Q-007 — `quantise_rgb888` panics on size mismatch (no `Result`)
This is a hot-path function called per pixel; size mismatches are caller bugs, not runtime conditions. Higher layers (`format`, CLI args) validate at their own surface before reaching this function.
**Manifests:** `crates/slippypack-core/src/quantise.rs::quantise_rgb888` (`assert!` on `input.len() % 3 == 0` and `output.len() == input.len() / 3`).
**Commit:** `cdd611d`.

---

## P — Projection module

### P-001 — `Projection` trait now (with one impl); Local Linear deferred
PLAN.md calls for both `Mercator` and `LocalLinear` in Phase 0. Local Linear's runtime support is Phase 10; landing Mercator first keeps the slice small. The trait is declared now so the eventual `LocalLinear` impl slots in without re-architecting.
**Manifests:** `crates/slippypack-core/src/projection/mod.rs` (trait declaration); `crates/slippypack-core/src/projection/mercator.rs` (sole impl).
**Open until:** Local Linear lands (either later in Phase 0 or with Phase 10).

### P-002 — `f64` throughout (not `f32`)
At v1's max zoom (z=17), a tile is ~2 cm wide at the equator. `f64` (~15 significant decimal digits) has comfortable margin; `f32` would work but with thinner headroom for accumulator math.
**Manifests:** `crates/slippypack-core/src/projection/mercator.rs` (all signatures and intermediate values).
**Commit:** `34521de`.

### P-003 — Platform `libm` dependency accepted (deterministic-modulo-libm)
Mercator's y-coordinate computation uses `f64::tan` and `f64::asinh`, which delegate to platform `libm`. For realistic user inputs the integer tile coordinates are byte-identical across platforms because the float result lands far from any `floor`-boundary. When slippypack-core switches to `no_std + alloc` (per W-008), this module will switch to the pure-Rust `libm` crate for guaranteed cross-platform identical output.
**Manifests:** module doc comment in `crates/slippypack-core/src/projection/mercator.rs`.
**Open until:** W-008 closes.

### P-004 — Out-of-range coordinate inputs clamp; do not panic
Out-of-range lat/lon usually comes from bbox edges that extend past Mercator's coverage (a country whose southernmost tip is below the LAT_LIMIT), not from buggy callers. Clamping (`lat → ±LAT_LIMIT_DEG`, `lon → ±180°`) is more useful than panicking.
**Manifests:** `crates/slippypack-core/src/projection/mercator.rs::lonlat_to_tile` (`.clamp` calls).
**Commit:** `34521de`.

### P-005 — `tile_to_lonlat` returns the tile's **NW corner**
Conventional in slippy-map literature. Easier to compose with bbox-edge math than "tile centre" or "tile SW corner."
**Manifests:** `crates/slippypack-core/src/projection/mercator.rs::tile_to_lonlat` doc.
**Commit:** `34521de`.

### P-006 — `LAT_LIMIT_DEG = 85.051_128_779_806_59`
Mercator coverage limit `atan(sinh(π)) × 180 / π`. Pinned to 15 significant decimal digits (f64's full precision).
**Manifests:** `crates/slippypack-core/src/projection/mercator.rs::Mercator::LAT_LIMIT_DEG`.
**Commit:** `34521de`.

### P-007 — `Mercator` is a unit struct (Default-constructible)
No state, but methods take `&self` to fit the trait shape that accommodates stateful projections (like `LocalLinear` with its affine matrix).
**Manifests:** `crates/slippypack-core/src/projection/mercator.rs::Mercator`.
**Commit:** `34521de`.

---

## W — Workspace (continued)

### W-009 — `extern crate alloc;` added at the lib root
Lets modules use the `alloc::*` path today (e.g. `alloc::collections::BTreeSet` in `RawtilesWriter`) even though slippypack-core is currently std-compiled. When W-008 closes (the no_std + alloc switch), code that already uses `alloc::*` paths needs no churn.
**Manifests:** `crates/slippypack-core/src/lib.rs` (`extern crate alloc;`).
**Commit:** to land with the format slice-B commit.

## F — Format module (byte-layout primitives)

### F-001 — Bbox stored as 4×i32 microdegrees in the header (16 bytes)
PLAN.md doesn't pin the on-disk byte layout for `bbox`. Choices considered: 4×f64 (32 bytes, brings float-determinism issues), 4×i32 microdegrees (16 bytes, matches the canonical descriptor's representation), 4×f32 (16 bytes, but precision-marginal at z=17). Picked microdegrees because they match the descriptor (one less encoding to reason about), give ~11 cm precision at the equator (well below tile granularity at any v1 zoom), and avoid all float-determinism concerns inside the header. The exact in-memory `i32` order is `min_lon, min_lat, max_lon, max_lat` (matches the descriptor key ordering).
**Manifests:** `crates/slippypack-core/src/format/header.rs::write_header` (offsets 62..78); `BoundingBox` struct in `identity.rs` (shared between descriptor and header).
**Commit:** to land with the format-primitives commit.

### F-002 — Header is exactly 322 bytes (`HEADER_BASE_SIZE`)
Computed from the spec field-by-field. `4 (magic) + 2 (version) + 48 (3 UUIDs) + 4 (4 enum bytes) + 2 (tile_dim_px) + 2 (zoom range) + 16 (bbox) + 8 (timestamp) + 4 (tile_count) + 8 (index_offset) + 216 (zoom_offsets[18]) + 8 (extensions_offset) = 322 bytes.` Pinned as a constant so callers can pre-allocate.
**Manifests:** `crates/slippypack-core/src/format/header.rs::HEADER_BASE_SIZE`.
**Commit:** to land with the format-primitives commit.

### F-003 — Header writer infallible; reader does all validation
`write_header(&PackMetadata, &DerivedHeaderFields) -> [u8; 322]` cannot fail — the type system enforces every legal enum value. Spec invariants (`pack_uuid != 0`, `parent_uuid == 0` in v1, `tile_dim_px >= 1`, `zoom_range.max >= zoom_range.min`, etc.) are checked at parse time via `read_header`. Rationale: invariants belong at the boundary where caller-provided data enters the spec, not at every intermediate hop.
**Manifests:** `crates/slippypack-core/src/format/header.rs::{write_header, read_header, HeaderError}`.
**Commit:** to land with the format-primitives commit.

### F-004 — `FORMAT_VERSION = (1, 0)` constant, not a field of `PackMetadata`
The writer always stamps the format-version from a build-time constant (`FORMAT_VERSION`). Callers don't pick the version — picking would let v1 builds produce v0.5 or v2 bytes by accident. When the format spec bumps, a single source-code change updates every pack slippypack produces.
**Manifests:** `crates/slippypack-core/src/format/types.rs::FORMAT_VERSION`; `PackMetadata` (no version field).
**Commit:** to land with the format-primitives commit.

### F-005 — Tile-index entry is exactly 24 bytes (`INDEX_ENTRY_SIZE`)
Per the una-sdk spec. Layout: `z (1) + compression (1) + flags (1) + reserved (1) + x (4) + y (4) + offset (8) + length (4) = 24`. Reader rejects non-zero compression (v1 supports only `0 = none`), non-zero flags, and non-zero reserved byte per the v1 forward-compatibility rules.
**Manifests:** `crates/slippypack-core/src/format/tile_index.rs::{INDEX_ENTRY_SIZE, write_index_entry, read_index_entry}`.
**Commit:** to land with the format-primitives commit.

### F-006 — Extension sections: `[tag (4) + length (4 LE) + payload + zero-pad-to-4]`
Wire format per the una-sdk spec. Section header is 8 bytes; payload is `length` bytes followed by 0-3 zero bytes to reach a 4-byte boundary. The reader's padding check is **strict** (non-zero padding is an error rather than a warning) — strict here trades a small chance of false-positive rejection (other writer made a mistake) for stronger determinism (we know exactly what bytes are in the buffer between sections).
**Manifests:** `crates/slippypack-core/src/format/extensions.rs::{write_extension_section, read_extension_sections, ExtensionError::NonZeroPadding}`.
**Commit:** to land with the format-primitives commit.

### F-007 — CRC-32/ISO-HDLC (the "PNG/zlib" CRC) for the pack footer
Polynomial `0xEDB88320` (reflected), init `0xFFFF_FFFF`, xor-out `0xFFFF_FFFF`. Standard variant used by PNG, gzip, zip, zlib — well-known and trivially auditable. Implementation is table-driven (1024-byte lookup table computed at compile time via `const fn`) for reasonable speed without a runtime initialization step or dependency.
**Manifests:** `crates/slippypack-core/src/format/crc.rs::{Crc32, crc32_ieee, CRC32_TABLE}`.
**Commit:** to land with the format-primitives commit.

### F-008 — Enum-byte parsers reject reserved values (v1 forward-compat)
`PixelFormat::from_byte`, `Projection::from_byte`, `AddressingScheme::from_byte`, `AxisConvention::from_byte`, and `Compression::from_byte` return `None` for reserved-but-not-implemented values. v1 readers MUST refuse packs that use them (per una-sdk § Forward-compatibility rules). Returning `None` lets the header/index parser surface this as a typed error (e.g. `HeaderError::InvalidPixelFormat(2)`) rather than silently misinterpreting.
**Manifests:** `crates/slippypack-core/src/format/types.rs::*::from_byte`; `crates/slippypack-core/src/format/tile_index.rs::Compression::from_byte`.
**Commit:** to land with the format-primitives commit.

### F-009 — `Compression` enum (with one variant) anticipates LZ4 / QOI reservations
v1 supports only `Compression::None`. The enum exists as a typed wrapper around the spec's `compression` byte so callers can't accidentally write a reserved value, and so future per-tile compression support (LZ4, QOI per una-sdk § Per-tile metadata) is a non-breaking addition via `#[non_exhaustive]`.
**Manifests:** `crates/slippypack-core/src/format/tile_index.rs::Compression`.
**Commit:** to land with the format-primitives commit.

### F-010 — `TileWriter` trait error type carries extra v1-only variants
PLAN.md § `TileWriter` trait pinned six `TileWriterError` variants. The implementation adds five more: `NotBegun`, `AlreadyBegun`, `TileTooLarge`, `ExtensionTooLarge`, `TileZoomOutOfRange`, `TileZoomTooHigh`. All represent caller misuse that the trait surface should reject explicitly rather than panic on. The enum is `#[non_exhaustive]`, so additions are non-breaking.
**Manifests:** `crates/slippypack-core/src/format/writer_trait.rs::TileWriterError`.
**Commit:** to land with the format slice-B commit.

### F-011 — `Write for Vec<u8>` + blanket `Write for &mut W`
Local `Write` trait gets an impl for `Vec<u8>` (with `Infallible` error) — convenient for in-memory tests and the OPFS round-trip — plus a blanket impl for `&mut W` so callers can pass `&mut buffer` to `finalize` without consuming the buffer. Both are local-trait impls so the orphan rule is satisfied.
**Manifests:** `crates/slippypack-core/src/format/writer_trait.rs` (`impl Write for Vec<u8>`, `impl<W: Write> Write for &mut W`).
**Commit:** to land with the format slice-B commit.

### F-012 — Tile blob starts at the first 4-byte-aligned offset after the index
The header is 322 bytes; the index is `N × 24` bytes. `322 mod 4 = 2`, so after the index the cursor is at offset `322 + 24N`, which is also `≡ 2 (mod 4)`. The writer emits **2 bytes of zero padding** between the index and the first tile to bring the tile blob to a 4-byte boundary, then aligns each subsequent tile by zero-padding 0-3 bytes after the previous tile's bytes. Per una-sdk PLAN.md "4-byte aligned tiles" for the watch's memcpy-blit hot path.
**Manifests:** `crates/slippypack-core/src/format/rawtiles_writer.rs::finalize` (the `pad_after_index` calculation and the per-tile alignment padding loop).
**Commit:** to land with the format slice-B commit.

### F-013 — `RawtilesWriter` state machine via single enum (`NotBegun` / `Building`)
Two states; transitions are NotBegun → Building (via begin_pack) and Building → consumed (via finalize). Each pre-build method (`add_tile_ref`, `add_extension`) checks state and returns `NotBegun` if begin_pack hasn't run. `register_byte_source` works in either state (byte sources are independent of pack metadata; SourceId is just an index into the Vec).
**Manifests:** `crates/slippypack-core/src/format/rawtiles_writer.rs::WriterState` and the `if let WriterState::Building(state) = ...` checks in each method.
**Commit:** to land with the format slice-B commit.

### F-014 — Sources held at `RawtilesWriter` level (not inside `Building`)
`byte_sources: Vec<Box<dyn TileByteSource<...>>>` lives at the writer level so `register_byte_source` can run before `begin_pack`. Alternative (sources inside Building) would force begin_pack-before-register or panic on pre-begin register. The current design is more flexible and matches the trait signature (register_byte_source doesn't return Result).
**Manifests:** `crates/slippypack-core/src/format/rawtiles_writer.rs::RawtilesWriter::byte_sources`.
**Commit:** to land with the format slice-B commit.

### F-015 — `add_tile_ref` validates zoom against `zoom_range` from metadata
A tile with `z < zoom_range.min` or `z > zoom_range.max` is rejected with `TileZoomOutOfRange`. Defensive: keeps the on-disk `zoom_offsets[18]` directory consistent with the header's declared range, and catches programmer mistakes early.
**Manifests:** `crates/slippypack-core/src/format/rawtiles_writer.rs::add_tile_ref` (the `if z < min || z > max` check).
**Commit:** to land with the format slice-B commit.

### F-016 — Reader holds buffer reference; metadata/index/extensions owned
`RawtilesReader<'a>` borrows the original buffer (so `tile_bytes` can return `&'a [u8]` zero-copy) but owns the parsed metadata, tile index, and extension sections (parsed once at `open`). The metadata struct's UUIDs etc. are 16-byte arrays — cheap to copy on parse, not worth chasing pointer-aliasing complexity for.
**Manifests:** `crates/slippypack-core/src/format/reader.rs::RawtilesReader`.
**Commit:** to land with the format slice-B commit.

### F-017 — `tile_bytes` binary-search within zoom_offsets[z] range
Lookup is O(log n) per the spec's mandatory binary-search rule (PLAN.md / una-sdk PLAN.md § Index lookup). `zoom_offsets[z]` gives the offset+count of tiles at zoom z; we binary-search within that range by `(x, y)`. Linear scan would be a spec conformance failure for the watch reader; slippypack-core's reader follows the same rule for consistency and as a behavioral reference.
**Manifests:** `crates/slippypack-core/src/format/reader.rs::RawtilesReader::tile_bytes`.
**Commit:** to land with the format slice-B commit.

### F-018 — Spec-layout test uses raw `.rawtiles` binary fixtures, not hex dumps
PLAN.md § Test plan called for `golden-pack-*.rawtiles.hex` text files. Implementation uses raw binary `.rawtiles` files instead because (a) total fixture size is ~3.2 KB (tiny — no diff-visibility benefit from hex encoding), (b) the test code is simpler with `std::fs::read` + byte-equal comparison, (c) `xxd file.rawtiles` is one command away when a diff is needed for forensics. Fixture-bootstrap is gated behind `BLESS_SPEC_LAYOUT=1 cargo test --test spec_layout` to prevent silent drift.
**Manifests:** `crates/slippypack-core/tests/spec_layout.rs::assert_matches_golden`; `crates/slippypack-core/tests/fixtures/format/*.rawtiles`.
**Commit:** to land with the spec-layout-test commit.

### F-019 — Pyramid spec-layout fixture trimmed to z=2..=4 (21 tiles), not the PLAN's z=2..=8 (5461 tiles)
PLAN.md sketched the synthetic-pyramid fixture as z=2..=8 with 5461 tiles. The smaller z=2..=4 form (1 + 4 + 16 = 21 tiles) is functionally equivalent for byte-layout coverage — it exercises the `zoom_offsets[18]` directory across three populated zooms (enough to verify the per-zoom offset arithmetic and the per-zoom count fields) without committing a ~150 KB golden file. Larger pyramids stress the watch reader's `O(log n)` lookup performance, but that's runtime correctness, not byte-layout correctness; spec_layout tests the latter.
**Manifests:** `crates/slippypack-core/tests/spec_layout.rs::build_pyramid_pack`.
**Commit:** to land with the spec-layout-test commit.

### F-020 — Spec-layout fixture tiles are 16-byte deterministic patterns, not real ABGR2222
The PLAN.md fixtures were sketched as PNG inputs producing ABGR2222 tile bytes. The spec_layout test instead uses 16-byte deterministic patterns keyed on `(z, x, y)` — no PNG decode involved. Rationale: spec_layout tests the **format module's** byte output, not the decode module's; using raw deterministic tile bytes keeps the test focused. The header still declares `tile_dim_px = 128` (no enforcement that actual tile content matches dim²) — the test exercises the on-disk header bytes, not the on-disk tile-content semantics. The decode module is tested separately (D-series).
**Manifests:** `crates/slippypack-core/tests/spec_layout.rs::synth_tile_content`.
**Commit:** to land with the spec-layout-test commit.

### F-021 — End-to-end pipeline test (PNG → decode → quantise → format)
Complements F-020 with an integration test that exercises the full pipeline against a committed PNG fixture. PR-1's `--source synthetic` and `--source <url>` paths follow exactly this composition (decode bytes from a source → quantise to ABGR2222 → write to a pack), so this test catches regressions in inter-stage shape contracts that per-stage unit tests miss. Uses `tile_dim_px = 2` (not the spec-mandated 128) since the test verifies pipeline composition rather than watch-loadability of the produced packs; this avoids committing 16 KB of decoded tile content per fixture. Same `BLESS_E2E=1` bootstrap pattern as spec_layout. **PNG-only — JPEG decode is too lossy / decoder-version-sensitive to commit a stable golden for.**
**Manifests:** `crates/slippypack-core/tests/roundtrip.rs`; `crates/slippypack-core/tests/fixtures/e2e/{input-2x2-rgb.png, golden-png-to-pack-*.rawtiles}`.
**Commit:** to land with the e2e-test commit.

---

## D — Decode module

### D-001 — `image` crate with `default-features = false, features = ["png", "jpeg"]`
Minimum format coverage for the slippypack pipeline. The `image` crate is used (rather than `png` / `jpeg-decoder` directly) for the unified API — `image::load_from_memory` auto-detects format from magic bytes and `DynamicImage::to_rgb8` handles palette / grayscale / RGBA → RGB flattening uniformly. Disabling default features keeps the WASM binary lean and pins the format scope at compile time (TIFF, WebP, AVIF, GIF, BMP are not compiled in; their magic bytes produce `DecodeError::DecodeFailed`).
**Manifests:** `crates/slippypack-core/Cargo.toml` `image` dep line.
**Commit:** to land with the decode module commit.

### D-002 — Alpha channel discarded; no compositing
`DynamicImage::to_rgb8()` discards alpha. Slippypack does not composite RGBA pixels against any background colour (black or white). Rationale: the downstream quantiser forces alpha = 3 (fully opaque) regardless, real-world map tiles are essentially always opaque, and compositing-over-background is a UI-policy decision the toolchain shouldn't impose. PNGs with transparency get their RGB channels used as-is.
**Manifests:** `crates/slippypack-core/src/decode.rs::decode_rgb888` (`dynamic.to_rgb8()` call); module-level doc explains.
**Commit:** to land with the decode module commit.

### D-003 — Grayscale broadcast to RGB; palette resolved
`image`'s `to_rgb8` conversion handles all common input variants: grayscale broadcasts the gray channel across R=G=B, palette inputs look up RGB values from the palette. No special handling needed in slippypack — the conversion is uniform and well-defined.
**Manifests:** `crates/slippypack-core/src/decode.rs::decode_rgb888`; module-level doc.
**Commit:** to land with the decode module commit.

### D-004 — `DecodeError` is a small payload-free enum
Three variants: `EmptyInput`, `DecodeFailed`, `ZeroDimension`. The underlying `image::ImageError` is mapped to `DecodeFailed` and discarded. Rationale: keeps slippypack's public surface stable across `image`-crate version bumps; decode failures in the pipeline are usually handled by "skip this tile, continue" so detailed error context isn't load-bearing. Can grow a richer variant later without breaking the simple case.
**Manifests:** `crates/slippypack-core/src/decode.rs::DecodeError`, `::map_image_err`.
**Commit:** to land with the decode module commit.

### D-005 — Test fixtures embedded as `&[u8]` byte literals (not files)
93-byte PNG and 415-byte JPEG fixtures embedded directly in `decode.rs` test module rather than committed as separate files. Smaller maintenance surface (no fixture files to track), tests are self-contained. Future tests with larger fixtures could move to `tests/fixtures/` (e.g. the synthetic-grid-z4 etc. fixtures for the format module's spec_layout test).
**Manifests:** `crates/slippypack-core/src/decode.rs::tests::FIXTURE_PNG_2X2`, `FIXTURE_JPEG_2X2`.
**Commit:** to land with the decode module commit.

### D-006 — JPEG test tolerance: ±16 per channel
JPEG is lossy. The decoded fixture pixels for pure-channel inputs (R=255 → channel ≈ 245) drift ~10 from the encoder's input. The test tolerance is 16 per channel — large enough to accept any reasonable JPEG decoder (including alternative decoders we might swap to later) while still failing if a decoder produces obviously-wrong output (e.g. swapped channels or completely garbled colors).
**Manifests:** `crates/slippypack-core/src/decode.rs::tests::JPEG_PER_CHANNEL_TOLERANCE = 16`, `assert_pixel_close`.
**Commit:** to land with the decode module commit.

---

## I — Identity module

### I-001 — `RAWTILES_NAMESPACE = 4e72f962-6632-4538-8e0a-7eab63350f3f`
Permanent UUIDv4 generated via `uuidgen` on macOS on 2026-05-13. Used as the seed for every UUIDv5 `pack_uuid` derivation. **Never changes** across slippypack versions — changing this value would alter every `pack_uuid` ever produced by slippypack and break the watch-side "is this pack already on the watch?" companion check.
**Manifests:** `crates/slippypack-core/src/identity.rs::RAWTILES_NAMESPACE`.
**Commit:** to land with the identity module commit.

### I-002 — Hand-rolled canonical JSON serializer, not `serde_json`
Three reasons: (a) the canonical form is precisely pinned by PLAN.md (sorted keys, no whitespace, no trailing newline) and `serde_json` defaults don't match cleanly, (b) zero dependency surface beyond `uuid`, (c) the descriptor schema is fixed so a hand-rolled serializer is small (~150 lines) and easy to audit.
**Manifests:** `crates/slippypack-core/src/identity.rs::canonical_descriptor_bytes` + internal `write_*` helpers.
**Commit:** to land with the identity module commit.

### I-003 — `uuid` crate, no default features, `v5` feature only
Minimal dependency surface: just UUIDv5 derivation. The `uuid` crate's pure-Rust SHA-1 backend is bundled with the `v5` feature, so no separate `sha1` dep. Compatible with `no_std + alloc` for the eventual switch (W-008).
**Manifests:** `crates/slippypack-core/Cargo.toml`.
**Commit:** to land with the identity module commit.

### I-004 — Control-character escapes use `\u00XX` form uniformly
JSON allows shorter escapes for common control chars (`\n`, `\t`, `\r`, `\b`, `\f`). The hand-rolled serializer uses the long `\u00XX` form for **every** control char (U+0000..U+001F), giving exactly one canonical representation. The plan pins a single canonical form; mixing short and long escapes would invite "did the spec really pin the short form?" ambiguity.
**Manifests:** `crates/slippypack-core/src/identity.rs::write_json_string`.
**Commit:** to land with the identity module commit.

### I-005 — `Source` enum variant + field declaration order is load-bearing
Variants are declared in alphabetical kind-name order (`Dir < Geotiff < Mbtiles < Pbf < Pmtiles < Style < Synthetic < Url`) so the derived `Ord` impl matches PLAN.md's canonical sort rule. Within each variant, the first field is the per-kind "identity" (`content_hash` for file-backed kinds, `template` for URL, `fixture_version` for synthetic) so derived `Ord` ties-break correctly.
**Manifests:** `crates/slippypack-core/src/identity.rs::Source` and the `sources.sort_by` call inside `write_sources`.
**Commit:** to land with the identity module commit.

### I-006 — Source sort: `(zoom_min, zoom_max)` major key + derived `Source` `Ord` minor key
PLAN.md says sort by `(zoom_min, zoom_max, kind, identity)`. Implemented as a two-stage sort: first by `(zoom_min, zoom_max)` (tuple comparison), then by `Source`'s derived `Ord` (which compares variant index then field-by-field). This collapses `(kind, identity)` into one step because the variant-declaration order is alphabetical and the first field is the identity.
**Manifests:** `crates/slippypack-core/src/identity.rs::write_sources` `sort_by`.
**Commit:** to land with the identity module commit.

### I-007 — `auth_kinds` sorted and deduplicated by the serializer
Callers can pass `Vec<AuthKind>` in any order (and even with duplicates). The serializer defensively sorts and dedups before emitting bytes, so caller mistakes don't break determinism.
**Manifests:** `crates/slippypack-core/src/identity.rs::write_auth_kinds`.
**Commit:** to land with the identity module commit.

### I-008 — `affn` top-level key deferred (Phase 10)
PLAN.md mentions an `affn` top-level key for Local-Linear / hand-drawn `image` packs. Not implemented yet — image sources are Phase 10. The `Source::Image` variant and the `affn` key will land together with Phase 10's runtime support.
**Manifests:** absence of `affn` handling in `canonical_descriptor_bytes`; no `Source::Image` variant.
**Open until:** Phase 10 lands.

### I-009 — Freshness accumulator (`build_timestamp`) lives per-front-end, not in `slippypack-core`
PLAN.md § The load-bearing observation lists "source-mtime / Last-Modified accumulator for build_timestamp" as an identity-module deliverable. On reflection, that accumulator is I/O-shaped (reads file mtimes on the CLI, parses HTTP headers on the PWA) and doesn't belong in `slippypack-core`'s `no_std + alloc` surface. The accumulator lives in the per-front-end glue; the core just accepts a `build_timestamp: u64` field on `PackMetadata` (when the format module lands) and stamps it into the header verbatim.
**Manifests:** absence of accumulator code in `identity.rs`; the `PackDescriptor` does not include `build_timestamp` (per PLAN.md § Canonical source descriptor — `build_timestamp` is in `PackMetadata`, not in the canonical descriptor, because it varies independently of the inputs that produce the same `pack_uuid`).
**Commit:** to land with the identity module commit.

---

## C — CLI (slippypack-cli)

### C-001 — Synthetic fixture: 16 distinct PNG tiles, 16×16 each
Per PLAN.md "gradient pattern, single zoom, 4×4 tiles." Each tile is a 16×16 solid-color PNG with a distinct hue (16 HSL hue steps around the colour wheel). Total fixture size ~1.4 KB. Each PNG is committed at `crates/slippypack-cli/fixtures/synthetic-pattern/tile-{x}-{y}.png` and `include_bytes!`-embedded into the CLI binary at compile time, so `cargo install`-ed binaries work without the source repo present.
**Manifests:** `crates/slippypack-cli/src/sources/synthetic.rs::tile_png_bytes`.
**Commit:** to land with the Phase 1 first-slice commit.

### C-002 — Synthetic packs use `tile_dim_px = 16`, not the spec-mandated 128
The synthetic source is for **pipeline validation**, not watch-loadability. The watch's TilePack reader will refuse a pack with `tile_dim_px != 128` per the una-sdk spec. Using 16 keeps the committed fixture small (each PNG ~85 bytes, the produced pack ~5 KB) without losing coverage of the decode → quantise → format pipeline. Phase 1.x or a follow-up can swap to 128×128 fixtures if/when watch-loadable synthetic packs become useful.
**Manifests:** `crates/slippypack-cli/src/sources/synthetic.rs::TILE_DIM_PX`.
**Commit:** to land with the Phase 1 first-slice commit.

### C-003 — Synthetic `pack_uuid` is a fixed deterministic 16-byte stand-in
~~"synthetic_pack!\0" — visibly test-like, never zero. Phase 1.x will swap this for the proper UUIDv5-from-canonical-descriptor derivation (per identity.rs). Pinned now so the golden-synthetic.rawtiles test stays stable across the v1 lifetime; the bytes match an ASCII string so anyone inspecting the pack header recognises it as a fixture, not a real pack.~~
**Superseded by C-009** — pack_uuid for synthetic builds is now UUIDv5-derived from the canonical source descriptor, matching the production code path.
**Commit:** to land with the Phase 1 first-slice commit.

### C-004 — Atomic write via `<out>.rawtiles.partial` → rename, with RAII cleanup
Per PLAN.md § CLI cancellation and atomic write. The CLI writes to `<out>.rawtiles.partial` first, then atomically renames on success. A `PartialFile` RAII struct deletes the partial file on drop if `commit()` wasn't called — so panics, error returns, and (Phase 1.x) SIGINT handlers all leave a clean filesystem.
**Manifests:** `crates/slippypack-cli/src/build.rs::PartialFile`.
**Commit:** to land with the Phase 1 first-slice commit.

### C-005 — In-memory pack assembly (no streaming) for first slice
The CLI fully buffers the synthetic pack's tile bytes in RAM before writing to disk. Phase 8 (PWA OPFS streaming) introduces the streaming path that flushes tile bytes through an external `TileByteSource`. For the synthetic source (16 tiles × 256 bytes = 4 KB), in-memory is trivial. For URL templates (Phase 1.x), in-memory works up to ~100 MB packs; country-scale packs need Phase 8's streaming.
**Manifests:** `crates/slippypack-cli/src/build.rs::build_synthetic` (uses `TileContent::Inline` exclusively, no `register_byte_source`).
**Commit:** to land with the Phase 1 first-slice commit.

### C-006 — `IoWriteAdapter` bridges `std::io::Write` to `slippypack_core::format::Write`
The format module defines its own local `Write` trait (W-009 / F-011) to keep slippypack-core no_std-ready. CLI-side, `std::fs::File` implements `std::io::Write` but not the format crate's `Write`. A thin wrapper `IoWriteAdapter` translates calls. Same shape will be used in slippypack-cli's URL-template path (Phase 1.x).
**Manifests:** `crates/slippypack-cli/src/build.rs::IoWriteAdapter`.
**Commit:** to land with the Phase 1 first-slice commit.

### C-007 — `--pack-uuid` and `--timestamp` overrides for CI reproducibility
Both flags exist per the CLI synopsis pinned in PLAN.md. `--pack-uuid` accepts either hyphenated UUID form (`4e72f962-6632-4538-8e0a-7eab63350f3f`) or unhyphenated (`4e72f9626632...`); case-insensitive hex. The CLI rejects all-zero (spec-invariant: `pack_uuid` must be non-zero). `--timestamp` takes a u64 seconds-since-Unix-epoch value with no further validation (slippypack accepts 0 as the "no freshness info" sentinel per Q-001 / PLAN.md § Numeric input precision).
**Manifests:** `crates/slippypack-cli/src/main.rs::parse_pack_uuid`; `crates/slippypack-cli/src/build.rs::BuildOptions`.
**Commit:** to land with the Phase 1 first-slice commit.

### C-008 — URL-template source routes on `http://` / `https://` prefix
The CLI dispatcher in `build()` matches `--source` against a small set of literal prefixes: `synthetic` for the embedded fixture, `http://` / `https://` for URL templates. Future source kinds get their own prefix (`mbtiles://`, `pmtiles://`, `dir://` per PLAN.md § Source-kind details). Prefix-matching is simpler than registering a parser per kind and matches the user-facing CLI shape: `--source 'https://.../{z}/{x}/{y}.png'` is the documented form.
**Manifests:** `crates/slippypack-cli/src/build.rs::build`.
**Commit:** to land with the URL-template slice.

### C-009 — `pack_uuid` derived from canonical descriptor for all source kinds
Both `synthetic` and URL-template builds construct a `PackDescriptor` and pass it through `slippypack_core::identity::derive_pack_uuid` (UUIDv5 against `RAWTILES_NAMESPACE`). Production code path is identical for every source kind — `--pack-uuid` override remains for CI determinism. Supersedes C-003 (which used a fixed `synthetic_pack!\0` stand-in).
**Manifests:** `crates/slippypack-cli/src/build.rs::build_metadata`, `synthetic_descriptor`, `url_template_descriptor`.
**Commit:** to land with the URL-template slice.

### C-010 — Synthetic source's canonical descriptor uses `fixture_version` integer, not file hashes
The synthetic source is built from PNG bytes embedded in the binary at compile time via `include_bytes!`. Hashing the fixtures at runtime to populate the descriptor would be wasted work (the fixtures don't change without a code change). Instead, the descriptor pins a `SYNTHETIC_FIXTURE_VERSION: u32 = 1` constant — bump it whenever a fixture is regenerated. Matches `Source::Synthetic { fixture_version }` per identity.rs § I-005.
**Manifests:** `crates/slippypack-cli/src/build.rs::SYNTHETIC_FIXTURE_VERSION`, `synthetic_descriptor`.
**Commit:** to land with the URL-template slice.

### C-011 — `--bbox` is decimal degrees; converted to integer microdegrees via banker's rounding
The CLI accepts `--bbox minLon,minLat,maxLon,maxLat` in decimal degrees (f64). Conversion to the on-disk integer microdegree form (i32, lat/lon × 10⁶) uses `f64::round_ties_even` so two inputs that differ by less than 10⁻⁶ degrees collapse to the same descriptor (and same `pack_uuid`). PLAN.md § Numeric input precision pins this rule.
**Manifests:** `crates/slippypack-cli/src/build.rs::deg_to_micro`, `BboxDeg::to_micro`.
**Commit:** to land with the URL-template slice.

### C-012 — `--zoom` accepts single-zoom or `min-max` inclusive range
`--zoom 8` builds just zoom 8; `--zoom 6-12` builds zooms 6 through 12 inclusive. Max zoom is capped at 17 (the spec's `ZOOM_OFFSETS_COUNT - 1`, since the header reserves 18 per-zoom directory entries for zooms 0..=17). Inverted ranges and non-numeric input are rejected at parse time.
**Manifests:** `crates/slippypack-cli/src/main.rs::parse_zoom`.
**Commit:** to land with the URL-template slice.

### C-013 — URL-template build: pre-fetch all tiles, then assemble pack
The URL-template path buffers every tile's bytes in memory before opening the writer. Three reasons: (1) lets `fetcher.max_last_modified()` accumulate across all responses before becoming the pack's `build_timestamp`; (2) surfaces fetch errors before any pack bytes are written (the `.partial` file is never created if any tile fails); (3) matches C-005's in-memory assembly model for the first-slice CLI. Phase 8's streaming path will refactor for large packs.
**Manifests:** `crates/slippypack-cli/src/build.rs::build_url_template`.
**Commit:** to land with the URL-template slice.

### C-014 — `UrlFetcher` holds a reusable `ureq::Agent` and accumulates `Last-Modified`
A single `ureq::Agent` instance is reused for every fetch in a build so TCP and TLS sessions stay pooled — typical slippy-map tile sources serve hundreds of tiles from one origin, and a fresh agent per request is wasteful. The fetcher tracks the maximum of every successful response's `Last-Modified` header (parsed via RFC 7231 IMF-fixdate) and that maximum becomes the pack's `build_timestamp`. Per PLAN.md § Pack identity: the timestamp records source-data freshness, not build wall-clock.
**Manifests:** `crates/slippypack-cli/src/sources/url_template.rs::UrlFetcher`.
**Commit:** to land with the URL-template slice.

### C-015 — `parse_http_date` only supports IMF-fixdate; RFC 850 / asctime are not handled
RFC 7231 defines three legal HTTP-date forms but mandates IMF-fixdate for new outputs. Every realistic tile-server response (OSM, MapTiler, Mapbox, Stadia) uses IMF-fixdate. Supporting the obsolete forms costs parser complexity for zero observed benefit; if a tile server emits a non-IMF-fixdate header, `parse_http_date` returns `None` and that response's freshness is skipped (worst-case: `build_timestamp` ends up smaller than reality).
**Manifests:** `crates/slippypack-cli/src/sources/url_template.rs::parse_http_date`.
**Commit:** to land with the URL-template slice.

### C-016 — URL-template `expected_dim` hardcoded to 256 for the first slice
Phase 1 first slice assumes the URL-template source serves 256×256 tiles — the dominant slippy-map size. The decode pipeline rejects tiles that don't match `expected_dim` with `BuildError::UnexpectedTileDimensions`. Phase 1.x will sample the first successfully decoded tile's dimensions and use those, allowing 128×128 sources (and the spec-pinned watch tile size).
**Manifests:** `crates/slippypack-cli/src/build.rs::build_url_template` (the `let expected_dim = 256;` line).
**Commit:** to land with the URL-template slice.

### C-017 — SIGINT cancellation via `Arc<AtomicBool>` polled between tile operations
The `ctrlc` crate's handler flips an `AtomicBool` that's plumbed through `BuildOptions::cancel`. The build loops poll the token between tile fetches and between tile decode-quantise-write iterations. On cancellation the loop returns `BuildError::Cancelled`, which propagates up through `run_build`; the `PartialFile` RAII guard's drop removes the `.partial` file because `commit()` was never called. The CLI surfaces exit code 130 (the conventional Ctrl-C exit). Polling between tile boundaries means the worst-case responsiveness is one tile's worth of work — fine for raster sources, may need finer granularity if Phase 2's vector renderer takes seconds per tile.
**Manifests:** `crates/slippypack-cli/src/build.rs::check_cancel`, `BuildOptions::cancel`; `crates/slippypack-cli/src/main.rs::install_cancel_handler`.
**Commit:** to land with the SIGINT slice.

### C-018 — `SLIPPYPACK_DEBUG_SLEEP_MS` env var to make synthetic builds testable under SIGINT
The synthetic source runs in milliseconds — too fast for a race-free SIGINT integration test. The build loop honors `SLIPPYPACK_DEBUG_SLEEP_MS` as a per-tile sleep, set only by `tests/cli_cancel.rs`. Production runs leave it unset (defaults to 0). An env var avoids cluttering the CLI surface with a hidden flag; the test fully owns the env var name.
**Manifests:** `crates/slippypack-cli/src/build.rs::debug_sleep_per_tile_ms`.
**Commit:** to land with the SIGINT slice.

### C-019 — `nix` (not `libc`) for the SIGINT integration test
The workspace forbids `unsafe_code` (W-005), so `libc::kill` (an unsafe extern) is not callable from test code. `nix` provides a safe `kill` wrapper. It's a Unix-only dev-dep on tests, not a production dep — Production code uses `ctrlc` to receive signals, and never sends them.
**Manifests:** `crates/slippypack-cli/Cargo.toml` (`[target.'cfg(unix)'.dev-dependencies]`); `crates/slippypack-cli/tests/cli_cancel.rs`.
**Commit:** to land with the SIGINT slice.

### C-020 — `slippypack debug uuid` shares descriptor construction with `make`
The descriptor-building helper `descriptor_for(&BuildOptions)` is the single source of truth for both `make`'s `pack_uuid` and `debug uuid`'s emitted UUIDv5 / canonical bytes. The integration test `debug_uuid_uuid_matches_make_output_for_synthetic` is the standing contract: two CLI invocations against the same source/bbox/zoom always produce the same UUID. Reusing the helper avoids two derivations drifting silently.
**Manifests:** `crates/slippypack-cli/src/build.rs::descriptor_for`; `crates/slippypack-cli/src/debug.rs::run_debug_uuid`; `crates/slippypack-cli/tests/cli_debug_uuid.rs::debug_uuid_uuid_matches_make_output_for_synthetic`.
**Commit:** to land with the `debug uuid` slice.

### C-021 — `debug uuid --bytes` emits canonical descriptor bytes without trailing newline
The `--bytes` mode is meant to be piped into other tools (`sha1sum`, `xxd`, `jq`, third-party UUIDv5 implementations) for independent verification. A trailing newline would break the hash chain (any byte appended changes the SHA-1 input), so `--bytes` writes exactly the canonical descriptor bytes — same shape as `identity::canonical_descriptor_bytes`. The default (UUID) mode adds a trailing newline since it's meant for human consumption in a terminal.
**Manifests:** `crates/slippypack-cli/src/debug.rs::run_debug_uuid` (the `DebugUuidFormat::Bytes` branch).
**Commit:** to land with the `debug uuid` slice.

### C-022 — `--bbox` accepts hyphen-leading values via `allow_hyphen_values`
Without this clap attribute, `slippypack make --bbox -0.15,51.49,-0.10,51.52` fails parsing because `-0.15` looks like a short flag. Setting `allow_hyphen_values = true` on the `--bbox` argument lets clap treat the leading `-` as part of the value. Real bboxes — especially anywhere in the Western Hemisphere — routinely start with a negative longitude, so this is essential ergonomics, not a corner case.
**Manifests:** `crates/slippypack-cli/src/main.rs` (`MakeArgs::bbox` and `DebugUuidCliArgs::bbox`).
**Commit:** to land with the `debug uuid` slice.

### C-023 — `--auth-header` / `--auth-query` only the *kind* enters the descriptor
PLAN.md § Source-kind details and identity.rs § "Auth values are deliberately NOT in the descriptor" pin the rule: the canonical descriptor records `auth_kinds: ["header"]` or `["query"]` (or both) when those flags are used, but never the names, keys, or values. The reason: API keys would leak into `pack_uuid`'s SHA-1 input, making the UUID an oracle for the key. Two builds with the same source/bbox/zoom but different API keys produce the same `pack_uuid`. Determinism for users sharing build configs but not credentials.
**Manifests:** `crates/slippypack-cli/src/build.rs::auth_kinds_from_options`, `url_template_descriptor`; `crates/slippypack-cli/src/debug.rs::tests::url_template_uuid_does_not_depend_on_auth_value`.
**Commit:** to land with the auth-flags slice.

### C-024 — Both flags treat the FIRST separator as the split point
`--auth-header "X-Custom: a:b:c"` parses name=`X-Custom`, value=`a:b:c` (only first `:` separates). `--auth-query "token=abc=="` parses key=`token`, value=`abc==` (only first `=`). Bearer tokens contain `:` in JWT-like payloads; base64-encoded values use `=` padding. Splitting on the first separator preserves both.
**Manifests:** `crates/slippypack-cli/src/sources/url_template.rs::AuthHeader::parse`, `AuthQuery::parse` (uses `str::split_once`).
**Commit:** to land with the auth-flags slice.

### C-025 — `append_auth_query` checks the URL for an existing `?` to pick `?` vs `&`
`UrlTemplate` substitution can produce URLs that already have a query string (e.g. tile servers that bake style hints into the template). `append_auth_query` checks for an existing `?` in the substituted URL and uses `&` as the leading separator if found, `?` otherwise. Subsequent `--auth-query` entries always join with `&`.
**Manifests:** `crates/slippypack-cli/src/sources/url_template.rs::append_auth_query`.
**Commit:** to land with the auth-flags slice.

---

## N — Naming

### F-035 — Spec § 10 cites CRC-32/ISO-HDLC by standard name + language references
Pre-1.0-freeze refinement. § 10 was hand-spelling the CRC-32 polynomial, init, reflection, and XOR-out — which is exactly the canonical specification of CRC-32/ISO-HDLC (a.k.a. the PNG / zlib CRC). Trimmed to a name-citation: ITU-T V.42 § 8.1.1.6.1 + RFC 2083 § 15, with the check value (`0xCBF43926` for `"123456789"`) kept as a quick gate. Added a one-liner pointing implementers at the variant in their standard library (`zlib.crc32`, `hash/crc32.IEEETable`, `java.util.zip.CRC32`, `crc32fast`, etc.) so nobody re-implements the polynomial unnecessarily.

Same "external standard, no need to restate" pattern as F-034 (JCS for canonicalization). Same wins: smaller conformance surface, third-party readers/writers get the algorithm from their language's standard library.

**Manifests**: `spec/rawtiles-v1.0-rc1.md` § 10 (rewritten, ~5 lines shorter).
**Commit**: to land with the CRC-cite slice.

### F-034 — Canonicalisation aligned to RFC 8785 (JCS)
Pre-1.0-freeze fix. Spec § A.3 was hand-rolling a partial restatement of RFC 8785's JSON Canonicalization Scheme: no whitespace, lex key order, no trailing newline, integer formatting rules, string escape rules, control-char `\uXXXX`. JCS already specifies all of that. Third-party writers should be able to use any off-the-shelf JCS library (Rust, Go, Python, JS, …) instead of re-implementing.

**One real divergence found** between our previous hand-rolled rule and JCS: control characters. We were emitting all U+0000–U+001F as `\u00XX`. JCS uses the ECMAScript `JSON.stringify` shortcuts for five of them — `\b \t \n \f \r` for U+0008 / 0009 / 000A / 000C / 000D — and `\u00XX` for the rest. Slippypack's canonicalizer is now aligned: shortcuts for those five, `\u00XX` otherwise.

**No `pack_uuid` drift** on any committed fixture: the baseline, synthetic, and all five spec_layout / e2e fixtures use ASCII-printable strings (no control chars in URLs, names, attribution text, etc.). Verified by running a Python `jcs` library against the synthetic descriptor's canonical bytes and confirming byte-equality with slippypack's output.

The `url_template_with_control_char_uses_u_escape` test was split into two: `url_template_with_shortcut_control_char_uses_shortcut` (verifies `\t` for tab) and `url_template_with_non_shortcut_control_char_uses_u_escape` (verifies `` for SOH). Both paths covered.

Spec § A.3 rewritten to reference RFC 8785 directly, with only two slippypack-specific rules layered on top (both content-shape, not JSON canonicalisation): SHA-256 content hashes as lowercase hex; microdegree integer rounding via banker's. The JCS reference paragraph was kept as informational for readers who want to verify slippypack output without pulling in a JCS library.

**Wins**:
- Third-party writers in any language get canonicalization for free from an existing JCS lib.
- Trims ~10 lines of spec prose that was duplicating RFC 8785.
- The "did I implement § A.3 correctly?" class of bugs reduces to "did the JCS library work" — a much smaller and externally-validated surface.

**Manifests**: `crates/slippypack-core/src/identity.rs::write_json_string` (added five shortcut branches); test renames + addition; `spec/rawtiles-v1.0-rc1.md` § A.3 rewritten.
**Commit**: to land with the JCS-alignment slice.

### F-033 — Eight more pre-1.0 spec refinements
Pure-doc batch closing eight more rough spots flagged by review. One small Rust + C++ change for the `index_offset` symmetry.

1. **§ 4 `reserved_v1_0` row** — clearer subject-first phrasing: "v1.0 writers MUST set this to `0x00 0x00`; readers MUST accept any value (forward-compat hole for v1.x minor bumps)."

2. **§ 4.11 reader-vs-writer symmetry** — was asymmetric: writers MUST emit `index_offset = 292`, readers accept anything `≥ 292`. Bytes between 292 and the actual `index_offset` were semantically undefined. Tightened to MUST equal 292 on both sides — no inter-region gap, no undefined bytes. Future minor versions that need extra inter-region space can add explicit fields. **Implementation tracked**: Rust reader gained `ReaderError::IndexOffsetNotAtHeaderEnd`; C++ validator's check is now `==` not `>=`.

3. **§ 14.3 implementation-specific bleed** — the env-var names `BLESS_SPEC_LAYOUT=1` / `BLESS_E2E=1` / `BLESS_CLI_SYNTHETIC=1` are slippypack test infrastructure, not normative spec. Rephrased generically: "the implementation's documented re-blessing procedure". Mechanics move out of scope for the spec.

4. **§ A.3 banker's rounding precision** — replaced "less than 10⁻⁶ degrees produce identical descriptors" with the precise equivalence: "two inputs produce equivalent descriptors iff they round to the same integer microdegrees under banker's rounding." Added worked examples at the ties (`0.0000005°` → `0 µ°`, `0.0000015°` → `2 µ°`) since language defaults diverge on round-half (Python 3: banker's; C `lround`: away-from-zero; many JS paths: up).

5. **§ A.4 sources without zoom fields** — `synthetic` and `image` kinds lack zoom_min/zoom_max but the sort key is `(zoom_min, zoom_max, derived_source_order)`. Pinned: treat such sources as `zoom_min = 0, zoom_max = 0` for sort purposes.

6. **§ A.5 intermediate SHA-1 hex** — added the 20-byte SHA-1 of `namespace_bytes ‖ canonical_bytes` (`5146db8e0859661c858045c6154e890d752c55ca`) so independent implementations can bisect a UUID mismatch. If their SHA-1 differs, canonicalisation is the bug; if SHA-1 matches but UUID doesn't, the v5 version/variant fixup is the bug.

7. **§ 7.4 NAME selection** — was MUST-mandate RFC 4647 lookup (a non-trivial parser). Relaxed to SHOULD; minimal readers MAY use byte-equal `bcp47_tag` comparison against the device locale with fallback to the unlocalized section. Embedded readers with kilobyte budgets don't have to ship an RFC 4647 implementation.

8. **§ 14.5 CRC-32 cross-reference** — was duplicating § 10's check value `0xCBF43926` for `"123456789"`. Replaced with a cross-reference + note that § 14.5 exists to flag the check as a conformance requirement without inviting drift between the two sections.

**Manifests**: `spec/rawtiles-v1.0-rc1.md` §§ 4 (row), 4.11, 7.4, 11 #20, 14.3, 14.5, A.3, A.4, A.5; `crates/slippypack-core/src/format/reader.rs::{ReaderError, open}`; `spec-validator-cpp/src/validator.cpp` (`==` vs `>=`).
**Commit**: to land with the eight-fix batch.

### F-032 — Spec internal consistency: § 8 offsets, § 11 #19 alignment, § 12 numbering
Three pure-doc cleanups triggered by review. None change wire format.

(a) **§§ 8.1–8.4 and § 8.6 parentheticals**: F-031 shifted every header field past offset 6 by +2 (inserting `reserved_v1_0`), but the parenthetical byte numbers in § 8's enum subsection headers and § 8.6's pairing rule didn't track. § 4's authoritative table had the right numbers; § 8 still cited the old ones. A fresh implementer using § 8's offsets would read `parent_uuid`'s last bytes as enums (rejecting every valid pack) or read the right bytes and assume the spec is internally inconsistent. Updated:
- § 8.1 "byte 54" → "byte 56"
- § 8.2 "byte 55" → "byte 57"
- § 8.3 "byte 56" → "byte 58"
- § 8.4 "byte 57" → "byte 59"
- § 8.6 "offsets 55 and 56" → "offsets 57 and 58"

(b) **§ 11 #19 contradicted § 3's new alignment claim**: F-031's § 3 says "every multi-byte header field is naturally aligned … strict-alignment platforms can do pointer-cast loads after a single `memcpy`-of-header". § 11 #19 still said "no multi-byte field is guaranteed to be naturally aligned in the file. Native pointer-cast reads fault." Both can't be true. Updated #19 to acknowledge the alignment guarantee: file-offset alignment is real, memory-address alignment depends on where the file's bytes live in memory. Pointer-casts are safe iff the load address is sufficiently aligned; mmap of an arbitrary file position requires memcpy-then-decode.

(c) **§ 12 SHOULD-list numbering off by one**: when F-030 inserted "Emit extension sections in a deterministic order" as MUST #10, MUST went to #13 but SHOULD still started at #13 (overlap), and MUST NOT started at #16 (skipping #14, #15). Renumbered SHOULD to #14–15; MUST NOT was already correct at #16–17.

**Manifests**: `spec/rawtiles-v1.0-rc1.md` §§ 8.1, 8.2, 8.3, 8.4, 8.6, 11 #19, 12 #14, 12 #15.
**Commit**: to land with the spec-consistency slice.

### F-031 — Header padded to 292 bytes for full natural alignment
Pre-1.0-freeze structural fix. The u32-offset shrink (F-027) removed the u64-misalignment trap but didn't fully land alignment: the 290-byte header ended at offset 290 (mod 4 = 2), so tile-index entries at `index_offset = 290` had their u32 fields at file offsets `mod 4 = 2`. The pre-header layout was also unaligned — pack_uuid at offset 6 cascaded the whole header off by 2.

Closed by inserting 2 reserved-zero bytes at offset 6 (between `format_version` and `pack_uuid`). Every multi-byte field is now naturally aligned at its file offset:

- u16 `tile_dim_px` at 60 (mod 2 = 0) ✓
- i32 `bbox.*` at 64, 68, 72, 76 (mod 4 = 0) ✓
- u64 `build_timestamp` at 80 (mod 8 = 0!) ✓
- u32 `tile_count` at 88 (mod 4 = 0) ✓
- u32 `index_offset` at 92 (mod 4 = 0) ✓
- u32 `zoom_offsets[i].offset` / `.count` at `96 + i*8 + {0,4}` (all mod 4 = 0; entries are mod 8 = 0) ✓
- u32 `extensions_offset` at 288 (mod 4 = 0) ✓
- 16-byte UUIDs at 8, 24, 40 (mod 8 = 0) ✓

`index_offset = 292` is mod 4 = 0, so the u32 fields *within* every tile-index entry (x at +4, y at +8, offset at +12, length at +16) are also naturally aligned.

Practical impact: strict-alignment readers (some Cortex-M configurations) can do native pointer-cast loads after a single `memcpy`-of-header into an 8-byte-aligned buffer; the byte-oriented memcpy-then-decode path also works (still recommended for cross-platform portability). The format is now both *aligned-load-friendly* and *byte-oriented-portable*.

Reserved-byte semantics (§ 4 row 6→8):
- v1.0 writers MUST emit 0x00 0x00.
- Readers MUST accept any value — this is the forward-compat hole for v1.x minor bumps that want to add 2 bytes of header data without breaking v1.0 readers.

Mechanical changes:
- HEADER_BASE_SIZE: 290 → 292; ZOOM_OFFSETS_START: 94 → 96; all field offsets in write_header/read_header shifted by +2 starting at offset 6.
- spec/rawtiles-v1.0-rc1.md § 3 (file structure diagram + alignment paragraph), § 4 (full table rewrite), § 11 #1 (min file size 296), § 11 #20 (`index_offset ≥ 292`), § 12 #4 (`index_offset = 292`).
- spec-validator-cpp constants + new `kOffReservedV1_0`.
- All 6 golden fixtures re-blessed.

**Manifests**: `format/header.rs::{HEADER_BASE_SIZE, ZOOM_OFFSETS_START, write_header, read_header}`; `spec/rawtiles-v1.0-rc1.md` § 3, § 4, § 11, § 12; `spec-validator-cpp/src/validator.cpp` constants.
**Commit**: to land with the alignment slice.

### F-030 — Spec § 12.1: deterministic extension-section emit order
Pre-1.0-freeze fix. § 14.1 newly asserts "A conforming writer applied twice to the same logical inputs MUST produce byte-identical output" but § 12 had no rule pinning *the order in which extension sections are emitted*. A writer free to emit NAME-then-SRCD one day and SRCD-then-NAME the next would silently violate § 14.1.

Rule (now § 12.1): emit extension sections sorted ascending by 4-byte tag, with payload-byte tie-break for multi-instance tags. Lex order of tags happens to give the canonical reserved-tag order `AFFN, ATTR, NAME, PLET, SRCD` for free. NAME-with-multiple-locales naturally sorts by BCP-47 tag (since the payload's first byte is `tag_length` and tags are ASCII).

slippypack's writer was emitting in input order; updated to sort via `extensions.sort_by(|a, b| (a.tag, &a.payload).cmp(&(b.tag, &b.payload)))` before serialization. New tests `extensions_emit_sorted_regardless_of_add_order` and `multiple_same_tag_sections_sort_by_payload` lock the invariant. One existing reader test had to flip its expected order (NAME → ATTR-first).

**Manifests**: `spec/rawtiles-v1.0-rc1.md` § 12 #10 + new § 12.1; `crates/slippypack-core/src/format/rawtiles_writer.rs::finalize` (sort step before write).
**Commit**: to land with the deterministic-ordering slice.

### F-029b — Appendix A.5 example synced with new affn rule
Worked example in Appendix A.5 had drifted from the F-028 rule that `affn` is always emitted. The example JSON was missing the `"affn":null` lex-first key, and its derived `pack_uuid` (`53077f67-...`) was computed from a JSON that no v1.0-conforming writer should produce.

Updated A.5 to include `"affn":null` and pinned the new derived UUID `5146db8e-0859-561c-8580-45c6154e890d` (the same value locked by `determinism_baseline_pack_uuid_is_committed` in slippypack-core).

**Manifests**: `spec/rawtiles-v1.0-rc1.md` § A.5.
**Commit**: to land with the deterministic-ordering slice.

### F-029 — Pre-1.0 spec corrections (fixes 6–14): bound MUSTs, definitions, conformance
Pure documentation pass — no wire-format change — closing nine spec ambiguities pre-1.0-freeze. Each one is a place where two spec-faithful readers could legally disagree on the same pack:

1. **In-zoom `(x, y)` sort MUST** (§ 5.2, § 11 #10). § 5.3's binary search depends on it; was implicit in "sorted by `(z, x, y)`" but easy to miss. Stated explicitly + tied to the lookup algorithm.
2. **Extension-section bound MUST** (§ 7.1). Each section's full extent (`tag + length + payload + padding`) MUST lie within `[extensions_offset, file_size − 4)`. Was implied; now explicit, with reader-MUST in § 11 #14.
3. **`extensions_offset` 4-aligned MUST** (§ 7.1, § 11 #13). Implied by the writer's pad-to-4 convention; readers MUST verify.
4. **First-section alignment MUST** (§ 7.1). The first section starts exactly at `extensions_offset` — no padding between the offset and the first tag byte.
5. **`tile_blob_start` defined once** (§ 3) — `align4(index_offset + 20 × tile_count)`. § 5.2, § 6, and § 11 #11 now all reference this single definition instead of re-deriving it inconsistently.
6. **ATTR ordering tied to A.4** (§ 7.3 table row). Strings MUST be ordered to match the canonical `sources` array order (sorted by `(zoom_min, zoom_max, kind, identity)`) so two writers given the same logical sources produce byte-identical `ATTR` payloads.
7. **`build_timestamp = 0` sentinel collision acknowledged** (§ 4.10). A real epoch-zero `mtime` is indistinguishable from "no info", but no real-world data has that mtime; writers needing to express "exactly the epoch" SHOULD use `1`.
8. **§ 14.1 round-trip restated** as writer-round-trips-own-output, not reader-then-writer. The latter requires more from a "conforming reader" than § 11 actually demands (e.g. recovering pre-sort tile order from the bytes alone).
9. **§ 14.3 pins golden fixtures by name** in a six-row table, each with path + what-it-exercises. Drift requires either a `quantiser_version` / `format_version` bump or an explicit re-bless via the documented env-var bootstrap.

Also fixed: § 11 #1 (file-size minimum 398 → 294 after the u32-offset shrink); added § 11 #8 (enum-pair legality cross-reference to § 8.6, complementing F-025).

**Manifests**: `spec/rawtiles-v1.0-rc1.md` §§ 3, 4.10, 5.2, 7.1, 7.3, 11, 14.1, 14.3.
**Commit**: to land with the spec-batch slice.

### F-028 — AFFN canonical bits committed as six hex-encoded `f64` bit-patterns
Pre-1.0-freeze fix for a LocalLinear `pack_uuid` collision. The previous spec text said the descriptor's `affn` key carried the six affine coefficients "as integer microunits" — but the on-disk `AFFN` extension stores six `f64` decimal-degree coefficients, and the conversion was undefined. Two writers given identical `f64`s could compute different "integer microunit" approximations (depending on rounding mode, language runtime, or float-to-decimal pathway), producing different `pack_uuid`s for byte-identical packs.

Fixed by committing the `f64` bit-patterns directly:

- `PackDescriptor::affn` is now `Option<[u64; 6]>` — the IEEE-754 bit-patterns of the on-disk `AFFN` values.
- Canonical JSON emits `"affn":[ ... ]` as a six-element array of 16-char lowercase hex u64 strings (lex-first, before `"bbox"`).
- `"affn":null` is emitted for non-`LocalLinear` packs (always present in the descriptor, uniform shape).

Byte-identical `AFFN` extension bytes ⇒ byte-identical `affn` key bytes ⇒ byte-identical canonical descriptor ⇒ byte-identical `pack_uuid`. No rounding question, no float-to-decimal precedent.

The canonical-bytes shape changes (new lex-first key), so the baseline `pack_uuid` rotated: `53077f67-522e-5cb0-b2b5-ffddba17d0db` → `5146db8e-0859-561c-8580-45c6154e890d`. CLI synthetic golden fixture re-blessed.

**Manifests**: `crates/slippypack-core/src/identity.rs::{PackDescriptor::affn, write_affn_bits, canonical_descriptor_bytes}`; spec `spec/rawtiles-v1.0-rc1.md` § A.3 (key table + worked example).
**Commit**: to land with the AFFN-bits slice.

### F-027 — Drop u64 offsets to u32 (header 290 bytes, tile-index entry 20 bytes)
Pre-1.0-freeze structural fix triggered by review of the misaligned-u64 trap. The previous layout had u64 `index_offset`, `extensions_offset`, `zoom_offsets[].offset`, and per-tile `offset` — none of which were 8-byte aligned in the file, requiring readers to `memcpy`-then-decode and creating a misalignment trap on strict-alignment platforms (Cortex-M0). The PMTiles model of u64 offsets is for HTTP-range mega-archives; that isn't the rawtiles use case.

Dropping all four to u32:
- **Pack size capped at 4 GiB** — plenty for any device-sideload use case. PMTiles-style multi-GB archives aren't the target.
- **Header shrinks 394 → 290 bytes** (−104 bytes).
- **Tile-index entry shrinks 24 → 20 bytes** (−4 bytes per tile). For an 80k-tile pack that's 320 KB shaved.
- **Eliminates the alignment question entirely**: all multi-byte fields in the on-disk layout are now u32 or u16, which u32-aligned readers handle natively. Even those that aren't naturally aligned have stricter alignment requirements only one level deep (memcpy-to-local-then-decode covers it).
- **build_timestamp stays u64** (Unix epoch seconds; u32 maxes at 2106). One u64 field remains at an unaligned offset (78); a single memcpy on pack open is fine.

Mechanical changes:
- `tile_index.rs::INDEX_ENTRY_SIZE`: 24 → 20; `TileIndexEntry::offset`: u64 → u32; write/read functions updated.
- `header.rs::HEADER_BASE_SIZE`: 394 → 290; `ZoomOffset::SIZE`: 12 → 8; `ZoomOffset::offset`: u64 → u32; `DerivedHeaderFields::{index_offset, extensions_offset}`: u64 → u32; field offsets in the layout table updated.
- `rawtiles_writer.rs`: layout planning stays in u64 to avoid intermediate overflow; converts to u32 at the boundary with checked `try_from` (returns `PackTooLarge` if any offset exceeds 4 GiB).
- `reader.rs`: parses u32 offsets; promotes to u64 only for bounds arithmetic.
- `spec-validator-cpp`: matching constant + decoder changes.
- spec/rawtiles-v1.0-rc1.md § 3, § 4, § 5: layout tables updated; "4 GiB cap" noted in § 3.
- All 6 golden fixtures re-blessed (header shape changed → CRC changed → bytes changed).

**Manifests**: `format/header.rs`, `format/tile_index.rs`, `format/rawtiles_writer.rs`, `format/reader.rs`; `spec-validator-cpp/src/validator.cpp`; `spec/rawtiles-v1.0-rc1.md` §§ 3, 4, 5, 11, 12; all 6 `golden-*.rawtiles` fixtures.
**Commit**: to land with the u32-offset slice.

### F-048 — Repoint repo-internal spec references to standalone rawtiles repo
Follow-up to commit `1c9c062` (spec extracted to its own repo). The spec doc moved to <https://github.com/tobymurray/rawtiles>, but ten in-tree files still pointed at `spec/rawtiles-v1.0-rc1.md` (dead path) or framed the spec as una-sdk-owned. Updated user-facing docs (`README.md`, `PLAN.md`, `spec-validator-cpp/README.md`) to link the GitHub URL directly, and rewrote Rust doc-comment spec citations (`crates/slippypack-core/src/format/{mod,types,tile_index,crc,extensions}.rs`, `crates/slippypack-core/src/identity.rs`, `crates/slippypack-cli/src/sources/synthetic.rs`) to cite "the rawtiles spec" with the URL or the relevant Appendix-A reference. Historical DECISIONS.md entries deliberately left untouched — they record the path that existed at the time of each decision.

Two scoping calls worth pinning:

- **una-sdk references about *reader* implementations were preserved** (watch firmware `TilePack`, MapTrack simulator round-trip, test 7 in PLAN.md). The spec move makes una-sdk a downstream reader, not the spec home; references that frame it as a reader implementation are still accurate.
- **The `identity.rs` module doc was rewritten** to cite the rawtiles spec Appendix A as the source of canonicalization rules. The corresponding "Canonical source descriptor" section in PLAN.md was already cut down to slippypack-specific notes (numeric input precision, duplicate-source rejection) in the staged edits that landed alongside this change; the spec-side rules now live exclusively in the rawtiles repo.

**Manifests**: `README.md`, `PLAN.md`, `spec-validator-cpp/README.md`, `crates/slippypack-core/src/format/{mod,types,tile_index,crc,extensions}.rs`, `crates/slippypack-core/src/identity.rs`, `crates/slippypack-cli/src/sources/synthetic.rs`.

### F-047 — Cross-writer determinism: 13 derivation/format gaps
Pre-1.0-freeze closure of 13 places where the spec let two "conforming" writers produce byte-different packs from the same logical inputs — the same load-bearing concern as F-040 (preprocessing pipeline) and F-042 (`build_timestamp`). Each gap broke the offline-delivery dedup contract that recipients depend on.

**Pinned canonical derivations (§§ 4.8, 4.9, 4.10, 8.4):**

- **§ 4.9 `bbox`** — was previously a writer parameter with no derivation rule. Now: for Quadtree with `tile_count > 0`, `bbox` is the tight i32-µdeg bounding box of the lon/lat patches covered by all tile-index entries, clipped to projection limits; for Quadtree with `tile_count == 0`, `bbox` is `(0, 0, 0, 0)` (origin sentinel); for SingleImage with LocalLinear, `bbox` is the bounding box of the four image-corner points transformed by `AFFN`.
- **§ 4.8 `zoom_min` / `zoom_max`** — was implicit. Now: the actual min/max `z` byte values present in the tile-index (so a writer that internally targets "5–15" but only finds source tiles at "6–12" emits `(6, 12)`); `(0, 0)` for `tile_count == 0`.
- **§ 4.10 `build_timestamp`** — was "most recent source mtime or HTTP Last-Modified", gestural. Now: max over sources of each source's freshness timestamp; absent freshness contributes `0` and does NOT count in the max; `0` overall if no source carries freshness.
- **§ 8.4 `tile_axis_convention` for SingleImage** — was "readers MUST accept any of 1 or 2" with no writer constraint. Now: writers MUST emit `1` (`XYZ`) for SingleImage. The byte appears in the canonical descriptor (§ A.3), so SingleImage writers emitting different values would yield different `pack_uuid`s for the same logical inputs.
- **§ 7.3 AFFN-derived bbox** — added the explicit derivation rule referenced from § 4.9: the four corners `(0, 0)`, `(W, 0)`, `(0, H)`, `(W, H)` transform through `AFFN`, the bounding box of those four lon/lat points becomes the pack `bbox`, rounded to nearest microdegree per § A.3's banker's rounding.

**Declared writer parameters (not derived):**

- **§ 4.7 `tile_dim_px`** — explicitly a writer parameter, not derived from inputs. Different `tile_dim_px` correctly yields different `pack_uuid` (it's in the descriptor); cross-writer dedup requires writers to agree on `tile_dim_px` out-of-band (a consumer-profile constant).
- **§ 7.4 NAME locale set** — explicitly writer parameter from caller input. Different locale sets between two writers yield byte-different extension content with the same `pack_uuid` (NAME isn't in the canonical descriptor) — which means metadata can legitimately vary between packs sharing UUID. Cross-writer dedup compatibility requires writers to agree on the locale set out-of-band.

**Pinned payload formats:**

- **§ 7.3 ATTR newline** — pinned to LF (`0x0A`) only. CRLF, bare CR, and embedded `\r` anywhere in the payload are forbidden. Writers MUST reject; readers MAY reject or strip.
- **§ 7.4 BCP-47 subset** — restricted to `lang[-REGION]`: two lowercase ASCII letters for language, optional hyphen + two uppercase ASCII letters for region. Closes the RFC 5646 parser-divergence trap (full BCP-47 ABNF is non-trivial; library implementations diverge). Case is normative: `en-us` is non-conforming. A future minor MAY relax the subset.
- **§ 7.3 SRCD status** — explicitly optional and advisory. Cross-writer-reproducible writers MUST either omit SRCD entirely OR document their canonical SRCD-derivation function and produce byte-identical SRCD across runs. v1 does NOT define a canonical SRCD-derivation function.

**Conflict resolution (§ 12 #6):**

- **Duplicate `(z, x, y)` across sources** — was "Reject duplicate inputs at write time," ambiguous for multi-source writers. Now: canonical policy is **later-source-wins** per § A.4's canonical sources ordering. Two sources supplying the same `(z, x, y)` yield the later source's tile; earlier is silently dropped. After this resolution pass, no two tile-index entries share `(z, x, y)`. Writers that apply a different policy (first-wins, alpha-blend, strict-reject) MUST NOT claim cross-writer reproducibility. Single-source writers' behavior unchanged (the policy is a no-op).

**Clarification (§ 12.1 NAME sort):**

- The previous claim "naturally orders the NAME sections by locale tag (alphabetical order)" was misleading for mixed-length tag lists. The sort is byte-order over the raw payload, dominated by the leading `tag_length` byte. New text + worked example: `tag_length=0` first, then 2-letter tags (alphabetical within length 2), then 5-letter `lang-REGION` tags (alphabetical within length 5). A 2-letter `zh` correctly sorts before a 5-letter `en-US` despite `zh` > `en` alphabetically.

**§ 12 #6 + § 12 #19 updates:**

- § 12 #6 now references the canonical conflict-resolution policy.
- § 12 #19 (SingleImage writer rules) now includes `tile_axis_convention = 1`.

No wire-format change. No constant change. No fixture rotation. The new rules constrain writer behavior to a canonical surface; existing valid packs (single-source, simple metadata) are unaffected. Gates: `cargo fmt --check` clean, `cargo clippy --all-targets --workspace -D warnings` clean, 281 tests passing.

**Manifests**: `spec/rawtiles-v1.0-rc1.md` §§ 4.7, 4.8, 4.9, 4.10, 7.3, 7.4, 8.4, 12 (#6, #19), 12.1.
**Commit**: to land with the cross-writer-determinism slice.

### F-046 — § 13.1: pin `reserved_v1_0` forward-compat hole as additive-only
Pre-freeze closure of one remaining silent-render trap. The `reserved_v1_0` bytes at header offset 6 are a forward-compat hole — v1.0 readers MUST accept any value. The implicit contract was "future v1.x will use these bytes additively, not to repurpose existing v1.0 field semantics," but the contract was social, not specified.

If v1.5 were to assign semantic-altering meaning to those bytes (e.g. "if reserved_v1_0 != 0, bbox is in Web Mercator pixels rather than microdegrees"), a v1.0 reader still parsing bbox as microdegrees would silently render at the wrong location. This is the worst-case forward-compat failure mode — packs that v1.0 readers accept as valid but silently misinterpret.

§ 13.1 now states explicitly: forward-compat holes are for additive information only. v1.x changes that would alter the interpretation of any v1.0 header or tile-index field require a major bump. Generalized to cover any future reserved bytes added by minor bumps, not just `reserved_v1_0` specifically.

Real-but-narrow trap — only fires if a future spec maintainer deliberately misuses the hole. Cost of the fix is one paragraph; value is converting the social contract to a spec rule a future maintainer can't accidentally violate.

No wire-format change. No semantic change. Spec-doc only. Gates: `cargo fmt --check` clean, `cargo clippy --all-targets --workspace -D warnings` clean, 281 tests passing.

**Manifests**: `spec/rawtiles-v1.0-rc1.md` § 13.1.
**Commit**: to land with the reserved-bytes-constraint slice.

### F-045 — § 11 completeness + 9 related spec gaps (final pre-freeze conformance pass)
Pre-1.0-freeze closure of nine ways a § 11-driven reader implementer could ship code that silently accepts malformed packs or traps on Cortex-M alignment.

**(1) § 11 is now actually complete.** The previous § 11 had 19 numbered MUSTs that did NOT cover several field-level invariants. A reader implementing the 19 was non-conforming for several real classes of malformed pack. Added rules:
- **#9** `tile_dim_px == 0` — was MUST in § 4.7 only. A pack with `tile_dim_px = 0` would slip through every previous § 11 rule.
- **#10** `zoom_max ≥ 24` or `zoom_min > zoom_max` — was MUST in § 4.8 only. `zoom_max = 25` is an actual buffer overrun: it lets a tile-index entry's `z = 25` index into `zoom_offsets[25]`, past the 24-entry array bound. Load-bearing safety check.
- **#11** `bbox` out of µdeg range or with min > max — was MUST in § 4.9 only. A pack with `min_lon = INT_MIN` would slip through.
- **#15** per-entry `z` within `[zoom_min, zoom_max]` — was MUST in § 5.2 only. Combined with #10 this bounds `z < 24`.
- **#16** v1 tile-length matches format-implied size: `length == tile_dim_px²` for every entry under `ABGR2222 + None`. Previously a pack with `length = 1` slipped through entirely and the reader returned 1 byte of "tile data" to a caller expecting 16,384 bytes.
- **#23** SingleImage shape contract: `tile_count == 1`, lone `z == 0`, `zoom_min == zoom_max == 0`, `zoom_offsets[1..24]` all-zero, `zoom_offsets[0] == (index_offset, 1)`. Was MUST in § 8.6 only.

§ 11 promotion of writer-MUST counterparts already mirrored in § 12 (F-040) — the two checklists are now genuine mirrors. Total MUSTs in § 11 grew from 19 to 25 (plus 2 SHOULDs).

**(2) § 11 rejection-timing pinned.** Previously § 10 said no tile bytes return while CRC mismatch possible, but the spec was silent on when other § 11 rejections fire. A lazy-validating reader (defer rejection to first lookup) was arguably conforming. New: "All rejection rules in § 11 MUST be enforced before any tile bytes, extension-payload bytes, or extension-tag information are returned to the caller." Streaming verify (§ 10) can interleave structural rejections with the CRC fold; lazy validation is non-conforming.

**(3) § 11 #14 + #19 overflow-safe arithmetic.** The bounds checks `offset + length ≤ extensions_offset` (tile-index) and `section_start + 8 + length ≤ file_size − 4` (extension-section walk) both wrap on 32-bit hosts for `length` near `u32::MAX`. A reader transcribing the spec literally into C fails the check exactly when it most needs to. Restated as subtraction: `length ≤ extensions_offset − offset` and `length ≤ (file_size − 4) − section_start − 8`.

**(4) Extension-payload alignment / AFFN-on-Cortex-M4 trap.** § 3's alignment guarantee covers only header and tile-index fields. Extension sections start at 4-aligned offsets, so a section's payload begins at `section_start + 8` — which is 4-aligned-not-8-aligned when `section_start mod 8 == 4`. AFFN's six `f64`s within the payload are then 4-aligned-not-8-aligned. Cortex-M4 in default config permits unaligned `LDR` for u32 but traps on misaligned `VLDR.64` / `LDRD`. A reader pointer-casting AFFN `f64`s (mirroring § 11 #20's previous permission for header u64 access) would fault on a subset of conforming packs. New § 11 #27 (SHOULD): readers MUST `memcpy` 64-bit values within extension payloads regardless of buffer base alignment. The previous SHOULD (#20 → #26) clarified that its alignment guarantee is scoped to header + tile-index fields only.

**(5) § 3 align4 contradiction with § 4.11 / § 11 #25 resolved.** Previous wording: "readers MUST compute via align4 to handle conforming packs that for some reason place the index further into the file" — but § 4.11 and § 11 #19 (now #25) say `index_offset == 292` is normative, so no such packs exist. Cut the "for some reason" half-sentence. The `align4` formulation is retained but reframed as forward-compatibility for a possible future major version with a non-multiple-of-4 header.

**(6) § 7.1 padding-byte reader verification stated explicitly.** § 7.1 said padding bytes MUST be `0x00`; § 11 didn't say readers verify it. New § 11 #19(a) folds the padding-zero check into the extension-section walk: "verify that the section's padding bytes (0–3 bytes between `payload` and the next 4-byte boundary) are all `0x00` — readers MUST reject non-zero padding."

**(7) § 5.3 lookup is now defensive against unconstrained caller input.** The lookup *algorithm* is well-defined for conforming packs, but the caller's `(z, x, y)` is unconstrained — a caller passing `z = 30` MUST NOT cause the reader to read `zoom_offsets[30]` past the 24-entry array bound. Step 1 (new) treats `z ≥ 24` as out-of-range and returns the absent outcome. Step 2 (rewritten) explicitly returns absent on `count == 0` without proceeding to binary search (a search over zero entries can read adjacent-entry bytes as garbage). § 5.3 is now phrased as MUST, not SHOULD — the algorithm's correctness underpins § 11 #17's invariants.

**(8) § 4.10 build_timestamp offset typo.** "different bytes at offset 78 → different CRC" — `build_timestamp` is at offset 80 per the § 4 header table. Cosmetic but the kind of mistake that bites a reader implementer hand-computing a CRC mismatch.

**(9) § 11 #13 strengthened to forbid duplicate `(z, x, y)` triples explicitly.** The "no duplicates" invariant was implicit in § 5.2 and the binary-search ordering but not in § 11's text. Now explicit.

Numbering ripple: § 11 grew from 20 items (19 MUSTs + 1 SHOULD) to 27 items (25 MUSTs + 2 SHOULDs). Internal `§ 11 #N` cross-references updated: `§ 3` (#19 → #25), `§ 10 streaming verify` (#9–#14 → #12–#19), `§ 11 #14` self-reference renumbered to #19. § 12 cross-references unchanged (§ 12 numbering unchanged).

No wire-format change. No constant change. No fixture rotation. The new rejection rules cover *malformed* packs that no conforming writer would have produced anyway, so existing valid packs are unaffected. Gates: `cargo fmt --check` clean, `cargo clippy --all-targets --workspace -D warnings` clean, 281 tests passing.

**Manifests**: `spec/rawtiles-v1.0-rc1.md` §§ 3, 4.10, 5.3, 11 (heavy revision).
**Commit**: to land with the § 11 completeness slice.

### F-044 — Spec scrub: remove all implementation ties (slippypack, una-sdk, TilePack)
Final implementation-decoupling pass. After F-041 cut slippypack-internal references (file paths, test names, env vars), 13 narrative-level references remained where the spec said "slippypack does X" or "una-sdk's watch firmware TilePack" or "byte-identical packs to slippypack". A wire-format spec should describe the format, not name its reference implementations.

For each reference, decided: (1) is the content needed by a conforming impl? If yes, rephrase generically. (2) If not, cut.

Cuts:

- **Status note**: "validation against an independent reader (currently slated for the una-sdk MapTrack simulator round-trip)" → "validation against an independent reader". The una-sdk slating is project planning, not spec content.
- **Intro (paragraphs 1 + 2 of body)**: "writers (e.g. slippypack)" / "readers (e.g. una-sdk's watch firmware TilePack, or any future device-side consumer)" / "The format's design home is this slippypack repository. The una-sdk watch firmware is one reader" — all collapsed into a single intro that names *categories* of consumer (firmware, validators, debug tools) and *device categories* (watches, embedded displays, kiosks, e-readers) without binding to any specific project.
- **Scope and audience**: "Writers (slippypack, third-party builders) need every section" → "Writers need every section". "Appendix A is normative only for writers that need to produce byte-identical .rawtiles files to slippypack for the same logical inputs" → "byte-identical .rawtiles files across implementations given the same logical inputs (the offline-delivery dedup contract)".
- **§ 4.7**: "For Quadtree, slippypack writes 128. For SingleImage, slippypack writes a value ≤ 240" — cut entirely. These are implementation choices; the spec only requires `tile_dim_px` non-zero.
- **§ 5.3**: "(For reference, slippypack's Rust reader uses Option<&[u8]>; a C reader might use a bool out_present parameter…)" — cut; the absent-tile contract is "implementation-defined except panic/exception throwers are non-conforming," which doesn't need Rust/C examples.
- **§ 9.1.1**: "Slippypack's canonical quantisation maps…" → "The canonical quantisation maps…". The quantisation IS the spec's canonical method; attributing it to slippypack was the wrong frame.
- **Appendix A header**: "This appendix defines slippypack's pack_uuid derivation. It is normative for writers that need to produce byte-identical packs to slippypack" → "This appendix defines the canonical pack_uuid derivation. It is normative for writers that need to produce byte-identical packs across implementations". Same pattern.
- **§ A.1**: "The slippypack UUID namespace" → "The rawtiles UUID namespace". "Changing it would break the 'did the watch already receive this pack?' deduplication check" → "Changing it would break the recipient-side deduplication check ('does the device already have this pack?')".
- **§ A.3**: "Two slippypack-specific rules apply on top of JCS" → "Two rawtiles-specific rules apply on top of JCS". "the JCS canonicalization rules slippypack relies on are…" / "slippypack's descriptor schema…" → "the JCS canonicalization rules this spec relies on are…" / "the descriptor schema…".
- **§ 10 streaming-verify**: "…which on a watch / SPI-flash combination can hide the verification latency entirely" — implementation-context tail cut. The carve-out's normative content stands without the watch/SPI-flash flavor.
- **§ 7.3 SRCD example**: "OSM 2026-04 Geofabrik Italy extract, MapLibre watch-tuned style v2" → "OSM 2026-04 Geofabrik Italy extract, MapLibre style v2". "Watch-tuned" was illustrative but device-specific.
- **Appendix C changelog**: "(una-sdk MapTrack simulator round-trip)" cut.

Kept on purpose:
- "watches, embedded displays, kiosks, e-readers" as a generic device-category list in the intro — describes the spec's target use case, not a specific implementation.
- "(some Cortex-M configurations)" in § 3 alignment note — describes a class of strict-alignment platforms, not a specific impl.
- "MapLibre style JSON" in § 7.3 and § A.4 — MapLibre Style Spec is a published external standard, not slippypack-internal. Coupling style_hash's domain to MapLibre is a deliberate spec choice (the renderer-style identity needs *some* concrete grammar).
- § 10 "Implementation note for resource-constrained readers (Cortex-M and similar)" perf paragraph — rationale, but cited as illustrative of the kind of consumer the streaming-verify carve-out targets. Borderline; left in for now. Same flavor as F-043 cuts; a follow-on slice could move it to DECISIONS.

Net spec change: 22 lines removed, 16 added (net −6). No wire-format change. No semantic change. The format is now self-contained: a third-party reader/writer/validator can read this document end-to-end without needing to know what "slippypack" is. Gates: `cargo fmt --check` clean, `cargo clippy --all-targets --workspace -D warnings` clean, 281 tests passing.

**Manifests**: `spec/rawtiles-v1.0-rc1.md` §§ status note, intro, Scope and audience, 4.7, 5.3, 7.3, 9.1.1, 10, A header, A.1, A.3, Appendix C changelog.
**Commit**: to land with the implementation-decoupling slice.

### F-043 — Spec scrub: cut rationale/commentary
Pre-freeze cleanup. The spec had accumulated nine sites of design-rationale text — paragraphs explaining *why* a choice was made rather than *what bytes the wire format defines*. A reader implementer doesn't need to know why length-prefix was chosen over a delimiter; they need to know the bytes. The rationale for each cut design choice is preserved in this file (F-022 through F-040) where it belongs.

Cuts:

- **§ 7.4 length-prefix-vs-delimiter rationale paragraph** — 5 lines explaining why the `NAME` payload uses length-prefixing. The bytes-level definition above the paragraph stands on its own. Decision rationale lives in F-023 / F-024.
- **§ A.3 "Why bit-patterns, not decimal degrees or microunits" paragraph** — 5 lines explaining the `affn` hex-bit-pattern choice. Decision rationale lives in F-028.
- **§ A.3 worked `affn` example** — 5 lines showing what an arbitrary affine looks like in hex. Useful for implementers but not normative; readers needing examples can consult the implementation tests. Cut.
- **§ 4.10 sentinel collision note** — 4 sentences of editorial explanation compressed to one: "Writers needing to express exactly the Unix epoch SHOULD use `1` to avoid collision with the sentinel."
- **§ 4.12 zoom-22-deepest-OSM-and-Google-Maps note** — 2 lines of rationale for why the per-zoom directory is 24 slots. The 24-slot accommodation is stated; why is design-decisions territory.
- **§ 5.2 sort-rule "Equivalently:" restatement** — the same sort rule was given twice in different prose. Kept the formal phrasing (`z` non-decreasing + within-zoom `(x, y)` strictly ascending) and removed the "Equivalently:" lead-in.
- **§ 6.2 "(= 16,384 bytes for the standard 128² watch tile)" parenthetical** — `tile_dim_px²` is the formula; the worked value for a specific watch's tile size is implementation context.
- **§ 8.6 pairing-table description column** — explanations of *why* the two illegal pairs are illegal ("A 'single Mercator image' has no canonical bounds" / "Local-linear coordinates have no canonical pyramidal subdivision"). The ✅/❌ column is the normative content. Description column removed.
- **§ 8.6 closing rationale on future tiled SingleImage forms** — "future tiled forms get a new `tile_addressing_scheme` enum value via a minor-version bump, not an ambiguous reinterpretation of `SingleImage = 2`". This belongs in § 13 versioning (which already covers the principle generally) and DECISIONS, not in the SingleImage constraint section. Cut.

Not touched in this slice (could be candidates for the same treatment, not explicitly listed by the user): § 8.6 Quadtree-tile_count-0 rationale paragraph ("useful for catalog stubs, source-attribution probes, sentinel packs"), § 4.11 "tighter symmetry … no semantically undefined bytes" paragraph, § 5.3 "Reader API surface for the absent outcome is implementation-defined" sentence ending with the slippypack/C/panic example. These are debatable: each has a small normative payload mixed with rationale. Left intact pending explicit direction.

Net spec change: 25 lines removed, 11 added (net −14). No wire-format change. No semantic change. Gates: `cargo fmt --check` clean, `cargo clippy --all-targets --workspace -D warnings` clean, 281 tests passing.

**Manifests**: `spec/rawtiles-v1.0-rc1.md` §§ 4.10, 4.12, 5.2, 6.2, 7.4, 8.6, A.3.
**Commit**: to land with the rationale-scrub slice.

### F-042 — Spec scrub: cut duplication (§ 14.5 fold, § 14.6 spec_layout ref, § 4.x reject restatements)
Pre-freeze cleanup of duplicated normative content. Two kinds of duplication had accumulated: (1) the CRC check value defined in § 10 was restated verbatim as § 14.5 "for emphasis"; (2) several § 4.x field definitions restated the corresponding "readers MUST reject" rule already covered in § 11. Duplication is a maintenance hazard — when one copy gets updated and the other doesn't, conforming implementations diverge.

Cuts:

- **§ 14.5 deleted entirely.** The CRC-32/ISO-HDLC check value (`"123456789"` → `0xCBF43926`) was defined in § 10 and restated in § 14.5 with a parenthetical "this section exists to avoid the duplication that would otherwise let § 10 and § 14 drift." That note was self-defeating — having two copies *is* the drift hazard. § 10 is the canonical site; § 14.5 added no information. § 14.6 (per-tile hash tables) renumbered to § 14.5; one self-reference in § 14.6's body updated to point at the new § 14.5 number.
- **§ 14.6 (now § 14.5) `spec_layout` reference generalized.** "even though `spec_layout`-style byte-equality tests (§ 14.3) would pass" → "even though the byte-equality fixtures of § 14.3 would pass." `spec_layout` is a slippypack Rust test name — same flavor as the F-041 § A.5 cut.
- **§ 4.1, § 4.2, § 4.3, § 4.5, § 4.6, § 4.11** — rejection-rule restatements removed. § 4 now describes what each field's bytes ARE; rejection rules live exclusively in § 11; writer rules live exclusively in § 12. Specifically dropped:
  - § 4.1: "Readers MUST reject any file whose first four bytes are not this sequence." (§ 11 #2)
  - § 4.2: "Readers MUST reject any pack whose `major ≠ 1`." (§ 11 #3) and "Readers MUST accept packs with `major = 1, minor > 0`." (§ 11 #4)
  - § 4.3: "MUST NOT be all-zero; writers MUST validate and readers MUST reject." (§ 11 #5, § 12 #2) — replaced with "The all-zero value is reserved."
  - § 4.5: "v1 writers MUST set this to all-zero. v1 readers MUST reject packs where this field is not all-zero." (§ 11 #6, § 12 #3) — replaced with "the only legal v1 value is all-zero."
  - § 4.6: "readers MUST reject any unknown value." (§ 11 #7) — implicit in "See § 8 for legal values."
  - § 4.11: "v1.0 writers MUST place the tile index immediately after the header. v1.0 readers MUST verify `index_offset == 292` and reject any other value." (§ 11 #19, § 12 #4) — replaced with "v1.0 fixes the tile index immediately after the header: `index_offset == 292`."

Not touched in this slice (deferred — user scoped explicitly to § 4.x): § 5.2's "Readers MUST reject non-zero values" for flags/reserved, § 7.1's concluding "Readers MUST reject packs that violate any of these", § 7.3's AFFN-required reader-MUST, § 8's enum-table "reader MUST reject" cells and intro line, § 8.6's pairing-table and SingleImage reject lines, § 13's "v1 readers MUST reject v2 packs", Appendix B's reserved-value reject. These are all candidates for the same treatment if the spec wants single-site rejection rules; left for a follow-on if desired.

Net spec change: 30 lines removed, 8 added (net −22). No wire-format change. No constant change. No semantic change to the conformance contract. Gates: `cargo fmt --check` clean, `cargo clippy --all-targets --workspace -D warnings` clean, 281 tests passing.

**Manifests**: `spec/rawtiles-v1.0-rc1.md` §§ 4.1, 4.2, 4.3, 4.5, 4.6, 4.11, 14.5 (deleted), 14.6 → 14.5 (renumbered).
**Commit**: to land with the duplication-scrub slice.

### F-041 — Spec scrub: remove slippypack-internal references
Pre-freeze cleanup. The spec had accumulated four references to slippypack's source tree, Rust test names, env vars, and internal docs — none of which belong in a format specification a third-party implementer would read. Cuts:

- **§ 14.3** — fixture table previously listed `crates/slippypack-core/tests/fixtures/format/` paths and an internal `BLESS_SPEC_LAYOUT=1` re-bless env var. Reduced to a narrative paragraph that says what the corpus exercises (smallest pack, multi-zoom directory, multi-source ATTR, end-to-end pipeline) without leaking slippypack's directory structure. Third-party implementers can find the fixtures via the reference implementation's repository without needing the spec to tell them the exact paths.
- **§ 14.2** — `spec-validator-cpp/` directory reference generalized to "a reference C++ validator is shipped alongside this specification". The validator's existence and its independent-derivation property are what matter; its checked-in path is reference-implementation detail.
- **§ A.5** — closing sentence cited the Rust test names `identity::tests::baseline_canonical_bytes_match_committed_string` and `identity::tests::determinism_baseline_pack_uuid_is_committed`. Cut; the worked-example values are pinned by being in the spec, not by being in a test of slippypack-core.
- **§ 4.10** — parenthetical "(slippypack uses this for the synthetic source kind…)" cut. The sentinel-value definition stands on its own.

No semantic change. No wire-format change. Spec-doc only; no code touched. Gates: `cargo fmt --check` clean, `cargo clippy --all-targets --workspace -D warnings` clean, all workspace tests passing.

**Manifests**: `spec/rawtiles-v1.0-rc1.md` §§ 4.10, 14.2, 14.3, A.5.
**Commit**: to land with the spec-scrub slice.

### F-040 — Spec pre-freeze: preprocessing-pipeline determinism, § 12 completeness, `build_timestamp` reproducibility
Pre-1.0-freeze closure of three load-bearing determinism gaps surfaced in cold review. Each gap was a place where two writers — both checking every box in § 12 — could legitimately produce byte-different packs with the same `pack_uuid`, defeating the offline-delivery dedup contract that reader caches depend on.

**(a) `content_hash` semantics moved from source-file bytes to pre-quantise RGB888 stream (§ A.4).** The old definition hashed the raw input file (PNG MD content, MBTiles SQLite rows, etc.). Two writers given the same PNG could legitimately decode it through different sRGB / linear / alpha-handling / bicubic-vs-Lanczos pipelines, yielding the same `content_hash` (same input bytes) but different RGB888 → different tile blobs → same `pack_uuid` with different bytes. **Worst-case dedup failure**: the recipient's cache hits a UUID it already has and never re-downloads the actually-different pack.

New definition (raster sources `dir`, `geotiff`, `mbtiles`, `pmtiles`, `image`): `content_hash` is the SHA-256 of the writer's pre-quantisation RGB888 byte stream, in ascending `(z, x, y)` tile order (or raster scanline order for single-image sources), three bytes per pixel `R, G, B`, no alpha, no padding. The preprocessing pipeline itself is left implementation-defined — what the hash promises is the pipeline's *output*. Two writers with different pipelines yield different `content_hash` → different `pack_uuid`, which is correct: they should not dedup-collide. Style and PBF kinds keep their existing semantics (SHA-256 of style JSON / raw PBF bytes; v1 doesn't render either).

No code rotation required: `content_hash` is plumbed through `identity.rs` as `[u8; 32]` opaque data, but no extant CLI ingest path actually computes it from real source files yet — Source::Url and Source::Synthetic both take a different identity path. The spec change pins the semantics for when the file-backed kinds get a real ingest implementation.

**(b) § 12 expanded from 17 items to 24, restating the field-level MUSTs from §§ 4–10.** A writer-implementer treating § 12 as the conformance checklist (which is what the section's name implies they should) would previously miss: § 8.6 SingleImage tile-index rules (`tile_count = 1`, `z = 0`, zoom_offsets[1..24] all-zero), § 4.9 bbox ordering and range checks, § 8.6 projection × addressing legal pairs, § 7.4 NAME payload framing, § 7.3 48-byte AFFN layout, § 4.13 extensions_offset upper bound and "= file_size − 4" no-extensions case, § 7.1 extension-section framing, § 7.2 non-letter-tag-first-byte prohibition. § 12 #1's "emit exactly the bytes defined by §§ 4–10" is technically a catch-all but writer-implementers reach for explicit checklists. New items #14–19, #22–23 inline or cross-reference each previously-implicit MUST.

**(c) `build_timestamp` SHOULD-vs-MUST contradiction resolved (§ 4.10, § 12 #20, § 14.1).** The field was a SHOULD in § 12 (set deterministically) but a load-bearing MUST under § 14.1 (round-trip byte-identity). It's the only header field that sits *inside* the CRC scope but *outside* the canonical descriptor (§ A.3) — so a wall-clock value produces byte-different packs with the same `pack_uuid`, exactly the dedup failure mode § 14.1 exists to prevent. § 4.10 now spells out the determinism asymmetry. § 12 #20 introduces a "reproducibility-claiming subset" of writer MUSTs that promotes the SHOULD to a MUST for writers that advertise round-trip byte-identity to their consumers. § 14.1 now decomposes the round-trip property into three concrete obligations (preprocessing-pipeline determinism, canonical quantiser, `build_timestamp` determinism), making the gates explicit.

No wire-format change. No constant change. No fixture rotation. Spec-doc edits only; no code touched. Gates: `cargo fmt --check` clean, `cargo clippy --all-targets --workspace -D warnings` clean, `cargo test --workspace` 281 passed.

**Manifests**: `spec/rawtiles-v1.0-rc1.md` §§ 4.10, 12 (and 12.1 numbering ripple), 14.1, A.4.
**Commit**: to land with the pre-freeze determinism slice.

### F-039 — Spec batch: extensions upper-bound, undefined tag bytes, stranded-byte gap, `tile_count = 0`, absent-tile API
Pre-1.0-freeze cleanup of six specification gaps surfaced in a final cold re-read. Each item closes an "undefined behavior" pocket where the spec implied an intent without nailing it down, leaving room for two conforming implementations to disagree.

1. **§ 11 #19 tautological clause removed.** Original text referenced a `tile_blob_size` variable the spec never defines; the actual invariant (tile blob extents bounded by `extensions_offset`) is already enforced by § 11 #11's per-entry `offset + length ≤ extensions_offset` check. Restating it as a global condition added no constraint and invited reader-side reimplementation of a derivable value. Dropped, with a parenthetical pointing back to #11.
2. **§ 11 #13 `extensions_offset` upper-bound MUST added.** § 7.1 said sections lie within `[extensions_offset, file_size − 4)` but did not constrain `extensions_offset` itself. A pack with `extensions_offset = file_size + 1000` would have § 11 #14's section-walk loop (`while pos < file_size − 4`) terminate after zero iterations and silently report "no extensions" instead of rejecting the malformed pack. Now § 11 #13 explicitly rejects `extensions_offset > file_size − 4` *before* the walk begins.
3. **§ 7.2 third bullet added for non-letter first bytes.** The case bifurcation only covered `A–Z` (SDK-reserved, reject) and `a–z` (application-private, accept). A tag like `0x30 0x31 0x32 0x33` ("0123") was reader-undefined: a defensive reader would reject, a lenient reader would silently ignore. Now: writers MUST NOT emit such tags in v1; readers MUST reject.
4. **§ 7.1 no-stranded-bytes MUST added; § 11 #14 cross-reference added.** § 7.1's section bounds did not require the last section's padded end to abut the CRC footer. A pack where the last section ended at `file_size − 12` (8 stranded bytes before CRC) had no defined reader behavior. Now the last section's padded end MUST equal `file_size − 4`; the "no extensions" case (`extensions_offset == file_size − 4`) is the zero-section degenerate form of the same invariant. § 11 #14 now requires readers to verify the walk's terminal position equals `file_size − 4`.
5. **§ 8.6 Quadtree `tile_count = 0` blessed.** § 8.6 nailed down `tile_count = 1` for `SingleImage` but left Quadtree silent. Zero-tile Quadtree packs are useful for metadata-only/sentinel/catalog-stub use cases (carry only `NAME`/`SRCD`/`ATTR`). Without explicit blessing, two readers might disagree on whether to accept them. Now explicitly accepted, with the structural shape pinned: every `zoom_offsets[z] == (0, 0)`, tile blob empty, `extensions_offset == 292`.
6. **§ 5.3 absent-tile API contract clarified.** The lookup *algorithm* was specified; the *reader API surface* for "absent" was not. Now: implementation-defined (nullable / sentinel / error variant — any idiomatic shape works), with the constraints that absent tiles never return arbitrary bytes and panic/exception-throwing APIs are non-conforming (they conflate "not in this pack" with "malformed pack").

Two of the original 8 review items in the batch were stale — already fixed in F-036 (`index_offset == 292` MUST/SHOULD contradiction, fixed when § 11 #19 became MUST) and F-037 (CRC verification timing § 10 vs § 11 #18, fixed when streaming/trusted carve-outs landed). Verified by reading current spec state before editing; no re-edits needed.

No wire-format change. No constant change. Spec-doc edits only; no code touched. Gates: `cargo fmt --check` clean, `cargo clippy --all-targets --workspace -D warnings` clean, `cargo test --workspace` 6 passed.

**Manifests**: `spec/rawtiles-v1.0-rc1.md` §§ 5.3, 7.1, 7.2, 8.6, 11 (#13, #14, #19).
**Commit**: to land with the pre-freeze spec-batch slice.

### F-038 — Reader conformance corpus: per-tile SHA-256 tables
Closing a real conformance gap. § 14.3 pinned the bytes of each golden pack; § 14.4 pinned the writer quantiser; § 14.2 ships a C++ validator that checks pack structure. None of these caught a reader that opens a golden pack but returns bytes for the *wrong* tile — off-by-one in binary search, wrong-zoom lookup, mis-extracted index entry. A reader could pass every existing gate and still be silently wrong.

Added § 14.6 + corpus: each of the 5 spec_layout / e2e golden packs gets a sibling `<pack>.hashes` file listing one line per tile (`<z> <x> <y> <sha256-hex>`, sorted by `(z, x, y)`, comment lines `#`-prefixed). A third-party reader's conformance check: open the pack, call `tile_bytes(z, x, y)` for each entry, SHA-256 the result, compare. Together § 14.3 + § 14.6 cover writer-output and reader-lookup correctness.

Slippypack-side: `tests/reader_conformance.rs` regenerates and verifies the corpus. Bless via `BLESS_READER_CONFORMANCE=1`. `sha2` added as a slippypack-core *dev* dep (not production).

**Known gap deliberately not closed in this slice**: negative-conformance corpus. A v1 reader currently can't prove it rejects everything § 11 says to reject without writing its own malformed-pack harness. § 14.6 flags this as a planned follow-on rc.

The 5 packs covered (synthetic / CLI synthetic isn't in this corpus because its tile bytes are descriptor-derived and may shift with unrelated CLI work; the format-side and e2e fixtures are the stable ones):
- `golden-grid.rawtiles` / `.hashes` — 25 tiles, single-zoom
- `golden-pyramid.rawtiles` / `.hashes` — 21 tiles, z=2..=4
- `golden-attr.rawtiles` / `.hashes` — 9 tiles + ATTR
- `golden-png-to-pack-1tile.rawtiles` / `.hashes` — smallest non-empty pack
- `golden-png-to-pack-5tiles.rawtiles` / `.hashes` — multi-zoom e2e output

**Manifests**: `crates/slippypack-core/tests/reader_conformance.rs` (new); `tests/fixtures/format/*.hashes` and `tests/fixtures/e2e/*.hashes` (5 new committed files); `crates/slippypack-core/Cargo.toml` (`sha2` dev-dep); `spec/rawtiles-v1.0-rc1.md` § 14.6 (new).
**Commit**: to land with the reader-conformance slice.

### F-037 — Spec § 10: streaming-verify and trusted-source carve-outs for the CRC
Pre-1.0-freeze refinement. § 10 had "Readers MUST verify the CRC at open time" with no escape hatch. On a 100 MHz Cortex-M4 with SPI flash at ~50 MB/s and software CRC-32/ISO-HDLC (slicing-by-4, 1 KB table), opening a 50 MiB pack costs ~2 s wall-clock. A watch enumerating 5 packs at boot would pay a 10–15 s eager-verify penalty before serving the first tile — a real UX problem for a category of consumers the format explicitly targets.

Resolution: keep the **integrity guarantee** unconditional, but make the **verification window** flexible:

- **Eager verify** (default): compute CRC at open before any API returns success. Simplest, fine when open-time latency isn't a constraint.
- **Streaming verify** (MAY): return from open immediately; run the CRC pass concurrently with the structural-check pass (§ 11 #9–#14, which already touches every byte). MUST complete before any tile/extension bytes are returned to the caller. Folds the CRC cost into work that was happening anyway — on the watch case, eliminates the user-visible latency entirely.
- **Caller-asserted trust** (MAY): skip CRC if the caller asserts integrity through another channel (signed installer, content-addressed storage, previously-verified cache). Readers exposing this MUST require explicit opt-in; the default path MUST verify.

§ 11 #18 cross-references the three options.

This is the same shape as "TLS sessions MUST verify the cert; reused sessions MAY skip the verify because it was already done": the *integrity contract* doesn't bend, but the spec gives implementers room to amortise the cost.

**Manifests**: `spec/rawtiles-v1.0-rc1.md` § 10 (rewritten verification-window section + implementation note for resource-constrained readers); § 11 #18 cross-reference.
**Commit**: to land with the CRC-window slice.

### F-036 — Spec § 11 #19 promoted from SHOULD to MUST (resolves contradiction with § 4.11)
F-033's earlier edit tightened § 4.11 to "v1.0 readers MUST verify `index_offset == 292`" but left § 11's redundant restatement under "A conforming v1 reader SHOULD" (item #20). A strict-reading implementer would have hit two contradictory normative levels for the same condition.

Resolved by promoting the check to MUST (now § 11 item #19) and demoting the alignment-strategy bullet to SHOULD #20. The MUST item also restates `extensions_offset ≥ tile_blob_start + tile_blob_size` from § 4.13 — both clauses are MUSTs per their referenced sections; § 11 now restates them in the actionable rejection sweep so a reader implementer reading top-to-bottom finds them at the right normative level.

**Manifests**: `spec/rawtiles-v1.0-rc1.md` § 11 (renumbered: MUST 1-19, SHOULD 20).
**Commit**: to land with the consistency-fix slice.

### N-002 — Spec document rebranded as `v1.0-rc1`
The wire format is pinned but the spec document is in release-candidate status — frozen pending real-world cross-implementation validation (currently slated for the una-sdk MapTrack simulator round-trip). Any wire-format-affecting change between rc1 and 1.0 would invalidate every `pack_uuid` derived under rc1; the `-rc1` marker tells early implementers that fixtures are provisional until 1.0 ships.

**Distinction**: the spec *document* version (`1.0-rc1`, `1.0-rc2`, `1.0`, `1.1`, …) is separate from the *wire format* `format_version` bytes (`(1, 0)`). Multiple spec-document revisions can describe the same wire format if changes are editorial / clarification-only.

Mechanical changes:
- `spec/rawtiles-v1.0.md` → `spec/rawtiles-v1.0-rc1.md`
- Title: "rawtiles format specification — version 1.0" → "... version 1.0-rc1"
- Status paragraph rewritten to call out RC status + the two-versioning distinction
- Appendix C — Change history: column header → "Spec version" + footnote
- All filename references in `PLAN.md`, `README.md`, `DECISIONS.md`, `crates/slippypack-core/src/format/{types,tile_index}.rs`, `spec-validator-cpp/README.md`

No code change beyond doc-comment filename updates; no fixture re-bless.

**Manifests**: `spec/rawtiles-v1.0-rc1.md` (renamed + updated header/footer); 7 reference points across `*.md`, `*.rs`.
**Commit**: to land with the rc1-rebrand slice.

### N-001 — Rename: `.upack` / `UPCK` → `.rawtiles` / `RAWT`
The format extension was originally `.upack` (Una Pack), naming it after the una-sdk project that motivated the design. As scope broadened to "any low-resource device that can do a memcpy" (per V-003 zoom expansion, V-004 quantiser trait, V-001 C++ validator), the vendor-named extension stopped fitting. `.upack` also clashed with Inedo's UPack universal-package format.

`.rawtiles` was chosen for naming what makes the format actually different: the tile bytes ARE raw pixel data, no decoder needed on the reader. Parallels `.mbtiles` (Mapbox) and `.pmtiles` (Protomaps) but emphasizes the property rather than the vendor. Magic bytes `RAWT` (4 ASCII).

**Key invariant preserved**: the namespace UUID's *bytes* stay the same (`4e72f962-6632-4538-8e0a-7eab63350f3f`), only the constant name changed (`SLIPPYPACK_NAMESPACE` → `RAWTILES_NAMESPACE`). The format-as-byte-layout is unchanged, only the human-readable name shifted.

**Mechanical scope**:
- Magic bytes: `UPCK` → `RAWT` in `format::types::MAGIC` and C++ validator constants
- File extension: `.upack` → `.rawtiles` across code, docs, fixtures, tests
- Type names: `UpackWriter` / `UpackReader` → `RawtilesWriter` / `RawtilesReader`
- Source file: `format/upack_writer.rs` → `format/rawtiles_writer.rs`
- Spec doc: `spec/upack-v1.0.md` → `spec/rawtiles-v1.0-rc1.md`
- All 6 golden fixtures renamed and re-blessed (CRC changes when magic does)
- C++ validator binary: `upack_validate` → `rawtiles_validate`
- Narrative: "una-sdk owns the format spec" → "`spec/rawtiles-v1.0-rc1.md` in this repo is authoritative; una-sdk is one reader implementation"

slippypack stays as the toolkit/project name (the writer). `rawtiles` is the format the toolkit produces.

**Manifests**: workspace-wide; `crates/slippypack-core/src/format/types.rs::MAGIC`; `crates/slippypack-core/src/identity.rs::RAWTILES_NAMESPACE`; all 6 `golden-*.rawtiles` fixtures; `spec/rawtiles-v1.0-rc1.md`.
**Commit**: to land with the rename slice.

---

## V — Cross-implementation validation

### V-001 — `spec-validator-cpp` exists as a second-opinion C++ reader
Phase 1.x deliverable, prompted by the need to catch writer-side bugs (endianness, padding, offsets) that slippypack's own writer+reader pair would miss because they share logic. The validator re-derives byte decoding from PLAN.md + the in-tree `crates/slippypack-core/src/format/*.rs` layout tables; it does NOT call slippypack code. If the two implementations disagree on a `.rawtiles`, the disagreement is the bug report.

**Status**: passes against the synthetic-source pack and rejects byte-mutated packs via CRC-32 mismatch.
**Manifests**: `spec-validator-cpp/src/validator.cpp`; `spec-validator-cpp/tests/run.sh`.
**Commit**: to land with the C++ validator slice.

### P-008 — Mercator transcendentals go through the `libm` crate, not `f64::*` std methods
Triggered by review: `f64::tan`, `f64::asinh`, `f64::sinh`, `f64::atan` delegate to the platform libc's libm — glibc, musl, macOS libm, ucrt, and the WASM runtime's host libm are NOT bit-for-bit agreeable. slippypack's CLI/PWA byte-identity guarantee was working "by coincidence" of the platforms the CI matrix happened to cover. Swapping to the pure-Rust [`libm`] crate forecloses this risk before the PWA ships, not after.

The four transcendental calls in `mercator.rs` are now `libm::tan`, `libm::asinh`, `libm::sinh`, `libm::atan`. Pure-arithmetic ops (`to_radians`, `to_degrees`, `floor`, `clamp`, `+ - * /`) stay on std — IEEE-754 already pins those.

On macOS the new f64 outputs happen to be bit-identical to the previous std-libm path for our committed determinism inputs (no test re-bless needed). The cross-platform guarantee, however, was previously "Linux x86 + macOS happen to agree"; now it's "libm-pure-Rust says the same bits everywhere".

**New test**: `determinism_committed_f64_bits_for_known_tiles` locks the actual f64 bit patterns for tile_to_lonlat at two committed inputs (London, Sydney). Stricter than the integer-tile gate — the integer test would pass even if floats drifted by sub-tile-boundary amounts; this catches any libm version regression at the ULP level.

**Manifests**: `crates/slippypack-core/Cargo.toml` (libm dep); `crates/slippypack-core/src/projection/mercator.rs::{lonlat_to_tile, tile_to_lonlat}`; new test `determinism_committed_f64_bits_for_known_tiles`.
**Commit**: to land with the libm-swap slice.

### F-026 — Spec § 8.6 pins `tile_count = 1` and `z = 0` for `SingleImage` packs
Pre-1.0-freeze closure of a spec ambiguity. v1.0 `SingleImage` (the future `LocalLinear` use case in Phase 10) is a one-image format; the constraint wasn't stated, leaving readers free to disagree on what a multi-tile `SingleImage` pack should mean. Pinned now:

- `tile_count = 1`
- The lone index entry's `z = 0`
- `zoom_min = zoom_max = 0`
- Only `zoom_offsets[0]` is non-zero

Readers MUST reject violations. If we ever want tiled multi-image packs, that's a new `tile_addressing_scheme` enum value via minor-version bump — not ambiguity in v1.0.

**Manifests**: `spec/rawtiles-v1.0-rc1.md` § 8.6 (added in the same section as F-025).
**Commit**: to land with the spec-alignment slice.

### F-025 — Spec § 8.6 pins legal `projection` × `tile_addressing_scheme` pairs
The previous § 8.3 said "Quadtree | v1 (used with `WebMercator`)" — advisory text, no reader MUST. Different readers were free to disagree on what `WebMercator + SingleImage` or `LocalLinear + Quadtree` mean. Closed now: exactly two pairs are legal in v1, readers MUST reject the rest:

| `projection` | `tile_addressing_scheme` | Legal |
|---|---|:---:|
| WebMercator | Quadtree | ✅ |
| WebMercator | SingleImage | ❌ MUST reject |
| LocalLinear | Quadtree | ❌ MUST reject |
| LocalLinear | SingleImage | ✅ |

Future projections / addressing schemes paired with existing values can be added via minor-version bumps.

**Manifests**: `spec/rawtiles-v1.0-rc1.md` § 8.6 (new section); § 8.3 cross-references § 8.6.
**Commit**: to land with the spec-alignment slice.

### F-024 — Spec § 7.3 / § 7.4 aligned to NAME length-prefixed payload
The spec doc shipped (briefly) describing the NAME payload as "optional BCP-47 language-tag prefix followed by a tab and the name" — a tab-delimited form. The implementation in `format/extensions.rs::build_name_payload` / `parse_name_payload` uses the length-prefixed form `uint8 tag_length | tag | name` (per F-023). A silent format divergence: every spec-faithful third-party reader would have failed on every slippypack-produced `NAME` section.

Fixed pre-1.0-freeze by updating spec § 7.3 to point at § 7.4, and adding § 7.4 with the length-prefixed payload table + the rationale for choosing length-prefixing over delimiter-separation (BCP-47 tags don't contain tabs, but names *could*, and the format's other conventions are length-prefixed).

The implementation is unchanged; only the spec catches up.

**Manifests**: `spec/rawtiles-v1.0-rc1.md` § 7.3 (row description) and new § 7.4 (full payload layout). Implementation reference: `crates/slippypack-core/src/format/extensions.rs::build_name_payload`, `parse_name_payload`.
**Commit**: to land with the spec-alignment slice.

### F-023 — `NAME` extension payload structured as `uint8 tag_length | bcp47_tag | name`
The slippypack `TAG_NAME` constant existed; the *payload* structure was only described vaguely in doc strings ("UTF-8 content with an optional BCP-47 language-tag prefix"). Now committed to match the una-sdk MapTrack spec exactly:

```
NAME payload = uint8 tag_length | bcp47_tag (tag_length bytes, UTF-8) | name (rest, UTF-8)
```

`tag_length = 0` means "no locale specified" — the unlocalized default name. Multiple `NAME` sections can coexist (one per locale); readers pick by BCP-47 lookup rules and fall back to the `tag_length=0` section.

**Why length-prefixed, not delimited**: BCP-47 tags contain `-`, names can contain anything (including punctuation that's hard to escape). A length prefix is unambiguous, compact (1 byte covers realistic tags — `en-Latn-GB-boont-x-private` is 28 bytes; 255 is more than enough), and matches how the rest of the format does framing (length-prefixed sections, length-prefixed extensions).

slippypack exposes `build_name_payload(bcp47_tag, name) -> Vec<u8>` and `parse_name_payload(payload) -> (tag, name)` so callers don't hand-encode bytes. The `NameSectionError` enum catches over-long tags, truncated payloads, and invalid UTF-8.

**Manifests**: `crates/slippypack-core/src/format/extensions.rs::{build_name_payload, parse_name_payload, NameSectionError}`; re-exported from `format::mod`.
**Commit**: to land with the NAME-payload-codec slice.

### F-022 — Higher minor versions in `format_version` are accepted, not rejected
Bug closed by review. `read_header` previously emitted `HeaderError::UnsupportedMinorVersion` for any pack whose minor version exceeded this build's. That defeats the whole point of having a minor version field — the format's forward-compat contract is "major locks the header layout, minor adds extension tags only," and unknown extension tags are already skipped by the extension iterator.

After fix:
- Same major, any minor → reads successfully. `ParsedHeader::format_version.minor` carries the actual minor value; callers can inspect if they need to.
- Different major → still `HeaderError::UnsupportedMajorVersion`.

`HeaderError::UnsupportedMinorVersion` is removed (it was never reachable after the fix). `#[non_exhaustive]` on the enum means downstream code that matched on it gets a clean compile error pointing at the change.

C++ validator (`spec-validator-cpp`) mirrors: a higher-minor pack produces a *warning* (it can't introspect new tags it doesn't know about), not a hard error.

**Manifests**: `crates/slippypack-core/src/format/header.rs::read_header`; tests `read_accepts_newer_minor_version` and `read_accepts_max_minor_version`; `spec-validator-cpp/src/validator.cpp::validate`.
**Commit**: to land with the minor-version-acceptance slice.

### I-010 — `Source::Style` carries a `content_hash` like every other file-backed source kind
Bug closed by review. `Source::Style { zoom_min, zoom_max }` had no content identity, so two builds with identical zoom ranges but different MapLibre Style JSONs collided on `pack_uuid`. Now `Source::Style { content_hash, zoom_min, zoom_max }` mirrors Dir / Geotiff / Mbtiles / Pbf / Pmtiles.

**Note on the top-level `style_hash` field** (PackDescriptor): this is *separate* and remains as-is. The top-level field captures the SHA-256 of the `--style` flag applied to a non-style source (typically a PBF), where the style is *external* to the source. `Source::Style`'s `content_hash` captures the SHA-256 of the style file that IS the source data (a `style:///path/to/style.json` source renders directly from the style's embedded `sources`). Two distinct concepts; both belong in the descriptor.

**Manifests**: `crates/slippypack-core/src/identity.rs::Source::Style`; serializer now uses `write_file_kind` for Style (same shape as the other file-backed kinds); new test `style_source_with_different_content_hash_changes_pack_uuid` locks the collision-closure.
**Commit**: to land with the Style content_hash slice.

### V-004 — `Quantiser` trait names the pixel-format seam; only `Abgr2222` ships in v1
Triggered by the broader-device-usefulness review. The format itself supports multiple pixel formats (the `pixel_format` byte is an enum), but the quantiser monoculture meant slippypack only produced ABGR2222 output. Adding a `Quantiser` trait now (purely additive — `quantise_rgb888` and `QUANTISER_VERSION` stay) names the seam where RGB565 / RGB888 / indexed-palette quantisers can land later as companion impls without touching `slippypack-core`'s public surface.

Each impl pins three associated `const`s — `VERSION`, `PIXEL_FORMAT`, `BYTES_PER_PIXEL` — and a `quantise(rgb888, output)` method. The trait is dyn-compatible (no associated types, no `Self: Sized` defaults) so future code paths can pick a quantiser at runtime from the metadata's `pixel_format` byte.

**Why not refactor the existing callsites to use the trait now**: the callsite (`build.rs::add_decoded_tile`) hardcodes ABGR2222. The refactor that lets the CLI pick a quantiser from `--pixel-format` is a Phase 1.x or Phase 2 follow-on. Naming the seam is cheap and unblocks the follow-on; doing the full refactor speculatively is over-engineering.

**Manifests**: `crates/slippypack-core/src/quantise.rs::{Quantiser, Abgr2222}`; `QUANTISER_VERSION` is now an alias for `Abgr2222::VERSION`.
**Commit**: to land with the Quantiser-trait slice.

### V-003 — `ZOOM_OFFSETS_COUNT` bumped from 18 to 24 (header 322→394 bytes)
Triggered by the "is anything watch-specific?" review — z=17 (max under 18 slots) is plenty for a watch displaying maps at walking pace, but constraining for car-nav (z=20), kiosk (z=18-20), and offline-GIS use cases. OSM and Google Maps publish through z=22. Bumping to z=0..=23 costs 72 bytes per pack's header (negligible) and unlocks devices beyond watches.

Mechanical changes:
- `HEADER_BASE_SIZE`: 322 → 394 (= 98 + 24×12 + 8)
- `extensions_offset` u64 moves: file offset 314 → 386
- `TileWriterError::TileZoomTooHigh` triggers at z >= 24, not z >= 18
- CLI's `parse_zoom` accepts z ≤ 23, not ≤ 17
- All 5 committed golden fixtures re-blessed (synthetic, grid, pyramid, attr, e2e 1tile + 5tile)
- C++ validator constants updated to match

This is **not** a format-version bump (we're still v1.0): nothing has shipped, and the format-version field doesn't track byte-layout-only changes. Pre-1.0 revisionism, justified.

**Manifests**: `crates/slippypack-core/src/format/header.rs::ZOOM_OFFSETS_COUNT`; `HEADER_BASE_SIZE` is now computed from it; `spec-validator-cpp/src/validator.cpp::kZoomOffsetsCount`.
**Commit**: to land with the zoom-expansion slice.

### V-002 — `.rawtiles` is byte-oriented; multi-byte fields are NOT required to be naturally aligned in the file
The C++ validator initially assumed `index_offset` must be 4-byte aligned, which failed on the synthetic pack (322 % 4 = 2). On reflection: slippypack writes everything LE byte-by-byte and the Rust reader does byte-by-byte decoding. **No multi-byte field in the on-disk format requires natural alignment.** Readers that want aligned access `memcpy` to a local before decoding (cheap; matches what watch firmware would do anyway).

**Caveat for watch firmware (ARM Cortex-M0/M0+):** unaligned `LDR` faults. Watch readers MUST do `memcpy`-then-decode, not direct dereference.

**Open question for Phase 2+**: if we ever ship a `slippypack-watch-firmware` consumer that benchmarks badly because of `memcpy` overhead, consider bumping the header to a multiple-of-8-size and 8-byte-aligning the u64 offsets. For v1 first slice the byte-oriented design wins — keeps the format honest about being LE byte-level encoded.

**Manifests**: removed the false 4-byte alignment check from `spec-validator-cpp/src/validator.cpp::validate`.
**Commit**: to land with the C++ validator slice.

---

## Cross-cutting

### X-001 — Inline `#[cfg(test)] mod tests` for module-level unit tests
Integration tests under `crates/*/tests/` are reserved for tests that exercise the format writer + reader together (per PLAN.md § Test plan). Pure-unit tests live inline alongside the module they test.
**Manifests:** `crates/slippypack-core/src/quantise.rs`, `crates/slippypack-core/src/projection/mercator.rs`.
**Commit:** established with `cdd611d`.

### X-002 — Determinism tests commit expected output bytes
Every module that produces deterministic bytes ships a `determinism_committed_output_*` test that locks the bytes against a committed expected value. Any drift fails the test and demands either a version bump (e.g. `QUANTISER_VERSION`) or a fix.
**Manifests:** `quantise::tests::determinism_committed_output_for_known_input`, `projection::mercator::tests::determinism_committed_output_for_known_coordinates`.
**Commit:** established with `cdd611d`.
