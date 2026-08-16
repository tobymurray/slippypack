#!/usr/bin/env python3
"""The candidates that are not generic byte compressors.

LZ4 because the format already reserves it as codec 3; row filtering
because PNG does it; per-tile best-of because the index carries a
compression byte per tile and could mix.

    python3 measure-variants.py <dir containing tiles/>
"""
import sys, glob, os, subprocess, zlib

BASE = sys.argv[1] if len(sys.argv) > 1 else '.'
TILES = sorted(glob.glob(f'{BASE}/tiles/*.raw'))
step = max(1, len(TILES) // 400)
files = TILES[::step]
raws = [open(f, 'rb').read() for f in files]
n = len(raws)

rle = {}
for line in open(f'{BASE}/tiles/manifest.tsv'):
    i, z, x, y, c, r = (int(v) for v in line.split())
    rle[i] = c
rle_sizes = [rle[int(os.path.basename(f)[:6])] for f in files]
base = sum(rle_sizes)

def show(name, sizes, note=''):
    print(f'{name:34}{sum(sizes) / n:7.0f} B/tile {sum(sizes) / base:6.2f}x '
          f'{sum(sizes) / n * len(TILES) / 1e6:7.1f} MB  {note}')

def deflate(b, level=9, wbits=12):
    c = zlib.compressobj(level, zlib.DEFLATED, -wbits)
    return len(c.compress(b) + c.flush())

print(f'{len(TILES)} tiles total; measuring {n}')
show('RLE8 (today)', rle_sizes)

lz4 = []
for r in raws:
    p = subprocess.run(['lz4', '-12', '--no-frame-crc', '-c'], input=r, capture_output=True)
    lz4.append(len(p.stdout))
show('LZ4 -12', lz4, '~1 KB decoder, no window buffer')

show('deflate -9 w=4KB', [deflate(r) for r in raws], '~3 KB decoder + 4 KB window')
show('deflate -6 w=4KB', [deflate(r, 6) for r in raws], 'faster to encode')

def upfilter(b, dim=256):
    """PNG's Up filter: each row minus the row above."""
    out = bytearray(len(b))
    prev = bytes(dim)
    for y in range(dim):
        row = b[y * dim:(y + 1) * dim]
        out[y * dim:(y + 1) * dim] = bytes((row[i] - prev[i]) & 0xff for i in range(dim))
        prev = row
    return bytes(out)

show('Up-filter + deflate -9 w=4KB', [deflate(upfilter(r)) for r in raws])
show('per-tile min(RLE8, deflate)',
     [min(a, b) for a, b in zip(rle_sizes, [deflate(r) for r in raws])],
     'the index already carries a codec per tile')
