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

// --- args
const arg = (k, d) => {
  const i = process.argv.indexOf('--' + k);
  return i === -1 ? d : process.argv[i + 1];
};
const BBOX = (arg('bbox', '-76.015,44.590,-75.889,44.662')).split(',').map(Number);
const LEVELS = arg('levels', '13,14,15,16,17').split(',').map(Number);
const STRATEGY = arg('strategy', 'block');
const BLOCK = Number(arg('block', 16));
const QUANT = arg('quant', '1') === '1';
const PASSES = Number(arg('passes', 1));
const LABEL = arg('label', 'run');
const SAMPLE = arg('sample', '');

const browser = await firefox.launch({ headless: false });
const ctx = await browser.newContext({ deviceScaleFactor: 1, viewport: { width: 2400, height: 2400 } });
const page = await ctx.newPage();

// Network accounting, split by host so vector-source bytes are separable
// from the local harness assets.
let net = {};
page.on('response', r => {
  const u = new URL(r.url());
  if (u.origin === base) return;
  const len = Number(r.headers()['content-length'] || 0);
  const k = u.host;
  net[k] = net[k] || { requests: 0, bytes: 0 };
  net[k].requests++;
  net[k].bytes += len;
});
page.on('pageerror', e => console.error('PAGE ERROR:', e.message));
page.on('console', m => { if (m.type() === 'error') console.error('CONSOLE:', m.text()); });

await page.goto(`${base}/harness.html`);
await page.waitForFunction(() => window.x4Ready === true, null, { timeout: 60000 });
const info = await page.evaluate(url => window.x4Boot(url), `${base}/watch-style.json`);
console.error(`GPU: ${info.vendor} / ${info.renderer}  dpr=${info.dpr}`);

if (SAMPLE) {
  for (const [fn, suffix] of [['x4Sample', ''], ['x4SampleQuantised', '-quantised']]) {
    const bytes = await page.evaluate(([f, a]) => window[f](a),
      [fn, { bbox: BBOX, level: LEVELS[LEVELS.length - 1], n: Number(SAMPLE) }]);
    fs.writeFileSync(path.join(ROOT, `sample-z${LEVELS[LEVELS.length - 1]}${suffix}.png`), Buffer.from(bytes));
  }
  console.error('samples written');
}

const results = [];
for (let pass = 1; pass <= PASSES; pass++) {
  const netAtStart = JSON.parse(JSON.stringify(net));
  for (const level of LEVELS) {
    const r = await page.evaluate(cfg => window.x4Pass(cfg),
      { bbox: BBOX, level, strategy: STRATEGY, blockN: BLOCK, quantise: QUANT });
    r.pass = pass; r.label = LABEL;
    results.push(r);
    console.error(`  pass${pass} L${level} ${STRATEGY}: ${r.tiles} tiles in ${(r.wallMs / 1000).toFixed(1)}s ` +
      `(${(r.tiles / (r.wallMs / 1000)).toFixed(0)} t/s), rle ${r.rlePct.toFixed(1)}%`);
  }
  const delta = {};
  for (const h of Object.keys(net)) {
    const b = netAtStart[h] || { requests: 0, bytes: 0 };
    if (net[h].bytes - b.bytes > 0 || net[h].requests - b.requests > 0)
      delta[h] = { requests: net[h].requests - b.requests, bytes: net[h].bytes - b.bytes };
  }
  results.push({ passNetwork: delta, pass, label: LABEL });
}

console.log(JSON.stringify(results, null, 2));
await browser.close();
server.close();
