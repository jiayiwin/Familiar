"""Lift 紫梦's painted face parts off the expression drawings: closed eyes and
smile (closed_smile.png / closed_sleep.png), worried brows and the surprised
mouth (expressions.png).

Each stroke is unmixed from the fur behind it (ink colour + coverage), so it
sits on the rig's own body layer. Parts are placed by the rig's face anchors,
not by image position: the two drawings are redraws of the hero, not overlays.

    python design/pet-art/tools/cut_zimeng_faces.py
"""
import json
from pathlib import Path

import numpy as np
from PIL import Image
from scipy import ndimage as ndi

ROOT = Path(__file__).resolve().parents[1] / "zimeng"
OUT = ROOT / "rig"
OVERSAMPLE = 4

# name: (source, box x0 y0 x1 y1, anchor it sits on, offset from the anchor in rig px[, source px -> rig px])
# The scale defaults to matching the whole figure's width; the expression sheet
# shows heads only, so its parts are scaled by the distance between the eyes.
SHEET = 0.88
PARTS = {
    "smile_l": ("closed_smile.png", (222, 466, 304, 514), "eye_l", (0, 1)),
    "smile_r": ("closed_smile.png", (416, 480, 502, 534), "eye_r", (0, 1)),
    "smile_mouth": ("closed_smile.png", (306, 526, 398, 568), "mouth", (0, 2)),
    "lid_l": ("closed_sleep.png", (178, 496, 260, 538), "eye_l", (0, 8)),
    "lid_r": ("closed_sleep.png", (366, 510, 454, 558), "eye_r", (0, 8)),
    "brow_l": ("expressions.png", (1164, 266, 1210, 287), "eye_l", (3.5, -27), SHEET),
    "brow_r": ("expressions.png", (1286, 266, 1334, 290), "eye_r", (-2.5, -29.5), SHEET),
    "mouth_o": ("expressions.png", (734, 326, 772, 364), "mouth", (0, 3), SHEET),
}


def figure_width(rgb):
    bg = np.median(rgb[:30].reshape(-1, 3), 0)
    cols = np.where((np.sqrt(((rgb - bg) ** 2).sum(-1)) > 14).sum(0) > 6)[0]
    return cols.max() - cols.min()


def main():
    rig = json.loads((OUT / "rig.json").read_text("utf-8"))
    hero_w = rig["bbox"][2] - rig["bbox"][0]
    layers = []
    cache = {}
    for name, (src, box, anchor, off, *scale) in PARTS.items():
        if src not in cache:
            rgb = np.asarray(Image.open(ROOT / src).convert("RGB")).astype(np.float32)
            cache[src] = (rgb, scale[0] if scale else hero_w / figure_width(rgb))   # source px -> rig px
        rgb, k = cache[src]
        x0, y0, x1, y1 = box
        win = rgb[y0:y1, x0:x1]
        lum = win @ np.array([0.299, 0.587, 0.114], np.float32)
        # Fur behind the stroke: the window with the dark stroke masked out, blurred.
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
        a = a[ys.min() - 2: ys.max() + 3, xs.min() - 2: xs.max() + 3]
        size = (max(2, round(a.shape[1] * k * OVERSAMPLE)), max(2, round(a.shape[0] * k * OVERSAMPLE)))
        al = np.asarray(Image.fromarray((a * 255).astype(np.float32), "F").resize(size, Image.LANCZOS)).clip(0, 255)
        out = np.concatenate([np.broadcast_to(ink, al.shape + (3,)), al[..., None]], -1).round().astype(np.uint8)
        Image.fromarray(out, "RGBA").save(OUT / f"{name}.png")
        w, h = size[0] / OVERSAMPLE, size[1] / OVERSAMPLE
        cx, cy = rig["anchors"][anchor]
        layers.append({"name": name, "file": f"{name}.png", "x": round(cx + off[0] - w / 2, 2), "y": round(cy + off[1] - h / 2, 2), "w": round(w, 2), "h": round(h, 2)})
        print(f"{name}: {size[0]}x{size[1]} px, ink {ink.round().tolist()}, scale {k:.3f}")
    (OUT / "faces.json").write_text(json.dumps({"layers": layers}, indent=2), "utf-8")


if __name__ == "__main__":
    main()
