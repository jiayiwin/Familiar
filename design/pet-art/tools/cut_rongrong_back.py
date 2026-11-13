"""Cut 茸茸's back view (the right-most figure of turnaround.png) into rig layers.

The sheet draws the figure smaller and narrower than the hero illustration, so
the crop is resized to the hero's height and (nearly) its width, and stood with
the middle of its legs on the middle of the hero's legs. Layers: body (with
shoulders of wool continued under the head, and wool painted through under the
tail), tail (the knob), head (the wool with both ears).

Here the outline does close the parts: the head is drawn in front of the body
and the tail knob in front of the rump, each with its own line.

    python design/pet-art/tools/cut_rongrong_back.py      (after cut_rongrong.py)
"""
import json
from pathlib import Path

import numpy as np
from PIL import Image
from scipy import ndimage as ndi

from cut_rongrong import scallop, wool_fill
from cut_zimeng_back import line_mask, poly_mask

ROOT = Path(__file__).resolve().parents[1] / "rongrong"
OUT = ROOT / "rig"
CROP = (1090, 225, 1536, 835)      # the back figure on the sheet
WORK = (758, 851)                  # crop resized: x1.70 wide, x1.395 tall
SCALE = 0.5                        # work space -> rig space
FRONT_FOOT_X = 272                 # the middle of the hero's legs, rig px

# Work-space landmarks, read off the drawing.
SEED_HEAD, SEED_BODY, SEED_TAIL = (378, 250), (378, 560), (378, 670)
SEED_EARS = [(90, 350), (680, 350)]
SEED_LEGS = [(290, 780), (470, 780)]
# Just under the head's lower outline: what lies above is the head's.
UNDER_HEAD = [(150, 430), (180, 454), (235, 460), (250, 472), (280, 484), (310, 478), (328, 470), (350, 480), (380, 486), (410, 480),
              (425, 468), (450, 478), (480, 478), (498, 468), (520, 473), (550, 463), (560, 453), (600, 454), (640, 430)]
NECK = (378, 468)                  # the point the head tilts about
# The body's shoulders under the head (invented): its outline from one side to the other.
DOME = [(182, 452), (176, 425), (190, 400), (225, 385), (300, 376), (378, 373), (456, 376), (531, 385), (566, 400), (584, 425), (598, 450)]
DOME_BUMP = (46, 7)
TAIL_PIVOT = (378, 672)            # the middle of the knob: it is sewn on there
GRAIN_PATCH = (300, 500, 440, 580)


def main():
    front = json.loads((OUT / "rig.json").read_text("utf-8"))

    sheet = Image.open(ROOT / "turnaround.png").convert("RGB")
    src = np.asarray(sheet.crop(CROP).resize(WORK, Image.LANCZOS)).astype(np.float32)
    shape = src.shape[:2]
    yy, xx = np.mgrid[: shape[0], : shape[1]]
    patch = src[GRAIN_PATCH[1]:GRAIN_PATCH[3], GRAIN_PATCH[0]:GRAIN_PATCH[2]]
    grain = patch - ndi.gaussian_filter(patch, (2, 2, 0))

    bg = np.median(np.concatenate([src[:20].reshape(-1, 3), src[-20:].reshape(-1, 3)]), 0)
    lab, _ = ndi.label(np.sqrt(((src - bg) ** 2).sum(-1)) < 14)
    border = set(np.unique(np.concatenate([lab[0], lab[-1], lab[:, 0], lab[:, -1]]))) - {0}
    fg = ndi.binary_fill_holes(~np.isin(lab, list(border)))
    l2, n2 = ndi.label(fg)
    fg = l2 == (1 + np.argmax(ndi.sum(fg, l2, range(1, n2 + 1))))      # the neighbour's tail pokes into the crop: not it

    # The paper's grain leaves the mask's edge ragged, which shows as bristles on a dark desktop: round it off.
    fg = ndi.gaussian_filter(fg.astype(np.float32), 1.6) > 0.5

    grow = lambda m, n=9: ndi.binary_dilation(m, iterations=n)
    shrink = lambda m, n=1: ndi.binary_erosion(m, iterations=n)
    outside = np.clip(ndi.gaussian_filter(shrink(fg, 2).astype(np.float32), 0.9) * 1.15, 0, 1)
    edge = lambda m: np.clip(ndi.gaussian_filter(shrink(m).astype(np.float32), 0.9) * 1.15, 0, 1) * outside
    dist = lambda m: ndi.distance_transform_edt(~m)

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

    tail_in = at(SEED_TAIL)
    ears_in = at(SEED_EARS[0]) | at(SEED_EARS[1])
    # The wool is drawn in closed cells: those above the head's lower outline are the head's.
    above = poly_mask(shape, UNDER_HEAD + [(shape[1] - 1, UNDER_HEAD[-1][1]), (shape[1] - 1, 0), (0, 0), (0, UNDER_HEAD[0][1])])
    cells = np.arange(1, reg.max() + 1)
    area = ndi.sum(np.ones(shape), reg, cells)
    up = ndi.sum(above, reg, cells) / np.maximum(area, 1)
    big = (area > 300) & ~np.isin(cells, np.unique(reg[tail_in | ears_in]))
    head_in = np.isin(reg, cells[big & (up > 0.5)]) | ears_in
    body_in = np.isin(reg, cells[big & (up <= 0.5)])
    assert head_in[SEED_HEAD[1], SEED_HEAD[0]] and body_in[SEED_BODY[1], SEED_BODY[0]], "the outline does not part head and body"
    for p in SEED_LEGS:
        assert body_in[p[1], p[0]], f"{p} is not part of the body"
    assert not tail_in[SEED_BODY[1], SEED_BODY[0]], "the tail is not closed off from the body"

    # Lines and flecks go to the nearest part; a line two parts share goes to the one in front.
    sets = [head_in, body_in, tail_in]
    seeds = np.zeros(shape, np.uint8)
    for i, m in enumerate(sets):
        seeds[m] = i + 1
    near = ndi.distance_transform_edt(seeds == 0, return_distances=False, return_indices=True)
    owner = seeds[near[0], near[1]]
    d_head, d_body, d_tail = (dist(m) for m in sets)
    lineish = seeds == 0
    R = 9
    owner[lineish & (d_head < R) & (d_body < R)] = 1
    owner[lineish & (d_tail < R) & (d_body < R)] = 3
    H, B, T = ((owner == i + 1) & fg for i in range(3))

    ink = np.median(src[line & grow(head_in, 8) & ~shrink(fg, 12) & (yy < 300)], 0)

    # ---------------------------------------------------------------- body: shoulders under the head, wool under the tail
    dome_line = scallop(DOME, *DOME_BUMP)
    dome = poly_mask(shape, dome_line + [(DOME[-1][0], DOME[-1][1] + 30), (DOME[0][0], DOME[0][1] + 30)])
    body = ndi.binary_fill_holes(B | T | (dome & (H | B)) | (grow(B, R + 2) & H & (d_body < R + 2)))
    body_known = shrink(body_in, 6) & ~grow(T, 8)
    body_rgb = wool_fill(rgb, body & ~B, body_known, grain, 20)
    s = np.clip(ndi.gaussian_filter(line_mask(shape, dome_line, 7), 1.3) * 1.15, 0, 1) * ~grow(B, 2) * H
    body_rgb = body_rgb * (1 - s[..., None] * 0.94) + ink * s[..., None] * 0.94
    body_a = np.maximum(edge(body), s * outside)

    head_a = edge(H)
    tail_a = edge(T)

    # ---------------------------------------------------------------- write, standing where the front stands
    ys, xs = np.where(fg)
    legs = body_in & (yy > ys.max() - 40)
    foot_work = (np.where(legs)[1].mean(), ys.max())
    offset = (round(FRONT_FOOT_X - foot_work[0] * SCALE), round(front["bbox"][3] - foot_work[1] * SCALE))
    layers = []

    def save_layer(name, col, alpha, pivot=None):
        ly, lx = np.where(alpha > 0.004)
        ya, yb = max(ly.min() - 4, 0), min(ly.max() + 5, shape[0])
        xa, xb = max(lx.min() - 4, 0), min(lx.max() + 5, shape[1])
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
    save_layer("back_tail", rgb, tail_a, pivot=TAIL_PIVOT)
    save_layer("back_head", rgb, head_a, pivot=NECK)
    (OUT / "back.json").write_text(json.dumps({"layers": layers}, indent=2), "utf-8")

    # Rest-pose check against the drawing.
    comp = np.broadcast_to(bg, shape + (3,)).astype(np.float32).copy()
    for col, a in [(body_rgb, body_a), (rgb, tail_a), (rgb, head_a)]:
        comp = comp * (1 - a[..., None]) + col * a[..., None]
    inner = shrink(fg, 8)
    err = np.abs(comp - src).max(-1)[inner]
    print(f"head {H.sum()}  body {body.sum()}  tail {T.sum()}  offset {offset}  figure in rig space: x {xs.min() * SCALE + offset[0]:.0f}..{xs.max() * SCALE + offset[0]:.0f}  y {ys.min() * SCALE + offset[1]:.0f}..{ys.max() * SCALE + offset[1]:.0f}")
    print(f"rest-pose error inside figure: mean {err.mean():.2f}  p99 {np.percentile(err, 99):.1f}  max {err.max():.0f}")


if __name__ == "__main__":
    main()
