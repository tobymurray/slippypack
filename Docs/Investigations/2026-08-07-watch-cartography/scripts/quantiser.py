"""Does spec 9.1.1's canonical quantiser suit the LS012B7DD06A?

Spec 9.1.1: quanta {0,1,2,3} "displayed as {0, 85, 170, 255} (8-bit equivalents)",
thresholds at 42/127/212. That is exactly right for a framebuffer whose 2-bit value is
bit-replicated to 8 bits and shown on a normal sRGB display.

E1 established the panel's four levels are AREA-modulated => linear in reflectance.
So what 8-bit sRGB value does each level actually LOOK like?
"""
import numpy as np
from panel import ALL_XYZ, ALL_LAB, code_index, WHITE_XYZ

def Y_to_srgb8(Y):
    lin=np.clip(Y,0,1)
    s=np.where(lin<=0.0031308, lin*12.92, 1.055*lin**(1/2.4)-0.055)
    return s*255

print("The neutral ramp, as the panel actually displays it:")
lv=[]
for k in range(4):
    Y=ALL_XYZ[code_index(k,k,k)][1]/WHITE_XYZ[1]
    s=Y_to_srgb8(Y); lv.append(s)
    print(f"  quantum {k}: L*={ALL_LAB[code_index(k,k,k)][0]:6.2f}  Y/Yw={Y:.4f}  "
          f"sRGB-equivalent {s:5.1f}   spec assumes {85*k:3d}")
lv=np.array(lv)
mid=(lv[:-1]+lv[1:])/2
print(f"\nPanel-correct thresholds : {mid.round(0)}")
print( "Spec 9.1.1 thresholds    : [ 42. 127. 212.]")

# how often do they disagree, over the grey axis and over random colours?
def spec_q(v):  return np.digitize(v,[43,128,213])
def panel_q(v): return np.digitize(v,mid)
g=np.arange(256)
d=spec_q(g)!=panel_q(g)
print(f"\ngrey values where the two disagree: {d.sum()} of 256 ({d.mean()*100:.0f} %)")
runs=[]
start=None
for i in range(256):
    if d[i] and start is None: start=i
    if not d[i] and start is not None: runs.append((start,i-1)); start=None
if start is not None: runs.append((start,255))
for a,b in runs:
    print(f"  input {a:3d}-{b:3d}: spec picks {spec_q(np.array([a]))[0]}, "
          f"panel-correct is {panel_q(np.array([a]))[0]}  (spec is too LIGHT)")

rng=np.random.default_rng(0); rnd=rng.integers(0,256,(200000,3))
dd=(spec_q(rnd)!=panel_q(rnd)).any(1)
print(f"\nrandom RGB888 colours quantised differently: {dd.mean()*100:.1f} %")

# the error, in dE2000, for the grey axis
from panel import ciede2000, XYZ_to_Lab, ALL_XYZ as AX
def lab_of_q(q): return ALL_LAB[code_index(q,q,q)]
def srgb_grey_lab(v):
    lin=(v/255.0); lin=np.where(lin<=0.04045,lin/12.92,((lin+0.055)/1.055)**2.4)
    return XYZ_to_Lab(np.stack([lin*0.9505,lin*1.0,lin*1.089],-1), np.array([0.9505,1.0,1.089]))
tgt=srgb_grey_lab(g.astype(float))
e_spec=ciede2000(tgt, lab_of_q(spec_q(g)))
e_pan =ciede2000(tgt, lab_of_q(panel_q(g)))
print(f"\nmean dE2000 to the intended grey, over the 0-255 axis:")
print(f"  spec 9.1.1 quantiser : {e_spec.mean():6.2f}   worst {e_spec.max():6.2f}")
print(f"  panel-aware          : {e_pan.mean():6.2f}   worst {e_pan.max():6.2f}")
