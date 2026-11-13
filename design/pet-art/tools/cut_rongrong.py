"""Cut the 茸茸 (sage sheep) source illustration into rig layers.

A sheep's wool has no outline that closes its parts: the head's cheeks run
into the shoulders, and the chest tuft fades into the face. So the parts are
split along hand-placed curves (the seam between head and body, the right
ear's faint upper edge) laid over the drawn lines where there are any, and
every line is given to one part:

  body   the wool torso, the tuft, the legs and the tail knob. Under the head it
         is continued as a rounded shoulder of wool with its own outline, which
         shows when the head tilts or lifts.
  ear_l  hangs behind the head: its root is continued under the head's wool.
  head   the wool cap, the face (eyes, brows and mouth painted out; the nose
         stays), the flower, and the seam line itself. Closed under the right
         ear, with a cheek outline that was never drawn.
  ear_r  hangs in FRONT of the head, as drawn (see "order" in rig.json): its
         whole outline is its own, and its root fades out under the flower.

The drawing is scaled into rig space first (the sheep is drawn larger than the
others), so all landmarks below are in that work space: 1024 px = 512 rig px.

    python design/pet-art/tools/cut_rongrong.py
"""
import json
import os
from pathlib import Path

import numpy as np
from PIL import Image
from scipy import ndimage as ndi

from cut_zimeng_back import fill_from, line_mask, poly_mask, smooth

ROOT = Path(__file__).resolve().parents[1] / "rongrong"
OUT = ROOT / "rig"
SCALE = 0.5                 # work space -> rig space
FIT, PASTE = 0.944, (41, 34)   # source -> work space: the figure 389 rig px tall, standing on y = 452
BG = (249, 243, 234)
DEBUG = os.environ.get("RONGRONG_DEBUG")   # a folder for pictures of the masks, when looking for a mistake

# Work-space landmarks, read off the illustration.
SEED_HEAD, SEED_BODY = (500, 300), (520, 720)
SEED_EAR_L, SEED_EAR_R = (220, 440), (790, 470)
SEED_LEGS = [(410, 860), (545, 870), (690, 840)]
SEED_TAIL = (832, 610)
FLOWER_BOX = (640, 290, 762, 412)
# The seam between head and body, left to right: the silhouette notch under the
# left cheek, the arcs of chest wool across the bottom of the face, the lilac
# line over the right shoulder, the short line under the right ear.
SEAM = [(278, 553), (284, 554), (300, 556), (314, 558), (324, 558), (332, 553), (345, 551), (355, 556), (362, 566), (368, 576),
        (375, 581), (388, 577), (400, 576), (410, 581), (418, 588), (440, 591), (460, 588), (480, 583), (495, 587), (508, 595),
        (525, 588), (535, 580), (545, 575), (560, 573), (575, 576), (580, 581), (610, 591), (640, 594), (670, 592), (700, 585),
        (715, 583), (730, 572), (738, 555), (737, 549), (745, 552), (760, 548), (767, 542), (772, 536), (776, 529)]
NECK = (500, 588)            # the point the head tilts about
# The right ear's upper edge is drawn faintly where it lies on the head, and its
# root has no line at all between the ear and the shadow of the flower.
EAR_R_TOP = [(731, 362), (740, 370), (750, 378), (765, 392), (782, 406), (800, 420), (820, 432), (840, 445), (855, 460)]
EAR_R_ROOT = [(682, 414), (684, 408), (689, 403), (698, 399)]
PIVOT_EAR_L, PIVOT_EAR_R = (280, 415), (710, 390)
# The left ear behind the head (invented): its outline carried on past where the
# head's wool covers it, upper edge and lower edge, and the root between the two.
EAR_L_LINE = [[(220, 390), (240, 372), (254, 360), (276, 346), (298, 338)], [(240, 490), (258, 484), (274, 471), (292, 462), (310, 457)]]
EAR_L_CAP = [(316, 352), (326, 396), (322, 440)]     # how far in the root goes
# The head's cheek where the right ear hangs over it (invented): two bumps of wool.
HEAD_UNDER_R = [(797, 392), (790, 405), (779, 428), (790, 440), (796, 462), (790, 484), (777, 498), (785, 508), (784, 522), (774, 535)]
# The body's shoulders under the head (invented): its outline from one side of the seam to the other.
DOME = [(284, 556), (276, 540), (270, 520), (275, 500), (292, 486), (330, 476), (400, 470), (500, 467), (600, 470),
        (680, 478), (735, 488), (765, 500), (778, 518), (774, 538)]
DOME_BUMP = (46, 7)          # it is wool: bumps this long and this high along that outline
EYE_BOX_L, EYE_BOX_R = (322, 412, 404, 508), (498, 424, 598, 532)
MOUTH_BOX = (412, 516, 450, 544)
GRAIN_PATCH = (430, 425, 500, 475)
EYE_BROW_PART = 0.42         # the brow lies within this much of the eye's height, from the top


def box_mask(shape, box):
    m = np.zeros(shape, bool)
    x0, y0, x1, y1 = box
    m[y0:y1, x0:x1] = True
    return m


def disk(r):
    y, x = np.mgrid[-r: r + 1, -r: r + 1]
    return x * x + y * y <= r * r


def eye_and_brow(lum, box):
    """The eye and the brow lying across its top: (eye, brow) masks.

    Both are the same dark ink and touch. The eye is an ellipse, fitted to the
    lower part of the blob's edge, where no brow is in the way; the brow is
    what of the blob lies outside it.
    """
    blob = ndi.binary_fill_holes((lum < 112) & box_mask(lum.shape, box))
    l, n = ndi.label(blob)
    blob = l == (1 + np.argmax(ndi.sum(blob, l, range(1, n + 1))))
    ys, xs = np.where(blob & ~ndi.binary_erosion(blob))
    low = ys > ys.min() + EYE_BROW_PART * (ys.max() - ys.min())
    x, y = xs[low].astype(float), ys[low].astype(float)
    ox, oy = x.mean(), y.mean()                                  # fit about the middle, for the arithmetic's sake
    A, B, C, D = np.linalg.lstsq(np.stack([(x - ox) ** 2, (y - oy) ** 2, x - ox, y - oy], 1), np.ones(len(x)), rcond=None)[0]
    cx, cy = ox - C / (2 * A), oy - D / (2 * B)
    k = 1 + C * C / (4 * A) + D * D / (4 * B)
    rx, ry = np.sqrt(k / A), np.sqrt(k / B)
    yy, xx = np.mgrid[: lum.shape[0], : lum.shape[1]]
    ell = ((xx - cx) / (rx + 0.5)) ** 2 + ((yy - cy) / (ry + 0.5)) ** 2 <= 1
    core = ell & ndi.binary_dilation(blob)
    brow = ndi.binary_opening(blob & ~ndi.binary_dilation(ell), iterations=1)
    return ndi.binary_dilation(core, iterations=3), ndi.binary_dilation(brow, iterations=3) & ~core


def scallop(pts, length, height, n=14):
    """A smooth curve turned into a row of wool bumps bulging to its left-hand side."""
    p = np.asarray(smooth(pts, 400), float)
    seg = np.hypot(*np.diff(p, axis=0).T)
    along = np.concatenate([[0], np.cumsum(seg)])
    count = max(2, round(along[-1] / length))
    out = []
    for i in range(count):
        a, b = along[-1] * i / count, along[-1] * (i + 1) / count
        for t in np.linspace(0, 1, n, endpoint=i == count - 1):
            d = a + (b - a) * t
            j = min(np.searchsorted(along, d), len(p) - 1)
            k = max(j - 1, 0)
            tx, ty = p[j] - p[k] if j != k else p[1] - p[0]
            norm = np.hypot(tx, ty) or 1
            h = height * np.sin(np.pi * t) ** 0.7
            out.append((p[j][0] + ty / norm * h, p[j][1] - tx / norm * h))
    return out


def wool_fill(rgb, hole, known, grain, sigma):
    """Paint `hole` from the nearest `known` colour, blurred wide: wool has no streaks."""
    idx = ndi.distance_transform_edt(~known, return_distances=False, return_indices=True)
    out = rgb.copy()
    out[hole] = rgb[idx[0][hole], idx[1][hole]]
    m = (known | hole).astype(np.float32)
    blur = ndi.gaussian_filter(out * m[..., None], (sigma, sigma, 0)) / np.maximum(ndi.gaussian_filter(m, sigma), 1e-3)[..., None]
    out[hole] = blur[hole]
    reps = (rgb.shape[0] // grain.shape[0] + 1, rgb.shape[1] // grain.shape[1] + 1, 1)
    out[hole] += np.tile(grain, reps)[: rgb.shape[0], : rgb.shape[1]][hole]
    return out


def diffuse_fill(rgb, hole, grain):
    """Fill `hole` by relaxing to the average of its neighbours (smooth areas only)."""
    out = rgb.copy()
    ys, xs = np.where(hole)
    y0, y1, x0, x1 = ys.min() - 4, ys.max() + 5, xs.min() - 4, xs.max() + 5
    win = out[y0:y1, x0:x1]
    h = hole[y0:y1, x0:x1]
    win[h] = win[~h].mean(0)
    for _ in range(900):
        avg = (np.roll(win, 1, 0) + np.roll(win, -1, 0) + np.roll(win, 1, 1) + np.roll(win, -1, 1)) / 4
        win[h] = avg[h]
    reps = (h.shape[0] // grain.shape[0] + 1, h.shape[1] // grain.shape[1] + 1, 1)
    win[h] += np.tile(grain, reps)[: h.shape[0], : h.shape[1]][h]
    return out


def save_layer(name, rgb, alpha, layers, pivot=None):
    ys, xs = np.where(alpha > 0.004)
    y0, y1 = max(ys.min() - 4, 0), min(ys.max() + 5, alpha.shape[0])
    x0, x1 = max(xs.min() - 4, 0), min(xs.max() + 5, alpha.shape[1])
    # Snap the crop to even pixels so the half-size layer lands on whole pixels.
    x0 -= x0 % 2; y0 -= y0 % 2; x1 += x1 % 2; y1 += y1 % 2
    a = alpha[y0:y1, x0:x1, None]
    pre = np.concatenate([rgb[y0:y1, x0:x1] * a, a * 255], -1)
    size = (int((x1 - x0) * SCALE), int((y1 - y0) * SCALE))
    small = np.stack(
        [np.asarray(Image.fromarray(pre[..., c].astype(np.float32), "F").resize(size, Image.LANCZOS)) for c in range(4)], -1
    )
    al = np.clip(small[..., 3:], 0, 255)
    col = np.where(al > 0.5, small[..., :3] / np.maximum(al / 255, 1e-4), 0)
    Image.fromarray(np.concatenate([np.clip(col, 0, 255), al], -1).round().astype(np.uint8), "RGBA").save(OUT / f"{name}.png")
    entry = {"name": name, "file": f"{name}.png", "x": x0 * SCALE, "y": y0 * SCALE, "w": size[0], "h": size[1]}
    if pivot:
        entry["pivot"] = [pivot[0] * SCALE, pivot[1] * SCALE]
    layers.append(entry)


def work_image():
    """The illustration in work space: scaled, and stood on the others' ground line."""
    im = Image.open(ROOT / "source.png").convert("RGB")
    side = round(im.width * FIT)
    work = Image.new("RGB", im.size, BG)
    work.paste(im.resize((side, side), Image.LANCZOS), PASTE)
    return np.asarray(work).astype(np.float32)


def main():
    OUT.mkdir(parents=True, exist_ok=True)
    rgb = work_image()
    shape = rgb.shape[:2]
    yy, xx = np.mgrid[: shape[0], : shape[1]]

    # Foreground: everything the background colour cannot reach from the border
    # (the largest piece: the watermark in the corner is another one).
    bg = np.array(BG, np.float32)
    lab, _ = ndi.label(np.sqrt(((rgb - bg) ** 2).sum(-1)) < 14)
    border = set(np.unique(np.concatenate([lab[0], lab[-1], lab[:, 0], lab[:, -1]]))) - {0}
    fg = ndi.binary_fill_holes(~np.isin(lab, list(border)))
    l2, n2 = ndi.label(fg)
    fg = l2 == (1 + np.argmax(ndi.sum(fg, l2, range(1, n2 + 1))))

    # The paper's grain leaves the mask's edge ragged, which shows as bristles on a dark desktop: round it off.
    fg = ndi.gaussian_filter(fg.astype(np.float32), 1.6) > 0.5

    grow = lambda m, n=9: ndi.binary_dilation(m, iterations=n)
    shrink = lambda m, n=1: ndi.binary_erosion(m, iterations=n)
    # A hard edge: one pixel in where two parts meet, two at the silhouette, where
    # the outline fades into the paper (no haze outside the figure).
    outside = np.clip(ndi.gaussian_filter(shrink(fg, 2).astype(np.float32), 0.9) * 1.15, 0, 1)
    edge = lambda m: np.clip(ndi.gaussian_filter(shrink(m).astype(np.float32), 0.9) * 1.15, 0, 1) * outside
    soft = lambda m, s=1.0: np.clip(ndi.gaussian_filter(m.astype(np.float32), s) * 1.2, 0, 1)
    sm = lambda pts: smooth(pts) if len(pts) >= 4 else list(pts)
    bar = lambda pts, w=5: line_mask(shape, pts, w) > 0          # as placed: the seam has corners
    dist = lambda m: ndi.distance_transform_edt(~m)

    # Outline strokes: darker than their surroundings.
    lum = rgb @ np.array([0.299, 0.587, 0.114], np.float32)
    line = ((ndi.gaussian_filter(lum, 6) - ndi.gaussian_filter(lum, 1.2)) > 5) & fg
    # The outer half of the outline is blended into the paper. On a dark desktop
    # that reads as a pale, speckled rim, so the outline's own colour is carried
    # out to the silhouette (`src` is the drawing with that done; `rgb` stays as drawn).
    src = rgb
    rgb = rgb.copy()
    rim = fg & ~shrink(fg, 5)
    darkest = np.stack([ndi.minimum_filter(src[..., c], size=7) for c in range(3)], -1)
    rgb[rim] = darkest[rim]
    bare, n_bare = ndi.label(fg & ~line & (np.sqrt(((src - bg) ** 2).sum(-1)) < 16))
    small = np.isin(bare, 1 + np.where(ndi.sum(np.ones(shape), bare, range(1, n_bare + 1)) < 120)[0]) & ndi.binary_dilation(line, iterations=3)
    small &= ~shrink(fg, 14)
    rgb[small] = darkest[small]

    # ---------------------------------------------------------------- who owns what
    seam_bar = bar(SEAM)
    reg, _ = ndi.label(fg & ~line & ~seam_bar & ~bar(EAR_R_TOP) & ~bar(EAR_R_ROOT))

    def at(p):
        assert reg[p[1], p[0]] != 0, f"seed {p} sits on a line"
        return reg == reg[p[1], p[0]]

    head_in, body_in = at(SEED_HEAD), at(SEED_BODY)
    ear_l_in, ear_r_in = at(SEED_EAR_L), at(SEED_EAR_R)
    for a, b, what in ((head_in, body_in, "the seam does not part head and body"), (head_in, ear_r_in, "the right ear is not closed off from the head"),
                       (body_in, ear_r_in, "the right ear is not closed off from the body"), (head_in, ear_l_in, "the left ear is not closed off from the head")):
        assert not (a & b).any(), what
    # The flower, petals and outline, is the head's.
    lilac = (src[..., 2] - src[..., 1] > 14) & (src[..., 0] - src[..., 1] > 8) & box_mask(shape, FLOWER_BOX) & fg
    flower = grow(ndi.binary_fill_holes(ndi.binary_closing(lilac, iterations=3)), 2)
    l3, n3 = ndi.label(flower)
    flower = l3 == (1 + np.argmax(ndi.sum(flower, l3, range(1, n3 + 1))))
    # The wool is drawn in many closed cells (the face, each cheek, the legs, the
    # tail knob): whatever lies above the seam is the head's, the rest the body's.
    above = poly_mask(shape, SEAM + [(shape[1] - 1, SEAM[-1][1]), (shape[1] - 1, 0), (0, 0), (0, SEAM[0][1])])
    cells = np.arange(1, reg.max() + 1)
    area = ndi.sum(np.ones(shape), reg, cells)
    up = ndi.sum(above, reg, cells) / np.maximum(area, 1)
    big = (area > 300) & ~np.isin(cells, [reg[SEED_EAR_L[1], SEED_EAR_L[0]], reg[SEED_EAR_R[1], SEED_EAR_R[0]]])
    head_in = np.isin(reg, cells[big & (up > 0.5)])
    body_in = np.isin(reg, cells[big & (up <= 0.5)])
    assert head_in[SEED_HEAD[1], SEED_HEAD[0]] and body_in[SEED_BODY[1], SEED_BODY[0]]
    for p in SEED_LEGS + [SEED_TAIL]:
        assert body_in[p[1], p[0]], f"{p} is not part of the body"

    # Every other pixel (lines, the bars, flecks closed off by a line) goes to the nearest part...
    sets = [head_in | flower, body_in & ~flower, ear_l_in & ~flower, ear_r_in & ~flower]
    seeds = np.zeros(shape, np.uint8)
    for i, m in enumerate(sets):
        seeds[m] = i + 1
    near = ndi.distance_transform_edt(seeds == 0, return_distances=False, return_indices=True)
    owner = seeds[near[0], near[1]]
    d_head, d_body, d_ear_l, d_ear_r = (dist(m) for m in sets)
    lineish = seeds == 0
    # ...except the lines two parts share, which go to the part drawn in front.
    R = 9
    owner[lineish & (d_head < R) & (d_body < R)] = 1        # the seam is the head's lower edge
    owner[lineish & (d_head < R) & (d_ear_l < R)] = 1       # the wool lies over the left ear
    own = d_ear_r < 7                                       # no further than its own outline is thick
    owner[lineish & (d_head < R) & own] = 4                 # the right ear lies over the wool
    owner[lineish & (d_body < R) & own] = 4
    H, B, EL, ER = ((owner == i + 1) & fg for i in range(4))
    ER &= ~flower

    patch = rgb[GRAIN_PATCH[1]:GRAIN_PATCH[3], GRAIN_PATCH[0]:GRAIN_PATCH[2]]
    grain = patch - ndi.gaussian_filter(patch, (2, 2, 0))
    ink = np.median(rgb[line & grow(head_in, 8) & ~shrink(fg, 12) & (yy < 400)], 0)      # the warm brown of the outline

    def inked(pts, width=7, colour=None, spline=True):
        """A stroke like the drawn ones: (colour, coverage)."""
        s = np.clip(ndi.gaussian_filter(line_mask(shape, sm(pts) if spline else list(pts), width), 1.3) * 1.15, 0, 1)
        return (ink if colour is None else colour), s

    def over(base, colour, a):
        return base * (1 - a[..., None] * 0.94) + colour * a[..., None] * 0.94

    # ---------------------------------------------------------------- face parts
    eye_l, brow_l = eye_and_brow(lum, EYE_BOX_L)
    eye_r, brow_r = eye_and_brow(lum, EYE_BOX_R)
    mouth = grow((lum < 150) & box_mask(shape, MOUTH_BOX), 3)

    # ---------------------------------------------------------------- right ear (in front of the head)
    # Its root has no edge: it fades out towards the flower over a few pixels, and
    # the head keeps the drawing there, so nothing is cut when it swings.
    root = np.clip((dist(flower) - 2) / 12, 0, 1)
    ear_r_a = edge(ER) * root
    ear_r_rgb = rgb

    # ---------------------------------------------------------------- head
    under_r = ER & poly_mask(shape, sm(HEAD_UNDER_R) + [(640, 545), (640, 340), (797, 340)])
    head = H | under_r
    head_rgb = rgb
    for hole in (eye_l | brow_l, eye_r | brow_r, mouth):
        head_rgb = diffuse_fill(head_rgb, grow(hole, 5), grain)
    # Under the ear: the drawing where the ear's root fades, wool further in.
    cheek = shrink(head_in, 6) & (xx > 640) & (yy > 360) & (yy < 590) & ~grow(flower, 6)
    wool = fill_from(head_rgb, under_r, cheek, grain)
    k = soft(under_r & (root >= 1), 2.5)[..., None] * (root >= 1)[..., None]
    head_rgb = head_rgb * (1 - k) + wool * k
    c, s = inked(HEAD_UNDER_R)
    s = s * grow(under_r, 3) * (root >= 1)
    head_rgb = over(head_rgb, c, s)
    # Its lower edge is a line where one is drawn. Where the seam only crosses
    # shading (under the left cheek, between the arcs of the chest) the head fades out instead.
    no_line = ndi.binary_opening(seam_bar & ~grow(line, 2), iterations=1)
    seam_line = line & grow(seam_bar, 5)
    fade = soft(grow(no_line, 7) & ~grow(seam_line, 4), 3.0)
    head_a = edge(head) * (1 - fade) + np.clip(ndi.gaussian_filter(head.astype(np.float32), 3.2), 0, 1) * outside * fade
    head_a = np.maximum(head_a, s * outside)

    # ---------------------------------------------------------------- left ear (behind the head)
    under_l = poly_mask(shape, sm(EAR_L_LINE[0][1:]) + sm(EAR_L_CAP) + sm(EAR_L_LINE[1][1:])[::-1]) & H
    ear_l = EL | under_l
    ear_fill = shrink(ear_l_in, 6) & (xx > 215) & (lum < 188)       # the green of the ear, not its pale rim
    ear_l_rgb = fill_from(rgb, under_l, ear_fill, grain)
    ear_l_a = edge(ear_l)
    for pts in EAR_L_LINE:
        c, s = inked(pts)
        s = s * grow(under_l, 2) * ~EL                 # only the part that was never drawn
        ear_l_rgb = over(ear_l_rgb, c, s)
        ear_l_a = np.maximum(ear_l_a, s * outside)

    # ---------------------------------------------------------------- body: shoulders of wool under the head
    dome_line = scallop(DOME, *DOME_BUMP)
    dome = poly_mask(shape, dome_line + [(x, y + 14) for x, y in SEAM[::-1]])
    body = ndi.binary_fill_holes(B | dome | (grow(B, R + 2) & H & grow(seam_bar, R)))
    body_known = shrink(body_in, 6) & (yy < 700) & (lum < 196)      # the green wool, not the white of the tuft
    # Where the head fades out, the body keeps the drawing a little way up, and that fades too.
    keep = np.maximum(B, soft(grow(no_line, 8) & ~grow(seam_line, 3) & ~ER, 3.5) * ~grow(seam_line, 2))[..., None]
    body_rgb = wool_fill(rgb, body & ~B, body_known, grain, 24) * (1 - keep) + rgb * keep
    c, s = inked(dome_line, spline=False)
    s = s * ~grow(B, 2)
    body_rgb = over(body_rgb, c, s)
    body_a = np.maximum(edge(body), s * outside)

    # ---------------------------------------------------------------- write
    for f in OUT.glob("*.png"):
        if f.stem in ("body", "ear_l", "ear_r", "head", "eye_l", "eye_r", "mouth"):
            f.unlink()
    layers = []
    save_layer("body", body_rgb, body_a, layers)
    save_layer("ear_l", ear_l_rgb, ear_l_a, layers, pivot=PIVOT_EAR_L)
    save_layer("ear_r", ear_r_rgb, ear_r_a, layers, pivot=PIVOT_EAR_R)
    save_layer("head", head_rgb, head_a, layers, pivot=NECK)
    save_layer("eye_l", rgb, soft(eye_l), layers)
    save_layer("eye_r", rgb, soft(eye_r), layers)
    save_layer("mouth", rgb, soft(mouth), layers)

    centre = lambda m: [float(np.where(m)[1].mean() * SCALE), float(np.where(m)[0].mean() * SCALE)]
    ys, xs = np.where(fg)
    # The white tuft on the chest: the pale, unsaturated wool of the body.
    pale = B & (lum > 205) & (src.max(-1) - src.min(-1) < 30) & (yy > 560)
    lt, nt = ndi.label(ndi.binary_opening(pale, iterations=3))
    pale = lt == (1 + np.argmax(ndi.sum(pale, lt, range(1, nt + 1))))
    ty, tx = np.where(pale)
    rig = {
        "id": "rongrong",
        "kind": "sheep",
        "size": [int(shape[1] * SCALE), int(shape[0] * SCALE)],
        "bbox": [float(xs.min() * SCALE), float(ys.min() * SCALE), float(xs.max() * SCALE), float(ys.max() * SCALE)],
        "anchors": {
            "eye_l": centre(eye_l), "eye_r": centre(eye_r), "mouth": centre(mouth),
            "neck": [NECK[0] * SCALE, NECK[1] * SCALE],
        },
        "tuft": [float(tx.min() * SCALE), float(ty.min() * SCALE), float(tx.max() * SCALE), float(ty.max() * SCALE)],
        # The drawing has the right ear hanging in front of the head's wool, and
        # the layers are cut for that: drawn before the head, most of it is covered.
        "order": ["body", "ear_l", "head", "ear_r", "eye_l", "eye_r", "mouth"],
        "layers": layers,
    }
    (OUT / "rig.json").write_text(json.dumps(rig, indent=2), "utf-8")

    # Rest-pose check: the stack, in rig.json's order, must rebuild the drawing.
    inner = shrink(fg, 8)   # the outermost pixels were recoloured on purpose (see `rim`)
    stack = [(body_rgb, body_a), (ear_l_rgb, ear_l_a), (head_rgb, head_a), (ear_r_rgb, ear_r_a),
             (rgb, soft(eye_l)), (rgb, soft(eye_r)), (rgb, soft(brow_l)), (rgb, soft(brow_r)), (rgb, soft(mouth))]
    comp = np.broadcast_to(bg, shape + (3,)).astype(np.float32).copy()
    for col, a in stack:
        comp = comp * (1 - a[..., None]) + col * a[..., None]
    err = np.abs(comp - src).max(-1)
    print(f"head {head.sum()}  body {body.sum()}  ear_l {ear_l.sum()}  ear_r {ER.sum()}  eyes {eye_l.sum()}/{eye_r.sum()}  brows {brow_l.sum()}/{brow_r.sum()}  mouth {mouth.sum()}")
    print(f"bbox {rig['bbox']}  tuft {rig['tuft']}")
    print(f"rest-pose error (with the brows): mean {err[inner].mean():.2f}  p99 {np.percentile(err[inner], 99):.1f}  max {err[inner].max():.0f}")
    bad = inner & (err > 12)
    if bad.any():
        lab2, n = ndi.label(grow(bad, 3))
        for i in np.argsort(ndi.sum(bad, lab2, range(1, n + 1)))[::-1][:8]:
            by, bx = np.where((lab2 == i + 1) & bad)
            print(f"  differs: {len(bx)} px around ({int(bx.mean())}, {int(by.mean())})")
    if DEBUG:
        d = Path(DEBUG)
        vis = np.zeros(shape + (3,), np.uint8)
        for m, colr in ((H, (90, 200, 120)), (B, (90, 120, 220)), (EL, (230, 180, 60)), (ER, (230, 90, 90)), (under_r, (150, 60, 60)), (under_l, (150, 120, 40)), (dome & ~B, (50, 70, 140))):
            vis[m] = colr
        vis[line] = vis[line] // 2 + 100
        vis[seam_bar] = (255, 255, 255)
        Image.fromarray(vis).save(d / "owners.png")
        Image.fromarray(np.clip(err * 6, 0, 255).astype(np.uint8)).save(d / "rest_error.png")
        Image.fromarray(np.clip(comp, 0, 255).astype(np.uint8)).save(d / "rest_comp.png")


if __name__ == "__main__":
    main()
