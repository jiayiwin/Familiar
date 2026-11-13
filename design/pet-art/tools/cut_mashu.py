"""Cut the 麻薯 (cream cat) source illustration into rig layers.

Regions are found from the drawn outline, which closes the head (with its ears),
the body, the tail, the collar band and the bow by itself; two hand-placed
curves close the ears across their bases, where the drawing has no line.

The figure is cut so that its parts can move against each other:
  tail   swings about its root, so the root hidden behind the body is painted in
  body   the lower blob with the collar band; under the head it gets a neck and
         the rest of the band, which show when the head tilts
  bow    the knot alone; the band is painted through under it
  ears   stand behind the head and reach a little into it
  head   closed across the ear bases (no line there, as in the drawing), with
         the eyes and mouth painted out and its lower line completed under the bow

    python design/pet-art/tools/cut_mashu.py
"""
import json
from pathlib import Path

import numpy as np
from PIL import Image
from scipy import ndimage as ndi

from cut_zimeng_back import fill_from, line_mask, poly_mask, smooth

ROOT = Path(__file__).resolve().parents[1] / "mashu"
OUT = ROOT / "rig"
SCALE = 0.5  # 1024 source -> 512 rig space

# Source-space (1024) landmarks, read off the illustration.
SEED_HEAD, SEED_BODY, SEED_TAIL = (520, 350), (521, 752), (287, 746)
SEED_EAR_L, SEED_PINK_L, SEED_EAR_R = (300, 200), (353, 227), (640, 140)
SEED_BAND = [(477, 618), (647, 604)]
SEED_BOW = [(558, 625), (615, 620), (590, 625)]
# Where each ear ends and the skull begins: no line in the drawing, so the cut
# runs just under the pink of the ear, from the notch in the outline to the top of the head.
EAR_L_CLOSE = [(277, 302), (300, 297), (335, 281), (380, 251), (425, 219), (455, 198), (470, 192)]
EAR_R_CLOSE = [(556, 167), (590, 175), (615, 197), (650, 222), (690, 244), (715, 254), (735, 258)]
PIVOT_EAR_L, PIVOT_EAR_R = (372, 250), (648, 218)
# What of each ear lies behind the head (invented): a strip of ear under the cut...
EAR_L_UNDER = [(482, 188), (508, 202), (494, 238), (442, 258), (395, 284), (350, 310), (310, 334), (290, 352), (283, 330)]
EAR_R_UNDER = [(729, 318), (716, 292), (690, 280), (650, 258), (612, 236), (580, 218), (545, 218), (514, 206), (522, 188), (540, 176)]
# ...and the ear's own outline carried on past where it meets the head's.
EAR_L_LINE = [[(268, 250), (277, 302), (283, 330), (289, 354)], [(430, 160), (462, 183), (480, 203), (495, 224)]]
EAR_R_LINE = [[(743, 205), (735, 258), (731, 290), (729, 320)], [(585, 148), (556, 170), (536, 188), (522, 206)]]
EYE_BOX_L, EYE_BOX_R = (435, 410, 530, 505), (665, 395, 750, 485)
MOUTH_BOX = (590, 464, 634, 494)
GRAIN_PATCH = (450, 250, 590, 350)
# The collar: its lower edge as drawn (it runs on behind the bow), and how thick the band is.
BAND_LOW = [(386, 604), (400, 616), (430, 624), (460, 630), (490, 634), (520, 636), (545, 637), (575, 634), (605, 628), (630, 621), (648, 614), (662, 607), (669, 598)]
BAND_THICK = 26
NECK_HIGH = 34
NECK = (525, 622)            # the point the head tilts about
# The head's lower line where the bow covers it.
CHIN = [(470, 610), (505, 612), (540, 612), (575, 609), (605, 605), (632, 600), (662, 591)]
# The tail's root, hidden behind the body.
TAIL_AXIS = [(318, 762), (384, 770)]
TAIL_HALF = 42
PIVOT_TAIL = (384, 770)


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
    sm = lambda pts: smooth(pts) if len(pts) >= 4 else list(pts)   # two or three points are a straight run
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
    # Where two strokes meet, a speck of bare paper is sometimes left between them.
    # On paper nobody sees it; on a dark desktop it is a bright dot. Ink them in.
    bare, n_bare = ndi.label(fg & ~line & (np.sqrt(((src - bg) ** 2).sum(-1)) < 16))
    small = np.isin(bare, 1 + np.where(ndi.sum(np.ones(shape), bare, range(1, n_bare + 1)) < 120)[0]) & ndi.binary_dilation(line, iterations=3)
    small &= ~shrink(fg, 14)                                       # only at the silhouette: inside, a pale gap beside a line is the drawing
    rgb[small] = darkest[small]

    bar_l, bar_r = curve(EAR_L_CLOSE), curve(EAR_R_CLOSE)
    reg, _ = ndi.label(fg & ~line & ~bar_l & ~bar_r)

    def at(p):
        assert reg[p[1], p[0]] != 0, f"seed {p} sits on a line"
        return reg == reg[p[1], p[0]]

    head_in, body_in, tail_in = at(SEED_HEAD), at(SEED_BODY), at(SEED_TAIL)
    ear_l_in, ear_r_in = at(SEED_EAR_L) | at(SEED_PINK_L), at(SEED_EAR_R)
    band_in = at(SEED_BAND[0]) | at(SEED_BAND[1])
    bow_in = at(SEED_BOW[0]) | at(SEED_BOW[1]) | at(SEED_BOW[2])
    assert not (head_in & ear_l_in).any() and not (head_in & ear_r_in).any(), "an ear is not closed off from the head"

    patch = rgb[GRAIN_PATCH[1]:GRAIN_PATCH[3], GRAIN_PATCH[0]:GRAIN_PATCH[2]]
    grain = patch - ndi.gaussian_filter(patch, (2, 2, 0))
    ink = np.median(rgb[line & grow(head_in, 8) & (yy < 560)], 0)           # the warm brown of the outline
    mint = np.median(rgb[shrink(band_in, 2)], 0)
    mint_ink = np.median(rgb[line & grow(band_in, 7) & ~grow(head_in, 9) & ~grow(body_in, 4)], 0)

    def inked(pts, width=7, colour=None):
        """A stroke like the drawn ones: (colour, coverage)."""
        s = np.clip(ndi.gaussian_filter(line_mask(shape, sm(pts), width), 1.3) * 1.15, 0, 1)
        return (ink if colour is None else colour), s

    def over(base, colour, a):
        return base * (1 - a[..., None] * 0.94) + colour * a[..., None] * 0.94

    # ---------------------------------------------------------------- the pieces as drawn
    # The bow: its three cells and their mint outline.
    mintish = (rgb[..., 1] - rgb[..., 0] > 9) & fg
    bow = ndi.binary_fill_holes(grow(bow_in, 6) & (mintish | bow_in))
    bow = ndi.binary_fill_holes(ndi.binary_opening(bow, iterations=3) | bow_in)
    # The head owns its outline, down to the line it shares with the collar.
    head_all = ndi.binary_fill_holes(grow(head_in | ear_l_in | ear_r_in, 12) & fg & ~body_in & ~tail_in & ~band_in & ~bow_in)
    head_all &= ~grow(body_in, 5)
    head_all = ndi.binary_opening(head_all, iterations=4)
    body_seen = ndi.binary_fill_holes(grow(body_in | band_in, 12) & fg & ~head_all & ~tail_in & ~bow) & ~bow
    body_seen = ndi.binary_opening(body_seen, iterations=3)
    tail_seen = ndi.binary_opening(grow(tail_in, 12) & fg & ~body_seen & ~head_all, iterations=3)

    # ---------------------------------------------------------------- ears
    def ear(inside, bar, under_pts, close_pts, lines):
        seen = grow(inside, 12) & head_all & ~grow(shrink(head_in, 3), 3) | inside
        seen = ndi.binary_opening(seen, iterations=4) | inside
        under = poly_mask(shape, smooth(close_pts) + under_pts) & head_all & ~seen
        col = fill_from(rgb, under | (bar & ~inside), shrink(inside & ~mintish, 5) & (rgb[..., 1] - rgb[..., 2] < 40) & (rgb[..., 0] - rgb[..., 1] < 12), grain)
        a = np.maximum(edge(seen | grow(under, 2) & head_all), 0)
        for pts in lines:
            c, s = inked(pts)
            s = s * grow(under, 10) * ~seen          # only the part that was never drawn
            col = over(col, c, s)
            a = np.maximum(a, s)
        return seen, under, col, a

    ear_l, under_l, ear_l_rgb, ear_l_a = ear(ear_l_in, bar_l, EAR_L_UNDER, EAR_L_CLOSE, EAR_L_LINE)
    ear_r, under_r, ear_r_rgb, ear_r_a = ear(ear_r_in, bar_r, EAR_R_UNDER, EAR_R_CLOSE, EAR_R_LINE)

    # ---------------------------------------------------------------- face parts
    dark = lum < 120
    eye = lambda box: grow(ndi.binary_fill_holes(dark & box_mask(shape, box)), 3)
    eye_l, eye_r = eye(EYE_BOX_L), eye(EYE_BOX_R)
    mouth = grow(line & box_mask(shape, MOUTH_BOX), 3)

    # ---------------------------------------------------------------- head
    head = head_all & ~ear_l & ~ear_r
    head_rgb = rgb
    for hole in (eye_l, eye_r, mouth):
        head_rgb = diffuse_fill(head_rgb, grow(hole, 5), grain)
    # Under the bow the head's lower line was never drawn: carry it across.
    chin = smooth(CHIN)
    above_chin = poly_mask(shape, chin + [(662, 500), (470, 500)])
    bow_big = grow(bow, 5)
    head_known = shrink(head_in, 6) & ~bow_big
    head = (head & ~bow_big) | (above_chin & bow_big & fg)
    head_rgb = fill_from(head_rgb, above_chin & bow_big, head_known, grain)
    chin_ink = np.median(src[line & box_mask(shape, (410, 596, 530, 618)) & (src[..., 0] > src[..., 1] + 6)], 0)   # this line is drawn lighter than the skull's
    c, s = inked(chin, 6, chin_ink)
    s = s * grow(bow_big, 3)
    head_rgb = over(head_rgb, c, s)
    head_a = np.maximum(edge(head), s)

    # ---------------------------------------------------------------- body: the blob, the whole collar, a neck
    low = smooth(BAND_LOW)
    high = [(x, y - BAND_THICK) for x, y in low]
    band_full = poly_mask(shape, low + high[::-1])
    neck_top = [(x + (6 if x < 520 else -6) * (1 - abs(x - 527) / 141) ** 0, y - NECK_HIGH) for x, y in high]
    neck = poly_mask(shape, high + neck_top[::-1]) | band_full
    under_bow = bow_big & ~head_all
    body = body_seen | under_bow & poly_mask(shape, low + [(669, 720), (386, 720)]) | band_full | neck
    body = ndi.binary_fill_holes(body)
    body_known = shrink(body_in, 5) & ~bow_big
    body_rgb = fill_from(rgb, (neck | under_bow | bow_big) & ~band_in & ~(body_seen & ~bow_big), body_known, grain)
    band_hidden = band_full & ~(shrink(band_in, 1) & ~bow_big)
    band_known = shrink(band_in, 2) & ~bow_big
    band_paint = fill_from(rgb, band_hidden, band_known, grain * 0.6)
    feather = soft(band_hidden, 1.2)[..., None]
    body_rgb = body_rgb * (1 - feather) + band_paint * feather
    # Its edges: the lower one behind the bow, the upper one and the two ends under the head.
    hidden_under_head = grow(head_all, 2) | bow_big
    for pts in (low, high, [low[0], high[0]], [low[-1], high[-1]]):
        c, s = inked(pts, 6, mint_ink)
        s = s * hidden_under_head * ~grow(band_in & ~bow_big, 1)
        body_rgb = over(body_rgb, c, s)
    # The neck's two sides, where it leaves the collar.
    for pts in ([high[0], neck_top[0]], [high[-1], neck_top[-1]]):
        c, s = inked(pts, 7)
        body_rgb = over(body_rgb, c, s * hidden_under_head)
    body_a = edge(body)

    # ---------------------------------------------------------------- tail
    tail_root = (line_mask(shape, TAIL_AXIS, TAIL_HALF * 2) > 0) | ((xx - PIVOT_TAIL[0]) ** 2 + (yy - PIVOT_TAIL[1]) ** 2 < TAIL_HALF ** 2)
    tail = ndi.binary_opening(ndi.binary_closing(tail_seen | tail_root, iterations=8), iterations=3)
    # Keep the tail as drawn (its fill and its own line) but not the fringe of the
    # body's line it ended on; everything else, root included, is painted from its fill.
    by_body = grow(body_seen, 7)
    drawn = tail_seen & (tail_in | line) & ~by_body
    tail_rgb = fill_from(rgb, tail & ~drawn, shrink(tail_in, 6) & ~by_body, grain)
    # Its outline goes on round the root, where none was drawn.
    own_line = grow(tail_seen & line & ~by_body, 4)
    rim_a = soft(tail & ~shrink(tail, 6) & ~own_line, 1.3)
    tail_rgb = over(tail_rgb, ink, rim_a)
    tail_a = edge(tail)

    # ---------------------------------------------------------------- write
    layers = []
    save_layer("tail", tail_rgb, tail_a, layers, pivot=PIVOT_TAIL)
    save_layer("body", body_rgb, body_a, layers)
    save_layer("bow", rgb, soft(bow, 0.9), layers, pivot=(590, 625))
    save_layer("ear_l", ear_l_rgb, ear_l_a, layers, pivot=PIVOT_EAR_L)
    save_layer("ear_r", ear_r_rgb, ear_r_a, layers, pivot=PIVOT_EAR_R)
    save_layer("head", head_rgb, head_a, layers, pivot=NECK)
    save_layer("eye_l", rgb, soft(eye_l), layers)
    save_layer("eye_r", rgb, soft(eye_r), layers)
    save_layer("mouth", rgb, soft(mouth), layers)

    centre = lambda m: [float(np.where(m)[1].mean() * SCALE), float(np.where(m)[0].mean() * SCALE)]
    ys, xs = np.where(fg)
    rig = {
        "id": "mashu",
        "kind": "cat",
        "size": [int(shape[1] * SCALE), int(shape[0] * SCALE)],
        "bbox": [float(xs.min() * SCALE), float(ys.min() * SCALE), float(xs.max() * SCALE), float(ys.max() * SCALE)],
        "anchors": {
            "eye_l": centre(eye_l), "eye_r": centre(eye_r), "mouth": centre(mouth),
            "neck": [NECK[0] * SCALE, NECK[1] * SCALE],
            "tail_root": [PIVOT_TAIL[0] * SCALE, PIVOT_TAIL[1] * SCALE],
        },
        # The drawing has the bow in front of the chin. An engine that draws the
        # bow before the head still works: the bow's top tucks under the chin.
        "order": ["tail", "body", "ear_l", "ear_r", "head", "bow", "eye_l", "eye_r", "mouth"],
        "layers": layers,
    }
    (OUT / "rig.json").write_text(json.dumps(rig, indent=2), "utf-8")

    # Rest-pose check: the stack, in the engine's order, must rebuild the source.
    inner = shrink(fg, 8)   # the outermost pixels were recoloured on purpose (see `rim`)
    bow_layer = (rgb, soft(bow, 0.9))

    def rebuilt(stack):
        comp = np.broadcast_to(bg, shape + (3,)).astype(np.float32).copy()
        for col, a in stack:
            comp = comp * (1 - a[..., None]) + col * a[..., None]
        return np.abs(comp - src).max(-1)

    below = [(tail_rgb, tail_a), (body_rgb, body_a)]
    heads = [(ear_l_rgb, ear_l_a), (ear_r_rgb, ear_r_a), (head_rgb, head_a)]
    face = [(rgb, soft(eye_l)), (rgb, soft(eye_r)), (rgb, soft(mouth))]
    over_chin = rebuilt(below + heads + [bow_layer] + face)      # rig.json's "order"
    print(f"rest-pose error, bow over the chin: mean {over_chin[inner].mean():.2f}  p99 {np.percentile(over_chin[inner], 99):.1f}  max {over_chin[inner].max():.0f}")
    err = rebuilt(below + [bow_layer] + heads + face)            # bow under the head
    print(f"ear_l {ear_l.sum()}  ear_r {ear_r.sum()}  head {head.sum()}  body {body.sum()}  tail {tail.sum()}  bow {bow.sum()}  eyes {eye_l.sum()}/{eye_r.sum()}  mouth {mouth.sum()}")
    print(f"rest-pose error, bow under the head: mean {err[inner].mean():.2f}  p99 {np.percentile(err[inner], 99):.1f}  max {err[inner].max():.0f}")
    bad = inner & (err > 12)
    if bad.any():
        lab2, n = ndi.label(grow(bad, 3))
        for i in np.argsort(ndi.sum(bad, lab2, range(1, n + 1)))[::-1][:6]:
            by, bx = np.where((lab2 == i + 1) & bad)
            print(f"  differs: {len(bx)} px around ({int(bx.mean())}, {int(by.mean())})")
    Image.fromarray(np.clip(err * 6, 0, 255).astype(np.uint8)).save(OUT.parent / "_rest_error.png")


if __name__ == "__main__":
    main()
