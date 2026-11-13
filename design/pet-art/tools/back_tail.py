"""A small tail for the back view of a pet whose drawing has none.

The back sheets of 菇菇 and 泡泡 show no tail, but the back view needs one:
it is what lifts in a kegel set and wags when the pet turns round in a dance.
This paints a small round nub in the body's own colours (sampled from
back_body.png), with the body's outline colour round it, and adds it to
back.json as `back_tail` with a pivot at its root.

    python design/pet-art/tools/back_tail.py gugu
    python design/pet-art/tools/back_tail.py paopao

The cut_<pet>_back.py scripts call `add()` when they finish, so a re-cut keeps
its tail.
"""
import json
import sys
from pathlib import Path

import numpy as np
from PIL import Image, ImageDraw, ImageFilter

ART = Path(__file__).resolve().parents[1]
# Where on the body the tail sits (fractions of the back_body box) and how big it is (rig px).
SHAPES = {
    "gugu": {"at": (0.5, 0.74), "size": (46, 42)},
    "paopao": {"at": (0.5, 0.76), "size": (40, 36)},
}
SS = 4  # supersampling


def add(pet: str) -> None:
    rig = ART / pet / "rig"
    back = json.loads((rig / "back.json").read_text(encoding="utf-8"))
    body = next(l for l in back["layers"] if l["name"] == "back_body")
    img = np.asarray(Image.open(rig / body["file"]).convert("RGBA")).astype(float)
    alpha = img[..., 3]
    h, w = alpha.shape
    shape = SHAPES[pet]
    fx, fy = shape["at"]
    # The body's colour, and its outline colour (the opaque pixels that touch the outside).
    # The body's own paler tone (the lighter half of its pixels): a small pale pom with a line
    # round it, as 麻薯's drawn tail is. A darker one reads as a grey ball stuck on.
    opaque = img[alpha > 250][:, :3]
    lum = opaque @ np.array([0.299, 0.587, 0.114])
    lo, hi = np.percentile(lum, [55, 88])
    base = np.median(opaque[(lum >= lo) & (lum <= hi)], axis=0)
    # Seen small and next to the body, a pom of exactly the body's colour looks greyer than the
    # body does: lift its colour a little further from grey.
    grey = base.mean()
    base = np.clip(grey + (base - grey) * 1.9, 0, 255)
    solid = alpha > 200
    outside = alpha < 40
    near_out = np.zeros_like(solid)
    for dy, dx in ((0, 1), (0, -1), (1, 0), (-1, 0), (2, 0), (-2, 0), (0, 2), (0, -2)):
        near_out |= np.roll(outside, (dy, dx), axis=(0, 1))
    edge = img[solid & near_out][:, :3]
    line = np.median(edge, axis=0) if len(edge) > 50 else base * 0.7

    tw, th = shape["size"]
    pad = 3
    W, H = (tw + 2 * pad) * SS, (th + 2 * pad) * SS
    yy, xx = np.mgrid[0:H, 0:W].astype(float)
    ex, ey = W / 2, H / 2
    rx, ry = tw * SS / 2, th * SS / 2
    d = np.sqrt(((xx - ex) / rx) ** 2 + ((yy - ey) / ry) ** 2)
    # Lit from the upper left, as the bodies are: paler toward the light, a touch deeper at the lower right.
    light = np.clip(1 - np.sqrt(((xx - (ex - rx * 0.3)) / rx) ** 2 + ((yy - (ey - ry * 0.35)) / ry) ** 2), 0, 1)
    col = base[None, None, :] * (0.93 + 0.0 * d[..., None]) + (255 - base[None, None, :]) * (0.42 * light[..., None] ** 1.3)
    col = col * (1 - 0.07 * np.clip(d[..., None] - 0.55, 0, 1) / 0.45)
    fill = np.dstack([np.clip(col, 0, 255), np.where(d <= 1, 255.0, 0.0)])
    tail = Image.fromarray(fill.astype(np.uint8), "RGBA")
    # The outline: one soft line in the body's own outline colour.
    ring = Image.new("L", (W, H), 0)
    ImageDraw.Draw(ring).ellipse([ex - rx, ey - ry, ex + rx, ey + ry], outline=255, width=int(1.7 * SS))
    ink = Image.new("RGBA", (W, H), tuple(int(v) for v in line) + (0,))
    ink.putalpha(ring.point(lambda v: int(v * 0.82)))
    tail = Image.alpha_composite(tail, ink)
    tail = tail.resize((tw + 2 * pad, th + 2 * pad), Image.LANCZOS).filter(ImageFilter.GaussianBlur(0.35))
    tail.save(rig / "back_tail.png")

    x = round(body["x"] + w * fx - (tw + 2 * pad) / 2)
    y = round(body["y"] + h * fy - (th + 2 * pad) / 2)
    layer = {"name": "back_tail", "file": "back_tail.png", "x": x, "y": y, "w": tw + 2 * pad, "h": th + 2 * pad,
             # It wags and lifts about its root, where it meets the body.
             "pivot": [x + (tw + 2 * pad) / 2, y + (th + 2 * pad) * 0.42]}
    back["layers"] = [l for l in back["layers"] if l["name"] != "back_tail"] + [layer]
    (rig / "back.json").write_text(json.dumps(back, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
    print(f"{pet}: back_tail {layer['w']}x{layer['h']} at ({x}, {y}), body colour {base.round().tolist()}, line {np.asarray(line).round().tolist()}")


if __name__ == "__main__":
    for name in sys.argv[1:] or list(SHAPES):
        add(name)
