"""Can a normal (always-antialiasing) vector renderer produce palette-first output?

arm C : render in the PANEL PALETTE colours, with AA (what MapLibre would give us),
        then quantise nearest-of-64.
arm D : same render, then SNAP nearest-of-N-palette-slots instead of nearest-of-64.
"""
import numpy as np
from PIL import Image, ImageDraw
from scene import geometry, PAL, W, H, font, srgb_to_lab, quantise
from panel import ALL_SRGB, ALL_LAB, CODES, code_index
from render import show

# sRGB preview colours of the chosen palette slots = what we tell the renderer to draw
SLOT_SRGB={k: tuple(int(round(c*255)) for c in ALL_SRGB[code_index(v&3,(v>>2)&3,(v>>4)&3)])
           for k,v in PAL.items()}
img=Image.new("RGB",(W,H),SLOT_SRGB["paper"])
d=ImageDraw.Draw(img)
geometry(d, SLOT_SRGB)
for xy,t,s in (((6,46),"MAIN ST",12),((156,176),"Mill Pond",11),((20,200),"Bear Wood",11)):
    f=font(s)
    for dx in (-1,0,1):
        for dy in (-1,0,1):
            if dx or dy: d.text((xy[0]+dx,xy[1]+dy),t,font=f,fill=SLOT_SRGB["halo"])
    d.text(xy,t,font=f,fill=SLOT_SRGB["ink"])

armC = quantise(img)

# arm D: snap to nearest of the N slots actually in the palette
slots=sorted(set(PAL.values()))
slot_lab=np.array([ALL_LAB[code_index(v&3,(v>>2)&3,(v>>4)&3)] for v in slots])
src=srgb_to_lab(np.asarray(img,float)).reshape(-1,3)
dist=((src[:,None,:]-slot_lab[None,:,:])**2).sum(-1)
armD=np.array([slots[k] for k in dist.argmin(1)],np.uint8).reshape(H,W)

def rle(dd):
    out=bytearray(); lit=bytearray(); i=0; n=len(dd)
    def flush():
        k=0
        while k<len(lit):
            c=lit[k:k+128]; out.append(len(c)-1); out.extend(c); k+=128
        lit.clear()
    while i<n:
        px=dd[i]; j=i+1
        while j<n and dd[j]==px and j-i<128: j+=1
        if j-i>=3: flush(); out.append(0x80|(j-i-1)); out.append(px); i=j
        else: lit.append(px); i+=1
    flush(); return bytes(out)

B=np.load("out/palette_first.npy")
for name,v in (("B  palette-first, no AA (reference)",B),
               ("C  palette colours + AA, nearest-of-64",armC),
               ("D  palette colours + AA, snap-to-slots",armD)):
    b=v.tobytes(); r=len(rle(b))
    inpal=np.isin(v, slots).mean()*100
    print(f"{name:40} codes {len(np.unique(v)):3d}  RLE {r/len(b)*100:5.1f}%  "
          f"pixels on a palette slot {inpal:6.2f}%")

S=2
sheet=Image.new("RGB",(240*S*3+40,240*S+20),(250,250,250))
for i,v in enumerate((B,armC,armD)):
    sheet.paste(show(v).resize((240*S,240*S),Image.NEAREST),(10+i*(240*S+10),10))
sheet.save("out/x2b_aa.png")

# does the LUT survive? invert paper<->ink and see if text stays coherent
lut=np.arange(256,dtype=np.uint8); lut[PAL["paper"]]=PAL["ink"]; lut[PAL["ink"]]=PAL["paper"]
sheet2=Image.new("RGB",(240*S*3+40,240*S+20),(250,250,250))
for i,v in enumerate((B,armC,armD)):
    sheet2.paste(show(lut[v]).resize((240*S,240*S),Image.NEAREST),(10+i*(240*S+10),10))
sheet2.save("out/x2b_lut_survives.png")
print("wrote out/x2b_aa.png, out/x2b_lut_survives.png")
