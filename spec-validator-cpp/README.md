# spec-validator-cpp

A standalone C++ utility that opens a slippypack-produced `.rawtiles` and
checks every byte against the v1 layout, independently from the Rust
writer/reader pair in `slippypack-core`.

## Why this exists

The authoritative `.rawtiles` v1.0 byte-level specification lives at
`spec/rawtiles-v1.0.md` (repo root). This validator is **a second opinion
on our own design**: it re-derives parsing from the spec doc without
calling any slippypack code, which catches bugs the Rust writer +
reader share by construction.

Two purposes:

1. **Catch writer-side bugs** (endianness, padding, offsets) that the
   Rust writer + reader pair share by construction, so they can't catch
   on their own — a writer that writes `index_offset` little-endian and
   a reader that reads `index_offset` little-endian agree even when
   both should be big-endian.

2. **Surface design questions.** If a rule is awkward to honestly
   re-derive from PLAN.md + the layout tables in a fresh codebase,
   that's a signal we should rethink the rule, not bandage the
   validator. Notes from this validator's first pass live in
   `DECISIONS.md`.

The validator re-derives byte decoding from the layout tables only;
it does NOT call slippypack code.

## Build

```sh
make
```

Produces `build/rawtiles_validate`. Requires a C++17 toolchain
(`clang++` or `g++`); no external libraries. CMake support
intentionally omitted for now — a Makefile is enough for a one-file
project.

## Usage

```sh
build/rawtiles_validate path/to/your.rawtiles
```

Exit 0 if every check passes; non-zero with reasons on stderr if not.
Warnings (v1-soft invariants) print to stderr without failing.

## Test orchestrator

```sh
make test
```

Builds slippypack in release mode, builds the validator, produces a
synthetic pack via `slippypack make --source synthetic`, validates it,
and (sanity check) verifies that mutating a byte produces a
validation failure.

## Coverage so far

The `make test` orchestrator runs the validator against six packs that
together exercise every interesting v1 layout shape:

| pack | what it stresses |
|------|------------------|
| `slippypack make --source synthetic` | the path the README points new users at |
| `golden-grid.rawtiles` (25 tiles, z=4) | largest single-zoom layout |
| `golden-pyramid.rawtiles` (21 tiles, z=2..=4) | multi-zoom `zoom_offsets[18]` directory |
| `golden-attr.rawtiles` (9 tiles + `ATTR` extension) | extension-section framing + padding |
| `golden-png-to-pack-1tile.rawtiles` | smallest non-empty pack |
| `golden-png-to-pack-5tiles.rawtiles` | end-to-end pipeline output |

All six pass independently in C++ — the byte-level format is
unambiguous enough that two implementations agree.

## What it checks

- Magic bytes `RAWT` at offset 0
- Format version (1, 0)
- `pack_uuid` non-zero; `parent_uuid` zero (reserved in v1)
- All enum-typed bytes (pixel_format, projection, addressing_scheme,
  axis_convention) accept only their v1 legal values
- Bounding box: integer microdegrees in valid ranges; min < max
- `index_offset` >= header size
- `extensions_offset` after the tile blob and ≤ file_size − 4 (CRC)
- For every tile-index entry:
  - 24-byte stride, sorted ascending by (z, x, y)
  - Reserved byte at position 3 is zero
  - Compression is 0 (only legal v1 value)
  - Flags is 0 (warn if not)
  - `z` < ZOOM_OFFSETS_COUNT (24)
  - `offset` lands inside the tile blob; `offset + length` ≤
    `extensions_offset`
- `zoom_offsets[z].count` matches the walked count; for non-empty
  zooms, `zoom_offsets[z].offset` points at the first index entry
- Extension sections: 8-byte header (tag + length), payload doesn't
  run past the footer, and the next section starts at a 4-byte
  aligned offset
- CRC-32/ISO-HDLC ("PNG/zlib") footer matches the body
