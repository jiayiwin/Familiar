"""Cut the 菇菇 (mushroom) source illustration into rig layers.

Regions are found from the drawn outline, which closes the cap (with its pale
spots), the head under it, the bib and the body by itself; one hand-placed
curve closes the head along its chin, where the line over the bib is too faint.

The figure is cut so that its parts can move against each other:
  body       torso, bib, both arms and the legs; under the head it gets its own
             rounded top and the rest of the bib, which show when the head tilts
  head       the face under the cap, eyes and mouth painted out, carried on
             upward as a dome under the cap, with its side lines continued
  cap        the whole cap: the shell with its spots, the rim line across the
             forehead, and the two bits of its inside seen beside the head
  cap_under  (extra, optional) the inside of the cap carried on behind the head.
             Drawn after the body and before the head, moving with the cap, it
             keeps a dark crack from opening beside the cheeks when the cap rocks.
             Without it the rest pose is still complete.

The drawing is 1024 px; the rig is 512 with the figure standing on y = 452, so
layers are scaled by 0.48 and shifted (see K and SHIFT).

    python design/pet-art/tools/cut_gugu.py
"""
import json
from pathlib import Path

import numpy as np
from PIL import Image
from scipy import ndimage as ndi

from cut_zimeng_back import fill_from, line_mask, poly_mask, smooth

ROOT = Path(__file__).resolve().parents[1] / "gugu"
OUT = ROOT / "rig"
K = 0.48               # source px -> rig px
SHIFT = (8, 9)         # rig px: the figure centred on x = 256, feet on y = 452
SNAP = 25              # crops snap to 25 source px = 12 rig px, so layers land on whole pixels

# Source-space (1024) landmarks, read off the illustration.
SEED_CAP, SEED_HEAD, SEED_BODY, SEED_BIB = (560, 330), (480, 630), (533, 830), (460, 745)
SEED_SPOTS = [(271, 321), (500, 308), (658, 220), (751, 382)]
# The five pale spots on the cap, each in a box of its own (the one at the top left is not closed by a line).
SPOT_BOXES = [(226, 280, 316, 362), (354, 162, 450, 252), (436, 248, 564, 368), (596, 160, 720, 282), (692, 318, 812, 446)]
SPOT_SEEDS = [(271, 321), (400, 205), (500, 308), (658, 220), (751, 382)]
# The head's lower line: drawn all the way, but too faint over the bib to close the region.
CHIN = [(275, 673), (325, 672), (350, 679), (375, 685), (400, 689), (425, 693), (450, 696), (475, 697), (500, 699), (525, 700), (550, 700), (575, 700), (600, 699), (625, 697), (650, 692), (675, 685)]
EYE_BOX_L, EYE_BOX_R = (285, 478, 382, 588), (515, 502, 622, 615)
MOUTH_BOX = (398, 570, 472, 608)
GRAIN_PATCH = (440, 618, 580, 686)
LIP_X = (256, 770)         # the columns where the cap's lip lies on the forehead
NECK = (512, 697)          # the point the head tilts about: middle of the chin line
CAP_ROOT = (508, 470)      # the point the cap rocks about: middle of its opening, on top of the head
# The head under the cap (invented): its two side lines carried on upward, and its top.
HEAD_DOME = [(262, 432), (268, 392), (292, 346), (340, 304), (410, 276), (500, 266), (590, 274), (670, 304), (728, 350), (762, 410), (774, 470), (774, 530)]
SIDE_L = [(238, 500), (250, 458), (262, 428), (270, 392), (286, 356)]     # from the drawn edge on up
SIDE_R = [(772, 560), (774, 520), (772, 470), (762, 412), (744, 372)]
# The body's own top, hidden behind the head (invented).
BODY_TOP = [(318, 690), (345, 662), (392, 646), (450, 638), (512, 636), (575, 638), (632, 646), (676, 662), (700, 692)]
BIB_UP = 30                # how far the bib goes on up behind the chin
UNDER_REACH = 52           # how far the cap's inside goes on behind the head


def box_mask(shape, box):
    m = np.zeros(shape, bool)
    x0, y0, x1, y1 = box
    m[y0:y1, x0:x1] = True
    return m


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


def to_rig(p):
    return [p[0] * K + SHIFT[0], p[1] * K + SHIFT[1]]


def save_layer(name, rgb, alpha, layers, pivot=None, out=OUT, shift=SHIFT):
    ys, xs = np.where(alpha > 0.004)
    y0, y1 = max(ys.min() - 4, 0), ys.max() + 5
    x0, x1 = max(xs.min() - 4, 0), xs.max() + 5
    x0 -= x0 % SNAP; y0 -= y0 % SNAP; x1 += -x1 % SNAP; y1 += -y1 % SNAP
    # The snapped crop may reach past the drawing: pad with nothing.
    h, w = alpha.shape
    pad = max(0, x1 - w, y1 - h)
    if pad:
        alpha = np.pad(alpha, ((0, pad), (0, pad)))
        rgb = np.pad(rgb, ((0, pad), (0, pad), (0, 0)))
    a = alpha[y0:y1, x0:x1, None]
    pre = np.concatenate([rgb[y0:y1, x0:x1] * a, a * 255], -1)
    size = (round((x1 - x0) * K), round((y1 - y0) * K))
    small = np.stack(
        [np.asarray(Image.fromarray(pre[..., c].astype(np.float32), "F").resize(size, Image.LANCZOS)) for c in range(4)], -1
    )
    al = np.clip(small[..., 3:], 0, 255)
    col = np.where(al > 0.5, small[..., :3] / np.maximum(al / 255, 1e-4), 0)
    Image.fromarray(np.concatenate([np.clip(col, 0, 255), al], -1).round().astype(np.uint8), "RGBA").save(out / f"{name}.png")
    entry = {"name": name, "file": f"{name}.png", "x": round(x0 * K + shift[0], 2), "y": round(y0 * K + shift[1], 2), "w": size[0], "h": size[1]}
    if pivot:
        entry["pivot"] = [round(pivot[0] * K + shift[0], 2), round(pivot[1] * K + shift[1], 2)]
    layers.append(entry)


def main():
    OUT.mkdir(parents=True, exist_ok=True)
    rgb = np.asarray(Image.open(ROOT / "source.png").convert("RGB")).astype(np.float32)
    shape = rgb.shape[:2]
    yy, xx = np.mgrid[: shape[0], : shape[1]]

    # Foreground: everything the background colour cannot reach from the border
    # (the largest piece: the watermark in the corner is another one).
    bg = np.median(np.concatenate([rgb[:40].reshape(-1, 3), rgb[:, :40].reshape(-1, 3), rgb[:, -40:].reshape(-1, 3)]), 0)
    lab, _ = ndi.label(np.sqrt(((rgb - bg) ** 2).sum(-1)) < 14)
    border = set(np.unique(np.concatenate([lab[0], lab[-1], lab[:, 0], lab[:, -1]]))) - {0}
    fg = ndi.binary_fill_holes(~np.isin(lab, list(border)))
    l2, n2 = ndi.label(fg)
    fg = l2 == (1 + np.argmax(ndi.sum(fg, l2, range(1, n2 + 1))))

    grow = lambda m, n=9: ndi.binary_dilation(m, iterations=n)
    shrink = lambda m, n=1: ndi.binary_erosion(m, iterations=n)
    # A hard edge: one pixel in where two parts meet, two at the silhouette, where
    # the outline fades into the paper (no haze outside the figure).
    outside = np.clip(ndi.gaussian_filter(shrink(fg, 2).astype(np.float32), 0.9) * 1.15, 0, 1)
    edge = lambda m: np.clip(ndi.gaussian_filter(shrink(m).astype(np.float32), 0.9) * 1.15, 0, 1) * outside
    soft = lambda m, s=1.0: np.clip(ndi.gaussian_filter(m.astype(np.float32), s) * 1.2, 0, 1)
    sm = lambda pts: smooth(pts) if len(pts) >= 4 else list(pts)
    curve = lambda pts, w=5: line_mask(shape, sm(pts), w) > 0

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

    bar = curve(CHIN)
    reg, _ = ndi.label(fg & ~line & ~bar)

    def at(p):
        assert reg[p[1], p[0]] != 0, f"seed {p} sits on a line"
        return reg == reg[p[1], p[0]]

    cap_in, head_in, body_in, bib_in = at(SEED_CAP), at(SEED_HEAD), at(SEED_BODY), at(SEED_BIB)
    for p in SEED_SPOTS:
        cap_in = cap_in | at(p)
    assert not (head_in & bib_in).any() and not (head_in & cap_in).any() and not (body_in & bib_in).any(), "the outline does not close a part"

    patch = rgb[GRAIN_PATCH[1]:GRAIN_PATCH[3], GRAIN_PATCH[0]:GRAIN_PATCH[2]]
    grain = patch - ndi.gaussian_filter(patch, (2, 2, 0))
    ink = np.median(src[line & grow(head_in, 6) & (yy > 560) & (yy < 690)], 0)       # the head's own line
    body_ink = np.median(src[line & grow(body_in, 6) & (yy > 760)], 0)
    bib_ink = np.median(src[line & grow(bib_in, 5) & (yy > 730)], 0)

    def inked(pts, width=7, colour=None):
        """A stroke like the drawn ones: (colour, coverage)."""
        s = np.clip(ndi.gaussian_filter(line_mask(shape, sm(pts), width), 1.3) * 1.15, 0, 1)
        return (ink if colour is None else colour), s

    def over(base, colour, a, k=0.94):
        return base * (1 - a[..., None] * k) + colour * a[..., None] * k

    # ---------------------------------------------------------------- the pieces as drawn
    # The cap sits in front: the line across the forehead is its rim. The head is in
    # front of the two bits of the cap's inside beside it: those lines are the head's.
    cols = np.where(head_in.any(0))[0]
    top = np.full(shape[1], shape[0])
    top[cols] = np.argmax(head_in, 0)[cols]                        # first row of the head in each column
    under_rim = (xx >= 264) & (xx <= 772)                           # between the two points where the rim meets the head's sides
    above_head = under_rim & (yy < top[None, :])
    # Where the head's line borders the cap's inside it is taken no wider than it is drawn;
    # at the silhouette it is taken out to the paper.
    by_cap = ((xx < 512) & (yy < 528)) | ((xx >= 512) & (yy < 606))
    reach = np.where(by_cap, grow(head_in, 8), grow(head_in, 12))
    head = ndi.binary_fill_holes(reach & fg & ~body_in & ~bib_in & ~cap_in) & ~above_head
    head = ndi.binary_opening(head, iterations=4) | head_in
    body_seen = ndi.binary_fill_holes(grow(body_in | bib_in, 12) & fg & ~head & ~cap_in) & ~head
    body_seen = ndi.binary_opening(body_seen, iterations=3) | body_in | bib_in
    # Under the rim line runs a darker band with a soft lower edge: the lip of the
    # cap, seen from below. It is the cap's, and goes where the cap goes. Its lower
    # edge is where the drawing gets lighter again, some 28 px under the rim.
    rim_y = top.astype(np.float32)
    rim_y[cols] = ndi.median_filter(top[cols].astype(np.float32), 15)
    rise = np.gradient(ndi.gaussian_filter(lum, 2.0), axis=0)
    thick = np.full(shape[1], np.nan, np.float32)
    for x in range(LIP_X[0], LIP_X[1] + 1):
        r = int(rim_y[x])
        seg = rise[r + 6: r + 46, x]
        if r < shape[0] - 50 and seg.max() > 1.2:
            thick[x] = 6 + int(np.argmax(seg))
    span = slice(LIP_X[0], LIP_X[1] + 1)
    t = thick[span]
    t[np.isnan(t)] = np.nanmedian(t)
    thick[span] = ndi.gaussian_filter1d(np.clip(ndi.median_filter(t, 41), 20, 40), 10)
    lip_low = np.minimum(rim_y + np.nan_to_num(thick), shape[0])
    side_lines = grow(~head & ((xx < 262) | (xx > 768)), 7)            # the head's own side lines stay the head's
    lip = head & ~side_lines & (xx >= LIP_X[0]) & (xx <= LIP_X[1]) & (yy < lip_low[None, :])
    cap = fg & ~(head & ~lip) & ~body_seen
    l3, n3 = ndi.label(cap)
    cap = l3 == l3[SEED_CAP[1], SEED_CAP[0]]
    # Whatever crumbs are left go to the part they touch most.
    crumbs = fg & ~head & ~body_seen & ~cap
    head |= crumbs & grow(head, 3)
    body_seen |= crumbs & ~head

    # ---------------------------------------------------------------- face parts
    dark = lum < 120
    eye = lambda box: grow(ndi.binary_fill_holes(dark & box_mask(shape, box)), 3)
    eye_l, eye_r = eye(EYE_BOX_L), eye(EYE_BOX_R)
    mouth = grow(line & box_mask(shape, MOUTH_BOX), 3)

    # ---------------------------------------------------------------- head: the face, and a dome under the cap
    head_rgb = rgb
    for hole in (eye_l, eye_r, mouth):
        head_rgb = diffuse_fill(head_rgb, grow(hole, 5), grain)
    dome = poly_mask(shape, smooth(HEAD_DOME) + [(774, 600), (238, 600)]) & ~head & (yy < 600)
    dome &= grow(cap, 2) | above_head                                  # only what the cap hides
    # Under the cap's lip the head is skin like the rest of the forehead: the lip's
    # place and the dome are painted from the skin just below it.
    fringe = head & under_rim & (yy < lip_low[None, :] + 3)
    head_known = shrink(head_in, 6) & ~grow(eye_l | eye_r | mouth, 8) & (yy > lip_low[None, :] + 8)
    head_rgb = fill_from(head_rgb, dome | fringe, head_known & (yy < 580), grain)
    head_full = head | dome
    head_a = edge(head) * (head | ~dome)
    dome_a = np.clip(ndi.gaussian_filter(shrink(head_full).astype(np.float32), 0.9) * 1.15, 0, 1)
    head_a = np.where(dome | grow(dome, 2), np.maximum(head_a, dome_a), head_a)
    # Its side lines go on up under the cap.
    for pts in (SIDE_L, SIDE_R):
        c, s = inked(pts, 6)
        s = s * grow(dome, 3) * ~shrink(head, 2)
        head_rgb = over(head_rgb, c, s)
        head_a = np.maximum(head_a, s)

    # ---------------------------------------------------------------- body: its own top and the rest of the bib, behind the head
    top_line = smooth(BODY_TOP)
    hidden_top = poly_mask(shape, top_line + [(700, 730), (318, 730)]) & head & ~body_seen
    body = ndi.binary_fill_holes(body_seen | hidden_top)
    body_known = shrink(body_in, 5) & (yy < 800)
    body_rgb = fill_from(rgb, hidden_top | (grow(hidden_top, 3) & head), body_known, grain)
    # The bib goes on up round the neck.
    bib_cols = np.where(bib_in.any(0))[0]
    bib_top = np.full(shape[1], shape[0])
    bib_top[bib_cols] = np.argmax(bib_in, 0)[bib_cols]
    bib_up = (yy < bib_top[None, :] + 4) & (yy > bib_top[None, :] - BIB_UP) & (xx > bib_cols.min() + 6) & (xx < bib_cols.max() - 6) & hidden_top
    bib_up = ndi.binary_opening(bib_up, iterations=2)
    bib_paint = fill_from(rgb, bib_up, shrink(bib_in, 4) & (yy < 730), grain * 0.6)
    feather = soft(bib_up, 1.5)[..., None]
    body_rgb = body_rgb * (1 - feather) + bib_paint * feather
    # The bib's two side lines carried up with it.
    for x_edge in (bib_cols.min() + 3, bib_cols.max() - 3):
        y_at = int(bib_top[int(np.clip(x_edge + (8 if x_edge < 470 else -8), 0, shape[1] - 1))])
        c, s = inked([(x_edge + (3 if x_edge < 470 else -3), y_at + 6), (x_edge + (5 if x_edge < 470 else -5), y_at - BIB_UP + 2)], 6, bib_ink)
        body_rgb = over(body_rgb, c, s * hidden_top)
    # The body's top line, where none was drawn.
    c, s = inked(top_line, 7, body_ink)
    s = s * grow(hidden_top, 4) * head
    body_rgb = over(body_rgb, c, s)
    body_a = np.maximum(edge(body), s * head)

    # ---------------------------------------------------------------- cap, and its inside carried on behind the head
    cap_rgb = rgb
    cap_a = edge(cap)
    cap_ink = np.median(src[line & grow(cap, 1) & ~shrink(fg, 9) & (yy > 380) & (yy < 600)], 0)
    under = np.zeros(shape, bool)
    foot_line = np.zeros(shape, np.float32)
    # The cap's inside beside the head: cap with the head right next to it on the
    # same row, below where the lip ends.
    beside_l = cap & ~lip & np.roll(head, -3, 1) & (xx < 262) & (yy > 412)
    beside_r = cap & ~lip & np.roll(head, 3, 1) & (xx > 768) & (yy > 520)
    for side, inward in ((beside_l, 1), (beside_r, -1)):
        rows = np.where(side.any(1))[0]
        for y in rows:
            xs_ = np.where(side[y])[0]
            x_edge = xs_.max() if inward > 0 else xs_.min()          # the cap's last pixel before the head
            lo, hi = (x_edge - 5, x_edge + UNDER_REACH) if inward > 0 else (x_edge - UNDER_REACH, x_edge + 5)
            under[y, max(lo, 0):hi] = True
            if y >= rows.max() - 6:                                    # the cap's lower line goes on behind the head
                foot_line[y, max(lo, 0):hi] = 1
    under &= head | cap
    inside_known = shrink(cap, 5) & ~lip & grow(head, 40) & ~line & ~above_head & (yy > 440) & ((xx < 250) | (xx > 780))
    under_rgb = fill_from(rgb, under, inside_known, grain * 0.6)
    under_rgb = over(under_rgb, cap_ink, np.clip(ndi.gaussian_filter(foot_line, 1.2), 0, 1) * under)
    under_a = np.clip(ndi.gaussian_filter(shrink(under).astype(np.float32), 1.0) * 1.15, 0, 1)

    # ---------------------------------------------------------------- spots: where a mallet would land
    spots = []
    for (x0, y0, x1, y1), seed in zip(SPOT_BOXES, SPOT_SEEDS):
        # Two kinds of pixel in the box, spot and cap: split them where the two are best told apart (Otsu).
        g = ndi.gaussian_filter(src[..., 1], 1.5)[y0:y1, x0:x1]
        levels = np.arange(np.floor(g.min()), np.ceil(g.max()) + 1)
        split = max(levels[1:-1], key=lambda t: (g < t).mean() * (g >= t).mean() * (g[g >= t].mean() - g[g < t].mean()) ** 2)
        ls, _ = ndi.label(g >= split)
        assert ls[seed[1] - y0, seed[0] - x0] != 0, f"spot seed {seed} is not on a spot"
        m = np.zeros(shape, bool)
        m[y0:y1, x0:x1] = ndi.binary_fill_holes(ls == ls[seed[1] - y0, seed[0] - x0])
        ringed = at(seed)                                              # a spot with a line all round it is that region and its line
        if ringed.sum() < 15000:
            m = grow(ringed, 3)
        cy, cx = ndi.center_of_mass(m)
        spots.append([round(float(cx * K + SHIFT[0]), 1), round(float(cy * K + SHIFT[1]), 1), round(float(np.sqrt(m.sum() / np.pi) * K), 1)])
    spots.sort()

    # ---------------------------------------------------------------- write
    layers = []
    save_layer("body", body_rgb, body_a, layers)
    save_layer("cap_under", under_rgb, under_a, layers, pivot=CAP_ROOT)
    save_layer("head", head_rgb, head_a, layers, pivot=NECK)
    save_layer("eye_l", rgb, soft(eye_l), layers)
    save_layer("eye_r", rgb, soft(eye_r), layers)
    save_layer("mouth", rgb, soft(mouth), layers)
    save_layer("cap", cap_rgb, cap_a, layers, pivot=CAP_ROOT)

    centre = lambda m: [round(float(np.where(m)[1].mean() * K + SHIFT[0]), 2), round(float(np.where(m)[0].mean() * K + SHIFT[1]), 2)]
    ys, xs = np.where(fg)
    rig = {
        "id": "gugu",
        "kind": "mushroom",
        "size": [512, 512],
        "bbox": [round(float(v), 1) for v in (xs.min() * K + SHIFT[0], ys.min() * K + SHIFT[1], xs.max() * K + SHIFT[0], ys.max() * K + SHIFT[1])],
        "anchors": {
            "eye_l": centre(eye_l), "eye_r": centre(eye_r), "mouth": centre(mouth),
            "neck": to_rig(NECK),
            "cap_root": to_rig(CAP_ROOT),
        },
        # The pale spots on the cap at rest: [x, y, radius], left to right.
        "spots": spots,
        # `cap_under` is optional: an engine that leaves it out still draws the rest pose whole.
        "order": ["body", "cap_under", "head", "eye_l", "eye_r", "mouth", "cap"],
        "layers": layers,
    }
    (OUT / "rig.json").write_text(json.dumps(rig, indent=2), "utf-8")

    # Rest-pose check: the stack, in the engine's order, must rebuild the source.
    inner = shrink(fg, 8)   # the outermost pixels were recoloured on purpose (see `rim`)

    def rebuilt(stack):
        comp = np.broadcast_to(bg, shape + (3,)).astype(np.float32).copy()
        for col, a in stack:
            comp = comp * (1 - a[..., None]) + col * a[..., None]
        return np.abs(comp - src).max(-1)

    face = [(rgb, soft(eye_l)), (rgb, soft(eye_r)), (rgb, soft(mouth))]
    for label, stack in (("with cap_under", [(body_rgb, body_a), (under_rgb, under_a), (head_rgb, head_a)] + face + [(cap_rgb, cap_a)]),
                         ("contract layers only", [(body_rgb, body_a), (head_rgb, head_a)] + face + [(cap_rgb, cap_a)])):
        err = rebuilt(stack)
        print(f"rest-pose error, {label}: mean {err[inner].mean():.2f}  p99 {np.percentile(err[inner], 99):.1f}  max {err[inner].max():.0f}")
        bad = inner & (err > 12)
        if bad.any():
            lab2, n = ndi.label(grow(bad, 3))
            for i in np.argsort(ndi.sum(bad, lab2, range(1, n + 1)))[::-1][:6]:
                by, bx = np.where((lab2 == i + 1) & bad)
                print(f"  differs: {len(bx)} px around ({int(bx.mean())}, {int(by.mean())})")
    print(f"cap {cap.sum()}  head {head.sum()} (+dome {dome.sum()})  body {body.sum()}  under {under.sum()}  eyes {eye_l.sum()}/{eye_r.sum()}  mouth {mouth.sum()}")
    print("bbox", rig["bbox"], "height", round(rig["bbox"][3] - rig["bbox"][1], 1), " spots", spots)
    Image.fromarray(np.clip(err * 6, 0, 255).astype(np.uint8)).save(OUT.parent / "_rest_error.png")


if __name__ == "__main__":
    main()
