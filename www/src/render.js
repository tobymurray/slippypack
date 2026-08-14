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

export const TILE_DIM = 128;

/** Ladder zoom (the m/px figures in MAP_CARTOGRAPHY_SPEC.md section 7,
 *  which are the 256 px tile scale) to the slippy grid level a 128 px
 *  tile must sit on to deliver it. */
export const ladderToGridLevel = (z) => z + 1;

/** MapLibre zoom that makes one grid-level-L tile exactly TILE_DIM px:
 *  the world is 512·2^Z px, so a level-L tile spans 512·2^(Z−L) px. */
const maplibreZoomFor = (level) => level - 2;

export async function renderRegion({
  map,
  builder,
  bbox,           // [minLon, minLat, maxLon, maxLat]
  gridLevels,     // e.g. [13, 14, 15, 16, 17]
  blockN = 16,
  onProgress = () => {},
}) {
  const canvas = new OffscreenCanvas(TILE_DIM, TILE_DIM);
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
    const zoom = maplibreZoomFor(level);
    const x0 = Math.floor(lon2x(bbox[0], level));
    const x1 = Math.floor(lon2x(bbox[2], level));
    const y0 = Math.floor(lat2y(bbox[3], level));
    const y1 = Math.floor(lat2y(bbox[1], level));

    for (let bx = x0; bx <= x1; bx += blockN) {
      const cols = Math.min(blockN, x1 - bx + 1);
      const column = new Map(); // "x:y" -> Uint8Array(RGBA)

      for (let by = y0; by <= y1; by += blockN) {
        const rows = Math.min(blockN, y1 - by + 1);
        resizeTo(cols * TILE_DIM, rows * TILE_DIM);

        const settled = idle();
        map.jumpTo({
          center: [x2lon(bx + cols / 2, level), y2lat(by + rows / 2, level)],
          zoom,
        });
        await settled;

        ctx.drawImage(map.getCanvas(), 0, 0);
        const { data } = ctx.getImageData(0, 0, cols * TILE_DIM, rows * TILE_DIM);
        const stride = cols * TILE_DIM * 4;

        for (let ty = 0; ty < rows; ty++) {
          for (let tx = 0; tx < cols; tx++) {
            const tile = new Uint8Array(TILE_DIM * TILE_DIM * 4);
            for (let py = 0; py < TILE_DIM; py++) {
              const from = (ty * TILE_DIM + py) * stride + tx * TILE_DIM * 4;
              tile.set(data.subarray(from, from + TILE_DIM * 4), py * TILE_DIM * 4);
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
