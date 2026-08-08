"""X7 - blit-time 64-entry LUT restyle, applied to palette-first content."""
import numpy as np
from PIL import Image
from scene import PAL
from render import show
from panel import ALL_LAB, code_index

buf=np.load("out/palette_first.npy")
def C(r,g,b): return (3<<6)|(b<<4)|(g<<2)|r
def mk(m):
    lut=np.arange(256,dtype=np.uint8)
    for k,v in m.items(): lut[k]=v
    return lut

DAY   = mk({})                                        # identity: the pack as authored
NIGHT = mk({PAL["paper"]:C(0,0,0), PAL["landuse"]:C(1,0,0), PAL["wood"]:C(0,1,0),
            PAL["wood_lt"]:C(0,1,0), PAL["water"]:C(0,0,2), PAL["water_dk"]:C(0,1,3),
            PAL["contour"]:C(1,0,0), PAL["building"]:C(1,0,0),
            PAL["road_major"]:C(3,3,3), PAL["road_minor"]:C(3,2,1),
            PAL["path"]:C(2,3,3), PAL["halo"]:C(0,0,0)})
HICON = mk({PAL["landuse"]:C(3,3,3), PAL["building"]:C(3,3,3), PAL["contour"]:C(3,3,3),
            PAL["wood_lt"]:C(3,3,3), PAL["wood"]:C(2,3,2), PAL["water"]:C(0,0,3),
            PAL["water_dk"]:C(0,0,3), PAL["road_minor"]:C(0,0,0)})
TRAIL = mk({PAL["building"]:C(3,3,3), PAL["landuse"]:C(3,3,3),   # drop urban clutter
            PAL["road_minor"]:C(2,2,2),                           # roads recede
            PAL["path"]:C(0,0,0), PAL["contour"]:C(1,0,0),        # trail + terrain promoted
            PAL["wood"]:C(1,3,1)})

arms=[("DAY (as authored)",DAY),("NIGHT",NIGHT),("HIGH CONTRAST",HICON),("TRAIL",TRAIL)]
S=2
sheet=Image.new("RGB",(240*S*4+10*5,240*S+20),(250,250,250))
for i,(n,lut) in enumerate(arms):
    v=lut[buf]
    sheet.paste(show(v).resize((240*S,240*S),Image.NEAREST),(10+i*(240*S+10),10))
    L=np.array([ALL_LAB[code_index(x&3,(x>>2)&3,(x>>4)&3)][0] for x in v.ravel()])
    print(f"{n:20} codes {len(np.unique(v)):3d}  mean L* {L.mean():6.2f}  frac<L*50 {(L<50).mean()*100:6.2f}%")
sheet.save("out/x7_lut_variants.png")
print("wrote out/x7_lut_variants.png")
