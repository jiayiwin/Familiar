"""Cut 菇菇's back view (4th figure of turnaround.png) into rig layers.

The sheet draws the figure with a smaller cap and longer legs than the hero
illustration. So that it does not change size as it turns, the cap and head
are resized to the hero's cap (x0.93 wide, x0.84 tall of the sheet) and the
body to the hero's body (x0.83 wide, x0.64 tall), both hung on the hero's neck
point: the seam between head and body lands on the front rig's `neck`, the top
of the cap on the front's top, the feet on the front's feet.

Layers: body (with its own rounded top carried on under the head), head (the
band seen under the cap, carried on upward as a dome under it), cap (with its
spots and its lower rim, drawn over the head).

    python design/pet-art/tools/cut_gugu_back.py
"""
import json
from pathlib import Path

import numpy as np
from PIL import Image
from scipy import ndimage as ndi

from cut_zimeng_back import fill_from, line_mask, poly_mask, smooth

ROOT = Path(__file__).resolve().parents[1] / "gugu"
OUT = ROOT / "rig"
CROP = (1110, 230, 1530, 800)      # the back figure on the sheet
TOP = (0.93, 0.84)                 # sheet px -> rig px for the cap and the head
LOW = (0.83, 0.64)                 # sheet px -> rig px for the body
CAP = (1.0, 0.84)                  # the cap a little wider than the head band: as wide as the front's cap (384 px)
WORK = (round((CROP[2] - CROP[0]) * TOP[0] * 2), round((CROP[3] - CROP[1]) * TOP[1] * 2))   # cut at twice the rig's size

# Work-space landmarks, read off the drawing.
SEED_CAP, SEED_HEAD, SEED_BODY = (300, 280), (392, 576), (388, 746)
SEED_SPOTS = [(204, 361), (386, 198), (402, 376), (579, 286)]
NECK = (389, 622)                  # the point the head tilts about: middle of the line between head and body
CAP_ROOT = (389, 470)              # the point the cap rocks about
GRAIN_PATCH = (300, 660, 440, 740)
# The head under the cap (invented): its top, and its two side lines carried on upward.
HEAD_DOME = [(138, 524), (150, 470), (200, 428), (290, 404), (389, 398), (490, 404), (580, 428), (630, 470), (642, 524)]
SIDE_L = [(146, 548), (139, 522), (146, 484), (170, 452)]
SIDE_R = [(634, 548), (641, 522), (634, 484), (610, 452)]
# The body's own top, hidden behind the head (invented).
BODY_TOP = [(208, 632), (236, 602), (300, 582), (389, 576), (478, 582), (542, 602), (570, 632)]


def main():
    front = json.loads((OUT / "rig.json").read_text("utf-8"))
    neck = front["anchors"]["neck"]

    sheet = Image.open(ROOT / "turnaround.png").convert("RGB")
    src = np.asarray(sheet.crop(CROP).resize(WORK, Image.LANCZOS)).astype(np.float32)
    shape = src.shape[:2]
    yy, xx = np.mgrid[: shape[0], : shape[1]]
    patch = src[GRAIN_PATCH[1]:GRAIN_PATCH[3], GRAIN_PATCH[0]:GRAIN_PATCH[2]]
    grain = patch - ndi.gaussian_filter(patch, (2, 2, 0))

    bg = np.median(np.concatenate([src[:20].reshape(-1, 3), src[:, -20:].reshape(-1, 3)]), 0)
    lab, _ = ndi.label(np.sqrt(((src - bg) ** 2).sum(-1)) < 14)
    border = set(np.unique(np.concatenate([lab[0], lab[-1], lab[:, 0], lab[:, -1]]))) - {0}
    fg = ndi.binary_fill_holes(~np.isin(lab, list(border)))
    l2, n2 = ndi.label(fg)
    fg = l2 == (1 + np.argmax(ndi.sum(fg, l2, range(1, n2 + 1))))

    grow = lambda m, n=9: ndi.binary_dilation(m, iterations=n)
    shrink = lambda m, n=1: ndi.binary_erosion(m, iterations=n)
    outside = np.clip(ndi.gaussian_filter(shrink(fg, 2).astype(np.float32), 0.9) * 1.15, 0, 1)
    edge = lambda m: np.clip(ndi.gaussian_filter(shrink(m).astype(np.float32), 0.9) * 1.15, 0, 1) * outside
    sm = lambda pts: smooth(pts) if len(pts) >= 4 else list(pts)

    lum = src @ np.array([0.299, 0.587, 0.114], np.float32)
    line = ((ndi.gaussian_filter(lum, 6) - ndi.gaussian_filter(lum, 1.2)) > 5) & fg
    # As on the front: the outline's colour carried out to the silhouette, for dark desktops.
    rgb = src.copy()
    rim = fg & ~shrink(fg, 5)
    darkest = np.stack([ndi.minimum_filter(src[..., c], size=7) for c in range(3)], -1)
    rgb[rim] = darkest[rim]

    reg, _ = ndi.label(fg & ~line)

    def at(p):
        assert reg[p[1], p[0]] != 0, f"seed {p} sits on a line"
        return reg == reg[p[1], p[0]]

    cap_in, head_in, body_in = at(SEED_CAP), at(SEED_HEAD), at(SEED_BODY)
    for p in SEED_SPOTS:
        cap_in = cap_in | at(p)
    assert not (head_in & body_in).any() and not (head_in & cap_in).any(), "the outline does not close a part"
    ink = np.median(src[line & grow(head_in, 6) & ~grow(cap_in, 12)], 0)
    body_ink = np.median(src[line & grow(body_in, 6) & (yy > 700)], 0)

    def inked(pts, width=7, colour=None):
        s = np.clip(ndi.gaussian_filter(line_mask(shape, sm(pts), width), 1.3) * 1.15, 0, 1)
        return (ink if colour is None else colour), s

    def over(base, colour, a):
        return base * (1 - a[..., None] * 0.94) + colour * a[..., None] * 0.94

    # ---------------------------------------------------------------- the parts as drawn
    # The cap is in front of the head, the head in front of the body: each owns the line it ends on.
    cap = ndi.binary_fill_holes(grow(cap_in, 12) & fg & ~head_in & ~body_in)
    cap = ndi.binary_opening(cap, iterations=4) | cap_in
    head = ndi.binary_fill_holes(grow(head_in, 12) & fg & ~cap & ~body_in) & ~cap
    head = ndi.binary_opening(head, iterations=3) | head_in
    body_seen = fg & ~cap & ~head
    l3, _ = ndi.label(body_seen)
    body_seen = l3 == l3[SEED_BODY[1], SEED_BODY[0]]
    crumbs = fg & ~cap & ~head & ~body_seen
    head |= crumbs & grow(head, 3)
    cap |= crumbs & ~head

    # ---------------------------------------------------------------- head: a dome under the cap
    dome = poly_mask(shape, smooth(HEAD_DOME) + [(642, 560), (138, 560)]) & ~head & cap
    head_known = shrink(head_in, 6)
    head_rgb = fill_from(rgb, dome, head_known, grain)
    head_a = np.maximum(edge(head), np.clip(ndi.gaussian_filter(shrink(head | dome).astype(np.float32), 0.9) * 1.15, 0, 1) * grow(dome, 2))
    for pts in (SIDE_L, SIDE_R):
        c, s = inked(pts, 7)
        s = s * grow(dome, 3) * ~shrink(head, 2)
        head_rgb = over(head_rgb, c, s)
        head_a = np.maximum(head_a, s * cap)

    # ---------------------------------------------------------------- body: its own top behind the head
    top_line = smooth(BODY_TOP)
    hidden_top = poly_mask(shape, top_line + [(570, 660), (208, 660)]) & head
    body = ndi.binary_fill_holes(body_seen | hidden_top)
    body_known = shrink(body_in, 5) & (yy < 720)
    body_rgb = fill_from(rgb, hidden_top | (grow(hidden_top, 3) & head), body_known, grain)
    c, s = inked(top_line, 7, body_ink)
    s = s * grow(hidden_top, 4) * head
    body_rgb = over(body_rgb, c, s)
    body_a = np.maximum(edge(body), s * head)

    cap_rgb, cap_a = rgb, edge(cap)

    # ---------------------------------------------------------------- write: each part at its own size, hung on the neck
    layers = []

    def save_layer(name, col, alpha, scale, pivot=None):
        """`scale`: sheet px -> rig px of this part, relative to the work image (which is at TOP x 2)."""
        fx, fy = scale[0] / TOP[0] / 2, scale[1] / TOP[1] / 2
        size = (round(shape[1] * fx), round(shape[0] * fy))
        fx, fy = size[0] / shape[1], size[1] / shape[0]
        ox, oy = round(neck[0] - NECK[0] * fx), round(neck[1] - NECK[1] * fy)
        pre = np.concatenate([col * alpha[..., None], alpha[..., None] * 255], -1)
        small = np.stack([np.asarray(Image.fromarray(pre[..., c].astype(np.float32), "F").resize(size, Image.LANCZOS)) for c in range(4)], -1)
        al = np.clip(small[..., 3:], 0, 255)
        out = np.where(al > 0.5, small[..., :3] / np.maximum(al / 255, 1e-4), 0)
        ys, xs = np.where(al[..., 0] > 1)
        y0, y1, x0, x1 = max(ys.min() - 2, 0), ys.max() + 3, max(xs.min() - 2, 0), xs.max() + 3
        Image.fromarray(np.concatenate([np.clip(out, 0, 255), al], -1).round().astype(np.uint8)[y0:y1, x0:x1], "RGBA").save(OUT / f"{name}.png")
        entry = {"name": name, "file": f"{name}.png", "x": int(x0 + ox), "y": int(y0 + oy), "w": int(min(x1, size[0]) - x0), "h": int(min(y1, size[1]) - y0)}
        if pivot:
            entry["pivot"] = [round(pivot[0] * fx + ox, 2), round(pivot[1] * fy + oy, 2)]
        layers.append(entry)
        ys, xs = np.where(alpha > 0.5)
        return [round(float(xs.min() * fx + ox), 1), round(float(ys.min() * fy + oy), 1), round(float(xs.max() * fx + ox), 1), round(float(ys.max() * fy + oy), 1)]

    b_body = save_layer("back_body", body_rgb, body_a, LOW)
    b_head = save_layer("back_head", head_rgb, head_a, TOP, pivot=NECK)
    b_cap = save_layer("back_cap", cap_rgb, cap_a, CAP, pivot=CAP_ROOT)
    (OUT / "back.json").write_text(json.dumps({"layers": layers}, indent=2), "utf-8")

    # Rest-pose check against the drawing, in the work space (the layers are resized after it).
    comp = np.broadcast_to(bg, shape + (3,)).astype(np.float32).copy()
    for col, a in [(body_rgb, body_a), (head_rgb, head_a), (cap_rgb, cap_a)]:
        comp = comp * (1 - a[..., None]) + col * a[..., None]
    inner = shrink(fg, 8)
    err = np.abs(comp - src).max(-1)[inner]
    print(f"cap {cap.sum()}  head {head.sum()} (+dome {dome.sum()})  body {body.sum()}")
    print(f"in rig space: cap {b_cap}  head {b_head}  body {b_body}  (front bbox {front['bbox']}, neck {neck})")
    print(f"rest-pose error inside figure, before resizing: mean {err.mean():.2f}  p99 {np.percentile(err, 99):.1f}  max {err.max():.0f}")


if __name__ == "__main__":
    main()
    # The drawing shows no tail from behind; the back view needs one (see back_tail.py).
    import back_tail

    back_tail.add("gugu")
