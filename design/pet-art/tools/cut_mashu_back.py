"""Cut 麻薯's back view (4th figure of turnaround-v2.png) into rig layers.

The sheet draws the figure at its own size and a little narrower than the hero
illustration, so the crop is resized to the hero's height and head width and
placed with its feet on the hero's foot point. Layers: body (with the collar
band, its knot, a neck under the head and the rump continued under the tail),
head (with both ears), tail.

The sheet paints the backs of the ears and the tail pink; from the front both
are cream. They are brought back to cream here (a faint blush is left), so the
figure does not change colour as it turns.

    python design/pet-art/tools/cut_mashu_back.py
"""
import json
from pathlib import Path

import numpy as np
from PIL import Image
from scipy import ndimage as ndi

from cut_zimeng_back import fill_from, line_mask, poly_mask, smooth

ROOT = Path(__file__).resolve().parents[1] / "mashu"
OUT = ROOT / "rig"
CROP = (1120, 220, 1536, 830)      # the back figure on the sheet
WORK = (658, 862)                  # crop resized: x1.58 wide, x1.413 tall
SCALE = 0.5                        # work space -> rig space
FOOT_WORK = (323, 822)             # bottom centre of the body, work space

# Work-space landmarks, read off the drawing.
SEED_HEAD, SEED_BODY, SEED_TAIL = (320, 300), (320, 650), (320, 760)
BAND_LOW = [(188, 518), (215, 527), (250, 534), (285, 539), (320, 542), (355, 539), (390, 534), (425, 527), (460, 516)]
BAND_THICK = 26
NECK_HIGH = 34
NECK = (322, 522)                  # the point the head tilts about
RUMP = [(236, 796), (262, 809), (290, 816), (320, 819), (350, 816), (378, 809), (404, 796)]   # body outline hidden by the tail
TAIL_PIVOT = (320, 714)
GRAIN_PATCH = (250, 250, 390, 350)
PINK_LEFT = 0.16                   # how much of the painted pink stays


def main():
    front = json.loads((OUT / "rig.json").read_text("utf-8"))
    x0, _, x1, y1 = front["bbox"]
    foot = ((x0 + x1) / 2 + 2, y1 - 4)   # same point rig.js stands the pet on
    offset = (round(foot[0] - FOOT_WORK[0] * SCALE), round(foot[1] - FOOT_WORK[1] * SCALE))

    sheet = Image.open(ROOT / "turnaround-v2.png").convert("RGB")
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
    soft = lambda m, s=1.0: np.clip(ndi.gaussian_filter(m.astype(np.float32), s) * 1.2, 0, 1)
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

    head_in, body_in, tail_in = at(SEED_HEAD), at(SEED_BODY), at(SEED_TAIL)
    # The band is too thin to seed by hand: it is whatever is mint between head and body.
    mintish = (src[..., 1] - src[..., 0] > 9) & fg & (yy > 480) & (yy < 600)
    knot_in = ndi.binary_opening(mintish & ~line, iterations=1) & ~head_in & ~body_in
    band_in = knot_in
    assert not (head_in & body_in).any() and not (body_in & tail_in).any(), "the outline does not close a part"
    ink = np.median(src[line & grow(head_in, 8) & (yy < 480)], 0)
    mint_ink = np.median(src[line & mintish], 0)

    def inked(pts, width=7, colour=None):
        s = np.clip(ndi.gaussian_filter(line_mask(shape, sm(pts), width), 1.3) * 1.15, 0, 1)
        return (ink if colour is None else colour), s

    def over(base, colour, a):
        return base * (1 - a[..., None] * 0.94) + colour * a[..., None] * 0.94

    def uncolour(col, where, known):
        """Bring painted pink back to the cream around it, leaving a blush."""
        pink = where & (col[..., 0] - col[..., 1] > 7)
        pink = grow(pink, 2) & where
        cream = fill_from(col, pink, known & ~grow(pink, 3), grain)
        k = soft(pink, 2.0)[..., None] * (1 - PINK_LEFT)
        return col * (1 - k) + cream * k

    # ---------------------------------------------------------------- the parts as drawn
    tail = ndi.binary_fill_holes(grow(tail_in, 10) & (line | tail_in))
    tail = ndi.binary_opening(tail, iterations=2) | tail_in
    head = ndi.binary_fill_holes(grow(head_in, 12) & fg & ~body_in & ~tail_in & ~band_in & ~knot_in)
    head &= ~grow(body_in, 5)
    head = ndi.binary_opening(head, iterations=4)
    body_seen = ndi.binary_fill_holes(grow(body_in | band_in | knot_in, 12) & fg & ~head & ~tail_in) & ~tail
    body_seen = ndi.binary_opening(body_seen, iterations=3)

    # ---------------------------------------------------------------- head: the ears' backs to cream
    head_rgb = uncolour(rgb, head_in & (yy < 230), shrink(head_in, 6))
    head_a = edge(head)

    # ---------------------------------------------------------------- body: neck and band under the head, rump under the tail
    low = smooth(BAND_LOW)
    high = [(x, y - BAND_THICK) for x, y in low]
    neck_top = [(x, y - NECK_HIGH) for x, y in high]
    band_full = poly_mask(shape, low + high[::-1])
    neck = poly_mask(shape, high + neck_top[::-1]) | band_full
    rump_line = smooth(RUMP)
    rump = poly_mask(shape, rump_line + [(404, 690), (236, 690)])
    body = ndi.binary_fill_holes(body_seen | (rump & (tail | body_seen)) | neck)
    body = ndi.binary_opening(ndi.binary_closing(body, iterations=5), iterations=2)
    body_known = shrink(body_in, 5) & ~grow(tail, 5) & ~grow(knot_in | band_in, 8)
    visible = shrink(body_seen, 2) & ~grow(tail, 4)
    body_rgb = fill_from(rgb, body & ~visible, body_known, grain)
    # The band: as drawn where it shows, painted on under the head.
    band_known = shrink(band_in, 2)
    band_hidden = band_full & ~grow(band_in | knot_in, 1) & ~body_seen
    band_paint = fill_from(rgb, band_hidden, band_known, grain * 0.6)
    feather = soft(band_hidden, 1.2)[..., None]
    body_rgb = body_rgb * (1 - feather) + band_paint * feather
    under_head = grow(head, 2)
    for pts in (high, [low[0], high[0]], [low[-1], high[-1]]):
        c, s = inked(pts, 6, mint_ink)
        body_rgb = over(body_rgb, c, s * under_head * ~grow(band_in, 1))
    for pts in ([high[0], neck_top[0]], [high[-1], neck_top[-1]]):
        c, s = inked(pts, 7)
        body_rgb = over(body_rgb, c, s * under_head)
    # The body's outline goes on under the tail.
    c, s = inked(rump_line, 7)
    s = s * grow(tail, 4)
    body_rgb = over(body_rgb, c, s)
    body_a = np.maximum(edge(body), s * outside)

    # ---------------------------------------------------------------- tail: cream like the front one
    tail_rgb = uncolour(rgb, tail_in, shrink(tail_in, 4) & (yy < 760))
    tail_a = soft(tail, 0.9) * np.maximum(outside, soft(tail_in))

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
    save_layer("back_tail", tail_rgb, tail_a, pivot=TAIL_PIVOT)
    save_layer("back_head", head_rgb, head_a, pivot=NECK)
    (OUT / "back.json").write_text(json.dumps({"layers": layers}, indent=2), "utf-8")

    # Rest-pose check against the drawing. The ears' backs and the tail were recoloured on purpose.
    comp = np.broadcast_to(bg, shape + (3,)).astype(np.float32).copy()
    for col, a in [(body_rgb, body_a), (tail_rgb, tail_a), (head_rgb, head_a)]:
        comp = comp * (1 - a[..., None]) + col * a[..., None]
    inner = shrink(fg, 8) & ~grow(tail, 3) & ~(head & (yy < 230))
    err = np.abs(comp - src).max(-1)[inner]
    ys, xs = np.where(fg)
    print(f"head {head.sum()}  body {body.sum()}  tail {tail.sum()}  offset {offset}  figure in rig space: x {xs.min() * SCALE + offset[0]:.0f}..{xs.max() * SCALE + offset[0]:.0f}  y {ys.min() * SCALE + offset[1]:.0f}..{ys.max() * SCALE + offset[1]:.0f}")
    print(f"rest-pose error inside figure (recoloured parts left out): mean {err.mean():.2f}  p99 {np.percentile(err, 99):.1f}  max {err.max():.0f}")


if __name__ == "__main__":
    main()
