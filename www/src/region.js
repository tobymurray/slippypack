// The region picker: an interactive map you drag a box on.
//
// It renders with watch-style.json — the same style the pack is cut
// from — so what you are looking at while you choose is what the watch
// will show. That is also why the picker exists on the page at all: a
// bbox typed as four numbers is not a thing anyone can check.
//
// The selection is drawn twice over, because two rectangles are true at
// once: the box you dragged, and the tile-aligned extent the pack will
// actually cover, which is a whole number of tiles and therefore always
// a little larger. Drawing the tile grid says both at the same time.
//
// The map is not the accessible path to a region — a drag has no
// keyboard equivalent here. The bbox field beside it is, and it stays
// the value of record: the picker writes to it, and typing in it moves
// the picker.

import * as maplibregl from 'maplibre-gl';
import { lon2x, lat2y, x2lon, y2lat } from './tiles.js';
import { SLOTS } from './palette.js';

/** The page paints itself out of the pack's own palette — these are the
 *  same eleven colours a tile is allowed to contain. */
const slot = (name) => {
  const { rgb } = SLOTS.find((s) => s.name === name);
  return `#${rgb.map((v) => v.toString(16).padStart(2, '0')).join('')}`;
};

/** Past this many tiles the lines fall closer together than they can be
 *  drawn, half of them alias away, and the result misinforms. Better no
 *  grid than a grid that is lying about its spacing. */
const MAX_GRID_TILES = 400;

/** Microdegrees are the descriptor's unit (`to_micro` in slippypack-web),
 *  so a coordinate finer than six decimals cannot survive into the pack
 *  anyway. Round here, and the field shows what the pack will record. */
const round6 = (n) => Math.round(n * 1e6) / 1e6;

const rect = ([w, s, e, n]) => ({
  type: 'Feature',
  geometry: { type: 'Polygon', coordinates: [[[w, s], [e, s], [e, n], [w, n], [w, s]]] },
});

const empty = { type: 'FeatureCollection', features: [] };

/** Tile-boundary lines for the pack's extent, at `level`. */
function gridLines(bbox, level) {
  const x0 = Math.floor(lon2x(bbox[0], level));
  const x1 = Math.floor(lon2x(bbox[2], level)) + 1;
  const y0 = Math.floor(lat2y(bbox[3], level));
  const y1 = Math.floor(lat2y(bbox[1], level)) + 1;
  if ((x1 - x0) * (y1 - y0) > MAX_GRID_TILES) return null;

  const [w, e] = [x2lon(x0, level), x2lon(x1, level)];
  const [n, s] = [y2lat(y0, level), y2lat(y1, level)];
  const lines = [];
  for (let x = x0; x <= x1; x++) {
    const lon = x2lon(x, level);
    lines.push([[lon, s], [lon, n]]);
  }
  for (let y = y0; y <= y1; y++) {
    const lat = y2lat(y, level);
    lines.push([[w, lat], [e, lat]]);
  }
  return { type: 'Feature', geometry: { type: 'MultiLineString', coordinates: lines } };
}

/**
 * @param container  element id for the map
 * @param style      parsed MapLibre style object
 * @param bbox       initial [minLon, minLat, maxLon, maxLat]
 * @param onSelect   called with a rounded bbox when a drag commits
 */
export function createPicker({ container, style, bbox, onSelect = () => {} }) {
  const map = new maplibregl.Map({
    container,
    style,
    bounds: [[bbox[0], bbox[1]], [bbox[2], bbox[3]]],
    fitBoundsOptions: { padding: 48 },
    boxZoom: false, // shift-drag belongs to the selection, not to zooming
    attributionControl: false, // the page shows it in full, always visible
  });
  map.addControl(new maplibregl.NavigationControl({ showCompass: false }), 'top-right');

  let current = bbox.slice();
  let levels = [];
  let armed = false;
  let ready = false;

  map.on('load', () => {
    map.addSource('extent', { type: 'geojson', data: empty });
    map.addSource('selection', { type: 'geojson', data: empty });
    map.addSource('grid', { type: 'geojson', data: empty });

    map.addLayer({
      id: 'extent-fill',
      type: 'fill',
      source: 'extent',
      paint: { 'fill-color': slot('water'), 'fill-opacity': 0.12 },
    });
    map.addLayer({
      id: 'grid-line',
      type: 'line',
      source: 'grid',
      paint: { 'line-color': slot('water_dk'), 'line-opacity': 0.45, 'line-width': 1 },
    });
    map.addLayer({
      id: 'selection-line',
      type: 'line',
      source: 'selection',
      paint: { 'line-color': slot('water_dk'), 'line-width': 1.75 },
    });
    ready = true;
    draw();
    // Fit again now the container has been through layout. The fit at
    // construction runs against whatever size the element had before the
    // grid sized it, which is not the size it ends up.
    map.resize();
    fit();
  });

  const fit = () => map.fitBounds(
    [[current[0], current[1]], [current[2], current[3]]],
    { padding: 48, animate: false },
  );

  /** The grid is the ladder's *coarsest* level, not its deepest. Those
   *  are the biggest tiles in the pack, so they are the ones that reach
   *  furthest past the box you drew — which is the thing the grid is
   *  here to show. A deeper grid would be finer, prettier, and would
   *  understate the overshoot. */
  function drawableLevel() {
    const coarsest = Math.min(...levels);
    return Number.isFinite(coarsest) && gridLines(current, coarsest) ? coarsest : null;
  }

  function draw() {
    if (!ready) return;
    const level = drawableLevel();
    const grid = level === null ? null : gridLines(current, level);
    map.getSource('selection').setData(rect(current));
    map.getSource('grid').setData(grid ?? empty);
    // The pack covers whole tiles, so its extent is the grid's outer edge
    // — or the selection itself when the grid is too fine to draw.
    map.getSource('extent').setData(grid ? bboxOfGrid(current, level) : rect(current));
  }

  function bboxOfGrid(box, lvl) {
    const w = x2lon(Math.floor(lon2x(box[0], lvl)), lvl);
    const e = x2lon(Math.floor(lon2x(box[2], lvl)) + 1, lvl);
    const n = y2lat(Math.floor(lat2y(box[3], lvl)), lvl);
    const s = y2lat(Math.floor(lat2y(box[1], lvl)) + 1, lvl);
    return rect([w, s, e, n]);
  }

  // --- drag to select -----------------------------------------------

  const canvas = () => map.getCanvasContainer();

  const boxBetween = (a, b) => [
    round6(Math.min(a.lng, b.lng)), round6(Math.min(a.lat, b.lat)),
    round6(Math.max(a.lng, b.lng)), round6(Math.max(a.lat, b.lat)),
  ];

  function onStart(e) {
    if (!armed && !e.originalEvent.shiftKey) return;
    e.preventDefault();
    const from = e.lngLat;
    const before = current;
    let moved = false;
    map.dragPan.disable();

    const onMove = (ev) => {
      moved = true;
      current = boxBetween(from, ev.lngLat);
      draw();
    };
    const onEnd = () => {
      map.off('mousemove', onMove);
      map.off('touchmove', onMove);
      map.off('mouseup', onEnd);
      map.off('touchend', onEnd);
      window.removeEventListener('mouseup', onEnd);
      map.dragPan.enable();
      setArmed(false);
      // A click with no drag is not a selection; keep what was there.
      if (moved) onSelect(current);
      else { current = before; draw(); }
    };

    map.on('mousemove', onMove);
    map.on('touchmove', onMove);
    map.on('mouseup', onEnd);
    map.on('touchend', onEnd);
    window.addEventListener('mouseup', onEnd, { once: true });
  }

  map.on('mousedown', onStart);
  map.on('touchstart', onStart);

  function setArmed(next) {
    armed = next;
    canvas().style.cursor = next ? 'crosshair' : '';
    canvas().classList.toggle('arming', next);
  }

  return {
    map,
    /** Move the selection from outside — the bbox field, or a preset. */
    setBbox(next, { zoomTo = false } = {}) {
      current = next.map(round6);
      draw();
      if (zoomTo && ready) fit();
    },
    /** The ladder's grid levels. The picker draws the deepest one whose
     *  lines are still distinguishable, and no grid at all past that. */
    setLevels(next) {
      levels = next;
      draw();
    },
    arm() { setArmed(true); },
    get armed() { return armed; },
  };
}
