import numpy as np
from panel import *

# WCAG contrast uses relative luminance; on a reflective panel the physically
# meaningful analogue is the ratio of reflectances, floor included.
Y = ALL_XYZ[:,1]
def cr(i,j):
    a,b = Y[i],Y[j]
    return max(a,b)/min(a,b)

names = {
 "paper white":      code_index(3,3,3),
 "paper warm":       code_index(3,3,2),
 "paper cool":       code_index(2,3,3),
 "ink black":        code_index(0,0,0),
 "ink red":          code_index(1,0,0),
 "ink blue":         code_index(0,0,1),
 "neutral mid":      code_index(1,1,1),
 "water blue":       code_index(0,0,3),
 "water pale":       code_index(1,2,3),
 "park green":       code_index(1,3,1),
 "green full":       code_index(0,3,0),
}
print("=== Reflectance contrast ratios (panel max is 25:1) ===")
print(f"{'':16}", "".join(f"{k[:9]:>10}" for k in names))
for k1,i in names.items():
    row=f"{k1:16}"
    for k2,j in names.items():
        row += f"{cr(i,j):10.1f}" if i!=j else f"{'-':>10}"
    print(row)

print()
print("=== Neutral ramp only (the 'ink' problem) ===")
for k in range(4):
    i=code_index(k,k,k)
    print(f"  grey{k}: L*={ALL_LAB[i][0]:6.2f}  Y={Y[i]*100:6.3f}%  "
          f"CR vs white = {cr(i,code_index(3,3,3)):5.1f}:1")
print()
print("Darkest codes available (L* ascending):")
o=np.argsort(ALL_LAB[:,0])[:8]
for i in o:
    r,g,b=CODES[i]
    print(f"  r{r}g{g}b{b} 0x{abgr2222_byte(r,g,b):02X}  L*={ALL_LAB[i][0]:6.2f} "
          f"C*={np.hypot(*ALL_LAB[i][1:]):6.2f}  CRvsWhite={cr(i,code_index(3,3,3)):5.1f}:1")
print()
print("=== Lightness is the green channel ===")
for g in range(4):
    sel=[i for i in range(64) if CODES[i][1]==g]
    Ls=ALL_LAB[sel,0]
    print(f"  g={g}: L* {Ls.min():6.2f} .. {Ls.max():6.2f}  (spread {Ls.max()-Ls.min():5.2f})")
