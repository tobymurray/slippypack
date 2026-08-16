#!/usr/bin/env python3
"""What each encoding would cost for the same 18,169 tiles.

Every candidate is measured PER TILE, because the watch reads one tile at
a time -- an encoding that only wins when the whole pack is one stream is
not a candidate, it is a different product.
"""
import sys, glob, hashlib, io, os, zlib
from compression import zstd
from collections import Counter
from PIL import Image

BASE = sys.argv[1] if len(sys.argv) > 1 else '.'
TILES = sorted(glob.glob(f'{BASE}/tiles/*.raw'))
DIM = 256
SAMPLE = int(sys.argv[2]) if len(sys.argv) > 2 else 1500
step = max(1, len(TILES) // SAMPLE)
sample = TILES[::step]
print(f'{len(TILES)} tiles total; measuring {len(sample)} (every {step}th)')

raws = [open(f, 'rb').read() for f in sample]
manifest = {}
for line in open(f'{BASE}/tiles/manifest.tsv'):
    i, z, x, y, comp, raw = line.split()
    manifest[int(i)] = int(comp)
idx = [int(os.path.basename(f)[:6]) for f in sample]

results = {}
results['raw (no compression)'] = sum(len(r) for r in raws)
results['RLE8 (today)'] = sum(manifest[i] for i in idx)

# --- generic byte compressors, per tile -----------------------------
results['deflate -9'] = sum(len(zlib.compress(r, 9)) for r in raws)
results['zstd -19'] = sum(len(zstd.compress(r, 19)) for r in raws)
results['RLE8 then deflate -9'] = sum(
    len(zlib.compress(zlib.decompress(zlib.compress(r, 9)), 9)) for r in raws[:0]) or None

# --- zstd with a dictionary trained on other tiles -------------------
train = [open(f, 'rb').read() for f in TILES[::max(1, len(TILES) // 300)]]
for dsize in (4096, 16384, 65536):
    d = zstd.train_dict(train, dsize)
    zd = zstd.ZstdDict(d.dict_content)
    c = zstd.ZstdCompressor(level=19, zstd_dict=zd)
    total = sum(len(c.compress(r, zstd.ZstdCompressor.FLUSH_FRAME)) for r in raws)
    results[f'zstd -19 + {dsize // 1024} KB dict'] = total

# --- 4-bit repack: only 11 palette codes are ever used ---------------
codes = Counter()
for r in raws:
    codes.update(r)
print(f'distinct palette codes in sample: {len(codes)}')
palette = sorted(codes)
lut = {c: i for i, c in enumerate(palette)}
def to_nibbles(r):
    out = bytearray(len(r) // 2)
    for i in range(0, len(r), 2):
        out[i // 2] = (lut[r[i]] << 4) | lut[r[i + 1]]
    return bytes(out)
nib = [to_nibbles(r) for r in raws]
results['4-bit indexed (raw)'] = sum(len(n) for n in nib)
results['4-bit + deflate -9'] = sum(len(zlib.compress(n, 9)) for n in nib)
results['4-bit + zstd -19'] = sum(len(zstd.compress(n, 19)) for n in nib)

# --- PNG, as the reference for "filters + deflate" -------------------
png_total = 0
for r in raws:
    im = Image.frombytes('P', (DIM, DIM), r)
    im.putpalette(b'\x00\x00\x00' * 256)
    buf = io.BytesIO()
    im.save(buf, format='PNG', optimize=True, bits=4)
    png_total += buf.tell()
results['PNG (8-bit palette, optimize)'] = png_total

base = results['RLE8 (today)']
print(f'\n{"encoding":34} {"bytes/tile":>11} {"vs RLE8":>9}   full pack')
for k, v in results.items():
    if v is None:
        continue
    per = v / len(raws)
    scaled = per * len(TILES) / 1e6
    print(f'{k:34} {per:11.0f} {v / base:8.2f}x {scaled:9.1f} MB')

# --- duplicate tiles -------------------------------------------------
hashes = Counter()
for f in TILES:
    hashes[hashlib.sha256(open(f, 'rb').read()).digest()] += 1
dupes = sum(c - 1 for c in hashes.values())
print(f'\nunique tiles: {len(hashes)} of {len(TILES)} '
      f'({dupes} duplicates, {dupes / len(TILES):.1%} of the pack)')
top = hashes.most_common(3)
print('most repeated tile appears', top[0][1], 'times')
