#!/usr/bin/env python3
"""Ratio against decoder cost, which is what an embedded target cares about.

Also: where the bytes actually are, by zoom.
"""
import sys, glob, os, sys, time, zlib
from collections import defaultdict
from compression import zstd

BASE = sys.argv[1] if len(sys.argv) > 1 else '.'
TILES = sorted(glob.glob(f'{BASE}/tiles/*.raw'))

# --- where the bytes are, by zoom (whole pack, not a sample) ----------
by_zoom = defaultdict(lambda: [0, 0])
for line in open(f'{BASE}/tiles/manifest.tsv'):
    i, z, x, y, comp, raw = (int(v) for v in line.split())
    by_zoom[z][0] += 1
    by_zoom[z][1] += comp
total = sum(v[1] for v in by_zoom.values())
print(f'{"zoom":>5} {"tiles":>8} {"RLE8 bytes":>13} {"share":>7}  cumulative')
cum = 0
for z in sorted(by_zoom):
    n, b = by_zoom[z]
    cum += b
    print(f'{z:5} {n:8} {b:13,} {b / total:6.1%} {cum / total:10.1%}')

# --- codecs, ranked by what the decoder costs -------------------------
step = max(1, len(TILES) // 800)
raws = [open(f, 'rb').read() for f in TILES[::step]]
n = len(raws)
print(f'\nmeasuring {n} tiles')

def report(name, sizes, seconds, note):
    per = sum(sizes) / n
    print(f'{name:28} {per:7.0f} B/tile {sum(sizes) / rle_total:6.2f}x RLE8'
          f' {per * len(TILES) / 1e6:8.1f} MB   enc {seconds:5.1f}s  {note}')

rle_total = 0
for line in open(f'{BASE}/tiles/manifest.tsv'):
    i, z, x, y, comp, raw = (int(v) for v in line.split())
    if i % step == 0:
        rle_total += comp

print(f'{"codec":28} {"size":>14} {"ratio":>12} {"pack":>11}   encode      decoder cost')
report('RLE8 (today)', [rle_total], 0.0, 'a few bytes of state')

for wbits, label in ((15, 'deflate w=32KB'), (12, 'deflate w=4KB')):
    t = time.time()
    sizes = []
    for r in raws:
        c = zlib.compressobj(9, zlib.DEFLATED, -wbits)
        sizes.append(len(c.compress(r) + c.flush()))
    report(label, sizes, time.time() - t, f'~3 KB code + {2**wbits // 1024} KB window')

for level, wlog in ((1, 16), (3, 16), (9, 16), (19, 16), (19, 15), (19, 12)):
    t = time.time()
    opts = {zstd.CompressionParameter.compression_level: level,
            zstd.CompressionParameter.window_log: wlog}
    sizes = [len(zstd.compress(r, options=opts)) for r in raws]
    win = 2**wlog // 1024
    report(f'zstd -{level} (window {win}KB)', sizes, time.time() - t,
           f'~60 KB code, window {win} KB')
