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
Catches stylistic issues early. `module_name_repetitions = allow` because the workspace expects module names to echo their parent (e.g., `Mercator` in `mercator.rs`, `UpackWriter` in `format/mod.rs`).
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
Lets modules use the `alloc::*` path today (e.g. `alloc::collections::BTreeSet` in `UpackWriter`) even though slippypack-core is currently std-compiled. When W-008 closes (the no_std + alloc switch), code that already uses `alloc::*` paths needs no churn.
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
**Manifests:** `crates/slippypack-core/src/format/upack_writer.rs::finalize` (the `pad_after_index` calculation and the per-tile alignment padding loop).
**Commit:** to land with the format slice-B commit.

### F-013 — `UpackWriter` state machine via single enum (`NotBegun` / `Building`)
Two states; transitions are NotBegun → Building (via begin_pack) and Building → consumed (via finalize). Each pre-build method (`add_tile_ref`, `add_extension`) checks state and returns `NotBegun` if begin_pack hasn't run. `register_byte_source` works in either state (byte sources are independent of pack metadata; SourceId is just an index into the Vec).
**Manifests:** `crates/slippypack-core/src/format/upack_writer.rs::WriterState` and the `if let WriterState::Building(state) = ...` checks in each method.
**Commit:** to land with the format slice-B commit.

### F-014 — Sources held at `UpackWriter` level (not inside `Building`)
`byte_sources: Vec<Box<dyn TileByteSource<...>>>` lives at the writer level so `register_byte_source` can run before `begin_pack`. Alternative (sources inside Building) would force begin_pack-before-register or panic on pre-begin register. The current design is more flexible and matches the trait signature (register_byte_source doesn't return Result).
**Manifests:** `crates/slippypack-core/src/format/upack_writer.rs::UpackWriter::byte_sources`.
**Commit:** to land with the format slice-B commit.

### F-015 — `add_tile_ref` validates zoom against `zoom_range` from metadata
A tile with `z < zoom_range.min` or `z > zoom_range.max` is rejected with `TileZoomOutOfRange`. Defensive: keeps the on-disk `zoom_offsets[18]` directory consistent with the header's declared range, and catches programmer mistakes early.
**Manifests:** `crates/slippypack-core/src/format/upack_writer.rs::add_tile_ref` (the `if z < min || z > max` check).
**Commit:** to land with the format slice-B commit.

### F-016 — Reader holds buffer reference; metadata/index/extensions owned
`UpackReader<'a>` borrows the original buffer (so `tile_bytes` can return `&'a [u8]` zero-copy) but owns the parsed metadata, tile index, and extension sections (parsed once at `open`). The metadata struct's UUIDs etc. are 16-byte arrays — cheap to copy on parse, not worth chasing pointer-aliasing complexity for.
**Manifests:** `crates/slippypack-core/src/format/reader.rs::UpackReader`.
**Commit:** to land with the format slice-B commit.

### F-017 — `tile_bytes` binary-search within zoom_offsets[z] range
Lookup is O(log n) per the spec's mandatory binary-search rule (PLAN.md / una-sdk PLAN.md § Index lookup). `zoom_offsets[z]` gives the offset+count of tiles at zoom z; we binary-search within that range by `(x, y)`. Linear scan would be a spec conformance failure for the watch reader; slippypack-core's reader follows the same rule for consistency and as a behavioral reference.
**Manifests:** `crates/slippypack-core/src/format/reader.rs::UpackReader::tile_bytes`.
**Commit:** to land with the format slice-B commit.

### F-018 — Spec-layout test uses raw `.upack` binary fixtures, not hex dumps
PLAN.md § Test plan called for `golden-pack-*.upack.hex` text files. Implementation uses raw binary `.upack` files instead because (a) total fixture size is ~3.2 KB (tiny — no diff-visibility benefit from hex encoding), (b) the test code is simpler with `std::fs::read` + byte-equal comparison, (c) `xxd file.upack` is one command away when a diff is needed for forensics. Fixture-bootstrap is gated behind `BLESS_SPEC_LAYOUT=1 cargo test --test spec_layout` to prevent silent drift.
**Manifests:** `crates/slippypack-core/tests/spec_layout.rs::assert_matches_golden`; `crates/slippypack-core/tests/fixtures/format/*.upack`.
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
**Manifests:** `crates/slippypack-core/tests/end_to_end.rs`; `crates/slippypack-core/tests/fixtures/e2e/{input-2x2-rgb.png, golden-png-to-pack-*.upack}`.
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

### I-001 — `SLIPPYPACK_NAMESPACE = 4e72f962-6632-4538-8e0a-7eab63350f3f`
Permanent UUIDv4 generated via `uuidgen` on macOS on 2026-05-13. Used as the seed for every UUIDv5 `pack_uuid` derivation. **Never changes** across slippypack versions — changing this value would alter every `pack_uuid` ever produced by slippypack and break the watch-side "is this pack already on the watch?" companion check.
**Manifests:** `crates/slippypack-core/src/identity.rs::SLIPPYPACK_NAMESPACE`.
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

## Cross-cutting

### X-001 — Inline `#[cfg(test)] mod tests` for module-level unit tests
Integration tests under `crates/*/tests/` are reserved for tests that exercise the format writer + reader together (per PLAN.md § Test plan). Pure-unit tests live inline alongside the module they test.
**Manifests:** `crates/slippypack-core/src/quantise.rs`, `crates/slippypack-core/src/projection/mercator.rs`.
**Commit:** established with `cdd611d`.

### X-002 — Determinism tests commit expected output bytes
Every module that produces deterministic bytes ships a `determinism_committed_output_*` test that locks the bytes against a committed expected value. Any drift fails the test and demands either a version bump (e.g. `QUANTISER_VERSION`) or a fix.
**Manifests:** `quantise::tests::determinism_committed_output_for_known_input`, `projection::mercator::tests::determinism_committed_output_for_known_coordinates`.
**Commit:** established with `cdd611d`.
