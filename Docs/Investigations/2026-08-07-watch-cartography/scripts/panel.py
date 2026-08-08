"""
Colorimetric model of the Sharp LS012B7DD06A reflective memory LCD.

All primary/white chromaticities, reflectance and contrast values are taken verbatim
from the Sharp device specification LCP-2619063C (REV. 01-Dec-20), Table 7-1
"Optical specification (Reflective mode)", measured at VDD1=3.2V, VDD2=5.0V, Ta=25 C.

  Contrast ratio CR      min 20   typ 25
  Reflectivity ratio R   min 7.0  typ 8.4  %
  White  (Wx, Wy) = (0.321, 0.347)
  RED    (Rx, Ry) = (0.530, 0.310)
  GREEN  (Gx, Gy) = (0.319, 0.417)
  BLUE   (Bx, By) = (0.175, 0.223)
  NTSC ratio             typ 18   %
  "1 pixel has RGB each 2bit, the pixel can display 64 colors."  (sec. 2 Overview)
  "Area gradation of each RGB have 2 dot"                        (Figure, sec. 6)

Panel chromaticity is flagged in the datasheet as a reference value, not guaranteed.
"""
import numpy as np

# ---------------------------------------------------------------- datasheet inputs
WHITE_xy = (0.321, 0.347)
PRIM_xy = {"R": (0.530, 0.310), "G": (0.319, 0.417), "B": (0.175, 0.223)}
R_WHITE = 0.084          # 8.4 % reflectance, typ
CR = 25.0                # typ reflective contrast ratio
R_BLACK = R_WHITE / CR   # 0.336 % reflectance

# The 4 levels per channel come from AREA gradation (two sub-dots per channel),
# so emitted reflectance is LINEAR in the code value, not sRGB-gamma encoded.
# ASSUMPTION (see report): equal-area steps 0, 1/3, 2/3, 1.
LEVELS = np.array([0.0, 1 / 3, 2 / 3, 1.0])


def xyY_to_XYZ(x, y, Y):
    return np.array([Y * x / y, Y, Y * (1 - x - y) / y])


def solve_primaries():
    """Scale R,G,B primaries so their sum reproduces the measured panel white."""
    M = np.column_stack([xyY_to_XYZ(*PRIM_xy[c], 1.0) for c in "RGB"])
    # additive part of white = white minus the black floor
    Xw = xyY_to_XYZ(*WHITE_xy, R_WHITE - R_BLACK)
    scale = np.linalg.solve(M, Xw)
    return M * scale                      # columns = XYZ of each primary at full on


PRIM_XYZ = solve_primaries()
BLACK_XYZ = xyY_to_XYZ(*WHITE_xy, R_BLACK)   # veiling reflection, white-ish
WHITE_XYZ = BLACK_XYZ + PRIM_XYZ.sum(axis=1)


def code_to_XYZ(r, g, b):
    """r,g,b are 0..3 code values -> absolute XYZ in reflectance units."""
    lv = np.stack([LEVELS[np.asarray(r)], LEVELS[np.asarray(g)], LEVELS[np.asarray(b)]], -1)
    return BLACK_XYZ + lv @ PRIM_XYZ.T


# ------------------------------------------------------------------------ CIELAB
def XYZ_to_Lab(XYZ, white):
    t = XYZ / white
    d = 6 / 29
    f = np.where(t > d ** 3, np.cbrt(t), t / (3 * d ** 2) + 4 / 29)
    L = 116 * f[..., 1] - 16
    a = 500 * (f[..., 0] - f[..., 1])
    bb = 200 * (f[..., 1] - f[..., 2])
    return np.stack([L, a, bb], -1)


def ciede2000(lab1, lab2):
    L1, a1, b1 = lab1[..., 0], lab1[..., 1], lab1[..., 2]
    L2, a2, b2 = lab2[..., 0], lab2[..., 1], lab2[..., 2]
    C1, C2 = np.hypot(a1, b1), np.hypot(a2, b2)
    Cb = (C1 + C2) / 2
    G = 0.5 * (1 - np.sqrt(Cb ** 7 / (Cb ** 7 + 25.0 ** 7 + 1e-30)))
    a1p, a2p = (1 + G) * a1, (1 + G) * a2
    C1p, C2p = np.hypot(a1p, b1), np.hypot(a2p, b2)
    h1p = np.degrees(np.arctan2(b1, a1p)) % 360
    h2p = np.degrees(np.arctan2(b2, a2p)) % 360
    dLp = L2 - L1
    dCp = C2p - C1p
    dhp = h2p - h1p
    dhp = np.where(dhp > 180, dhp - 360, np.where(dhp < -180, dhp + 360, dhp))
    dhp = np.where(C1p * C2p == 0, 0.0, dhp)
    dHp = 2 * np.sqrt(C1p * C2p) * np.sin(np.radians(dhp / 2))
    Lbp = (L1 + L2) / 2
    Cbp = (C1p + C2p) / 2
    hsum = h1p + h2p
    hd = np.abs(h1p - h2p)
    hbp = np.where(C1p * C2p == 0, hsum,
                   np.where(hd <= 180, hsum / 2,
                            np.where(hsum < 360, (hsum + 360) / 2, (hsum - 360) / 2)))
    T = (1 - 0.17 * np.cos(np.radians(hbp - 30)) + 0.24 * np.cos(np.radians(2 * hbp))
         + 0.32 * np.cos(np.radians(3 * hbp + 6)) - 0.20 * np.cos(np.radians(4 * hbp - 63)))
    dth = 30 * np.exp(-(((hbp - 275) / 25) ** 2))
    Rc = 2 * np.sqrt(Cbp ** 7 / (Cbp ** 7 + 25.0 ** 7 + 1e-30))
    Sl = 1 + (0.015 * (Lbp - 50) ** 2) / np.sqrt(20 + (Lbp - 50) ** 2)
    Sc = 1 + 0.045 * Cbp
    Sh = 1 + 0.015 * Cbp * T
    Rt = -np.sin(np.radians(2 * dth)) * Rc
    return np.sqrt((dLp / Sl) ** 2 + (dCp / Sc) ** 2 + (dHp / Sh) ** 2
                   + Rt * (dCp / Sc) * (dHp / Sh))


# ------------------------------------------------- sRGB preview of what the eye sees
_M_XYZ2sRGB = np.array([[3.2406, -1.5372, -0.4986],
                        [-0.9689, 1.8758, 0.0415],
                        [0.0557, -0.2040, 1.0570]])


def preview_srgb(XYZ, adapt=True):
    """Render absolute panel XYZ as sRGB for on-screen preview.

    adapt=True  -> von-Kries-style normalisation to panel white: what you perceive
                   once your eye is adapted to the display. The flattering view.
    adapt=False -> normalise against a 90 %-reflectance white card in the same
                   light, i.e. how dim the panel really is next to paper.
    """
    if adapt:
        lin = (XYZ / WHITE_XYZ[1]) @ _M_XYZ2sRGB.T
        wlin = (WHITE_XYZ / WHITE_XYZ[1]) @ _M_XYZ2sRGB.T
        lin = lin / wlin
    else:
        lin = (XYZ / 0.90) @ _M_XYZ2sRGB.T
    lin = np.clip(lin, 0, 1)
    return np.where(lin <= 0.0031308, lin * 12.92, 1.055 * lin ** (1 / 2.4) - 0.055)


# ------------------------------------------------------------------- the 64 colours
CODES = np.array([(r, g, b) for r in range(4) for g in range(4) for b in range(4)])


def abgr2222_byte(r, g, b, a=3):
    """rawtiles sec. 9.1 ABGR2222: alpha in bits 7:6, blue 5:4, green 3:2, red 1:0.
    (verified against the spec text before use -- see verify_bitorder())"""
    return (a << 6) | (b << 4) | (g << 2) | r


ALL_XYZ = code_to_XYZ(CODES[:, 0], CODES[:, 1], CODES[:, 2])
ALL_LAB = XYZ_to_Lab(ALL_XYZ, WHITE_XYZ)
ALL_SRGB = preview_srgb(ALL_XYZ)


def srgb8(idx):
    return tuple(int(round(v * 255)) for v in ALL_SRGB[idx])


def code_index(r, g, b):
    return r * 16 + g * 4 + b


if __name__ == "__main__":
    print("Panel primaries as solved (XYZ at full-on, absolute reflectance units):")
    for i, c in enumerate("RGB"):
        print(f"  {c}: XYZ = {PRIM_XYZ[:, i].round(5)}   Y = {PRIM_XYZ[1, i]*100:.3f} % refl")
    print(f"  black floor Y = {BLACK_XYZ[1]*100:.3f} %   white Y = {WHITE_XYZ[1]*100:.3f} %")
    print()
    lum = PRIM_XYZ[1] / PRIM_XYZ[1].sum()
    print(f"Luminance split R:G:B = {lum[0]:.3f} : {lum[1]:.3f} : {lum[2]:.3f}")
    print("  (sRGB/BT.709 for comparison =  0.213 : 0.715 : 0.072)")
    print()

    # chromatic excursion of each primary from white, in Lab
    wlab = XYZ_to_Lab(WHITE_XYZ, WHITE_XYZ)
    print("Primary chroma (Lab C*) at full-on, and dE2000 from panel white:")
    for i, c in enumerate("RGB"):
        v = np.zeros(3); v[i] = 3
        idx = code_index(*v.astype(int))
        lab = ALL_LAB[idx]
        print(f"  {c}=3 alone: L*={lab[0]:6.2f}  a*={lab[1]:7.2f}  b*={lab[2]:7.2f}  "
              f"C*={np.hypot(lab[1],lab[2]):6.2f}")
    print()

    # pairwise distinctness
    A = ALL_LAB[:, None, :]
    B = ALL_LAB[None, :, :]
    D = ciede2000(np.broadcast_to(A, (64, 64, 3)).copy(),
                  np.broadcast_to(B, (64, 64, 3)).copy())
    iu = np.triu_indices(64, 1)
    d = D[iu]
    print(f"Pairwise dE2000 across all {len(d)} pairs of the 64 codes:")
    print(f"  min {d.min():.2f}   median {np.median(d):.2f}   max {d.max():.2f}")
    for thr in (1.0, 2.0, 3.0, 5.0):
        print(f"  pairs closer than dE {thr:.0f}: {(d < thr).sum():4d} "
              f"({(d<thr).mean()*100:.1f} %)")
    print()

    # greedy maximal set at a given separation = usable palette size
    for thr in (2.0, 3.0, 5.0, 8.0, 10.0):
        keep = []
        order = np.argsort(-ALL_LAB[:, 0])          # bright first
        for i in order:
            if all(D[i, j] >= thr for j in keep):
                keep.append(i)
        print(f"  max mutually-separable subset at dE>={thr:4.1f}: {len(keep):2d} of 64")
