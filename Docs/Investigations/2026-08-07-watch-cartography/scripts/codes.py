import struct, numpy as np
from panel import CODES, ALL_LAB, code_index
P="/Users/tobymurray/git/una-sdk/Docs/Investigations/2026-08-06-athens-pack/athens.rawtiles"
b=open(P,'rb').read()
D=struct.unpack_from('<H',b,60)[0]; N=struct.unpack_from('<I',b,88)[0]; IO=struct.unpack_from('<I',b,92)[0]
px=[]
for i in range(N):
    o=IO+20*i; x,y,off,ln=struct.unpack_from('<IIII',b,o+4)
    px.append(np.frombuffer(b,np.uint8,ln,off))
allpx=np.concatenate(px)
h=np.bincount(allpx,minlength=256)
print(f"{'byte':>5} {'r g b':>7} {'L*':>7} {'share%':>8}   plausible osm-carto role")
role={0xFF:"background / paper (#f2efe9-ish) + road fill white",
      0xEA:"grey landuse / residential+farmland fill",
      0xEF:"major road fill (yellow/cream)",
      0xFA:"water fill (light blue)",
      0xF9:"water (deeper) / waterway line",
      0xEE:"woodland / park green",
      0xFE:"water pale",
      0xDA:"road casing / building outline",
      0xEB:"building fill (pink)",
      0xE9:"?",0xD5:"label text (mid grey)",0xFB:"?"}
for v in np.argsort(-h):
    if h[v]==0: continue
    r=v&3; g=(v>>2)&3; bl=(v>>4)&3
    L=ALL_LAB[code_index(r,g,bl)][0]
    print(f" 0x{v:02X} {r} {g} {bl}   {L:7.2f} {h[v]/len(allpx)*100:8.4f}   {role.get(v,'')}")
print()
print(f"share of pixels using the darkest code 0xC0 (L*23.7, 25:1): {h[0xC0]/len(allpx)*100:.5f}%")
print(f"share with L* < 50 (any real 'ink'): ", end="")
s=0
for v in range(256):
    if h[v]==0 or (v>>6)!=3: continue
    r=v&3; g=(v>>2)&3; bl=(v>>4)&3
    if ALL_LAB[code_index(r,g,bl)][0] < 50: s+=h[v]
print(f"{s/len(allpx)*100:.5f}%")
print(f"share with L* > 80 (all 'paper'):   ", end="")
s=0
for v in range(256):
    if h[v]==0 or (v>>6)!=3: continue
    r=v&3; g=(v>>2)&3; bl=(v>>4)&3
    if ALL_LAB[code_index(r,g,bl)][0] > 80: s+=h[v]
print(f"{s/len(allpx)*100:.5f}%")
