// What a build will cost, worked out before anyone presses the button.
//
// Two numbers decide whether a region is buildable, and they are not the
// same number:
//
//   1. The PACK. Every tile stays in WASM memory until finish(), which
//      then copies the whole thing out into a JS Uint8Array — so the
//      peak is about twice the pack.
//
//   2. The BLOCK COLUMN, which is bigger and less obvious. render.js
//      buffers a whole block-column of *raw RGBA* before it can emit
//      x-major, so the peak is `blockN x ny` tiles at 4 bytes a pixel.
//      For a 25 km region at 256 px that is ~480 MB while the pack
//      itself is ~100 MB. The column, not the pack, is what kills a tab.
//
// The column scales with blockN, so this picks the block size rather
// than refusing outright: 16 where it fits, 8 or 4 where it doesn't.
// That choice is made from the region's geometry alone — never from
// device memory or anything else that varies per machine — because the
// block size changes the rendered pixels (X4, F3) and is therefore part
// of the pack's identity. Same inputs, same pack, any browser.

import { lon2x, lat2y } from './tiles.js';

/** Block sizes to try, largest first. 16 is the fast one the X4
 *  investigation measured; smaller only buys headroom. */
export const BLOCK_SIZES = [16, 8, 4];

const MiB = 1024 * 1024;
const COLUMN_BUDGET = 128 * MiB;   // shrink the block size to stay under
const COLUMN_CEILING = 384 * MiB;  // refuse past here, at any block size
const PACK_CEILING = 256 * MiB;    // ~512 MB peak across finish()
const PACK_WARN = 96 * MiB;

// The buffers are alive at the same time, and budgeting them separately is
// how a build that satisfies every individual limit still kills the tab.
// Measured: a 687-tile uncompressed 256 px pack crashed Firefox with a
// 128 MB column, its 67 MB ImageData and 90 MB of pack-and-copy -- each
// under its own ceiling, ~246 MiB together -- so the budget sits below
// that, not at it. The same region as RLE8, same
// canvas, same block size, built fine, which is what pinned it on the sum
// rather than on the canvas.
const TOTAL_BUDGET = 192 * MiB;    // shrink the block size to stay under
const TOTAL_CEILING = 384 * MiB;   // refuse past here, at any block size

/** Live bytes at the worst moment: the block column and the block's own
 *  ImageData, plus the pack in WASM and the copy finish() makes of it. */
const peakTotalFor = (columnBytes, packBytes) => 2 * columnBytes + 2 * packBytes;
const SLOW = 300; // seconds; past this, say so before they commit

/** Starting rates, replaced by measurements from this browser once it has
 *  built something big enough to measure.
 *
 *  Time is charged **per pixel**, not per tile, which is the correction
 *  that matters here: the same region at 128 px has four times the tiles
 *  of one at 256 px and draws exactly the same pixels, so a per-tile
 *  model quotes it at four times the truth. Fitted to two measured
 *  builds 29x apart in pixels: 2,467 tiles at 128 px in 18.5 s, and
 *  18,169 tiles at 256 px in 617.7 s.
 *
 *  `msPerRender` is what a render costs before any pixels: the jump, the
 *  resize, and the wait for the map to go idle. At 210 ms it is five
 *  times the ~41 ms X4 measured — that figure came from 128 px canvases,
 *  and this cost grows with the canvas being settled.
 *
 *  `packedFraction` is what RLE8 leaves: 65 MB of a raw 1,191 MB on the
 *  18,169-tile build, and 2.26 MB of 40 MB on the 2,467-tile one. The two
 *  agree at 0.055 across a 4x difference in tile size, which is a better
 *  constant than either alone. */
const DEFAULT_RATES = { msPerMegapixel: 313, msPerRender: 210, packedFraction: 0.055 };
const RATES_KEY = 'slippypack.rates.v2';

/** Spinning up the render map and its first tiles, before any pack tile
 *  exists. Charged once per build, so it has to be its own term — folded
 *  into the per-tile rate it would make small builds look catastrophic. */
const STARTUP_MS = 2500;

/** Under this, a build is mostly startup and measures nothing useful
 *  about the steady state. Learning from one would poison every estimate
 *  after it: a three-second build reads as six times the true rate. */
const LEARNABLE_SECONDS = 10;

/** Raw ABGR2222 bytes per tile: one byte a pixel. ABGR2222 is 2 bits per
 *  *channel* across four channels, so a 256 px tile is 64 KiB, not 16. */
export const rawBytesPerTile = (tileDim) => tileDim * tileDim;

/** Pixels are the unit that actually costs time. */
const megapixels = (tiles, tileDim) => (tiles * tileDim * tileDim) / 1e6;

/** The tile grid a region covers at one level. */
function extent(bbox, level) {
  const x0 = Math.floor(lon2x(bbox[0], level));
  const x1 = Math.floor(lon2x(bbox[2], level));
  const y0 = Math.floor(lat2y(bbox[3], level));
  const y1 = Math.floor(lat2y(bbox[1], level));
  return { x0, x1, y0, y1, nx: x1 - x0 + 1, ny: y1 - y0 + 1 };
}

/** Peak block-column bytes, render count and per-level breakdown for one
 *  block size. The breakdown is what makes a live ETA possible: tiles
 *  arrive shallowest-first, and a shallow level costs far more per tile
 *  than a deep one, so progress measured in tiles is not progress
 *  measured in work. */
function shape(bbox, gridLevels, tileDim, blockN) {
  let peakBytes = 0;
  let renders = 0;
  let tiles = 0;
  const perLevel = [];
  for (const level of gridLevels) {
    const { nx, ny } = extent(bbox, level);
    const levelRenders = Math.ceil(nx / blockN) * Math.ceil(ny / blockN);
    perLevel.push({ level, tiles: nx * ny, renders: levelRenders });
    tiles += nx * ny;
    renders += levelRenders;
    peakBytes = Math.max(peakBytes, Math.min(blockN, nx) * ny * tileDim * tileDim * 4);
  }
  return { peakBytes, renders, tiles, perLevel };
}

export function loadRates() {
  try {
    return { ...DEFAULT_RATES, ...JSON.parse(localStorage.getItem(RATES_KEY) ?? '{}') };
  } catch {
    return { ...DEFAULT_RATES };
  }
}

/** Fold a finished build into the rates, so the next estimate is this
 *  machine's rather than the reference machine's. Half-weighted, so one
 *  odd build cannot swing it. */
export function recordBuild({ tiles, renders, seconds, bytes, tileDim, compression = 'rle8' }) {
  if (!tiles || seconds < LEARNABLE_SECONDS) return;
  // An uncompressed build measures the renderer, not the compressor; its
  // "fraction" is 1 by construction and would wreck the learned one.
  const learnSize = compression !== 'none';
  const rates = loadRates();
  const mix = (was, now) => (was + now) / 2;
  const perMegapixel = Math.max(
    10,
    (seconds * 1000 - STARTUP_MS - renders * rates.msPerRender) / megapixels(tiles, tileDim),
  );
  const next = {
    ...rates,
    msPerMegapixel: mix(rates.msPerMegapixel, perMegapixel),
    packedFraction: learnSize
      ? mix(rates.packedFraction, bytes / (tiles * rawBytesPerTile(tileDim)))
      : rates.packedFraction,
  };
  try {
    localStorage.setItem(RATES_KEY, JSON.stringify(next));
  } catch {
    /* private mode, or a full quota — estimates just stay generic */
  }
  return next;
}

/**
 * Cost of building `bbox` over `gridLevels`, and whether to allow it.
 *
 * Returns the chosen `blockN` alongside the numbers, because the caller
 * has to render with the same one the estimate was made for.
 */
export function estimate({ bbox, gridLevels, tileDim, compression = 'rle8', rates = loadRates() }) {
  if (!gridLevels.length || bbox[2] <= bbox[0] || bbox[3] <= bbox[1]) {
    return { tiles: 0, verdict: 'empty', reason: 'Drag on the map to choose an area.' };
  }

  // An uncompressed pack is exactly its raw size. Charging it the RLE8
  // fraction would under-quote by 18x, and the ceilings below are the only
  // thing standing between a careless drag and a 193 MB pack.
  const fraction = compression === 'none' ? 1 : rates.packedFraction;
  const sized = BLOCK_SIZES.map((blockN) => ({ blockN, ...shape(bbox, gridLevels, tileDim, blockN) }));
  const bytes = Math.round(sized[0].tiles * rawBytesPerTile(tileDim) * fraction);

  // Largest block size that fits both budgets; the smallest one if none do,
  // so that the ceilings below are what refuses.
  const fits = (s) => s.peakBytes <= COLUMN_BUDGET
    && peakTotalFor(s.peakBytes, bytes) <= TOTAL_BUDGET;
  const chosen = sized.find(fits) ?? sized.at(-1);

  const { tiles, renders, peakBytes, blockN } = chosen;
  const peakTotal = peakTotalFor(peakBytes, bytes);
  const cost = (l) => megapixels(l.tiles, tileDim) * rates.msPerMegapixel
    + l.renders * rates.msPerRender;
  const work = chosen.perLevel.map((l) => ({ ...l, ms: cost(l) }));
  const workMs = work.reduce((sum, l) => sum + l.ms, 0);
  const seconds = (STARTUP_MS + workMs) / 1000;

  let verdict = 'ok';
  let reason = '';
  if (peakTotal > TOTAL_CEILING) {
    verdict = 'refuse';
    reason = `This needs about ${formatBytes(peakTotal)} of memory at once — the tiles `
      + `being rendered and the pack being assembled, both live. Lower the top zoom, `
      + `draw a smaller area, or use RLE8 if the reader on the other end can take it.`;
  } else if (peakBytes > COLUMN_CEILING) {
    verdict = 'refuse';
    reason = `Rendering this needs ${formatBytes(peakBytes)} of working memory in one go. `
      + `Lower the top zoom, or draw a shorter area — height costs more than width.`;
  } else if (bytes > PACK_CEILING) {
    verdict = 'refuse';
    reason = `A pack this size (${formatBytes(bytes)}) has to be held in memory whole. `
      + `Lower the top zoom, or split the region into two packs.`;
  } else if (bytes > PACK_WARN || peakBytes > COLUMN_BUDGET || seconds > SLOW) {
    verdict = 'heavy';
    reason = 'Large build. Leave this tab in the foreground until it finishes.';
  }

  // Why a build is slow is worth saying out loud: the block size is the
  // whole reason, and it is a memory decision the user never made. The
  // fix is theirs to choose — a shorter area or a lower top zoom both
  // buy the fast block size back.
  if (verdict !== 'refuse' && blockN < BLOCK_SIZES[0]) {
    reason += `${reason ? ' ' : ''}Rendering in ${blockN}-tile blocks to keep memory `
      + `under ${formatBytes(COLUMN_BUDGET)}, which is most of the time above. `
      + 'A shorter area, or one zoom less at the top, renders in 16-tile blocks.';
  }

  return { tiles, renders, blockN, bytes, seconds, peakBytes, peakTotal, verdict, reason, work, workMs };
}

/**
 * Predicted milliseconds of work represented by `done` tiles, using the
 * per-level breakdown from `estimate`. Tiles are emitted level by level
 * in ascending order, so a cumulative count places itself.
 *
 * Divide the real elapsed time by this to get how much slower or faster
 * this machine is than the model, which is what turns the model into an
 * ETA that holds up in the first ten seconds.
 */
export function workDone(work, done) {
  let tiles = 0;
  let ms = 0;
  for (const level of work) {
    if (done <= tiles + level.tiles) {
      return ms + (level.tiles ? ((done - tiles) / level.tiles) * level.ms : 0);
    }
    tiles += level.tiles;
    ms += level.ms;
  }
  return ms;
}

export function formatBytes(bytes) {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < MiB) return `${(bytes / 1024).toFixed(0)} KB`;
  if (bytes < 1024 * MiB) return `${(bytes / MiB).toFixed(bytes < 10 * MiB ? 1 : 0)} MB`;
  return `${(bytes / (1024 * MiB)).toFixed(1)} GB`;
}

export function formatDuration(seconds) {
  if (seconds < 1) return '<1 s';
  if (seconds < 90) return `${Math.round(seconds)} s`;
  if (seconds < 3600) return `${Math.round(seconds / 60)} min`;
  return `${(seconds / 3600).toFixed(1)} h`;
}
