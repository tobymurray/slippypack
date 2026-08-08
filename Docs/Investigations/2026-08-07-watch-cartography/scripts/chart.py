import numpy as np
from PIL import Image, ImageDraw, ImageFont
from panel import CODES, ALL_SRGB, ALL_LAB, code_index, abgr2222_byte
from scene import PAL
def f(s):
    try: return ImageFont.truetype("/System/Library/Fonts/Supplemental/Arial.ttf",s)
    except OSError: return ImageFont.load_default()
CW,CH=118,54
img=Image.new("RGB",(CW*8+20,CH*8+70),(255,255,255))
d=ImageDraw.Draw(img)
d.text((10,8),"LS012B7DD06A - all 64 ABGR2222 codes, ordered by L*  (display-adapted preview)",font=f(15),fill=(0,0,0))
d.text((10,28),"slot names mark the codes used by the candidate watch palette",font=f(12),fill=(90,90,90))
rev={v:k for k,v in PAL.items()}
order=np.argsort(-ALL_LAB[:,0])
for n,i in enumerate(order):
    r,g,b=CODES[i]; byte=abgr2222_byte(r,g,b)
    col=tuple(int(round(c*255)) for c in ALL_SRGB[i])
    x=10+(n%8)*CW; y=50+(n//8)*CH
    d.rectangle([x,y,x+CW-4,y+CH-4],fill=col,outline=(160,160,160))
    L=ALL_LAB[i][0]
    tc=(0,0,0) if L>60 else (255,255,255)
    d.text((x+4,y+3),f"0x{byte:02X}  r{r}g{g}b{b}",font=f(11),fill=tc)
    d.text((x+4,y+17),f"L*{L:.1f}",font=f(11),fill=tc)
    if byte in rev: d.text((x+4,y+31),rev[byte],font=f(11),fill=tc)
img.save("out/palette_chart.png")
print("wrote out/palette_chart.png")
print("\nCandidate palette, ABGR2222 bytes:")
for k,v in PAL.items():
    r=v&3; g=(v>>2)&3; b=(v>>4)&3
    i=code_index(r,g,b); L,a,bb=ALL_LAB[i]
    s=tuple(int(round(c*255)) for c in ALL_SRGB[i])
    print(f"  {k:11} 0x{v:02X}  r{r} g{g} b{b}  L*{L:6.2f} a*{a:7.2f} b*{bb:7.2f}  preview #{s[0]:02X}{s[1]:02X}{s[2]:02X}")
