"""Cut 紫梦's back view (4th figure of turnaround-v2.png) into rig layers.

The back is drawn more upright than the hero illustration, so the crop is
squashed vertically to the hero's proportions and placed with its feet on the
hero's foot point. Layers: body (with the head continued under the ears and the
rump continued under the tail), two ears, tail. The painted star is dropped:
the clip sits on the front of the ear, and the sheet shows it on the wrong side.

    python design/pet-art/tools/cut_zimeng_back.py
"""
import json
from pathlib import Path

import numpy as np
from PIL import Image, ImageDraw
from scipy import ndimage as ndi
from scipy.interpolate import splev, splprep

ROOT = Path(__file__).resolve().parents[1] / "zimeng"
OUT = ROOT / "rig"
CROP = (1090, 330, 1536, 830)      # the back figure on the sheet
WORK = (892, 880)                  # crop resized: x2 wide, x1.76 tall
SCALE = 0.5                        # work space -> rig space
FOOT_WORK = (452, 795)             # bottom centre of the body, work space

# Work-space landmarks, read off the drawing.
NOTCH_L, NOTCH_R = (325, 114), (590, 114)          # where each ear meets the top of the head
LINE_TOP_L, LINE_TOP_R = (300, 204), (612, 206)    # top end of each inner ear line
SEED_EAR_L, SEED_EAR_R = (150, 450), (780, 450)
SEED_TAIL, SEED_STAR = (450, 720), (580, 400)
# The head's own contour where an ear hangs over it.
# (Each starts on the visible outline so the join is tangent; smoothed by a spline.)
UNDER_L = [(372, 90), (325, 112), (294, 136), (266, 170), (240, 228), (220, 312), (207, 410), (198, 510), (192, 584)]
UNDER_R = [(543, 90), (590, 112), (621, 136), (648, 170), (674, 228), (694, 312), (706, 410), (715, 510), (724, 584)]
RUMP = [(340, 780), (368, 787), (410, 794), (452, 797), (494, 794), (536, 787), (564, 780)]   # body outline hidden by the tail
TAIL_PIVOT = (450, 690)
GRAIN_PATCH = (380, 150, 520, 250)


def smooth(pts, n=80):
    tck, _ = splprep(np.asarray(pts, float).T, s=0, k=3)
    return [tuple(v) for v in np.stack(splev(np.linspace(0, 1, n), tck), -1)]


def poly_mask(shape, pts):
    im = Image.new("L", (shape[1], shape[0]), 0)
    ImageDraw.Draw(im).polygon(pts, fill=255)
    return np.asarray(im) > 0


def line_mask(shape, pts, width):
    im = Image.new("L", (shape[1], shape[0]), 0)
    ImageDraw.Draw(im).line(pts, fill=255, width=width, joint="curve")
    return np.asarray(im, np.float32) / 255


def fill_from(rgb, hole, known, grain):
    """Paint `hole` with the nearest `known` colour, smoothed, plus paper grain."""
    idx = ndi.distance_transform_edt(~known, return_distances=False, return_indices=True)
    out = rgb.copy()
    out[hole] = rgb[idx[0][hole], idx[1][hole]]
    m = (known | hole).astype(np.float32)
    blur = ndi.gaussian_filter(out * m[..., None], (14, 14, 0)) / np.maximum(ndi.gaussian_filter(m, 14), 1e-3)[..., None]
    out[hole] = blur[hole]
    reps = (rgb.shape[0] // grain.shape[0] + 1, rgb.shape[1] // grain.shape[1] + 1, 1)
    out[hole] += np.tile(grain, reps)[: rgb.shape[0], : rgb.shape[1]][hole]
    return out


def save_layer(name, rgb, alpha, layers, offset, pivot=None, z=0):
    ys, xs = np.where(alpha > 0.004)
    y0, y1 = max(ys.min() - 4, 0), min(ys.max() + 5, alpha.shape[0])
    x0, x1 = max(xs.min() - 4, 0), min(xs.max() + 5, alpha.shape[1])
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
    entry = {"name": name, "file": f"{name}.png", "x": x0 * SCALE + offset[0], "y": y0 * SCALE + offset[1], "w": size[0], "h": size[1], "z": z}
    if pivot:
        entry["pivot"] = [pivot[0] * SCALE + offset[0], pivot[1] * SCALE + offset[1]]
    layers.append(entry)


def main():
    front = json.loads((OUT / "rig.json").read_text("utf-8"))
    x0, _, x1, y1 = front["bbox"]
    foot = ((x0 + x1) / 2 + 2, y1 - 4)   # same point rig.js stands the pet on
    offset = (round(foot[0] - FOOT_WORK[0] * SCALE), round(foot[1] - FOOT_WORK[1] * SCALE))

    sheet = Image.open(ROOT / "turnaround-v2.png").convert("RGB")
    rgb = np.asarray(sheet.crop(CROP).resize(WORK, Image.LANCZOS)).astype(np.float32)
    shape = rgb.shape[:2]
    grain_src = rgb[GRAIN_PATCH[1]:GRAIN_PATCH[3], GRAIN_PATCH[0]:GRAIN_PATCH[2]]
    grain = grain_src - ndi.gaussian_filter(grain_src, (2, 2, 0))

    bg = np.median(np.concatenate([rgb[:30].reshape(-1, 3), rgb[:, :30].reshape(-1, 3)]), 0)
    lab, _ = ndi.label(np.sqrt(((rgb - bg) ** 2).sum(-1)) < 14)
    border = set(np.unique(np.concatenate([lab[0], lab[-1], lab[:, 0], lab[:, -1]]))) - {0}
    fg = ndi.binary_fill_holes(~np.isin(lab, list(border)))
    l2, n2 = ndi.label(fg)
    fg = l2 == (1 + np.argmax(ndi.sum(fg, l2, range(1, n2 + 1))))
    alpha = np.clip(ndi.gaussian_filter(ndi.binary_erosion(fg).astype(np.float32), 0.9) * 1.15, 0, 1)

    lum = rgb @ np.array([0.299, 0.587, 0.114], np.float32)
    line = ((ndi.gaussian_filter(lum, 6) - ndi.gaussian_filter(lum, 1.2)) > 5) & fg
    bar_l = line_mask(shape, [NOTCH_L, LINE_TOP_L], 5) > 0
    bar_r = line_mask(shape, [NOTCH_R, LINE_TOP_R], 5) > 0
    free = fg & ~line & ~bar_l & ~bar_r
    reg, _ = ndi.label(free)
    at = lambda p: reg == reg[p[1], p[0]]
    ear_l_in, ear_r_in, tail_in, star_in = at(SEED_EAR_L), at(SEED_EAR_R), at(SEED_TAIL), at(SEED_STAR)
    grow = lambda m, n=9: ndi.binary_dilation(m, iterations=n)
    soft = lambda m, s=0.9: np.clip(ndi.gaussian_filter(m.astype(np.float32), s) * 1.2, 0, 1)

    blob = lambda inner: ndi.binary_fill_holes(grow(inner, 10) & (line | inner))
    tail = ndi.binary_opening(blob(tail_in), iterations=2) | tail_in
    star = ndi.binary_opening(blob(star_in), iterations=2) | star_in
    body_core = ndi.binary_opening(free & ~ear_l_in & ~ear_r_in & ~tail_in, iterations=3)
    ear_l = grow(ear_l_in, 12) & fg & ~body_core & ~ear_r_in & ~bar_l & ~tail
    ear_r = grow(ear_r_in, 12) & fg & ~body_core & ~ear_l_in & ~bar_r & ~tail
    # Drop the stubs of body outline that stick out where an ear crosses it.
    ear_l, ear_r = (ndi.binary_opening(m, iterations=5) for m in (ear_l, ear_r))

    # Body: what is left, continued under the ears and the tail.
    under_l, under_r, rump_line = smooth(UNDER_L), smooth(UNDER_R), smooth(RUMP)
    under = poly_mask(shape, under_l + [(300, 584), (372, 300)]) | poly_mask(shape, under_r + [(612, 584), (543, 300)])
    rump = poly_mask(shape, rump_line + [(564, 600), (340, 600)])
    seen = fg & ~ear_l & ~ear_r & ~tail
    body = ndi.binary_fill_holes(seen | (under & (ear_l | ear_r | seen)) | (rump & (tail | seen)))
    body = ndi.binary_opening(ndi.binary_closing(body, iterations=6), iterations=3)
    known = ndi.binary_erosion(body_core & ~grow(star, 6), iterations=5)
    hole = body & ~known
    body_rgb = fill_from(rgb, hole & ~ndi.binary_dilation(seen & ~grow(star | ear_l | ear_r | tail, 6), iterations=1), known, grain)

    ink = np.median(rgb[line & grow(body_core, 9)], 0)
    stroke = np.zeros(shape, np.float32)
    for pts in (under_l, under_r, rump_line):
        stroke = np.maximum(stroke, line_mask(shape, pts, 7))
    stroke = np.clip(ndi.gaussian_filter(stroke, 1.3) * 1.15, 0, 1) * grow(ear_l | ear_r | tail, 5)  # only where it was hidden
    body_rgb = body_rgb * (1 - stroke[..., None] * 0.92) + ink * stroke[..., None] * 0.92
    edge = np.clip(ndi.gaussian_filter(ndi.binary_erosion(body).astype(np.float32), 0.9) * 1.15, 0, 1)
    body_a = np.maximum(edge, stroke)

    def root_feather(bar, ear, reach):
        d = ndi.distance_transform_edt(~bar)
        t = np.clip(1 - d / reach, 0, 1)
        return np.where(ear, 1.0, (t * t * (3 - 2 * t)) * (fg & ~ear))
    ear_l_a = root_feather(bar_l, ear_l, 44) * alpha
    ear_r_a = root_feather(bar_r, ear_r, 44) * alpha

    layers = []
    save_layer("back_body", body_rgb, body_a, layers, offset, z=0)
    save_layer("back_tail", rgb, soft(tail) * np.maximum(alpha, soft(tail_in)), layers, offset, pivot=TAIL_PIVOT, z=1)
    save_layer("back_ear_l", rgb, ear_l_a, layers, offset, pivot=NOTCH_L, z=2)
    save_layer("back_ear_r", rgb, ear_r_a, layers, offset, pivot=NOTCH_R, z=2)
    (OUT / "back.json").write_text(json.dumps({"layers": layers}, indent=2), "utf-8")

    # Rest-pose check against the drawing (the star is meant to be gone).
    comp = np.zeros(shape + (3,), np.float32)
    for src, a in [(body_rgb, body_a), (rgb, soft(tail)), (rgb, ear_l_a), (rgb, ear_r_a)]:
        comp = comp * (1 - a[..., None]) + src * a[..., None]
    inner = ndi.binary_erosion(fg, iterations=4) & ~grow(star, 8)
    err = np.abs(comp - rgb).max(-1)[inner]
    print(f"ear_l {ear_l.sum()}  ear_r {ear_r.sum()}  tail {tail.sum()}  body {body.sum()}  offset {offset}")
    print(f"rest-pose error inside figure: mean {err.mean():.2f}  p99 {np.percentile(err, 99):.1f}  max {err.max():.0f}")


if __name__ == "__main__":
    main()
