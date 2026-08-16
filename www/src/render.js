// Render a region into a .rawtiles pack, entirely in the browser.
//
// The two things in here that are not obvious, both measured in
// Docs/Investigations/2026-08-14-x4-browser-render:
//
//   1. Render in BLOCKS, not per tile. One map render per pack tile
//      takes 105.7 s for a trail-sized pack and misses the usability
//      criterion; 16x16 blocks sliced afterwards take 9.2 s for the same
//      pack. The per-tile cost is fixed overhead (~41 ms), so it does
//      not improve with caching. This is the difference between the
//      product working and not.
//
//   2. Emit in ascending (z, x, y), where x is the MAJOR axis. That is
//      how the writer sorts the tile index, and the pack's source
//      content_hash is a hash of the tile stream, so the order is part
//      of the pack's identity. Buffering a block-ROW is the intuitive
//      thing and it is wrong; the unit here is a block-COLUMN.

import { PALETTE_RGB, PALETTE_CODES } from './palette.js';
import { lon2x, lat2y, x2lon, y2lat } from './tiles.js';

/** Default pack tile size. MAP_CARTOGRAPHY_SPEC.md section 7 specifies 128
 *  for the RAM reason (a 256 px ABGR2222 tile is 64 KiB, and a 240 px
 *  viewport can straddle four).
 *
 *  It is a parameter and not a constant because the watch does not agree
 *  yet: MapKit's MapMath::TILE_DIM is 256 and its mosaic arithmetic is
 *  shifts by TILE_SHIFT = 8, so PackCatalog rejects any pack that is not
 *  256. Until one side moves, building for a real watch means passing 256. */
export const TILE_DIM = 128;

/** How many halvings from MapLibre's 512 px tile to ours. 128 → 2, 256 → 1. */
const shiftFor = (tileDim) => Math.log2(512 / tileDim);

/** Ladder zoom (the m/px figures in MAP_CARTOGRAPHY_SPEC.md section 7, which
 *  are the *256 px* tile scale) to the slippy grid level a tile of `tileDim`
 *  px must sit on to deliver it.
 *
 *  At 256 px that is the ladder's own number. At 128 px the grid shifts up
 *  one, because halving the tile at the same zoom would halve the ground
 *  resolution — the finding recorded as F2. */
export const ladderToGridLevel = (z, tileDim = TILE_DIM) => z + shiftFor(tileDim) - 1;

/** MapLibre zoom that makes one grid-level-L tile exactly `tileDim` px: the
 *  world is 512·2^Z px, so a level-L tile spans 512·2^(Z−L) px. */
const maplibreZoomFor = (level, tileDim) => level - shiftFor(tileDim);

/** A ladder range, as the grid levels to render. */
export function ladderLevels(min, max, tileDim = TILE_DIM) {
  const levels = [];
  for (let z = min; z <= max; z++) levels.push(ladderToGridLevel(z, tileDim));
  return levels;
}

export async function renderRegion({
  map,
  builder,
  bbox,           // [minLon, minLat, maxLon, maxLat]
  gridLevels,     // e.g. [13, 14, 15, 16, 17]
  blockN = 16,
  tileDim = TILE_DIM,
  signal,         // AbortSignal; checked between blocks
  onProgress = () => {},
}) {
  // A block is the smallest unit that can be abandoned: half a block is
  // half a canvas readback, and there is nothing useful to do with it.
  // So a cancel takes effect within one block — under a second at any
  // block size worth using — rather than instantly.
  const checkAborted = () => {
    if (signal?.aborted) throw signal.reason ?? new DOMException('Build cancelled', 'AbortError');
  };
  checkAborted();
  const canvas = new OffscreenCanvas(tileDim, tileDim);
  let ctx = canvas.getContext('2d', { willReadFrequently: true });
  let surface = canvas;

  const resizeTo = (w, h) => {
    if (surface.width !== w || surface.height !== h) {
      surface = new OffscreenCanvas(w, h);
      ctx = surface.getContext('2d', { willReadFrequently: true });
    }
    const el = map.getContainer();
    if (el.style.width === `${w}px` && el.style.height === `${h}px`) return;
    el.style.width = `${w}px`;
    el.style.height = `${h}px`;
    map.resize();
  };

  const idle = () => new Promise((resolve, reject) => {
    const timer = setTimeout(() => reject(new Error('map never went idle')), 60_000);
    map.once('idle', () => { clearTimeout(timer); resolve(); });
  });

  let done = 0;
  for (const level of gridLevels) {
    const zoom = maplibreZoomFor(level, tileDim);
    const x0 = Math.floor(lon2x(bbox[0], level));
    const x1 = Math.floor(lon2x(bbox[2], level));
    const y0 = Math.floor(lat2y(bbox[3], level));
    const y1 = Math.floor(lat2y(bbox[1], level));

    for (let bx = x0; bx <= x1; bx += blockN) {
      const cols = Math.min(blockN, x1 - bx + 1);
      const column = new Map(); // "x:y" -> Uint8Array(RGBA)

      for (let by = y0; by <= y1; by += blockN) {
        checkAborted();
        const rows = Math.min(blockN, y1 - by + 1);
        resizeTo(cols * tileDim, rows * tileDim);

        const settled = idle();
        map.jumpTo({
          center: [x2lon(bx + cols / 2, level), y2lat(by + rows / 2, level)],
          zoom,
        });
        await settled;

        ctx.drawImage(map.getCanvas(), 0, 0);
        const { data } = ctx.getImageData(0, 0, cols * tileDim, rows * tileDim);
        const stride = cols * tileDim * 4;

        for (let ty = 0; ty < rows; ty++) {
          for (let tx = 0; tx < cols; tx++) {
            const tile = new Uint8Array(tileDim * tileDim * 4);
            for (let py = 0; py < tileDim; py++) {
              const from = (ty * tileDim + py) * stride + tx * tileDim * 4;
              tile.set(data.subarray(from, from + tileDim * 4), py * tileDim * 4);
            }
            column.set(`${bx + tx}:${by + ty}`, tile);
          }
        }
      }

      // x-major, because that is the order the pack is sorted in.
      for (let x = bx; x < bx + cols; x++) {
        for (let y = y0; y <= y1; y++) {
          const tile = column.get(`${x}:${y}`);
          if (tile) {
            builder.add_tile_rgba(level, x, y, tile);
            done++;
          }
        }
      }
      onProgress({ level, done });
    }
  }
  return done;
}

/** Tile count for a region, for a size/time estimate before building. */
export function countTiles(bbox, gridLevels) {
  let total = 0;
  for (const level of gridLevels) {
    const nx = Math.floor(lon2x(bbox[2], level)) - Math.floor(lon2x(bbox[0], level)) + 1;
    const ny = Math.floor(lat2y(bbox[1], level)) - Math.floor(lat2y(bbox[3], level)) + 1;
    total += nx * ny;
  }
  return total;
}

export { PALETTE_RGB, PALETTE_CODES };
