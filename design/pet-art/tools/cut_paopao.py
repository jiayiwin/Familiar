"""Cut the 泡泡 (mist-blue seal pup) source illustration into rig layers.

The drawing closes very little by itself: the head runs into the body with no
line under the chin, and the curl on top grows out of the head. So the parts
are separated by three hand-placed curves (the seam under the chin, the arc
across the foot of the curl) and everything a part hides is painted in:

  body   torso, both flippers, both feet; under the head it gets a rounded top
         with its own outline, which shows when the head tilts or lifts
  curl   the antenna; its root is carried on down behind the head so it stays
         covered while it swings
  head   with the shell; closed across the foot of the curl, the eyes and the
         mouth painted out, and the little hook of line the curl leaves on the
         head painted out too (it would stay behind when the curl moves)

The drawing is 869 px tall; it is brought to the size of the other pets
(about 390 rig px, standing on y = 452) by SCALE and OFFSET.

    python design/pet-art/tools/cut_paopao.py
"""
import json
import sys
from pathlib import Path

import numpy as np
from PIL import Image
from scipy import ndimage as ndi

from cut_zimeng_back import fill_from, line_mask, poly_mask, smooth

ROOT = Path(__file__).resolve().parents[1] / "paopao"
OUT = ROOT / "rig"
SCALE = 0.45           # source px -> rig px (9/20: crops are snapped to 20 px so layers land on whole pixels)
OFFSET = (27, 30)      # rig px: centres the figure and stands it on y = 452

# Source-space (1024) landmarks, read off the illustration.
# The seam: where the head ends and the body begins. Its two ends follow the
# creases the drawing has there (they are the head's lower line, so the cut runs
# just under them); between them nothing is drawn.
SEAM = [(200, 575), (300, 588), (338, 594), (352, 596), (372, 603), (392, 608), (430, 611), (510, 612), (590, 611), (624, 608),
        (644, 603), (660, 598), (674, 594), (720, 588), (830, 572)]
SEAM_SOFT = (400, 616)                 # between these x the seam has no line: the head fades into the body
NECK = (510, 606)                      # the point the head tilts about
# The top of the torso, hidden under the head.
DOME = [(338, 596), (350, 574), (376, 556), (420, 544), (470, 538), (510, 536), (550, 538), (600, 544), (642, 556), (664, 574), (674, 596)]
# The arc across the foot of the curl, from where the head's top line meets the
# curl's left line to where the curl's right line turns into the head's (that
# turn stays with the head: lifted off, it would be a hook in the air).
CURL_ARC = [(457, 203), (470, 200), (486, 196), (503, 193), (518, 191), (532, 189)]
CURL_SIDE = [(532, 189), (538, 187), (560, 189), (615, 186), (615, 50), (425, 50), (425, 150), (440, 172), (447, 187), (452, 198)]   # with CURL_ARC: all of the curl
CURL_ROOT = (495, 196)                 # the curl swings about this
# The curl's left line runs on into the head as a little hook. It goes with the
# curl: it is the left side of the root hidden behind the head, and is painted out of the head.
HOOK = [(459, 199), (466, 208), (475, 218), (484, 225), (491, 229)]
ROOT_LOW = [(491, 234), (505, 236), (530, 235), (547, 232)]          # how far the root goes on behind the head
ROOT_LINE = [(527, 184), (532, 189), (539, 199), (544, 210), (546, 221), (547, 232)]   # its right side, where no line was drawn
EYE_BOX_L, EYE_BOX_R = (300, 380, 402, 482), (506, 392, 612, 494)
MOUTH_BOX = (412, 476, 456, 500)
SHELL_BOX = (270, 190, 450, 360)
GRAIN_PATCH = (400, 300, 520, 400)
NOTCHES = [(343, 590), (669, 592)]    # where the head's line meets a flipper's: glow is caught in the angle


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
    return [p[0] * SCALE + OFFSET[0], p[1] * SCALE + OFFSET[1]]


def save_layer(name, rgb, alpha, layers, pivot=None):
    ys, xs = np.where(alpha > 0.004)
    y0, y1 = max(ys.min() - 4, 0), min(ys.max() + 5, alpha.shape[0])
    x0, x1 = max(xs.min() - 4, 0), min(xs.max() + 5, alpha.shape[1])
    # Snap the crop to 20 source px, so that at 9/20 the layer is a whole number of rig pixels and sits on one.
    x0 -= x0 % 20; y0 -= y0 % 20; x1 += -x1 % 20; y1 += -y1 % 20
    a = np.zeros((y1 - y0, x1 - x0, 1), np.float32)
    col = np.zeros((y1 - y0, x1 - x0, 3), np.float32)
    yb, xb = min(y1, alpha.shape[0]), min(x1, alpha.shape[1])
    a[: yb - y0, : xb - x0, 0] = alpha[y0:yb, x0:xb]
    col[: yb - y0, : xb - x0] = rgb[y0:yb, x0:xb]
    pre = np.concatenate([col * a, a * 255], -1)
    size = (round((x1 - x0) * SCALE), round((y1 - y0) * SCALE))
    small = np.stack(
        [np.asarray(Image.fromarray(pre[..., c].astype(np.float32), "F").resize(size, Image.LANCZOS)) for c in range(4)], -1
    )
    al = np.clip(small[..., 3:], 0, 255)
    out = np.where(al > 0.5, small[..., :3] / np.maximum(al / 255, 1e-4), 0)
    Image.fromarray(np.concatenate([np.clip(out, 0, 255), al], -1).round().astype(np.uint8), "RGBA").save(OUT / f"{name}.png")
    entry = {"name": name, "file": f"{name}.png", "x": round(x0 * SCALE) + OFFSET[0], "y": round(y0 * SCALE) + OFFSET[1], "w": size[0], "h": size[1]}
    if pivot:
        entry["pivot"] = to_rig(pivot)
    layers.append(entry)


def main(debug=None):
    OUT.mkdir(parents=True, exist_ok=True)
    rgb = np.asarray(Image.open(ROOT / "source.png").convert("RGB")).astype(np.float32)
    shape = rgb.shape[:2]
    yy, xx = np.mgrid[: shape[0], : shape[1]]

    # Foreground. The figure stands in a pale glow that runs from the paper's
    # colour up to white, so "not the background colour" would take the glow
    # too. Paper and glow are both light and warm; the figure's own lights are
    # cool (the blue) or far yellower (the shell).
    bg = np.median(np.concatenate([rgb[:40].reshape(-1, 3), rgb[:, :40].reshape(-1, 3), rgb[:, -40:].reshape(-1, 3)]), 0)
    paper = (rgb[..., 0] >= 246) & (rgb[..., 1] >= 238) & (rgb[..., 2] >= 220) & (rgb[..., 0] >= rgb[..., 2] - 1)
    lab, _ = ndi.label(paper)
    border = set(np.unique(np.concatenate([lab[0], lab[-1], lab[:, 0], lab[:, -1]]))) - {0}
    fg = ndi.binary_fill_holes(~np.isin(lab, list(border)))
    l2, n2 = ndi.label(fg)
    fg = l2 == (1 + np.argmax(ndi.sum(fg, l2, range(1, n2 + 1))))
    fg = ndi.binary_opening(fg, iterations=2)

    grow = lambda m, n=9: ndi.binary_dilation(m, iterations=n)
    shrink = lambda m, n=1: ndi.binary_erosion(m, iterations=n)
    # A hard edge: one pixel in where two parts meet, two at the silhouette (no haze outside the figure).
    outside = np.clip(ndi.gaussian_filter(shrink(fg, 2).astype(np.float32), 0.9) * 1.15, 0, 1)
    edge = lambda m: np.clip(ndi.gaussian_filter(shrink(m).astype(np.float32), 0.9) * 1.15, 0, 1) * outside
    soft = lambda m, s=1.0: np.clip(ndi.gaussian_filter(m.astype(np.float32), s) * 1.2, 0, 1)
    sm = lambda pts: smooth(pts) if len(pts) >= 4 else list(pts)

    lum = rgb @ np.array([0.299, 0.587, 0.114], np.float32)
    # The outline is the only warm thing on a blue figure (the shell aside).
    warm = (rgb[..., 0] - rgb[..., 2] > 22) & fg
    shell_zone = box_mask(shape, SHELL_BOX)
    # The outer half of the outline is blended into the glow. On a dark desktop
    # that reads as a pale rim, so the outline's own colour is carried out to
    # the silhouette (`src` is the drawing as drawn; `rgb` has that done).
    src = rgb
    rgb = rgb.copy()
    rim = fg & ~shrink(fg, 5)
    darkest = np.stack([ndi.minimum_filter(src[..., c], size=7) for c in range(3)], -1)
    rgb[rim] = darkest[rim]
    # Where two outlines meet at a sharp angle (head and flipper), a little of
    # the glow is caught between them. On paper nobody sees it; on a dark
    # desktop it is a bright dot. Ink it in.
    notch = np.zeros(shape, bool)
    for x, y in NOTCHES:
        notch |= (xx - x) ** 2 + (yy - y) ** 2 < 22 ** 2
    glow = fg & notch & (src[..., 0] >= 236) & (src[..., 1] >= 226) & (src[..., 2] >= 204) & (src[..., 0] >= src[..., 2])
    glow = ndi.binary_dilation(glow, iterations=1) & fg & notch
    darker = np.stack([ndi.minimum_filter(src[..., c], size=13) for c in range(3)], -1)
    rgb[glow] = darker[glow]

    patch = rgb[GRAIN_PATCH[1]:GRAIN_PATCH[3], GRAIN_PATCH[0]:GRAIN_PATCH[2]]
    grain = patch - ndi.gaussian_filter(patch, (2, 2, 0))
    ink = np.median(src[warm & ~shell_zone & ~shrink(fg, 9) & shrink(fg, 2) & (yy > 600)], 0)       # the brown of the outline
    # Where the light falls on the figure the outline is drawn almost white. On
    # paper it melts into the glow; on a dark desktop it is a white wire round
    # the figure. The outermost pixels are brought towards the outline's brown
    # there, by as much as they are paler than it.
    wire = fg & ~shrink(fg, 3)
    pale = np.clip((rgb @ np.array([0.299, 0.587, 0.114], np.float32) - float(ink @ np.array([0.299, 0.587, 0.114]))) / 45, 0, 1) * 0.75
    k = (pale * wire)[..., None]
    rgb = rgb * (1 - k) + ink * k

    def inked(pts, width=7, colour=None):
        """A stroke like the drawn ones: (colour, coverage)."""
        s = np.clip(ndi.gaussian_filter(line_mask(shape, sm(pts), width), 1.3) * 1.15, 0, 1)
        return (ink if colour is None else colour), s

    def over(base, colour, a):
        return base * (1 - a[..., None] * 0.94) + colour * a[..., None] * 0.94

    # ---------------------------------------------------------------- the three parts as drawn
    seam = sm(SEAM)
    above_seam = poly_mask(shape, seam + [(shape[1], 0), (0, 0)])
    arc = sm(CURL_ARC)
    curl_zone = poly_mask(shape, CURL_SIDE + arc)
    curl_seen = ndi.binary_opening(fg & curl_zone, iterations=2)
    head = fg & above_seam & ~curl_seen
    body_seen = fg & ~above_seam

    # ---------------------------------------------------------------- face parts
    neutral = (rgb[..., 2] - rgb[..., 0] < 12) & (lum < 175)

    def eye(box):
        m = ndi.binary_opening(neutral & box_mask(shape, box), iterations=2)
        l, n = ndi.label(m)
        m = l == (1 + np.argmax(ndi.sum(m, l, range(1, n + 1))))
        return grow(ndi.binary_fill_holes(m), 3)

    eye_l, eye_r = eye(EYE_BOX_L), eye(EYE_BOX_R)
    stroke = (ndi.gaussian_filter(lum, 6) - ndi.gaussian_filter(lum, 1.2)) > 7
    m = stroke & box_mask(shape, MOUTH_BOX)
    l, n = ndi.label(m)
    mouth = grow(l == (1 + np.argmax(ndi.sum(m, l, range(1, n + 1)))), 3)

    # ---------------------------------------------------------------- head
    head_rgb = rgb
    for hole in (eye_l, eye_r, mouth):
        head_rgb = diffuse_fill(head_rgb, grow(hole, 5), grain)
    # The hook: the curl's left line runs a little way into the head. Painted out.
    hook_path = sm(HOOK)
    hook = (line_mask(shape, hook_path, 13) > 0) & head & grow(warm, 2) & ~grow(curl_seen, 5)
    head_rgb = diffuse_fill(head_rgb, grow(hook, 3) & head, grain)
    # Its edge is hard where a line was drawn and soft where the head melts into
    # the body under the chin and into the curl on top.
    mid = (xx > SEAM_SOFT[0]) & (xx < SEAM_SOFT[1])
    seam_soft = soft((line_mask(shape, seam, 44) > 0) & mid, 6)
    arc_soft = soft(line_mask(shape, arc, 30) > 0, 4) * soft(grow(curl_seen, 12), 3)
    melt = np.clip(np.maximum(seam_soft, arc_soft), 0, 1)
    wide = np.clip(ndi.gaussian_filter(head.astype(np.float32), 3.2), 0, 1) * outside
    head_a = edge(head) * (1 - melt) + wide * melt

    # ---------------------------------------------------------------- body: torso, flippers, feet, and a top under the head
    dome_line = sm(DOME)
    dome = poly_mask(shape, dome_line + [p for p in seam if DOME[0][0] <= p[0] <= DOME[-1][0]][::-1]) & above_seam
    body = ndi.binary_fill_holes(body_seen | dome)
    # Just above the seam the body keeps what is drawn there (the shadow under
    # the chin), so the soft seam leaves the rest pose untouched; higher up it is painted.
    keep = dome & grow(body_seen, 14) & mid
    body_known = shrink(body_seen, 6) & (yy < 700) & (xx > 360) & (xx < 650) & ~warm
    painted = fill_from(rgb, dome & ~keep, body_known | keep, grain)
    feather = soft(dome & ~keep, 2.5)[..., None]
    body_rgb = rgb * (1 - feather) + painted * feather
    c, s = inked(dome_line, 7)
    s = s * dome * ~grow(body_seen, 3)
    body_rgb = over(body_rgb, c, s)
    body_a = edge(body)

    # ---------------------------------------------------------------- curl: as drawn, and a root behind the head
    # The root is the foot of the stem as the drawing has it (the hook is its
    # left side), so nothing is invented there but its right side.
    left = [(x - 3, y + 1) for x, y in hook_path]      # the line itself is the root's edge
    root = poly_mask(shape, left + ROOT_LOW + ROOT_LINE[:0:-1] + arc[::-1]) & fg & ~curl_seen
    curl = curl_seen | root
    curl_rgb = rgb
    c, s = inked(ROOT_LINE, 7)
    s = s * root * ~grow(curl_seen, 1)
    curl_rgb = over(curl_rgb, c, s)
    curl_a = np.maximum(edge(curl), s * outside)

    # ---------------------------------------------------------------- write
    layers = []
    save_layer("body", body_rgb, body_a, layers)
    save_layer("curl", curl_rgb, curl_a, layers, pivot=CURL_ROOT)
    save_layer("head", head_rgb, head_a, layers, pivot=NECK)
    save_layer("eye_l", rgb, soft(eye_l), layers)
    save_layer("eye_r", rgb, soft(eye_r), layers)
    save_layer("mouth", rgb, soft(mouth), layers)

    centre = lambda m: to_rig((float(np.where(m)[1].mean()), float(np.where(m)[0].mean())))
    ys, xs = np.where(fg)
    rig = {
        "id": "paopao",
        "kind": "blob",
        "size": [512, 512],
        "bbox": to_rig((float(xs.min()), float(ys.min()))) + to_rig((float(xs.max()), float(ys.max()))),
        "anchors": {
            "eye_l": centre(eye_l), "eye_r": centre(eye_r), "mouth": centre(mouth),
            "neck": to_rig(NECK),
            "curl_root": to_rig(CURL_ROOT),
        },
        "order": ["body", "curl", "head", "eye_l", "eye_r", "mouth"],
        "layers": layers,
    }
    (OUT / "rig.json").write_text(json.dumps(rig, indent=2), "utf-8")

    # Rest-pose check: the stack, in the engine's order, must rebuild the source.
    inner = shrink(fg, 8)   # the outermost pixels were recoloured on purpose (see `rim`)
    comp = np.broadcast_to(bg, shape + (3,)).astype(np.float32).copy()
    for col, a in [(body_rgb, body_a), (curl_rgb, curl_a), (head_rgb, head_a), (rgb, soft(eye_l)), (rgb, soft(eye_r)), (rgb, soft(mouth))]:
        comp = comp * (1 - a[..., None]) + col * a[..., None]
    err = np.abs(comp - src).max(-1)
    print(f"figure {xs.min()}..{xs.max()} x {ys.min()}..{ys.max()} source px  ->  bbox {[round(v, 1) for v in rig['bbox']]}  ink {ink.round().tolist()}")
    print(f"head {head.sum()}  body {body.sum()}  curl {curl.sum()}  eyes {eye_l.sum()}/{eye_r.sum()}  mouth {mouth.sum()}  hook {hook.sum()}")
    print(f"rest-pose error: mean {err[inner].mean():.2f}  p99 {np.percentile(err[inner], 99):.1f}  max {err[inner].max():.0f}")
    bad = inner & (err > 12)
    if bad.any():
        lab2, n = ndi.label(grow(bad, 3))
        for i in np.argsort(ndi.sum(bad, lab2, range(1, n + 1)))[::-1][:6]:
            by, bx = np.where((lab2 == i + 1) & bad)
            print(f"  differs: {len(bx)} px around ({int(bx.mean())}, {int(by.mean())})")
    if debug:
        # The partition over the drawing, for placing the landmarks.
        tint = src.copy()
        for m, colour in ((head, (255, 60, 60)), (body_seen, (60, 200, 60)), (curl_seen, (255, 220, 0)), (dome & ~body_seen, (0, 255, 255)), (root & ~curl_seen, (255, 0, 255)), (eye_l | eye_r | mouth | hook, (255, 255, 255))):
            edge_px = m & ~shrink(m, 1)
            tint[edge_px] = colour
        Image.fromarray(tint.astype(np.uint8)).save(Path(debug) / "dbg_parts.png")
        Image.fromarray(np.clip(err * 6, 0, 255).astype(np.uint8)).save(Path(debug) / "dbg_rest_error.png")


if __name__ == "__main__":
    main(sys.argv[1] if len(sys.argv) > 1 else None)
