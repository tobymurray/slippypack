import numpy as np
from panel import *

print("=== The grey ramp: the four neutral levels ===")
print(f"{'code':6} {'Y refl %':>9} {'L*':>7}  step in L*")
prev=None
for k in range(4):
    idx=code_index(k,k,k); Y=ALL_XYZ[idx][1]*100; L=ALL_LAB[idx][0]
    step = "" if prev is None else f"{L-prev:+.1f}"
    print(f"({k},{k},{k}) {Y:9.3f} {L:7.2f}  {step}")
    prev=L

print()
print("=== L* range and what it costs ===")
Ls=ALL_LAB[:,0]
print(f"L* spans {Ls.min():.1f} .. {Ls.max():.1f}  (an emissive panel would span 0..100)")
print(f"Number of codes with L* < 40 (usable as 'dark ink'): {(Ls<40).sum()}")
print(f"Number of codes with L* < 50: {(Ls<50).sum()}")
print(f"Number of codes with L* > 80 (usable as 'paper'):    {(Ls>80).sum()}")

print()
print("=== Codes sorted by lightness, with sRGB preview and ABGR2222 byte ===")
order=np.argsort(Ls)
for i in order:
    r,g,b=CODES[i]; L,a,bb=ALL_LAB[i]
    print(f"  r{r} g{g} b{b}  byte 0x{abgr2222_byte(r,g,b):02X}  "
          f"L*{L:6.2f} a*{a:7.2f} b*{bb:7.2f}  C*{np.hypot(a,bb):6.2f}  "
          f"sRGB #{srgb8(i)[0]:02X}{srgb8(i)[1]:02X}{srgb8(i)[2]:02X}")
