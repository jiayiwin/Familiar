"""Cut the 紫梦 (lop-eared bunny) source illustration into rig layers.

Regions are found from the drawn outline: the outline splits the figure into
closed areas, and two short hand-placed barriers close the ears at their roots.
Layers overlap at the joints, so the rest pose reproduces the source exactly.

    python design/pet-art/tools/cut_zimeng.py
"""
import json
from pathlib import Path

import numpy as np
from PIL import Image, ImageDraw
from scipy import ndimage as ndi

from cut_zimeng_back import fill_from, line_mask, poly_mask, smooth

ROOT = Path(__file__).resolve().parents[1] / "zimeng"
OUT = ROOT / "rig"
SCALE = 0.5  # 1024 source -> 512 rig space

# Source-space (1024) landmarks, read off the illustration.
BARRIER_L = [(347, 190), (296, 278)]           # left ear root: outline junction -> start of inner line
BARRIER_R = [[(670, 208), (652, 276)], [(660, 345), (678, 396)]]  # right ear root, above and below the star
SEED_EAR_L = (150, 500)
SEED_EAR_R = (850, 500)
SEED_STAR = (645, 312)
PIVOT_L = (342, 196)
PIVOT_R = (668, 214)
EYE_BOX_L = (270, 385, 380, 495)   # x0, y0, x1, y1
EYE_BOX_R = (500, 405, 610, 520)
MOUTH_BOX = (392, 482, 452, 518)
GRAIN_PATCH = (420, 250, 560, 350)
STAR_SPLIT = [(652, 276), (660, 345)]  # where the ear root runs under the star


# The left ear hangs behind the head, so half of it was never painted: this is
# its hidden inner edge, root to tip. The right ear lies over the head instead:
# that is the head's contour underneath it. Both are invented.
EAR_L_INNER = [(349, 192), (353, 240), (346, 300), (336, 360), (323, 430), (309, 495), (291, 545), (266, 579), (240, 593), (218, 595)]
HEAD_R_UNDER = [(640, 196), (670, 208), (708, 240), (735, 290), (750, 360), (756, 430), (760, 500), (772, 570), (795, 640)]


def seg_mask(shape, segs, width):
    im = Image.new("L", (shape[1], shape[0]), 0)
    d = ImageDraw.Draw(im)
    for a, b in segs:
        d.line([a, b], fill=255, width=width)
    return np.asarray(im) > 0


def box_mask(shape, box):
    m = np.zeros(shape, bool)
    x0, y0, x1, y1 = box
    m[y0:y1, x0:x1] = True
    return m


def diffuse_fill(rgb, hole):
    """Fill `hole` by relaxing to the average of its neighbours (smooth areas only)."""
    out = rgb.copy()
    ys, xs = np.where(hole)
    y0, y1, x0, x1 = ys.min() - 4, ys.max() + 5, xs.min() - 4, xs.max() + 5
    win = out[y0:y1, x0:x1]
    h = hole[y0:y1, x0:x1]
    win[h] = win[~h].mean(0)
    for _ in range(600):
        avg = (np.roll(win, 1, 0) + np.roll(win, -1, 0) + np.roll(win, 1, 1) + np.roll(win, -1, 1)) / 4
        win[h] = avg[h]
    # Put back a little of the paper grain, taken from a featureless patch of the forehead.
    gy0, gy1, gx0, gx1 = GRAIN_PATCH[1], GRAIN_PATCH[3], GRAIN_PATCH[0], GRAIN_PATCH[2]
    patch = rgb[gy0:gy1, gx0:gx1]
    grain = patch - ndi.gaussian_filter(patch, (2, 2, 0))
    reps = (h.shape[0] // grain.shape[0] + 1, h.shape[1] // grain.shape[1] + 1, 1)
    win[h] += np.tile(grain, reps)[: h.shape[0], : h.shape[1]][h]
    return out


def save_layer(name, rgb, alpha, layers, pivot=None, z=0):
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
    entry = {"name": name, "file": f"{name}.png", "x": x0 * SCALE, "y": y0 * SCALE, "w": size[0], "h": size[1], "z": z}
    if pivot:
        entry["pivot"] = [pivot[0] * SCALE, pivot[1] * SCALE]
    layers.append(entry)


def main():
    OUT.mkdir(parents=True, exist_ok=True)
    rgb = np.asarray(Image.open(ROOT / "source.png").convert("RGB")).astype(np.float32)
    shape = rgb.shape[:2]

    # Foreground: everything the background colour cannot reach from the border.
    bg = np.median(np.concatenate([rgb[:40].reshape(-1, 3), rgb[:, :40].reshape(-1, 3), rgb[:, -40:].reshape(-1, 3)]), 0)
    lab, _ = ndi.label(np.sqrt(((rgb - bg) ** 2).sum(-1)) < 14)
    border = set(np.unique(np.concatenate([lab[0], lab[-1], lab[:, 0], lab[:, -1]]))) - {0}
    fg = ndi.binary_fill_holes(~np.isin(lab, list(border)))
    l2, n2 = ndi.label(fg)
    fg = l2 == (1 + np.argmax(ndi.sum(fg, l2, range(1, n2 + 1))))
    alpha = np.clip(ndi.gaussian_filter(ndi.binary_erosion(fg).astype(np.float32), 0.9) * 1.15, 0, 1)

    # Outline strokes: darker than their surroundings.
    lum = rgb @ np.array([0.299, 0.587, 0.114], np.float32)
    line = ((ndi.gaussian_filter(lum, 6) - ndi.gaussian_filter(lum, 1.2)) > 5) & fg

    bar_l = seg_mask(shape, [BARRIER_L], 5)
    bar_r = seg_mask(shape, BARRIER_R, 5)
    free = fg & ~line & ~bar_l & ~bar_r
    reg, _ = ndi.label(free)
    at = lambda p: reg == reg[p[1], p[0]]
    ear_l_in, ear_r_in, star_in = at(SEED_EAR_L), at(SEED_EAR_R), at(SEED_STAR)
    grow = lambda m, n=9: ndi.binary_dilation(m, iterations=n)

    star = ndi.binary_fill_holes(grow(star_in, 10) & (line | star_in))
    star = ndi.binary_opening(star, iterations=2) | star_in
    # Thin slivers between an outline and the background belong to whichever part they hug.
    body_core = ndi.binary_opening(free & ~ear_l_in & ~ear_r_in, iterations=3)
    ear_l = grow(ear_l_in, 12) & fg & ~body_core & ~ear_r_in & ~bar_l
    ear_r = grow(ear_r_in, 12) & fg & ~body_core & ~ear_l_in & ~bar_r
    # Drop the stubs of body outline that stick out where an ear crosses it.
    ear_l, ear_r = (ndi.binary_opening(m, iterations=5) for m in (ear_l, ear_r))
    body = ndi.binary_fill_holes(grow(body_core, 12) & fg & ~ear_l_in & ~ear_r_in)

    # The right ear continues under the star: the ear side of the star is ear.
    (sx0, sy0), (sx1, sy1) = STAR_SPLIT
    yy, xx = np.mgrid[: shape[0], : shape[1]]
    soft_mask = lambda m, s=0.9: np.clip(ndi.gaussian_filter(m.astype(np.float32), s) * 1.2, 0, 1)
    ear_r |= star & (xx > sx0 + (yy - sy0) * (sx1 - sx0) / (sy1 - sy0))
    rgb_nostar = diffuse_fill(rgb, grow(star, 4))

    # The right ear reaches a little into the head at the root and fades out there.
    def root_feather(bar, ear, reach):
        d = ndi.distance_transform_edt(~bar)
        t = np.clip(1 - d / reach, 0, 1)
        return np.where(ear, 1.0, (t * t * (3 - 2 * t)) * (fg & ~ear))
    ear_r_a = root_feather(seg_mask(shape, [BARRIER_R[0], STAR_SPLIT], 5), ear_r, 46) * alpha

    patch_src = rgb[GRAIN_PATCH[1]:GRAIN_PATCH[3], GRAIN_PATCH[0]:GRAIN_PATCH[2]]
    grain = patch_src - ndi.gaussian_filter(patch_src, (2, 2, 0))
    ink = np.median(rgb[line & ndi.binary_dilation(body_core, iterations=9)], 0)

    def inked(pts, where):
        s = np.clip(ndi.gaussian_filter(line_mask(shape, pts, 7), 1.3) * 1.15, 0, 1)
        return s * where

    # Left ear, whole: the painted half plus the half the head covers. It is drawn
    # behind the body, so the invented half only shows once the ear swings out.
    inner_l = smooth(EAR_L_INNER)
    hidden = poly_mask(shape, inner_l + [(200, 575), (150, 450), (200, 300), (300, 215)]) & fg & ~ear_l
    ear_l_full = ndi.binary_opening(ear_l | hidden, iterations=6)   # also drops the stub of body outline at the tip
    hidden &= ear_l_full
    cheek_line = ear_l & ~ear_l_in & grow(body_core, 14) & (yy > 236)   # the head's line, not the ear's
    ear_l_rgb = fill_from(rgb, hidden | cheek_line, ndi.binary_erosion(ear_l_in, iterations=4), grain)
    stroke_l = inked(inner_l, grow(hidden, 4))
    ear_l_rgb = ear_l_rgb * (1 - stroke_l[..., None] * 0.92) + ink * stroke_l[..., None] * 0.92
    ear_l_a = np.maximum(soft_mask(ear_l_full) * alpha, stroke_l)

    # Face parts.
    dark = lum < 120
    eye = lambda box: ndi.binary_dilation(ndi.binary_fill_holes(dark & box_mask(shape, box)), iterations=3)
    eye_l, eye_r = eye(EYE_BOX_L), eye(EYE_BOX_R)
    mouth = ndi.binary_dilation(line & box_mask(shape, MOUTH_BOX), iterations=4)
    soft = lambda m, s=1.2: np.clip(ndi.gaussian_filter(m.astype(np.float32), s) * 1.2, 0, 1)

    face_hole = grow(eye_l | eye_r | mouth, 5)
    body_rgb = diffuse_fill(rgb, grow(eye_l, 5))
    body_rgb = diffuse_fill(body_rgb, grow(eye_r, 5))
    body_rgb = diffuse_fill(body_rgb, grow(mouth, 5))
    body_rgb = diffuse_fill(body_rgb, grow(star, 4))  # the clip is its own layer, so the back of the head has none

    # The head goes on under the right ear: paint it out to its own contour, so a
    # lifted ear shows head there, not a hole.
    under_r_line = smooth(HEAD_R_UNDER)
    under_r = poly_mask(shape, under_r_line + [(700, 640), (660, 400), (650, 220)]) & fg
    body = ndi.binary_fill_holes(ndi.binary_closing(body | under_r, iterations=6)) & fg
    body_known = ndi.binary_erosion(body_core, iterations=5) & ~grow(star, 6)
    # Repaint what the ear covered, and the ear's own edge line with it.
    body_rgb = fill_from(body_rgb, body & grow(under_r, 8) & grow(ear_r, 14) & ~body_known, body_known, grain)
    stroke = inked(under_r_line, grow(ear_r, 5))
    body_rgb = body_rgb * (1 - stroke[..., None] * 0.92) + ink * stroke[..., None] * 0.92
    body_a = np.maximum(soft(body, 0.9) * alpha, stroke)
    # At the left ear's root the painting has no line: head and ear melt together.
    # `root_l_melt` keeps that at rest; `root_l_line` is the head's edge once the ear lifts away.
    d = ndi.distance_transform_edt(~bar_l)
    t = np.clip(1 - d / 26, 0, 1)
    melt = (t * t * (3 - 2 * t)) * ear_l * alpha
    root_line = inked([(347, 190), (317, 231), (296, 278)], 1.0)

    layers = []
    save_layer("body", body_rgb, body_a, layers, z=0)
    save_layer("eye_l", rgb, soft(eye_l), layers, z=1)
    save_layer("eye_r", rgb, soft(eye_r), layers, z=1)
    save_layer("mouth", rgb, soft(mouth), layers, z=1)
    save_layer("ear_l", ear_l_rgb, ear_l_a, layers, pivot=PIVOT_L, z=-1)
    save_layer("root_l_melt", rgb, melt, layers, z=1)
    save_layer("root_l_line", np.broadcast_to(ink, rgb.shape), root_line, layers, z=1)
    save_layer("ear_r", rgb_nostar, ear_r_a, layers, pivot=PIVOT_R, z=2)
    save_layer("star", rgb, soft(star, 0.9), layers, z=3)

    cy = lambda m: [float(np.where(m)[1].mean() * SCALE), float(np.where(m)[0].mean() * SCALE)]
    ys, xs = np.where(fg)
    rig = {
        "id": "zimeng",
        "size": [int(shape[1] * SCALE), int(shape[0] * SCALE)],
        "bbox": [float(xs.min() * SCALE), float(ys.min() * SCALE), float(xs.max() * SCALE), float(ys.max() * SCALE)],
        "anchors": {
            "eye_l": cy(eye_l), "eye_r": cy(eye_r), "mouth": cy(mouth),
            "ear_tip_l": [float(v * SCALE) for v in (140, 585)], "ear_tip_r": [float(v * SCALE) for v in (860, 625)],
        },
        "layers": layers,
    }
    (OUT / "rig.json").write_text(json.dumps(rig, indent=2), "utf-8")

    # Rest-pose check: the stack must rebuild the source.
    comp = np.zeros(shape + (3,), np.float32)
    cov = np.zeros(shape, np.float32)
    for src, a in [(ear_l_rgb, ear_l_a), (body_rgb, body_a), (rgb, melt), (rgb, soft(eye_l)), (rgb, soft(eye_r)), (rgb, soft(mouth)),
                   (rgb_nostar, ear_r_a), (rgb, soft(star, 0.9))]:
        comp = comp * (1 - a[..., None]) + src * a[..., None]
        cov = cov * (1 - a) + a
    inner = ndi.binary_erosion(fg, iterations=4)
    err = np.abs(comp - rgb).max(-1)[inner]
    print(f"ear_l {ear_l.sum()}  ear_r {ear_r.sum()}  body {body.sum()}  star {star.sum()}  eyes {eye_l.sum()}/{eye_r.sum()}  mouth {mouth.sum()}")
    print(f"rest-pose error inside figure: mean {err.mean():.2f}  p99 {np.percentile(err, 99):.1f}  max {err.max():.0f}   uncovered px {(cov[inner] < 0.98).sum()}")


if __name__ == "__main__":
    main()
