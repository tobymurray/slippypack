import math
def ntiles(w,s,e,n,z):
    def xt(lon): return (lon+180)/360*2**z
    def yt(lat):
        r=math.radians(lat)
        return (1-math.log(math.tan(r)+1/math.cos(r))/math.pi)/2*2**z
    x0,x1=math.floor(xt(w)),math.floor(xt(e))
    y0,y1=math.floor(yt(n)),math.floor(yt(s))
    return (x1-x0+1)*(y1-y0+1)

# measured spec-RLE ratio by zoom on the real 687-tile Athens pack (osm-carto quantised)
RLE={12:.303,13:.196,14:.144,15:.090,16:.060,17:.045,18:.035,10:.45,11:.38}
def pack(bbox,zmin,zmax,dim=128,ratio=None):
    n=0; body=0
    for z in range(zmin,zmax+1):
        t=ntiles(*bbox,z); n+=t
        body += t*dim*dim*(ratio if ratio else RLE.get(z,.06))
    return n, 292+20*n+body+4

REGIONS={
 "Saturday run, 8x6 km (Athens ON)": (-76.015,44.590,-75.889,44.662),
 "City, 25x20 km (Vancouver)":       (-123.27,49.20,-122.99,49.32),
 "National park (Lake District)":    (-3.45,54.30,-2.85,54.80),
 "Metro region 90 km (Gr. Vancouver)":(-123.3,49.0,-122.4,49.4),
 "Small country (Switzerland)":      (5.96,45.82,10.49,47.81),
 "England+Wales":                    (-5.72,49.9,1.77,55.8),
}
print("=== pack size at measured RLE ratios, tile_dim=128 ===")
print(f"{'region':38} {'z12-16':>18} {'z10-17':>18}")
for k,bb in REGIONS.items():
    n1,s1=pack(bb,12,16); n2,s2=pack(bb,10,17)
    print(f"{k:38} {n1:7,}t {s1/1048576:7.1f}MiB {n2:7,}t {s2/1048576:7.1f}MiB")

print()
print("=== global archive, watch-tuned ABGR2222 + spec RLE, tile_dim=128 ===")
LAND=0.148  # fraction of the z-grid that is land between 85S..85N; the rest is one blue tile
tot=0; raw=0
for z in range(0,16):
    t=4**z
    eff=t if z<8 else t*LAND
    b=eff*128*128*RLE.get(z,.06)
    tot+=b; raw+=eff*16384
    if z>=10: print(f"  z{z:2d}: {eff/1e6:10.2f} M land tiles  {b/1e9:8.2f} GB")
print(f"  z0-15 total: {tot/1e9:.1f} GB compressed   ({raw/1e9:.0f} GB raw)")
t16=4**16*LAND; b16=t16*128*128*RLE[16]
print(f"  adding z16:  +{t16/1e6:.0f} M tiles  +{b16/1e9:.1f} GB  -> {(tot+b16)/1e9:.0f} GB")

print()
print("=== hosting cost / month (assumptions stated) ===")
# R2/B2-class object storage: $0.015/GB-mo, ZERO egress. S3: $0.023/GB-mo + $0.09/GB egress.
for name,store,egress in (("Cloudflare R2",0.015,0.0),("Backblaze B2",0.006,0.0),
                          ("AWS S3 + CloudFront",0.023,0.085)):
    for owners,packs_per_owner,mb in ((100,2,8),(10_000,2,8),(100_000,2,8)):
        gb_out=owners*packs_per_owner*mb/1024
        arch=(tot+b16)/1e9
        c=arch*store + gb_out*egress
        print(f"  {name:22} {owners:7,} owners: storage ${arch*store:8,.0f} "
              f"+ egress ${gb_out*egress:8,.0f} = ${c:9,.0f}/mo")
    print()
