import struct, numpy as np
from PIL import Image, ImageDraw
from render import show, upscale

P="/Users/tobymurray/git/una-sdk/Docs/Investigations/2026-08-06-athens-pack/athens.rawtiles"
b=open(P,'rb').read()
D=struct.unpack_from('<H',b,60)[0]; N=struct.unpack_from('<I',b,88)[0]; IO=struct.unpack_from('<I',b,92)[0]
ents=[]
for i in range(N):
    o=IO+20*i
    z=b[o]; x,y,off,ln=struct.unpack_from('<IIII',b,o+4)
    ents.append((z,x,y,off,ln))

def tile(z,x,y):
    for (tz,tx,ty,off,ln) in ents:
        if (tz,tx,ty)==(z,x,y): return np.frombuffer(b,np.uint8,D*D,off).reshape(D,D)
    return None

# assemble a 3x3 mosaic at z16 around the middle of the pack's z16 block
z16=[e for e in ents if e[0]==16]
xs=sorted(set(e[1] for e in z16)); ys=sorted(set(e[2] for e in z16))
cx,cy = xs[len(xs)//2], ys[len(ys)//2]
for z,cxx,cyy,tag in [(16,cx,cy,'z16'),(14,None,None,'z14')]:
    if z==14:
        z14=[e for e in ents if e[0]==14]
        xs4=sorted(set(e[1] for e in z14)); ys4=sorted(set(e[2] for e in z14))
        cxx,cyy=xs4[len(xs4)//2],ys4[len(ys4)//2]
    mos=np.full((D*3,D*3),0xFF,np.uint8)
    for j,dy in enumerate((-1,0,1)):
        for i,dx in enumerate((-1,0,1)):
            t=tile(z,cxx+dx,cyy+dy)
            if t is not None: mos[j*D:(j+1)*D, i*D:(i+1)*D]=t
    show(mos).save(f"out/athens_{tag}_mosaic.png")
    # a single 240x240 viewport crop = what the watch actually shows
    off=(D*3-240)//2
    vp=mos[off:off+240, off:off+240]
    show(vp).resize((480,480), Image.NEAREST).save(f"out/athens_{tag}_viewport.png")
    show(vp, dim=True).resize((480,480), Image.NEAREST).save(f"out/athens_{tag}_viewport_daylight.png")
    u,c=np.unique(vp,return_counts=True)
    print(f"{tag} viewport: {len(u)} distinct codes; top: "
          + ", ".join(f"0x{v:02X}={n/vp.size*100:.1f}%" for v,n in
                      sorted(zip(u,c),key=lambda p:-p[1])[:5]))
