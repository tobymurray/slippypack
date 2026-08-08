import numpy as np, time
from PIL import Image
from scene import render_palette_first, render_srgb, quantise, PAL
from render import show
from panel import ALL_LAB, code_index

def rle(d):
    out=bytearray(); lit=bytearray(); i=0; n=len(d)
    def flush():
        k=0
        while k<len(lit):
            c=lit[k:k+128]; out.append(len(c)-1); out.extend(c); k+=128
        lit.clear()
    while i<n:
        px=d[i]; j=i+1
        while j<n and d[j]==px and j-i<128: j+=1
        if j-i>=3:
            flush(); out.append(0x80|(j-i-1)); out.append(px); i=j
        else:
            lit.append(px); i+=1
    flush(); return bytes(out)

arms={}
arms["B palette-first (no AA)"]      = render_palette_first()
arms["A1 quantise 1x (AA)"]          = quantise(render_srgb(1))
big = render_srgb(2)
arms["A2 quantise 2x->downsample"]   = quantise(big.resize((240,240), Image.LANCZOS))
arms["A3 quantise 1x + dither"]      = quantise(render_srgb(1), dither=True)

print(f"{'arm':34} {'codes':>6} {'RLE B':>8} {'% raw':>7} {'x':>6}  {'frac<L*50':>10}")
print("-"*82)
for k,v in arms.items():
    b=v.tobytes(); r=len(rle(b))
    L=np.array([ALL_LAB[code_index(x&3,(x>>2)&3,(x>>4)&3)][0] for x in v.ravel()])
    print(f"{k:34} {len(np.unique(v)):6d} {r:8,} {r/len(b)*100:6.1f}% {len(b)/r:6.2f}  {(L<50).mean()*100:9.2f}%")

S=2
names=list(arms)
sheet=Image.new("RGB",(240*S*len(names)+10*(len(names)+1), 240*S+20),(250,250,250))
for i,k in enumerate(names):
    sheet.paste(show(arms[k]).resize((240*S,240*S),Image.NEAREST),(10+i*(240*S+10),10))
sheet.save("out/x12_arms.png")

# 1:1 crops so line weight is judged honestly, not through a 2x upscale
crop=Image.new("RGB",(240*len(names)+10*(len(names)+1),240+20),(250,250,250))
for i,k in enumerate(names):
    crop.paste(show(arms[k]),(10+i*250,10))
crop.save("out/x12_arms_1to1.png")
np.save("out/palette_first.npy", arms["B palette-first (no AA)"])
print("\nwrote out/x12_arms.png and out/x12_arms_1to1.png")
