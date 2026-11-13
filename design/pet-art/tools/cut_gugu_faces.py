"""Lift 菇菇's painted face parts off its expression drawings: closed eyes and
the smile (closed_smile.png), shut lids (closed_sleep.png) and the surprised
mouth (expressions.png, figure 3).

The worried brows are drawn here instead of lifted: on the sheet (figure 4) they
lie across the top of the eyes and cannot be separated from them. They follow
the sheet's brows in place, slant, length and colour.

Each stroke is unmixed from the skin behind it (ink colour + coverage), so it
sits on the rig's own head layer. Parts are placed by the rig's face anchors,
not by image position. The sheets show the head turned the same way as the hero
pose, so both sides keep the size they were drawn at.

    python design/pet-art/tools/cut_gugu_faces.py
"""
import json
from pathlib import Path

import numpy as np
from PIL import Image, ImageDraw
from scipy import ndimage as ndi

ROOT = Path(__file__).resolve().parents[1] / "gugu"
OUT = ROOT / "rig"
OVERSAMPLE = 4

# Source px -> rig px, from the distance between the eyes (hero: 114 rig px).
CLOSED = 0.56      # closed_smile.png / closed_sleep.png: eyes 204 px apart
SHEET = 0.83       # expressions.png, figure 3: eyes 138 px apart

# name: (source, box x0 y0 x1 y1, anchor it sits on, offset from the anchor in rig px, source px -> rig px)
PARTS = {
    "smile_l": ("closed_smile.png", (220, 560, 300, 604), "eye_l", (0, 2), CLOSED),
    "smile_r": ("closed_smile.png", (422, 584, 510, 632), "eye_r", (0, 2), CLOSED),
    "smile_mouth": ("closed_smile.png", (314, 616, 380, 652), "mouth", (0, 1), CLOSED),
    "lid_l": ("closed_sleep.png", (166, 592, 248, 630), "eye_l", (0, 7), CLOSED),
    "lid_r": ("closed_sleep.png", (364, 606, 455, 652), "eye_r", (0, 7), CLOSED),
    "mouth_o": ("expressions.png", (1208, 352, 1243, 386), "mouth", (0, 1), SHEET * 0.85),
}

# name: (anchor, outer end, inner end) as offsets from the eye's centre in rig px
# (read off expressions.png figure 4: the inner ends are the high ones).
BROWS = {
    "brow_l": ("eye_l", (-13.5, -19.5), (10.5, -31.0)),
    "brow_r": ("eye_r", (16.0, -18.0), (-14.0, -32.5)),
}
BROW_INK = (196, 163, 152)
BROW_WIDTH = 3.0


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
        a = a[max(ys.min() - 2, 0): ys.max() + 3, max(xs.min() - 2, 0): xs.max() + 3]
        size = (max(2, round(a.shape[1] * k * OVERSAMPLE)), max(2, round(a.shape[0] * k * OVERSAMPLE)))
        al = np.asarray(Image.fromarray((a * 255).astype(np.float32), "F").resize(size, Image.LANCZOS)).clip(0, 255)
        out = np.concatenate([np.broadcast_to(ink, al.shape + (3,)), al[..., None]], -1).round().astype(np.uint8)
        Image.fromarray(out, "RGBA").save(OUT / f"{name}.png")
        w, h = size[0] / OVERSAMPLE, size[1] / OVERSAMPLE
        cx, cy = rig["anchors"][anchor]
        layers.append({"name": name, "file": f"{name}.png", "x": round(cx + off[0] - w / 2, 2), "y": round(cy + off[1] - h / 2, 2), "w": round(w, 2), "h": round(h, 2)})
        print(f"{name}: {size[0]}x{size[1]} px, ink {ink.round().tolist()}, scale {k:.3f}")
    for name, (anchor, a, b) in BROWS.items():
        pad = 4
        x0, y0 = min(a[0], b[0]) - pad, min(a[1], b[1]) - pad
        w, h = abs(a[0] - b[0]) + 2 * pad, abs(a[1] - b[1]) + 2 * pad
        im = Image.new("L", (round(w * OVERSAMPLE), round(h * OVERSAMPLE)), 0)
        # A slight arch, bulging away from the eye.
        mid = ((a[0] + b[0]) / 2, (a[1] + b[1]) / 2 - 2.2)
        pts = [((1 - t) ** 2 * a[0] + 2 * t * (1 - t) * mid[0] + t * t * b[0], (1 - t) ** 2 * a[1] + 2 * t * (1 - t) * mid[1] + t * t * b[1]) for t in np.linspace(0, 1, 24)]
        d = ImageDraw.Draw(im)
        px = [((x - x0) * OVERSAMPLE, (y - y0) * OVERSAMPLE) for x, y in pts]
        d.line(px, fill=255, width=round(BROW_WIDTH * OVERSAMPLE), joint="curve")
        r = BROW_WIDTH * OVERSAMPLE / 2
        for x, y in (px[0], px[-1]):
            d.ellipse([x - r, y - r, x + r, y + r], fill=255)
        al = ndi.gaussian_filter(np.asarray(im, np.float32), 0.35 * OVERSAMPLE)
        out = np.concatenate([np.broadcast_to(np.array(BROW_INK, np.float32), al.shape + (3,)), al[..., None]], -1).round().astype(np.uint8)
        Image.fromarray(out, "RGBA").save(OUT / f"{name}.png")
        cx, cy = rig["anchors"][anchor]
        layers.append({"name": name, "file": f"{name}.png", "x": round(cx + x0, 2), "y": round(cy + y0, 2), "w": round(w, 2), "h": round(h, 2)})
        print(f"{name}: drawn, {im.size[0]}x{im.size[1]} px")
    (OUT / "faces.json").write_text(json.dumps({"layers": layers}, indent=2), "utf-8")


if __name__ == "__main__":
    main()
