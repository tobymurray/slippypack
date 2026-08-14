// End-to-end check: drive the page in a real browser, build a pack, and
// write it to disk so the Rust CLI can be the one that says whether it
// is valid. Run twice to prove the build is reproducible.
//
//   node scripts/verify-e2e.mjs [--out ../target/browser.rawtiles]
//   cargo run -p slippypack-cli -- inspect ../target/browser.rawtiles
import http from 'node:http';
import fs from 'node:fs';
import path from 'node:path';
import zlib from 'node:zlib';
import { fileURLToPath } from 'node:url';
import { firefox } from 'playwright';

const HERE = path.dirname(fileURLToPath(import.meta.url));
// --root lets this verify the assembled dist/ as well as the source
// tree, so what gets deployed is what gets tested.
const ROOT = path.resolve(HERE, '..', process.argv.includes('--root') ? process.argv[process.argv.indexOf('--root') + 1] : '.');
const MIME = {
  '.html': 'text/html', '.mjs': 'text/javascript', '.js': 'text/javascript',
  '.json': 'application/json', '.css': 'text/css', '.wasm': 'application/wasm',
  '.map': 'application/json',
};

const arg = (name, fallback) => {
  const i = process.argv.indexOf(`--${name}`);
  return i === -1 ? fallback : process.argv[i + 1];
};
const OUT = path.resolve(HERE, '..', arg('out', '../target/browser.rawtiles'));
const BBOX = arg('bbox', '-76.015,44.590,-75.889,44.662').split(',').map(Number);
const LADDER = arg('ladder', '12-16').split('-').map(Number);

if (!fs.existsSync(path.join(ROOT, 'pkg', 'slippypack_web.js'))) {
  console.error(`pkg/ is missing under ${ROOT} — run scripts/build-wasm.sh first`);
  process.exit(1);
}

const server = http.createServer((req, res) => {
  const file = path.join(ROOT, decodeURIComponent(req.url.split('?')[0]));
  if (!file.startsWith(ROOT) || !fs.existsSync(file) || fs.statSync(file).isDirectory()) {
    res.writeHead(404);
    return res.end('not found');
  }
  res.writeHead(200, { 'content-type': MIME[path.extname(file)] || 'application/octet-stream' });
  fs.createReadStream(file).pipe(res);
});
await new Promise((r) => server.listen(0, '127.0.0.1', r));
const base = `http://127.0.0.1:${server.address().port}`;

const browser = await firefox.launch({ headless: false });
const page = await (
  await browser.newContext({ deviceScaleFactor: 1, viewport: { width: 2400, height: 2400 } })
).newPage();
page.on('pageerror', (e) => console.error('PAGE ERROR:', e.message));
page.on('console', (m) => { if (m.type() === 'error') console.error('CONSOLE:', m.text()); });

await page.goto(`${base}/index.html`);

// Drive the page's own modules rather than a private copy of them, so
// this verifies what a user would actually run.
const result = await page.evaluate(async ({ bbox, ladder, origin }) => {
  // Bare specifiers here resolve through index.html's import map.
  const maplibregl = await import('maplibre-gl');
  const { Protocol } = await import('pmtiles');
  const wasm = await import(`${origin}/pkg/slippypack_web.js`);
  const { renderRegion, ladderToGridLevel, PALETTE_RGB, PALETTE_CODES } =
    await import(`${origin}/src/render.js`);
  const { sha256 } = await import(`${origin}/src/tiles.js`);

  maplibregl.addProtocol('pmtiles', new Protocol().tile);
  await wasm.default(`${origin}/pkg/slippypack_web_bg.wasm`);

  const styleText = await (await fetch(`${origin}/watch-style.json`)).text();
  const styleHash = await sha256(styleText);
  const gridLevels = [];
  for (let z = ladder[0]; z <= ladder[1]; z++) gridLevels.push(ladderToGridLevel(z));

  const map = new maplibregl.Map({
    container: 'map',
    style: JSON.parse(styleText),
    center: [(bbox[0] + bbox[2]) / 2, (bbox[1] + bbox[3]) / 2],
    zoom: 2,
    preserveDrawingBuffer: true,
    fadeDuration: 0,
    pixelRatio: 1,
    interactive: false,
    attributionControl: false,
  });
  await new Promise((r) => map.once('load', r));

  const builder = new wasm.PackBuilder(
    128, gridLevels[0], gridLevels.at(-1),
    new Float64Array(bbox), PALETTE_RGB, PALETTE_CODES, styleHash,
    'Map data from OpenStreetMap (ODbL) · basemap © Protomaps',
    1_760_000_000, // pinned so two runs are comparable
  );

  const started = performance.now();
  const tiles = await renderRegion({ map, builder, bbox, gridLevels });
  const contentHash = [...builder.rendered_content_hash()];
  const bytes = builder.finish();
  return {
    pack: [...bytes],
    tiles,
    contentHash,
    styleHash: [...styleHash],
    seconds: (performance.now() - started) / 1000,
  };
}, { bbox: BBOX, ladder: LADDER, origin: base });

fs.mkdirSync(path.dirname(OUT), { recursive: true });
fs.writeFileSync(OUT, Buffer.from(result.pack));

// The footer CRC is what MapManager verifies on the watch, so check it
// here rather than finding out over USB.
const body = Buffer.from(result.pack.slice(0, -4));
const declared = Buffer.from(result.pack.slice(-4)).readUInt32LE(0);
const actual = zlib.crc32(body) >>> 0;

console.log(`tiles        : ${result.tiles.toLocaleString()}`);
console.log(`seconds      : ${result.seconds.toFixed(1)}`);
console.log(`bytes        : ${result.pack.length.toLocaleString()}`);
console.log(`content_hash : ${Buffer.from(result.contentHash).toString('hex')}`);
console.log(`style_hash   : ${Buffer.from(result.styleHash).toString('hex')}`);
console.log(`footer crc32 : 0x${declared.toString(16).padStart(8, '0')} over ${body.length} bytes — ${declared === actual ? 'MATCH' : 'MISMATCH'}`);
console.log(`written      : ${OUT}`);

await browser.close();
server.close();
