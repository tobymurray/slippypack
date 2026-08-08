"""Render ABGR2222 bytes as the LS012B7DD06A actually shows them."""
import numpy as np
from PIL import Image
from panel import CODES, ALL_SRGB, preview_srgb, code_to_XYZ

# lookup: byte value -> sRGB preview triple (display-adapted)
LUT = np.zeros((256,3), np.uint8)
for i,(r,g,b) in enumerate(CODES):
    v = (3<<6)|(b<<4)|(g<<2)|r
    LUT[v] = np.round(ALL_SRGB[i]*255)
# codes with alpha != 3 are illegal in v1 (spec 9.1 pins alpha opaque); render magenta
for v in range(256):
    if (v>>6)!=3: LUT[v]=(255,0,255)

# unadapted variant: how dim it is next to paper
XYZ = code_to_XYZ(CODES[:,0],CODES[:,1],CODES[:,2])
UN = preview_srgb(XYZ, adapt=False)
LUT_DIM = np.zeros((256,3), np.uint8)
for i,(r,g,b) in enumerate(CODES):
    LUT_DIM[(3<<6)|(b<<4)|(g<<2)|r] = np.round(UN[i]*255)

def show(buf, dim=False):
    """buf: (H,W) uint8 of ABGR2222 -> PIL RGB image"""
    return Image.fromarray((LUT_DIM if dim else LUT)[buf])

def upscale(img, n):
    return img.resize((img.width*n, img.height*n), Image.NEAREST)
