// Does block-and-slice produce the same pack tiles as rendering each tile alone?
// The 19x speedup is only real if the answer is yes.
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
const LEVEL = 15;

const browser = await firefox.launch({ headless: false });
const page = await (await browser.newContext({ deviceScaleFactor: 1, viewport: { width: 2400, height: 2400 } })).newPage();
page.on('pageerror', e => console.error('PAGE ERROR:', e.message));
await page.goto(`${base}/harness.html`);
await page.waitForFunction(() => window.x4Ready === true, null, { timeout: 60000 });
const STYLE = process.env.X4_STYLE || 'watch-style.json';
console.log(`style: ${STYLE}`);
await page.evaluate(url => window.x4Boot(url), `${base}/${STYLE}`);

const runs = {};
for (const [name, cfg] of [
  ['tile',   { strategy: 'tile',  blockN: 1 }],
  ['block4', { strategy: 'block', blockN: 4 }],
  ['block16',{ strategy: 'block', blockN: 16 }],
]) {
  const r = await page.evaluate(c => window.x4Pass(c),
    { bbox: BBOX, level: LEVEL, quantise: true, hashes: true, ...cfg });
  runs[name] = r.hashes;
  console.log(`${name}: ${Object.keys(r.hashes).length} tiles, rle ${r.rlePct.toFixed(2)}%`);
}

const keys = Object.keys(runs.tile);
for (const other of ['block4', 'block16']) {
  const missing = keys.filter(k => !(k in runs[other]));
  const differ = keys.filter(k => k in runs[other] && runs[other][k] !== runs.tile[k]);
  console.log(`tile vs ${other}: ${keys.length} compared, ${missing.length} missing, ` +
    `${differ.length} differing (${(100 * differ.length / keys.length).toFixed(1)}%)`);
  if (differ.length) console.log('  e.g.', differ.slice(0, 5).join(', '));
}

await browser.close();
server.close();
