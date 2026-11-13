"""Lift 茸茸's painted face parts off its drawings: closed eyes (closed_smile.png),
shut lids (closed_sleep.png), the smile and the surprised mouth (expressions.png,
figures 1 and 2), and the brows of its own rest face (source.png).

The brows lie across the top of the eyes in the same ink. They are separated
from them the way cut_rongrong.py does it (the eye is the round part of the
dark blob, the brow the thin stroke left over) and kept as drawn, in place.

Each other stroke is unmixed from the wool behind it (ink colour + coverage),
so it sits on the rig's own head layer, and is placed by the rig's face
anchors, not by image position. The sheets show the head turned the same way
as the hero pose, so the far side needs no extra shrinking.

    python design/pet-art/tools/cut_rongrong_faces.py      (after cut_rongrong.py)
"""
import json
from pathlib import Path

import numpy as np
from PIL import Image
from scipy import ndimage as ndi

import cut_rongrong as front

ROOT = Path(__file__).resolve().parents[1] / "rongrong"
OUT = ROOT / "rig"
OVERSAMPLE = 4

# Source px -> rig px, from the distance between the eyes (hero: 94.7 rig px).
CLOSED = 0.50      # closed_smile.png / closed_sleep.png: eyes 189 px apart
SHEET = 0.78       # expressions.png: eyes 121 px apart

# name: (source, box x0 y0 x1 y1, anchor it sits on, offset from the anchor in rig px, source px -> rig px)
# The closed eyes sit lower than the open eye's centre, as on the sheets (measured from the nose).
PARTS = {
    "smile_l": ("closed_smile.png", (214, 476, 292, 518), "eye_l", (0, 6.5), CLOSED),
    "smile_r": ("closed_smile.png", (396, 495, 484, 542), "eye_r", (0, 8), CLOSED),
    "smile_mouth": ("expressions.png", (206, 306, 264, 340), "mouth", (2, 1.5), SHEET),
    "lid_l": ("closed_sleep.png", (174, 488, 253, 526), "eye_l", (0, 9.5), CLOSED),
    "lid_r": ("closed_sleep.png", (356, 504, 448, 548), "eye_r", (0, 10), CLOSED),
    "mouth_o": ("expressions.png", (708, 312, 744, 347), "mouth", (1, 4.5), SHEET * 0.85),
}


def main():
    rig = json.loads((OUT / "rig.json").read_text("utf-8"))
    layers = []
    cache = {}
    for name, (src, box, anchor, off, k) in PARTS.items():
        if src not in cache:
            cache[src] = np.asarray(Image.open(ROOT / src).convert("RGB")).astype(np.float32)
        rgb = cache[src]
        x0, y0, x1, y1 = box
        win = rgb[y0:y1, x0:x1]
        lum = win @ np.array([0.299, 0.587, 0.114], np.float32)
        # Wool behind the stroke: the window with the dark stroke masked out, blurred.
        rough = (ndi.gaussian_filter(lum, 8) - lum) > 9
        keep = (~ndi.binary_dilation(rough, iterations=4)).astype(np.float32)
        fur = ndi.gaussian_filter(lum * keep, 7) / np.maximum(ndi.gaussian_filter(keep, 7), 1e-3)
        core = rough & (lum < np.percentile(lum[rough], 35))
        ink = np.median(win[core], 0)
        ink_lum = float(ink @ np.array([0.299, 0.587, 0.114]))
        a = np.clip((fur - lum) / np.maximum(fur - ink_lum, 1), 0, 1)
        a[a < 0.08] = 0
        # Keep only the stroke itself (largest blob), not specks of paper grain.
        lab, n = ndi.label(a > 0.25)
        big = lab == (1 + np.argmax(ndi.sum(a, lab, range(1, n + 1))))
        a *= ndi.binary_dilation(big, iterations=3)
        ys, xs = np.where(a > 0)
        a = a[max(ys.min() - 2, 0): ys.max() + 3, max(xs.min() - 2, 0): xs.max() + 3]
        size = (max(2, round(a.shape[1] * k * OVERSAMPLE)), max(2, round(a.shape[0] * k * OVERSAMPLE)))
        al = np.asarray(Image.fromarray((a * 255).astype(np.float32), "F").resize(size, Image.LANCZOS)).clip(0, 255)
        out = np.concatenate([np.broadcast_to(ink, al.shape + (3,)), al[..., None]], -1).round().astype(np.uint8)
        Image.fromarray(out, "RGBA").save(OUT / f"{name}.png")
        w, h = size[0] / OVERSAMPLE, size[1] / OVERSAMPLE
        cx, cy = rig["anchors"][anchor]
        layers.append({"name": name, "file": f"{name}.png", "x": round(cx + off[0] - w / 2, 2), "y": round(cy + off[1] - h / 2, 2), "w": round(w, 2), "h": round(h, 2)})
        print(f"{name}: {size[0]}x{size[1]} px, ink {ink.round().tolist()}, scale {k:.3f}")

    # The brows, lifted from the hero drawing where they stand (work space is twice rig space).
    work = front.work_image()
    lum = work @ np.array([0.299, 0.587, 0.114], np.float32)
    for name, box in (("brow_l", front.EYE_BOX_L), ("brow_r", front.EYE_BOX_R)):
        _, brow = front.eye_and_brow(lum, box)
        a = np.clip(ndi.gaussian_filter(brow.astype(np.float32), 1.0) * 1.2, 0, 1)
        ys, xs = np.where(a > 0.004)
        x0, y0, x1, y1 = xs.min() - 2, ys.min() - 2, xs.max() + 3, ys.max() + 3
        x0 -= x0 % 2; y0 -= y0 % 2; x1 += x1 % 2; y1 += y1 % 2
        out = np.concatenate([work[y0:y1, x0:x1], a[y0:y1, x0:x1, None] * 255], -1).round().clip(0, 255).astype(np.uint8)
        Image.fromarray(out, "RGBA").save(OUT / f"{name}.png")
        layers.append({"name": name, "file": f"{name}.png", "x": x0 * front.SCALE, "y": y0 * front.SCALE, "w": (x1 - x0) * front.SCALE, "h": (y1 - y0) * front.SCALE})
        print(f"{name}: lifted, {x1 - x0}x{y1 - y0} px")
    (OUT / "faces.json").write_text(json.dumps({"layers": layers}, indent=2), "utf-8")


if __name__ == "__main__":
    main()
