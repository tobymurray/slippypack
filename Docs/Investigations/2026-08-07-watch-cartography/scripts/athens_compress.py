"""X3 - re-measure compression on the real 687-tile Athens pack (n=687, not n=12)."""
import struct, zlib, numpy as np, collections
P="/Users/tobymurray/git/una-sdk/Docs/Investigations/2026-08-06-athens-pack/athens.rawtiles"
b=open(P,'rb').read()
D=struct.unpack_from('<H',b,60)[0]
N=struct.unpack_from('<I',b,88)[0]
IO=struct.unpack_from('<I',b,92)[0]
print(f"pack {len(b):,} B   tile_dim={D}   tiles={N}   index_off={IO}")
tiles=[]; zs=[]
for i in range(N):
    o=IO+20*i
    z=b[o]
    x,y,off,ln=struct.unpack_from('<IIII',b,o+4)
    tiles.append(b[off:off+ln]); zs.append(z)
RAW=sum(len(t) for t in tiles)
print(f"raw tile bytes {RAW:,}  ({RAW/1024/1024:.1f} MiB)")
print("per-zoom tile counts:", dict(sorted(collections.Counter(zs).items())))

def rle(d):
    """rawtiles v0.6 sec 9.11 pixel-level RLE, 1 byte per pixel for ABGR2222."""
    out=bytearray(); lit=bytearray(); i=0; n=len(d)
    def flush():
        k=0
        while k<len(lit):
            c=lit[k:k+128]; out.append(len(c)-1); out.extend(c); k+=128
        lit.clear()
    while i<n:
        px=d[i]; j=i+1
        while j<n and d[j]==px and j-i<128: j+=1
        run=j-i
        if run>=3:
            flush(); out.append(0x80|(run-1)); out.append(px); i=j
        else:
            lit.append(px); i+=1
    flush(); return bytes(out)

import time
t0=time.time()
rle_sz=[len(rle(t)) for t in tiles]
print(f"  (RLE of all {N} tiles took {time.time()-t0:.1f}s)")
dfl_sz=[len(zlib.compress(t,9)) for t in tiles]
dfl15=[len(zlib.compressobj(9,zlib.DEFLATED,-15).compress(t)+zlib.compressobj(9,zlib.DEFLATED,-15).flush()) for t in tiles]

def pct(v): return sum(v)/RAW*100
print()
print(f"{'scheme':32} {'bytes':>12} {'% of raw':>9} {'x':>6}")
print("-"*64)
print(f"{'None (as shipped)':32} {RAW:12,} {100.0:8.1f}% {1.0:6.2f}")
print(f"{'spec RLE (sec 9.11)':32} {sum(rle_sz):12,} {pct(rle_sz):8.1f}% {RAW/sum(rle_sz):6.2f}")
print(f"{'deflate-9 32KiB window':32} {sum(dfl_sz):12,} {pct(dfl_sz):8.1f}% {RAW/sum(dfl_sz):6.2f}")

# per-zoom breakdown - does the ratio depend on zoom (content density)?
print()
print(f"{'zoom':>5} {'tiles':>6} {'raw MiB':>9} {'RLE %':>8} {'deflate %':>10}")
for z in sorted(set(zs)):
    idx=[i for i,zz in enumerate(zs) if zz==z]
    r=sum(len(tiles[i]) for i in idx)
    print(f"{z:5d} {len(idx):6d} {r/1048576:9.2f} "
          f"{sum(rle_sz[i] for i in idx)/r*100:7.1f}% {sum(dfl_sz[i] for i in idx)/r*100:9.1f}%")

# colour histogram: how many of the 64 codes does real OSM raster actually use?
allpx=np.frombuffer(b''.join(tiles),dtype=np.uint8)
h=np.bincount(allpx,minlength=256)
used=(h>0).sum()
top=np.argsort(-h)[:12]
print()
print(f"distinct byte values used across {len(allpx):,} pixels: {used}")
print("top 12 codes (byte, share):")
for v in top:
    r=v&3; g=(v>>2)&3; bl=(v>>4)&3; a=(v>>6)&3
    print(f"  0x{v:02X} a{a} b{bl} g{g} r{r}   {h[v]/len(allpx)*100:5.2f}%")
cum=np.sort(h)[::-1].cumsum()/len(allpx)
for k in (4,8,16,32):
    print(f"  top {k:2d} codes cover {cum[k-1]*100:5.2f}% of pixels")
