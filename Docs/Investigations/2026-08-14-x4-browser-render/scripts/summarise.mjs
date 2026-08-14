import fs from 'node:fs';

const MiB = 1048576;
const load = f => JSON.parse(fs.readFileSync(f, 'utf8'));

function passRows(rows, pass) {
  return rows.filter(r => r.level && r.pass === pass);
}
function total(rows, k) { return rows.reduce((s, r) => s + r[k], 0); }

function report(file, title) {
  if (!fs.existsSync(file)) return;
  const rows = load(file);
  console.log(`\n### ${title}  (${file})`);
  const passes = [...new Set(rows.filter(r => r.level).map(r => r.pass))];
  for (const p of passes) {
    const rs = passRows(rows, p);
    const wall = total(rs, 'wallMs') / 1000;
    console.log(`\npass ${p}: ${total(rs, 'tiles').toLocaleString()} tiles, ` +
      `${total(rs, 'renders')} map renders, wall ${wall.toFixed(1)}s ` +
      `(${(total(rs, 'tiles') / wall).toFixed(0)} tiles/s, ` +
      `${(total(rs, 'rawBytes') / 1e6 / wall).toFixed(0)} Mpx/s)`);
    console.log(`  breakdown: render ${(total(rs, 'render') / 1000).toFixed(1)}s | ` +
      `readback ${(total(rs, 'readback') / 1000).toFixed(1)}s | ` +
      `quantise ${(total(rs, 'quant') / 1000).toFixed(1)}s | ` +
      `rle ${(total(rs, 'rle') / 1000).toFixed(1)}s | ` +
      `resize ${(total(rs, 'resize') / 1000).toFixed(1)}s`);
    console.log(`  pack: ${(total(rs, 'rawBytes') / MiB).toFixed(1)} MiB raw -> ` +
      `${(total(rs, 'rleBytes') / MiB).toFixed(2)} MiB RLE ` +
      `(${(100 * total(rs, 'rleBytes') / total(rs, 'rawBytes')).toFixed(1)} %)`);
    console.log('  | grid L | ladder z | tiles | wall s | tiles/s | RLE % | codes | MiB |');
    console.log('  |---:|---:|---:|---:|---:|---:|---:|---:|');
    for (const r of rs) {
      console.log(`  | ${r.level} | z${r.level - 1} | ${r.tiles.toLocaleString()} | ` +
        `${(r.wallMs / 1000).toFixed(1)} | ${(r.tiles / (r.wallMs / 1000)).toFixed(0)} | ` +
        `${r.rlePct.toFixed(1)} | ${r.codes} | ${(r.rleBytes / MiB).toFixed(2)} |`);
    }
    const net = rows.find(r => r.passNetwork && r.pass === p);
    if (net) for (const [h, v] of Object.entries(net.passNetwork)) {
      console.log(`  network ${h}: ${v.requests.toLocaleString()} requests, ` +
        `${(v.bytes / MiB).toFixed(2)} MiB`);
    }
  }
}

report('out-block16.json', 'Athens ON trail pack — block-and-slice (16x16)');
report('out-tile.json', 'Athens ON trail pack — one render per tile');
report('out-metro90.json', 'Ottawa 90 km metro pack — block-and-slice (16x16)');
report('out-swrender.json', 'Athens ON trail pack — software rasteriser (llvmpipe)');
