"""X1/X7 - restyle the real Athens geometry through a 64-entry blit-time LUT.

Same pack bytes, three different LUTs. This is the mechanism spec-adequacy U8 says
a reader is free to apply; here it is applied in simulation to real tile content.
"""
import struct, numpy as np
from PIL import Image
from render import LUT, LUT_DIM, show
from panel import code_index, ALL_LAB

P="/Users/tobymurray/git/una-sdk/Docs/Investigations/2026-08-06-athens-pack/athens.rawtiles"
b=open(P,'rb').read()
D=struct.unpack_from('<H',b,60)[0]; N=struct.unpack_from('<I',b,88)[0]; IO=struct.unpack_from('<I',b,92)[0]
ents=[]
for i in range(N):
    o=IO+20*i; z=b[o]; x,y,off,ln=struct.unpack_from('<IIII',b,o+4); ents.append((z,x,y,off,ln))
def tile(z,x,y):
    for (tz,tx,ty,off,ln) in ents:
        if (tz,tx,ty)==(z,x,y): return np.frombuffer(b,np.uint8,D*D,off).reshape(D,D)
    return np.full((D,D),0xFF,np.uint8)

def C(r,g,bl): return (3<<6)|(bl<<4)|(g<<2)|r
WHITE=C(3,3,3); INK=C(0,0,0); INK_R=C(1,0,0); INK_B=C(0,0,1)

# semantic reading of the 27 codes the pack actually contains (see codes.py output)
BG, LANDUSE, ROAD, WATER1, WATER2, WOOD, WATER3 = 0xFF,0xEA,0xEF,0xFA,0xF9,0xEE,0xFE
CASING, BUILDING, MISC1, LABEL = 0xDA,0xEB,0xE9,0xD5

def mk(mapping, default=None):
    lut=np.arange(256,dtype=np.uint8)
    for k,v in mapping.items(): lut[k]=v
    if default is not None:
        for v in range(256):
            if v not in mapping and (v>>6)==3: lut[v]=default
    return lut

DAY = mk({
    BG:       WHITE,
    LANDUSE:  C(2,3,2),    # L*95.7 - recede almost to paper
    ROAD:     INK,         # the road you are on is the darkest thing after text
    CASING:   WHITE,       # casing becomes a white halo around the black road
    WATER1:   C(0,1,3),    # L*70.4 clear blue, 2.8:1
    WATER2:   C(0,0,3),    # L*51.8 deeper blue for waterway lines, 5.0:1
    WATER3:   C(0,1,3),
    WOOD:     C(0,2,1),    # L*77.3 the strongest usable green
    BUILDING: C(2,2,2),    # L*86 - buildings are context, not content
    MISC1:    C(2,2,2),
    LABEL:    INK,         # text at full 25:1
})
NIGHT = mk({
    BG:       INK,         # only viable dark background
    LANDUSE:  INK_R,       # L*36.6, the one other dark tone
    ROAD:     WHITE,
    CASING:   INK,
    WATER1:   INK_B,       # L*36.6 bluish
    WATER2:   C(0,0,2),
    WATER3:   INK_B,
    WOOD:     C(0,1,0),    # L*57.4
    BUILDING: INK_R,
    MISC1:    INK_R,
    LABEL:    WHITE,
})
HICON = mk({           # maximum-legibility: everything is ink or paper
    BG:WHITE, LANDUSE:WHITE, ROAD:INK, CASING:WHITE,
    WATER1:C(0,0,3), WATER2:C(0,0,3), WATER3:C(0,0,3),
    WOOD:WHITE, BUILDING:WHITE, MISC1:WHITE, LABEL:INK,
})

z16=[e for e in ents if e[0]==16]
xs=sorted(set(e[1] for e in z16)); ys=sorted(set(e[2] for e in z16))
cx,cy=xs[len(xs)//2],ys[len(ys)//2]
mos=np.full((D*2,D*2),0xFF,np.uint8)
for j,dy in enumerate((0,1)):
    for i,dx in enumerate((0,1)):
        mos[j*D:(j+1)*D,i*D:(i+1)*D]=tile(16,cx+dx,cy+dy)
o=(D*2-240)//2
vp=mos[o:o+240,o:o+240]

panels=[("as-built (osm-carto quantised)",vp),
        ("DAY LUT",DAY[vp]),("NIGHT LUT",NIGHT[vp]),("HI-CONTRAST LUT",HICON[vp])]
S=2
sheet=Image.new("RGB",(240*S*4+5*10,240*S+20),(255,255,255))
for i,(name,buf) in enumerate(panels):
    sheet.paste(show(buf).resize((240*S,240*S),Image.NEAREST),(10+i*(240*S+10),10))
sheet.save("out/restyle_compare.png")

for name,buf in panels:
    L=np.array([ALL_LAB[code_index(v&3,(v>>2)&3,(v>>4)&3)][0] for v in buf.ravel()])
    print(f"{name:34} L* mean {L.mean():6.2f}  p5 {np.percentile(L,5):6.2f}  "
          f"p95 {np.percentile(L,95):6.2f}  frac<L*50 {(L<50).mean()*100:5.2f}%  "
          f"codes {len(np.unique(buf))}")
print("\nwrote out/restyle_compare.png")
