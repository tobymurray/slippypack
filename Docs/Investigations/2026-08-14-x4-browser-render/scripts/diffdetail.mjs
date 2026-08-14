// How different are the differing tiles? A handful of anti-aliased edge pixels is
// cosmetic; a whole-tile shift is a slicing bug. This measures which.
import http from 'node:http';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { firefox } from 'playwright';

const ROOT = path.dirname(fileURLToPath(import.meta.url));
const MIME = { '.html': 'text/html', '.mjs': 'text/javascript', '.js': 'text/javascript',
               '.json': 'application/json', '.css': 'text/css', '.map': 'application/json' };
const server = http.createServer((req, res) => {
  const p = path.join(ROOT, decodeURIComponent(req.url.split('?')[0]));
  if (!p.startsWith(ROOT) || !fs.existsSync(p) || fs.statSync(p).isDirectory()) {
    res.writeHead(404); return res.end('nope');
  }
  res.writeHead(200, { 'content-type': MIME[path.extname(p)] || 'application/octet-stream' });
  fs.createReadStream(p).pipe(res);
});
await new Promise(r => server.listen(0, '127.0.0.1', r));
const base = `http://127.0.0.1:${server.address().port}`;

const BBOX = [-76.015, 44.590, -75.889, 44.662];
const LEVEL = 15, D = 128;

const browser = await firefox.launch({ headless: false });
const page = await (await browser.newContext({ deviceScaleFactor: 1, viewport: { width: 2400, height: 2400 } })).newPage();
page.on('pageerror', e => console.error('PAGE ERROR:', e.message));
await page.goto(`${base}/harness.html`);
await page.waitForFunction(() => window.x4Ready === true, null, { timeout: 60000 });
await page.evaluate(url => window.x4Boot(url), `${base}/${process.env.X4_STYLE || 'watch-style.json'}`);

const grab = async cfg => {
  const r = await page.evaluate(c => window.x4Pass(c),
    { bbox: BBOX, level: LEVEL, quantise: true, hashes: 'bytes', ...cfg });
  const out = {};
  for (const [k, v] of Object.entries(r.hashes)) out[k] = Buffer.from(v, 'base64');
  return out;
};
const A = await grab({ strategy: 'tile', blockN: 1 });
const B = await grab({ strategy: 'block', blockN: 16 });

// Is the difference explained by a whole-tile translation? Try every offset in +/-2 px
// and report the best.
function bestShift(a, b) {
  let best = null;
  for (let dy = -2; dy <= 2; dy++) {
    for (let dx = -2; dx <= 2; dx++) {
      let diff = 0, counted = 0;
      for (let y = 0; y < D; y++) {
        const sy = y + dy; if (sy < 0 || sy >= D) continue;
        for (let x = 0; x < D; x++) {
          const sx = x + dx; if (sx < 0 || sx >= D) continue;
          counted++;
          if (a[y * D + x] !== b[sy * D + sx]) diff++;
        }
      }
      const pct = 100 * diff / counted;
      if (!best || pct < best.pct) best = { dx, dy, pct };
    }
  }
  return best;
}

const keys = Object.keys(A);
let differing = 0;
const stats = [];
for (const k of keys) {
  let d = 0;
  for (let i = 0; i < A[k].length; i++) if (A[k][i] !== B[k][i]) d++;
  if (d === 0) continue;
  differing++;
  stats.push({ k, pct: 100 * d / A[k].length, shift: bestShift(A[k], B[k]) });
}
stats.sort((a, b) => b.pct - a.pct);
const med = stats.length ? stats[Math.floor(stats.length / 2)].pct : 0;
console.log(`${differing}/${keys.length} tiles differ`);
console.log(`pixels differing, of 16384: median ${med.toFixed(2)} %, ` +
  `worst ${stats[0]?.pct.toFixed(2)} %, best ${stats[stats.length - 1]?.pct.toFixed(2)} %`);
const shifts = {};
for (const s of stats) {
  const key = `${s.shift.dx},${s.shift.dy}`;
  shifts[key] = shifts[key] || { n: 0, resid: 0 };
  shifts[key].n++; shifts[key].resid += s.shift.pct;
}
console.log('best-fit whole-tile shift (dx,dy) -> count, mean residual % after shifting:');
for (const [k, v] of Object.entries(shifts).sort((a, b) => b[1].n - a[1].n))
  console.log(`  (${k}) -> ${v.n} tiles, residual ${(v.resid / v.n).toFixed(2)} %`);

await browser.close();
server.close();
