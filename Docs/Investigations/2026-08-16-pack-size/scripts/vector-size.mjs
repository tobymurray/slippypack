// How big is the SAME region as vector tiles? Read straight from the
// Protomaps archive the raster build already renders from, so this is a
// like-for-like comparison of source data, not an estimate.
// Run from www/ (npm i pmtiles), or point this at the vendored copy.
import { PMTiles } from 'pmtiles';

const URL_ = 'https://data.source.coop/protomaps/openstreetmap/v4.pmtiles';
const bbox = [-76.26755, 44.40142, -75.63645, 44.85058]; // the 25 km pack
const lon2x = (lon, z) => Math.floor(((lon + 180) / 360) * 2 ** z);
const lat2y = (lat, z) => {
  const r = (lat * Math.PI) / 180;
  return Math.floor(((1 - Math.log(Math.tan(r) + 1 / Math.cos(r)) / Math.PI) / 2) * 2 ** z);
};

const p = new PMTiles(URL_);
const header = await p.getHeader();
console.log(`archive: max zoom ${header.maxZoom}, tile type ${header.tileType} (1 = MVT), compression ${header.tileCompression}`);

for (const z of [12, 13, 14]) {
  const [x0, x1] = [lon2x(bbox[0], z), lon2x(bbox[2], z)];
  const [y0, y1] = [lat2y(bbox[3], z), lat2y(bbox[1], z)];
  const total = (x1 - x0 + 1) * (y1 - y0 + 1);
  // Sample across the region rather than fetching all of it.
  const picks = [];
  const stride = Math.max(1, Math.floor(Math.sqrt(total / 24)));
  for (let x = x0; x <= x1; x += stride) for (let y = y0; y <= y1; y += stride) picks.push([x, y]);
  let bytes = 0, got = 0;
  for (const [x, y] of picks) {
    const t = await p.getZxy(z, x, y);
    if (t?.data) { bytes += t.data.byteLength; got++; }
  }
  const mean = got ? bytes / got : 0;
  console.log(`z${z}: ${total} tiles over the region, sampled ${got}, mean ${(mean / 1024).toFixed(1)} KB`
    + ` -> ${(mean * total / 1e6).toFixed(1)} MB for the whole region`);
}
