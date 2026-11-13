"""Cut 泡泡's back view (4th figure of turnaround.png) into rig layers.

The sheet draws the figure smaller and a little slimmer than the hero
illustration, so the crop is resized to the hero's height and head width and
placed with its feet on the hero's. Layers, as on the front: body (torso,
flippers, feet, and a top under the head), curl (with its root carried on
behind the head), head (with the shell showing past its edge).

Seen from behind everything is mirrored: the curl's line runs into the head on
the right, and turns into the head's top line on the left.

    python design/pet-art/tools/cut_paopao_back.py
"""
import json
import sys
from pathlib import Path

import numpy as np
from PIL import Image
from scipy import ndimage as ndi

from cut_zimeng_back import line_mask, poly_mask, smooth, fill_from

ROOT = Path(__file__).resolve().parents[1] / "paopao"
OUT = ROOT / "rig"
CROP = (1120, 150, 1500, 810)      # the back figure on the sheet
WORK = (526, 840)                  # crop resized: x1.385 wide, x1.273 tall
SCALE = 0.5                        # work space -> rig space
FOOT_WORK = (258, 806)             # between the feet, on the ground, work space
FOOT_FRONT = (510, 937)            # the same point on the hero illustration (source px)

# Work-space landmarks, read off the drawing (see cut_paopao.py for what each is).
SEAM = [(0, 470), (80, 478), (108, 486), (120, 489), (140, 496), (160, 502), (190, 505), (260, 506), (330, 505), (360, 502),
        (380, 496), (400, 489), (412, 486), (440, 478), (525, 470)]
SEAM_SOFT = (168, 352)
NECK = (260, 500)
DOME = [(108, 486), (118, 466), (140, 450), (180, 440), (220, 435), (260, 433), (300, 435), (340, 440), (380, 450), (402, 466), (412, 486)]
CURL_ARC = [(254, 132), (266, 133), (280, 134), (294, 136), (304, 138), (312, 141)]
CURL_SIDE = [(312, 141), (317, 137), (322, 126), (336, 110), (336, 15), (200, 15), (200, 128), (230, 129), (246, 130)]   # with CURL_ARC: all of the curl
CURL_ROOT = (283, 135)
HOOK = [(312, 140), (306, 147), (299, 153), (293, 157), (289, 159)]
ROOT_LOW = [(288, 164), (275, 166), (258, 165), (246, 161)]
ROOT_LINE = [(259, 126), (254, 132), (249, 141), (246, 151), (245, 161)]
NOTCHES = [(112, 482), (407, 481)]
SHELL_BOX = (330, 130, 480, 280)
GRAIN_PATCH = (200, 250, 320, 350)


def diffuse_fill(rgb, hole, grain):
    """Fill `hole` by relaxing to the average of its neighbours (smooth areas only)."""
    out = rgb.copy()
    ys, xs = np.where(hole)
    y0, y1, x0, x1 = ys.min() - 4, ys.max() + 5, xs.min() - 4, xs.max() + 5
    win = out[y0:y1, x0:x1]
    h = hole[y0:y1, x0:x1]
    win[h] = win[~h].mean(0)
    for _ in range(700):
        avg = (np.roll(win, 1, 0) + np.roll(win, -1, 0) + np.roll(win, 1, 1) + np.roll(win, -1, 1)) / 4
        win[h] = avg[h]
    reps = (h.shape[0] // grain.shape[0] + 1, h.shape[1] // grain.shape[1] + 1, 1)
    win[h] += np.tile(grain, reps)[: h.shape[0], : h.shape[1]][h]
    return out


def main(debug=None):
    front = json.loads((OUT / "rig.json").read_text("utf-8"))
    # Where the hero's feet stand, in rig space (cut_paopao.py: SCALE 0.45, OFFSET).
    assert front["id"] == "paopao"
    import cut_paopao
    foot = cut_paopao.to_rig(FOOT_FRONT)
    offset = (round(foot[0] - FOOT_WORK[0] * SCALE), round(foot[1] - FOOT_WORK[1] * SCALE))

    sheet = Image.open(ROOT / "turnaround.png").convert("RGB")
    src = np.asarray(sheet.crop(CROP).resize(WORK, Image.LANCZOS)).astype(np.float32)
    shape = src.shape[:2]
    yy, xx = np.mgrid[: shape[0], : shape[1]]

    bg = np.median(np.concatenate([src[:20].reshape(-1, 3), src[:, -20:].reshape(-1, 3)]), 0)
    # Paper and the glow round the figure are light and warm (see cut_paopao.py).
    paper = (src[..., 0] >= 246) & (src[..., 1] >= 238) & (src[..., 2] >= 218) & (src[..., 0] >= src[..., 2] - 1)
    lab, _ = ndi.label(paper)
    border = set(np.unique(np.concatenate([lab[0], lab[-1], lab[:, 0], lab[:, -1]]))) - {0}
    fg = ndi.binary_fill_holes(~np.isin(lab, list(border)))
    l2, n2 = ndi.label(fg)
    fg = l2 == (1 + np.argmax(ndi.sum(fg, l2, range(1, n2 + 1))))
    fg = ndi.binary_opening(fg, iterations=2)

    grow = lambda m, n=9: ndi.binary_dilation(m, iterations=n)
    shrink = lambda m, n=1: ndi.binary_erosion(m, iterations=n)
    outside = np.clip(ndi.gaussian_filter(shrink(fg, 2).astype(np.float32), 0.9) * 1.15, 0, 1)
    edge = lambda m: np.clip(ndi.gaussian_filter(shrink(m).astype(np.float32), 0.9) * 1.15, 0, 1) * outside
    soft = lambda m, s=1.0: np.clip(ndi.gaussian_filter(m.astype(np.float32), s) * 1.2, 0, 1)
    sm = lambda pts: smooth(pts) if len(pts) >= 4 else list(pts)

    def box_mask(box):
        m = np.zeros(shape, bool)
        m[box[1]:box[3], box[0]:box[2]] = True
        return m

    lumw = np.array([0.299, 0.587, 0.114], np.float32)
    warm = (src[..., 0] - src[..., 2] > 22) & fg
    shell_zone = box_mask(SHELL_BOX)
    # As on the front: the outline's colour carried out to the silhouette, the
    # glow caught in the two notches inked in, and the white wire toned down.
    rgb = src.copy()
    rim = fg & ~shrink(fg, 5)
    darkest = np.stack([ndi.minimum_filter(src[..., c], size=7) for c in range(3)], -1)
    rgb[rim] = darkest[rim]
    notch = np.zeros(shape, bool)
    for x, y in NOTCHES:
        notch |= (xx - x) ** 2 + (yy - y) ** 2 < 22 ** 2
    glow = fg & notch & (src[..., 0] >= 236) & (src[..., 1] >= 226) & (src[..., 2] >= 204) & (src[..., 0] >= src[..., 2])
    glow = ndi.binary_dilation(glow, iterations=1) & fg & notch
    darker = np.stack([ndi.minimum_filter(src[..., c], size=13) for c in range(3)], -1)
    rgb[glow] = darker[glow]
    patch = rgb[GRAIN_PATCH[1]:GRAIN_PATCH[3], GRAIN_PATCH[0]:GRAIN_PATCH[2]]
    grain = patch - ndi.gaussian_filter(patch, (2, 2, 0))
    ink = np.median(src[warm & ~shell_zone & ~shrink(fg, 9) & shrink(fg, 2) & (yy > 520)], 0)
    wire = fg & ~shrink(fg, 3)
    pale = np.clip((rgb @ lumw - float(ink @ lumw)) / 45, 0, 1) * 0.75
    kk = (pale * wire)[..., None]
    rgb = rgb * (1 - kk) + ink * kk

    def inked(pts, width=7):
        return ink, np.clip(ndi.gaussian_filter(line_mask(shape, sm(pts), width), 1.3) * 1.15, 0, 1)

    def over(base, colour, a):
        return base * (1 - a[..., None] * 0.94) + colour * a[..., None] * 0.94

    # ---------------------------------------------------------------- the three parts as drawn
    seam = sm(SEAM)
    above_seam = poly_mask(shape, seam + [(shape[1], 0), (0, 0)])
    arc = sm(CURL_ARC)
    curl_seen = ndi.binary_opening(fg & poly_mask(shape, CURL_SIDE + arc), iterations=2)
    head = fg & above_seam & ~curl_seen
    body_seen = fg & ~above_seam

    # ---------------------------------------------------------------- head
    hook_path = sm(HOOK)
    # Here the line lies in the curl's shadow and is duller than the rest of the outline.
    hook = (line_mask(shape, hook_path, 11) > 0) & head & grow((src[..., 0] - src[..., 2] > 4) & fg, 2) & ~grow(curl_seen, 4)
    head_rgb = diffuse_fill(rgb, grow(hook, 3) & head, grain)
    mid = (xx > SEAM_SOFT[0]) & (xx < SEAM_SOFT[1])
    seam_soft = soft((line_mask(shape, seam, 44) > 0) & mid, 6)
    arc_soft = soft(line_mask(shape, arc, 30) > 0, 4) * soft(grow(curl_seen, 12), 3)
    melt = np.clip(np.maximum(seam_soft, arc_soft), 0, 1)
    wide = np.clip(ndi.gaussian_filter(head.astype(np.float32), 3.2), 0, 1) * outside
    head_a = edge(head) * (1 - melt) + wide * melt

    # ---------------------------------------------------------------- body
    dome_line = sm(DOME)
    dome = poly_mask(shape, dome_line + [p for p in seam if DOME[0][0] <= p[0] <= DOME[-1][0]][::-1]) & above_seam
    body = ndi.binary_fill_holes(body_seen | dome)
    keep = dome & grow(body_seen, 14) & mid
    body_known = shrink(body_seen, 6) & (yy < 580) & (xx > 130) & (xx < 390) & ~warm
    painted = fill_from(rgb, dome & ~keep, body_known | keep, grain)
    feather = soft(dome & ~keep, 2.5)[..., None]
    body_rgb = rgb * (1 - feather) + painted * feather
    c, s = inked(dome_line, 7)
    s = s * dome * ~grow(body_seen, 3)
    body_rgb = over(body_rgb, c, s)
    body_a = edge(body)

    # ---------------------------------------------------------------- curl
    right = [(x + 3, y + 1) for x, y in hook_path]
    root = poly_mask(shape, right + ROOT_LOW + ROOT_LINE[:0:-1] + arc) & fg & ~curl_seen
    curl = curl_seen | root
    c, s = inked(ROOT_LINE, 7)
    s = s * root * ~grow(curl_seen, 1)
    curl_rgb = over(rgb, c, s)
    curl_a = np.maximum(edge(curl), s * outside)

    layers = []

    def save_layer(name, col, alpha, pivot=None):
        ys, xs = np.where(alpha > 0.004)
        ya, yb = max(ys.min() - 4, 0), min(ys.max() + 5, shape[0])
        xa, xb = max(xs.min() - 4, 0), min(xs.max() + 5, shape[1])
        xa -= xa % 2; ya -= ya % 2; xb += xb % 2; yb += yb % 2
        a = alpha[ya:yb, xa:xb, None]
        pre = np.concatenate([col[ya:yb, xa:xb] * a, a * 255], -1)
        size = (int((xb - xa) * SCALE), int((yb - ya) * SCALE))
        small = np.stack(
            [np.asarray(Image.fromarray(pre[..., c].astype(np.float32), "F").resize(size, Image.LANCZOS)) for c in range(4)], -1
        )
        al = np.clip(small[..., 3:], 0, 255)
        out = np.where(al > 0.5, small[..., :3] / np.maximum(al / 255, 1e-4), 0)
        Image.fromarray(np.concatenate([np.clip(out, 0, 255), al], -1).round().astype(np.uint8), "RGBA").save(OUT / f"{name}.png")
        entry = {"name": name, "file": f"{name}.png", "x": xa * SCALE + offset[0], "y": ya * SCALE + offset[1], "w": size[0], "h": size[1]}
        if pivot:
            entry["pivot"] = [pivot[0] * SCALE + offset[0], pivot[1] * SCALE + offset[1]]
        layers.append(entry)

    save_layer("back_body", body_rgb, body_a)
    save_layer("back_curl", curl_rgb, curl_a, pivot=CURL_ROOT)
    save_layer("back_head", head_rgb, head_a, pivot=NECK)
    (OUT / "back.json").write_text(json.dumps({"layers": layers}, indent=2), "utf-8")

    # Rest-pose check against the drawing.
    comp = np.broadcast_to(bg, shape + (3,)).astype(np.float32).copy()
    for col, a in [(body_rgb, body_a), (curl_rgb, curl_a), (head_rgb, head_a)]:
        comp = comp * (1 - a[..., None]) + col * a[..., None]
    inner = shrink(fg, 8)
    err = np.abs(comp - src).max(-1)
    ys, xs = np.where(fg)
    print(f"head {head.sum()}  body {body.sum()}  curl {curl.sum()}  hook {hook.sum()}  offset {offset}  figure in rig space: x {xs.min() * SCALE + offset[0]:.0f}..{xs.max() * SCALE + offset[0]:.0f}  y {ys.min() * SCALE + offset[1]:.0f}..{ys.max() * SCALE + offset[1]:.0f}")
    print(f"rest-pose error inside figure: mean {err[inner].mean():.2f}  p99 {np.percentile(err[inner], 99):.1f}  max {err[inner].max():.0f}")
    bad = inner & (err > 12)
    if bad.any():
        lab2, n = ndi.label(grow(bad, 3))
        for i in np.argsort(ndi.sum(bad, lab2, range(1, n + 1)))[::-1][:4]:
            by, bx = np.where((lab2 == i + 1) & bad)
            print(f"  differs: {len(bx)} px around ({int(bx.mean())}, {int(by.mean())})")
    if debug:
        tint = src.copy()
        for m, colour in ((head, (255, 60, 60)), (body_seen, (60, 200, 60)), (curl_seen, (255, 220, 0)), (dome & ~body_seen, (0, 255, 255)), (root & ~curl_seen, (255, 0, 255)), (hook, (255, 255, 255))):
            tint[m & ~shrink(m, 1)] = colour
        Image.fromarray(tint.astype(np.uint8)).save(Path(debug) / "dbg_back_parts.png")


if __name__ == "__main__":
    main(sys.argv[1] if len(sys.argv) > 1 else None)
    # The drawing shows no tail from behind; the back view needs one (see back_tail.py).
    import back_tail

    back_tail.add("paopao")
