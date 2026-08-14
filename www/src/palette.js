// The declared palette, from MAP_CARTOGRAPHY_SPEC.md section 3.
//
// Each entry pairs the sRGB colour the style paints with and the
// ABGR2222 byte a pixel of that colour must become. The quantiser snaps
// to the nearest of these rather than to the nearest of all 64 codes, so
// anti-aliased edges come back as codes the style declared instead of
// codes it never asked for (DECISIONS.md Q-008).
//
// ORDER IS SIGNIFICANT. It breaks ties in the nearest-slot search, so it
// must match the style's declaration order or the pack changes.
//
// `trace` (0xC3) is deliberately absent: R5 makes it app-drawn, never
// baked. `halo` == `paper` and `road_major` == `ink`, so 14 named slots
// collapse to the 11 distinct entries here.
export const SLOTS = [
  { name: 'paper',      rgb: [0xFF, 0xFF, 0xFF], code: 0xFF },
  { name: 'landuse',    rgb: [0xE9, 0xF6, 0xE8], code: 0xEE },
  { name: 'wood_lt',    rgb: [0xD0, 0xED, 0xCD], code: 0xDD },
  { name: 'building',   rgb: [0xD7, 0xD7, 0xD7], code: 0xEA },
  { name: 'wood',       rgb: [0x93, 0xCA, 0xB5], code: 0xD8 },
  { name: 'water',      rgb: [0x62, 0xB7, 0xD5], code: 0xF4 },
  { name: 'contour',    rgb: [0xA5, 0x94, 0x7A], code: 0xC5 },
  { name: 'water_dk',   rgb: [0x00, 0x84, 0xC2], code: 0xF0 },
  { name: 'road_minor', rgb: [0x87, 0x41, 0x49], code: 0xC1 },
  { name: 'path',       rgb: [0x28, 0x5B, 0x7D], code: 0xD0 },
  { name: 'ink',        rgb: [0x38, 0x38, 0x38], code: 0xC0 },
];

export const PALETTE_RGB = new Uint8Array(SLOTS.flatMap((s) => s.rgb));
export const PALETTE_CODES = new Uint8Array(SLOTS.map((s) => s.code));
