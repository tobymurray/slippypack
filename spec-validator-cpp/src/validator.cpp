// Standalone C++ validator for slippypack `.upack` files.
//
// Reads a `.upack` from disk and validates every byte against the v1
// layout. Independent from slippypack's Rust reader — re-derives byte
// decoding from the layout *tables* in `header.rs`, `tile_index.rs`,
// `extensions.rs`, and `crc.rs` without calling slippypack code.
//
// **Status: this is a second opinion on *our* design, not a check
// against an external spec.** slippypack is defining the `.upack`
// format; PLAN.md + the in-tree `*.rs` layout tables ARE the spec.
// This validator's purpose is twofold:
//
//   1. Catch writer-side bugs (endianness, padding, offsets) that
//      slippypack's writer+reader pair would miss because they share
//      logic.
//   2. Surface design issues — if a rule is awkward to implement
//      honestly in a fresh codebase, that's a signal we should
//      reconsider the rule, not just bandage it.
//
// Exit 0 → all checks passed. Non-zero → at least one check failed,
// the failing reasons are printed to stderr. Warnings (v1-soft
// invariants) print to stderr but don't fail.
//
// Layout tables consulted (NOT logic):
//   - PLAN.md § Canonical source descriptor (file layout context)
//   - crates/slippypack-core/src/format/header.rs (header byte table)
//   - crates/slippypack-core/src/format/tile_index.rs (24-byte entry layout)
//   - crates/slippypack-core/src/format/extensions.rs (tag/len framing)
//   - crates/slippypack-core/src/format/crc.rs (CRC-32/ISO-HDLC, "PNG/zlib")

#include <algorithm>
#include <array>
#include <cstdint>
#include <cstdio>
#include <cstdlib>
#include <cstring>
#include <fstream>
#include <iostream>
#include <iterator>
#include <optional>
#include <string>
#include <vector>

namespace upack {

// -------- Spec constants --------------------------------------------

constexpr std::size_t kZoomOffsetsCount = 24;
constexpr std::size_t kZoomOffsetSize = 12;  // u64 offset + u32 count
constexpr std::size_t kIndexEntrySize = 24;
// 98 fixed-field bytes + 24 * 12 zoom-directory bytes + 8 extensions_offset = 394.
constexpr std::size_t kHeaderBaseSize = 98 + kZoomOffsetsCount * kZoomOffsetSize + 8;
constexpr std::size_t kFooterCrcSize = 4;
constexpr std::size_t kSectionHeaderSize = 8;  // tag(4) + len(4)
constexpr std::array<std::uint8_t, 4> kMagic = {'U', 'P', 'C', 'K'};
constexpr std::uint8_t kFormatMajor = 1;
constexpr std::uint8_t kFormatMinor = 0;

// Header field offsets — derived from header.rs table, NOT copied from
// the writer code. If header.rs renumbers these, this validator either
// catches the change (good — that's an incompatible writer) or needs an
// update of its own (acceptable — the validator tracks the spec, not
// any particular writer).
constexpr std::size_t kOffMagic = 0;
constexpr std::size_t kOffVersion = 4;            // major then minor
constexpr std::size_t kOffPackUuid = 6;
constexpr std::size_t kOffSupersedesUuid = 22;
constexpr std::size_t kOffParentUuid = 38;
constexpr std::size_t kOffPixelFormat = 54;
constexpr std::size_t kOffProjection = 55;
constexpr std::size_t kOffAddressingScheme = 56;
constexpr std::size_t kOffAxisConvention = 57;
constexpr std::size_t kOffTileDimPx = 58;
constexpr std::size_t kOffZoomMin = 60;
constexpr std::size_t kOffZoomMax = 61;
constexpr std::size_t kOffBbox = 62;              // 4 × i32 LE
constexpr std::size_t kOffBuildTimestamp = 78;
constexpr std::size_t kOffTileCount = 86;
constexpr std::size_t kOffIndexOffset = 90;
constexpr std::size_t kOffZoomOffsets = 98;       // 24 × 12 bytes = 288
constexpr std::size_t kOffExtensionsOffset = kOffZoomOffsets + kZoomOffsetsCount * kZoomOffsetSize;

// -------- Little-endian decoders ------------------------------------

std::uint16_t le16(const std::uint8_t *p) {
  return static_cast<std::uint16_t>(
      static_cast<std::uint32_t>(p[0]) |
      (static_cast<std::uint32_t>(p[1]) << 8));
}

std::uint32_t le32(const std::uint8_t *p) {
  return static_cast<std::uint32_t>(p[0]) |
         (static_cast<std::uint32_t>(p[1]) << 8) |
         (static_cast<std::uint32_t>(p[2]) << 16) |
         (static_cast<std::uint32_t>(p[3]) << 24);
}

std::int32_t le32_signed(const std::uint8_t *p) {
  return static_cast<std::int32_t>(le32(p));
}

std::uint64_t le64(const std::uint8_t *p) {
  std::uint64_t v = 0;
  for (int i = 7; i >= 0; --i) {
    v = (v << 8) | static_cast<std::uint64_t>(p[static_cast<std::size_t>(i)]);
  }
  return v;
}

// -------- CRC-32/ISO-HDLC (PNG / zlib variant) ----------------------
//
// Polynomial: 0xEDB88320 (reflected). Init: 0xFFFFFFFF.
// XOR-out: 0xFFFFFFFF. Reflect input: yes. Reflect output: yes.
// This matches the variant slippypack writes (see crc.rs F-002).

std::array<std::uint32_t, 256> make_crc_table() {
  std::array<std::uint32_t, 256> table{};
  for (std::uint32_t i = 0; i < 256; ++i) {
    std::uint32_t c = i;
    for (int k = 0; k < 8; ++k) {
      c = (c >> 1) ^ ((c & 1u) ? 0xEDB88320u : 0u);
    }
    table[i] = c;
  }
  return table;
}

std::uint32_t crc32_iso_hdlc(const std::uint8_t *data, std::size_t len) {
  static const auto table = make_crc_table();
  std::uint32_t crc = 0xFFFFFFFFu;
  for (std::size_t i = 0; i < len; ++i) {
    crc = (crc >> 8) ^ table[(crc ^ data[i]) & 0xFFu];
  }
  return crc ^ 0xFFFFFFFFu;
}

// -------- Validation result -----------------------------------------

struct Report {
  std::vector<std::string> errors;
  std::vector<std::string> warnings;

  void error(const std::string &msg) { errors.push_back(msg); }
  void warn(const std::string &msg) { warnings.push_back(msg); }
  bool ok() const { return errors.empty(); }
};

// -------- Decoded header --------------------------------------------

struct ZoomOffset {
  std::uint64_t offset;
  std::uint32_t count;
};

struct Header {
  std::uint8_t version_major;
  std::uint8_t version_minor;
  std::array<std::uint8_t, 16> pack_uuid;
  std::array<std::uint8_t, 16> supersedes_uuid;
  std::array<std::uint8_t, 16> parent_uuid;
  std::uint8_t pixel_format;
  std::uint8_t projection;
  std::uint8_t addressing_scheme;
  std::uint8_t axis_convention;
  std::uint16_t tile_dim_px;
  std::uint8_t zoom_min;
  std::uint8_t zoom_max;
  std::int32_t bbox_min_lon_micro;
  std::int32_t bbox_min_lat_micro;
  std::int32_t bbox_max_lon_micro;
  std::int32_t bbox_max_lat_micro;
  std::uint64_t build_timestamp;
  std::uint32_t tile_count;
  std::uint64_t index_offset;
  std::array<ZoomOffset, kZoomOffsetsCount> zoom_offsets;
  std::uint64_t extensions_offset;
};

void uuid_copy(const std::uint8_t *src, std::array<std::uint8_t, 16> &dst) {
  std::copy_n(src, 16, dst.begin());
}

std::string uuid_to_hex(const std::array<std::uint8_t, 16> &uuid) {
  static const char *hex_digits = "0123456789abcdef";
  std::string out;
  out.reserve(36);
  for (std::size_t i = 0; i < 16; ++i) {
    if (i == 4 || i == 6 || i == 8 || i == 10) out.push_back('-');
    out.push_back(hex_digits[uuid[i] >> 4]);
    out.push_back(hex_digits[uuid[i] & 0x0F]);
  }
  return out;
}

Header decode_header(const std::uint8_t *bytes) {
  Header h{};
  h.version_major = bytes[kOffVersion];
  h.version_minor = bytes[kOffVersion + 1];
  uuid_copy(bytes + kOffPackUuid, h.pack_uuid);
  uuid_copy(bytes + kOffSupersedesUuid, h.supersedes_uuid);
  uuid_copy(bytes + kOffParentUuid, h.parent_uuid);
  h.pixel_format = bytes[kOffPixelFormat];
  h.projection = bytes[kOffProjection];
  h.addressing_scheme = bytes[kOffAddressingScheme];
  h.axis_convention = bytes[kOffAxisConvention];
  h.tile_dim_px = le16(bytes + kOffTileDimPx);
  h.zoom_min = bytes[kOffZoomMin];
  h.zoom_max = bytes[kOffZoomMax];
  h.bbox_min_lon_micro = le32_signed(bytes + kOffBbox + 0);
  h.bbox_min_lat_micro = le32_signed(bytes + kOffBbox + 4);
  h.bbox_max_lon_micro = le32_signed(bytes + kOffBbox + 8);
  h.bbox_max_lat_micro = le32_signed(bytes + kOffBbox + 12);
  h.build_timestamp = le64(bytes + kOffBuildTimestamp);
  h.tile_count = le32(bytes + kOffTileCount);
  h.index_offset = le64(bytes + kOffIndexOffset);
  for (std::size_t i = 0; i < kZoomOffsetsCount; ++i) {
    auto base = bytes + kOffZoomOffsets + i * kZoomOffsetSize;
    h.zoom_offsets[i].offset = le64(base);
    h.zoom_offsets[i].count = le32(base + 8);
  }
  h.extensions_offset = le64(bytes + kOffExtensionsOffset);
  return h;
}

// -------- Tile index entry ------------------------------------------

struct IndexEntry {
  std::uint8_t z;
  std::uint8_t compression;
  std::uint8_t flags;
  std::uint8_t reserved;
  std::uint32_t x;
  std::uint32_t y;
  std::uint64_t offset;
  std::uint32_t length;
};

IndexEntry decode_index_entry(const std::uint8_t *bytes) {
  IndexEntry e{};
  e.z = bytes[0];
  e.compression = bytes[1];
  e.flags = bytes[2];
  e.reserved = bytes[3];
  e.x = le32(bytes + 4);
  e.y = le32(bytes + 8);
  e.offset = le64(bytes + 12);
  e.length = le32(bytes + 20);
  return e;
}

// -------- Top-level validation --------------------------------------

void check(Report &r, bool cond, const std::string &msg) {
  if (!cond) r.error(msg);
}

void validate(const std::vector<std::uint8_t> &bytes, Report &r) {
  if (bytes.size() < kHeaderBaseSize + kFooterCrcSize) {
    r.error("file shorter than header+crc minimum (" +
            std::to_string(kHeaderBaseSize + kFooterCrcSize) + " bytes)");
    return;
  }

  // ---- Magic + version ----
  if (!std::equal(bytes.begin() + kOffMagic, bytes.begin() + kOffMagic + 4,
                  kMagic.begin())) {
    r.error("bad magic: expected 'UPCK' at offset 0");
  }
  if (bytes[kOffVersion] != kFormatMajor) {
    r.error("unsupported major version: got " +
            std::to_string(bytes[kOffVersion]) + ", expected " +
            std::to_string(kFormatMajor));
  }
  if (bytes[kOffVersion + 1] > kFormatMinor) {
    r.error("unsupported minor version: got " +
            std::to_string(bytes[kOffVersion + 1]) + ", supported up to " +
            std::to_string(kFormatMinor));
  }

  // ---- Decode + validate header invariants ----
  Header h = decode_header(bytes.data());

  // pack_uuid must be non-zero.
  bool all_zero = std::all_of(h.pack_uuid.begin(), h.pack_uuid.end(),
                              [](std::uint8_t b) { return b == 0; });
  check(r, !all_zero, "pack_uuid must not be all zero");

  // parent_uuid is reserved in v1 — MUST be zero.
  bool parent_zero = std::all_of(h.parent_uuid.begin(), h.parent_uuid.end(),
                                 [](std::uint8_t b) { return b == 0; });
  check(r, parent_zero, "parent_uuid must be zero in v1");

  check(r, h.pixel_format == 1, "pixel_format must be 1 (ABGR2222) in v1");
  check(r, h.projection == 1 || h.projection == 3,
        "projection must be 1 (Mercator) or 3 (Local Linear)");
  check(r, h.addressing_scheme == 1, "tile_addressing_scheme must be 1 (Quadtree) in v1");
  check(r, h.axis_convention == 1 || h.axis_convention == 2,
        "tile_axis_convention must be 1 (XYZ) or 2 (TMS)");
  check(r, h.tile_dim_px > 0, "tile_dim_px must be > 0");
  check(r, h.zoom_min <= h.zoom_max,
        "zoom_min must be <= zoom_max");

  // bbox coords are integer microdegrees in [-180e6, 180e6] / [-90e6, 90e6].
  // (The 85.05° Mercator-clip limit is enforced upstream of the format.)
  check(r, h.bbox_min_lon_micro >= -180'000'000 && h.bbox_max_lon_micro <= 180'000'000,
        "bbox longitudes out of [-180, 180] degrees");
  check(r, h.bbox_min_lat_micro >= -90'000'000 && h.bbox_max_lat_micro <= 90'000'000,
        "bbox latitudes out of [-90, 90] degrees");
  check(r, h.bbox_min_lon_micro < h.bbox_max_lon_micro,
        "bbox min_lon must be < max_lon");
  check(r, h.bbox_min_lat_micro < h.bbox_max_lat_micro,
        "bbox min_lat must be < max_lat");

  // index_offset must point past the header. In v1 the index sits
  // immediately after the 394-byte header (index_offset == 394); the
  // format is byte-oriented (LE-encoded with no native struct dumps),
  // so multi-byte fields don't require alignment in the file. Readers
  // that want aligned reads `memcpy` to a local before decoding.
  check(r, h.index_offset >= kHeaderBaseSize,
        "index_offset (" + std::to_string(h.index_offset) +
            ") must be >= header size (" + std::to_string(kHeaderBaseSize) + ")");

  // The tile blob starts AFTER (index_offset + 24 * tile_count), but
  // the writer pads to a 4-byte boundary first (since 322 + 24N has
  // residue 2 mod 4 for the v1 header). Verify the index region fits.
  std::uint64_t index_end = h.index_offset +
                            static_cast<std::uint64_t>(h.tile_count) * kIndexEntrySize;
  std::uint64_t tile_blob_start = (index_end + 3) & ~static_cast<std::uint64_t>(3);
  check(r, h.extensions_offset >= tile_blob_start,
        "extensions_offset must be after the tile blob");
  check(r, h.extensions_offset % 4 == 0,
        "extensions_offset must be 4-byte aligned");

  // The footer's 4-byte CRC must be at exactly file_size - 4.
  std::uint64_t file_size = bytes.size();
  check(r, h.extensions_offset <= file_size - kFooterCrcSize,
        "extensions_offset (" + std::to_string(h.extensions_offset) +
            ") plus footer (4 B) overflows file size (" +
            std::to_string(file_size) + ")");

  if (!r.ok()) return;  // further checks would deref bad offsets.

  // ---- Walk tile index ----
  std::array<std::uint32_t, kZoomOffsetsCount> per_zoom_count{};
  std::optional<IndexEntry> previous;
  for (std::uint32_t i = 0; i < h.tile_count; ++i) {
    std::uint64_t entry_off = h.index_offset + static_cast<std::uint64_t>(i) * kIndexEntrySize;
    if (entry_off + kIndexEntrySize > file_size) {
      r.error("tile index entry " + std::to_string(i) + " runs past EOF");
      return;
    }
    IndexEntry e = decode_index_entry(bytes.data() + entry_off);

    // Sort invariant: entries are in (z, x, y) ascending order.
    if (previous.has_value()) {
      auto cur = std::make_tuple(e.z, e.x, e.y);
      auto prev = std::make_tuple(previous->z, previous->x, previous->y);
      if (!(prev < cur)) {
        r.error("tile-index sort violation at entry " + std::to_string(i) +
                ": prev (z=" + std::to_string(previous->z) + ", x=" +
                std::to_string(previous->x) + ", y=" + std::to_string(previous->y) +
                ") not strictly less than cur (z=" + std::to_string(e.z) + ", x=" +
                std::to_string(e.x) + ", y=" + std::to_string(e.y) + ")");
      }
    }
    previous = e;

    // Per-tile invariants.
    if (e.reserved != 0) {
      r.error("tile index entry " + std::to_string(i) +
              ": reserved byte (position 3) must be 0, got " +
              std::to_string(e.reserved));
    }
    if (e.compression != 0) {
      r.error("tile index entry " + std::to_string(i) +
              ": compression must be 0 (none) in v1, got " +
              std::to_string(e.compression));
    }
    if (e.flags != 0) {
      // Phase 1 first slice ships with flags = 0 only; warn rather
      // than error to leave room for flag bits the spec may add later.
      r.warn("tile index entry " + std::to_string(i) +
             ": flags is " + std::to_string(e.flags) + " (expected 0 in v1)");
    }
    if (e.z >= kZoomOffsetsCount) {
      r.error("tile index entry " + std::to_string(i) +
              ": zoom " + std::to_string(e.z) +
              " >= ZOOM_OFFSETS_COUNT");
      continue;
    }

    // Tile bytes must be inside the tile blob region:
    //   tile_blob_start <= offset
    //   offset + length <= extensions_offset.
    if (e.offset < tile_blob_start) {
      r.error("tile " + std::to_string(i) + ": offset " +
              std::to_string(e.offset) + " < tile-blob start " +
              std::to_string(tile_blob_start));
    }
    if (e.offset + e.length > h.extensions_offset) {
      r.error("tile " + std::to_string(i) + ": offset+length " +
              std::to_string(e.offset + e.length) +
              " > extensions_offset " + std::to_string(h.extensions_offset));
    }

    per_zoom_count[e.z]++;
  }

  // ---- Verify zoom_offsets vs walked counts ----
  for (std::size_t z = 0; z < kZoomOffsetsCount; ++z) {
    std::uint32_t walked = per_zoom_count[z];
    std::uint32_t declared = h.zoom_offsets[z].count;
    if (walked != declared) {
      r.error("zoom " + std::to_string(z) +
              ": zoom_offsets.count = " + std::to_string(declared) +
              " but walked " + std::to_string(walked) + " entries");
    }
    // For zoom levels with entries, zoom_offsets[z].offset must point
    // to the first entry. For empty zooms, offset must be 0.
    if (walked > 0) {
      // Find the first entry at this zoom; its offset within the file
      // should equal zoom_offsets[z].offset.
      for (std::uint32_t i = 0; i < h.tile_count; ++i) {
        IndexEntry e = decode_index_entry(
            bytes.data() + h.index_offset +
            static_cast<std::uint64_t>(i) * kIndexEntrySize);
        if (e.z == z) {
          std::uint64_t expected_off =
              h.index_offset + static_cast<std::uint64_t>(i) * kIndexEntrySize;
          if (h.zoom_offsets[z].offset != expected_off) {
            r.error("zoom " + std::to_string(z) +
                    ": zoom_offsets.offset = " +
                    std::to_string(h.zoom_offsets[z].offset) +
                    " but first entry at this zoom is at file offset " +
                    std::to_string(expected_off));
          }
          break;
        }
      }
    } else if (h.zoom_offsets[z].offset != 0) {
      r.error("zoom " + std::to_string(z) +
              ": no entries but zoom_offsets.offset is non-zero (" +
              std::to_string(h.zoom_offsets[z].offset) + ")");
    }
  }

  // ---- Walk extensions ----
  std::uint64_t pos = h.extensions_offset;
  std::uint64_t footer_pos = file_size - kFooterCrcSize;
  while (pos < footer_pos) {
    if (pos + kSectionHeaderSize > footer_pos) {
      r.error("extension at offset " + std::to_string(pos) +
              ": header (8 B) runs past footer");
      break;
    }
    const std::uint8_t *p = bytes.data() + pos;
    std::array<char, 5> tag = {static_cast<char>(p[0]), static_cast<char>(p[1]),
                               static_cast<char>(p[2]), static_cast<char>(p[3]), 0};
    std::uint32_t payload_len = le32(p + 4);
    std::uint64_t payload_end = pos + kSectionHeaderSize + payload_len;
    if (payload_end > footer_pos) {
      r.error("extension '" + std::string(tag.data()) + "' at " +
              std::to_string(pos) + ": payload (" +
              std::to_string(payload_len) + " B) runs past footer");
      break;
    }
    // Padding to next 4-byte boundary.
    pos = (payload_end + 3) & ~static_cast<std::uint64_t>(3);
  }
  if (pos > footer_pos) {
    r.error("extension walk landed past footer (" + std::to_string(pos) +
            " > " + std::to_string(footer_pos) + ")");
  }

  // ---- CRC-32 footer ----
  std::uint32_t expected_crc = le32(bytes.data() + footer_pos);
  std::uint32_t computed_crc = crc32_iso_hdlc(bytes.data(), static_cast<std::size_t>(footer_pos));
  if (expected_crc != computed_crc) {
    char buf[128];
    std::snprintf(buf, sizeof(buf),
                  "CRC-32 mismatch: footer=0x%08x, computed=0x%08x",
                  expected_crc, computed_crc);
    r.error(buf);
  }

  // ---- Summary print on success ----
  if (r.ok()) {
    std::cout << "OK  " << uuid_to_hex(h.pack_uuid) << "\n"
              << "    version " << static_cast<int>(h.version_major) << "."
              << static_cast<int>(h.version_minor) << "\n"
              << "    tile_dim_px " << h.tile_dim_px << "\n"
              << "    zoom_range " << static_cast<int>(h.zoom_min) << ".."
              << static_cast<int>(h.zoom_max) << "\n"
              << "    tile_count " << h.tile_count << "\n"
              << "    file_size " << file_size << " bytes\n"
              << "    crc32 0x" << std::hex << expected_crc << std::dec << "\n";
  }
}

}  // namespace upack

int main(int argc, char **argv) {
  if (argc != 2) {
    std::fprintf(stderr, "usage: upack_validate <path.upack>\n");
    return 2;
  }
  std::ifstream in(argv[1], std::ios::binary);
  if (!in) {
    std::fprintf(stderr, "error: could not open '%s'\n", argv[1]);
    return 2;
  }
  std::vector<std::uint8_t> bytes(
      (std::istreambuf_iterator<char>(in)),
      std::istreambuf_iterator<char>());

  upack::Report r;
  upack::validate(bytes, r);

  for (const auto &w : r.warnings) {
    std::fprintf(stderr, "warning: %s\n", w.c_str());
  }
  if (!r.ok()) {
    for (const auto &e : r.errors) {
      std::fprintf(stderr, "error: %s\n", e.c_str());
    }
    return 1;
  }
  return 0;
}
