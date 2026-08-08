"""
X1/X2 - the same map scene rendered two ways:

  A. "quantise-after"  : rendered in sRGB with anti-aliasing in an osm-carto-like
                         palette, then nearest-neighbour quantised to ABGR2222.
  B. "palette-first"   : rendered directly into code indices with NO anti-aliasing,
                         every feature class occupying exactly one of the 64 codes.

Geometry is synthetic but built to exercise the real cases: a road hierarchy with
casing, water, woodland, contours, buildings, a dashed path, and labels.
It is NOT real OSM geometry -- see the investigation README for why and what that
limits. The palette conclusions transfer; the byte sizes must be re-measured on a
real renderer before they are quoted as product numbers.
"""
import numpy as np
from PIL import Image, ImageDraw, ImageFont
from panel import CODES, ALL_SRGB, ALL_LAB, code_index

def C(r, g, b):
    return (3 << 6) | (b << 4) | (g << 2) | r

# ----------------------------------------------------------------- the palette
# Slot names are semantic. Byte values are the ABGR2222 codes chosen for them.
PAL = {
    "paper":      C(3, 3, 3),   # L*100.0  the ground
    "landuse":    C(2, 3, 2),   # L* 95.7  built-up wash, one step off paper
    "wood":       C(0, 2, 1),   # L* 77.3  strongest usable green, 1.9:1 vs paper
    "wood_lt":    C(1, 3, 1),   # L* 91.1  light green for large forest blocks
    "water":      C(0, 1, 3),   # L* 70.4  2.8:1, clearly not-land, still recessive
    "water_dk":   C(0, 0, 3),   # L* 51.8  5.0:1, waterway lines and lake edges
    "contour":    C(1, 1, 0),   # L* 62.2  warm mid, reads as terrain not as road
    "building":   C(2, 2, 2),   # L* 86.0  context only
    "road_major": C(0, 0, 0),   # L* 23.7  25:1  the darkest thing on the map
    "road_minor": C(1, 0, 0),   # L* 36.6  10.7:1 second ink, warm
    "path":       C(0, 0, 1),   # L* 36.6  10.7:1 second ink, cool - distinct from road
    "ink":        C(0, 0, 0),   # text
    "halo":       C(3, 3, 3),   # text halo == paper
    "trace":      C(3, 0, 0),   # L* 51.8  C*57 the most saturated code on the panel
}
# osm-carto-ish sRGB for the quantise-after arm
SRGB = {
    "paper": (242, 239, 233), "landuse": (233, 229, 220), "wood": (173, 209, 158),
    "wood_lt": (200, 226, 189), "water": (170, 211, 223), "water_dk": (120, 180, 200),
    "contour": (198, 168, 130), "building": (217, 208, 201),
    "road_major": (248, 213, 122), "road_minor": (255, 255, 255), "path": (150, 100, 90),
    "ink": (60, 60, 60), "halo": (255, 255, 255), "trace": (220, 30, 120),
}

W = H = 240

def geometry(draw, col, aa_scale=1):
    """Draw the scene. `col` maps slot name -> whatever colour type the target wants."""
    s = aa_scale
    def L(pts, key, width):
        draw.line([(x * s, y * s) for x, y in pts], fill=col[key], width=max(1, int(width * s)))
    def P(pts, key):
        draw.polygon([(x * s, y * s) for x, y in pts], fill=col[key])

    draw.rectangle([0, 0, W * s, H * s], fill=col["paper"])
    # ---- landuse
    P([(0, 0), (240, 0), (240, 96), (150, 110), (60, 88), (0, 100)], "landuse")
    P([(0, 150), (70, 140), (120, 170), (110, 240), (0, 240)], "wood_lt")
    P([(10, 168), (62, 158), (98, 182), (92, 228), (18, 232)], "wood")
    # ---- water: a lake and a river
    P([(150, 128), (208, 120), (238, 152), (232, 200), (176, 206), (146, 172)], "water")
    L([(240, 132), (206, 126), (176, 140), (152, 168), (150, 200), (130, 232)], "water_dk", 2)
    # ---- contours (terrain), drawn before roads so roads win
    for k, base in enumerate((196, 176, 158, 142)):
        pts = [(x, base + 14 * np.sin(x / 34.0 + k) - k * 2) for x in range(0, 245, 5)]
        L(pts, "contour", 1)
    # ---- buildings
    rng = np.random.default_rng(7)
    for _ in range(38):
        bx = rng.integers(8, 226); by = rng.integers(8, 96)
        bw = int(rng.integers(4, 9)); bh = int(rng.integers(4, 9))
        draw.rectangle([bx * s, by * s, (bx + bw) * s, (by + bh) * s], fill=col["building"])
    # ---- road hierarchy. Casing first (paper-coloured halo), then the ink line.
    major = [(0, 62), (66, 58), (120, 70), (168, 64), (240, 74)]
    major2 = [(128, 0), (122, 46), (132, 96), (126, 158), (140, 240)]
    minors = [[(20, 0), (26, 60)], [(60, 12), (66, 58), (54, 130)],
              [(168, 64), (180, 20)], [(168, 64), (210, 92), (240, 88)],
              [(66, 58), (78, 108), (120, 122)]]
    for m in (major, major2):
        L(m, "halo", 7); L(m, "road_major", 4)
    for m in minors:
        L(m, "halo", 5); L(m, "road_minor", 2)
    # ---- a dashed path (trail)
    trail = [(30, 236), (52, 208), (70, 190), (96, 186), (118, 168), (146, 150)]
    for i in range(len(trail) - 1):
        (x0, y0), (x1, y1) = trail[i], trail[i + 1]
        n = int(np.hypot(x1 - x0, y1 - y0) / 5)
        for j in range(0, max(n, 1), 2):
            t0, t1 = j / max(n, 1), min((j + 1) / max(n, 1), 1.0)
            L([(x0 + (x1 - x0) * t0, y0 + (y1 - y0) * t0),
               (x0 + (x1 - x0) * t1, y0 + (y1 - y0) * t1)], "path", 2)
    return major, major2


def font(size):
    for p in ("/System/Library/Fonts/Supplemental/Arial Bold.ttf",
              "/System/Library/Fonts/Helvetica.ttc",
              "/System/Library/Fonts/SFNSMono.ttf"):
        try:
            return ImageFont.truetype(p, size)
        except OSError:
            continue
    return ImageFont.load_default()


def label(draw, xy, text, col, size, s=1, halo=True):
    f = font(int(size * s))
    x, y = xy[0] * s, xy[1] * s
    if halo:
        for dx in (-s, 0, s):
            for dy in (-s, 0, s):
                if dx or dy:
                    draw.text((x + dx, y + dy), text, font=f, fill=col["halo"])
    draw.text((x, y), text, font=f, fill=col["ink"])


# =========================================================== arm B: palette-first
def hard_text(buf, xy, text, size, ink, halo):
    """Aliased text: render the glyph coverage mask, threshold it, stamp one code.

    This is the whole point of palette-first -- text must occupy exactly one code
    or the blit-time LUT (and the RLE run structure) both break.
    """
    m = Image.new("L", (W, H), 0)
    ImageDraw.Draw(m).text(xy, text, font=font(size), fill=255)
    core = np.array(m) > 128
    if halo is not None:
        grown = core.copy()
        for dx in (-1, 0, 1):
            for dy in (-1, 0, 1):
                grown |= np.roll(np.roll(core, dy, 0), dx, 1)
        buf[grown & ~core] = halo
    buf[core] = ink
    return buf


def render_palette_first():
    img = Image.new("P", (W, H))
    d = ImageDraw.Draw(img)
    geometry(d, PAL)
    buf = np.array(img, dtype=np.uint8)
    hard_text(buf, (6, 46), "MAIN ST", 12, PAL["ink"], PAL["halo"])
    hard_text(buf, (156, 176), "Mill Pond", 11, PAL["ink"], PAL["halo"])
    hard_text(buf, (20, 200), "Bear Wood", 11, PAL["ink"], PAL["halo"])
    return buf


# ========================================================== arm A: quantise-after
_LAB64 = ALL_LAB
def srgb_to_lab(rgb):
    a = np.asarray(rgb, float) / 255.0
    lin = np.where(a <= 0.04045, a / 12.92, ((a + 0.055) / 1.055) ** 2.4)
    M = np.array([[0.4124, 0.3576, 0.1805], [0.2126, 0.7152, 0.0722], [0.0193, 0.1192, 0.9505]])
    XYZ = lin @ M.T
    wp = np.array([0.9505, 1.0, 1.089])
    t = XYZ / wp
    d = 6 / 29
    f = np.where(t > d ** 3, np.cbrt(t), t / (3 * d ** 2) + 4 / 29)
    return np.stack([116 * f[..., 1] - 16, 500 * (f[..., 0] - f[..., 1]), 200 * (f[..., 1] - f[..., 2])], -1)


def quantise(rgb_img, dither=False):
    """Nearest-neighbour in Lab against the 64 panel colours, optional ordered dither."""
    src = srgb_to_lab(np.asarray(rgb_img, float))
    if dither:
        bayer = np.array([[0, 8, 2, 10], [12, 4, 14, 6], [1, 9, 3, 11], [13, 5, 15, 7]]) / 16.0 - 0.5
        b = np.tile(bayer, (src.shape[0] // 4 + 1, src.shape[1] // 4 + 1))[:src.shape[0], :src.shape[1]]
        src = src + b[..., None] * np.array([14.0, 6.0, 6.0])
    flat = src.reshape(-1, 3)
    out = np.empty(flat.shape[0], np.uint8)
    step = 20000
    for i in range(0, flat.shape[0], step):
        chunk = flat[i:i + step]
        d = ((chunk[:, None, :] - _LAB64[None, :, :]) ** 2).sum(-1)
        idx = d.argmin(1)
        out[i:i + step] = [(3 << 6) | (CODES[k][2] << 4) | (CODES[k][1] << 2) | CODES[k][0] for k in idx]
    return out.reshape(src.shape[:2])


def render_srgb(scale=1):
    img = Image.new("RGB", (W * scale, H * scale), SRGB["paper"])
    d = ImageDraw.Draw(img)
    geometry(d, SRGB, aa_scale=scale)
    label(d, (6, 46), "MAIN ST", SRGB, 12, s=scale)
    label(d, (156, 176), "Mill Pond", SRGB, 11, s=scale)
    label(d, (20, 200), "Bear Wood", SRGB, 11, s=scale)
    return img
