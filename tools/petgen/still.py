"""One picture -> every action, without any AI: cut the character out, then
animate it by squashing, tilting and hopping, with small drawn effects
(thought dots, "!", sweat drop, Zzz, hearts, stars).

Used by `build_pack.py --image`. Requires Pillow and numpy.
"""

import math
from pathlib import Path

import numpy as np
from PIL import Image, ImageDraw, ImageOps


class CutoutError(Exception):
    pass


# ---------------------------------------------------------------- cut-out

def _dilate(m: np.ndarray, r: int = 1) -> np.ndarray:
    out = m.copy()
    for _ in range(r):
        d = out.copy()
        d[1:, :] |= out[:-1, :]
        d[:-1, :] |= out[1:, :]
        d[:, 1:] |= out[:, :-1]
        d[:, :-1] |= out[:, 1:]
        out = d
    return out


def _connected_to_border(cand: np.ndarray) -> np.ndarray:
    """Pixels of `cand` reachable from the image border through `cand`."""
    seed = np.zeros_like(cand)
    seed[0, :], seed[-1, :], seed[:, 0], seed[:, -1] = cand[0, :], cand[-1, :], cand[:, 0], cand[:, -1]
    for _ in range(4 * sum(cand.shape)):
        grown = _dilate(seed) & cand
        if (grown == seed).all():
            break
        seed = grown
    return seed


MODEL = Path(__file__).resolve().parents[2] / "app" / "public" / "models" / "u2netp.onnx"


def _segment(rgb: np.ndarray) -> np.ndarray:
    """Foreground alpha (0..1) from U-2-Net (u2netp, Apache-2.0) via onnxruntime."""
    try:
        import onnxruntime as ort
    except ImportError:
        raise CutoutError(
            "The background is busy. Install onnxruntime to cut it out automatically (pip install onnxruntime), "
            "or use the gallery's Make a pet from a picture button.\n"
            "背景比较复杂。安装 onnxruntime 后可以自动抠图（pip install onnxruntime），或者直接用图鉴里的「用图片做宠物」。"
        )
    h, w = rgb.shape[:2]
    small = np.asarray(Image.fromarray((rgb * 255).astype(np.uint8)).resize((320, 320), Image.LANCZOS)).astype(np.float32)
    x = small / max(small.max(), 1.0)
    x = (x - np.array([0.485, 0.456, 0.406])) / np.array([0.229, 0.224, 0.225])
    sess = ort.InferenceSession(str(MODEL), providers=["CPUExecutionProvider"])
    out = sess.run(None, {sess.get_inputs()[0].name: x.transpose(2, 0, 1)[None].astype(np.float32)})[0][0, 0]
    v = (out - out.min()) / max(float(out.max() - out.min()), 1e-6)
    a = np.clip((v - 0.15) / 0.5, 0, 1)
    a = a * a * (3 - 2 * a)
    return np.asarray(Image.fromarray((a * 255).astype(np.uint8)).resize((w, h), Image.LANCZOS)).astype(np.float32) / 255.0


def cutout(path: Path, max_side: int = 720) -> np.ndarray:
    """The character as float RGBA (0..1), trimmed to its bounding box.

    A PNG with transparency is used as is. A flat one-colour background
    (green screen, white, ...) is keyed out from the edges inwards, so the
    same colour inside the character is kept. Anything else goes through a
    small segmentation model (needs onnxruntime).
    """
    im = ImageOps.exif_transpose(Image.open(path)).convert("RGBA")
    im.thumbnail((max_side, max_side), Image.LANCZOS)
    a = np.asarray(im).astype(np.float32) / 255.0
    alpha = a[..., 3]
    if (alpha < 0.9).mean() > 0.02:
        rgba = a.copy()
        rgba[..., 3] = np.where(alpha < 0.04, 0, alpha)
    else:
        rgb = a[..., :3]
        b = 3
        border = np.concatenate([rgb[:b].reshape(-1, 3), rgb[-b:].reshape(-1, 3), rgb[:, :b].reshape(-1, 3), rgb[:, -b:].reshape(-1, 3)])
        bg = np.median(border, axis=0)
        spread = np.percentile(np.linalg.norm(border - bg, axis=1), 85)
        if spread > 0.12:
            # Busy background: the same local segmentation model as the app.
            rgba = np.dstack([rgb, _segment(rgb)])
            return _trim(rgba)
        if bg[1] - max(bg[0], bg[2]) > 0.3:
            # Green screen: key by "greenness" everywhere, so gaps between
            # arms, hair and body go too (same keyer as the video route).
            r, g, b_ = rgb[..., 0], rgb[..., 1], rgb[..., 2]
            green = g - np.maximum(r, b_)
            alpha = np.clip(1.0 - (green - 0.08) / 0.20, 0.0, 1.0)
            g2 = np.where(green > 0, np.maximum(r, b_) + np.clip(green, 0, 0.06), g)
            rgba = np.dstack([r, g2, b_, alpha])
            rgba[alpha < 0.08] = 0
            return _trim(rgba)
        d = np.linalg.norm(rgb - bg, axis=-1)
        lo, hi = 0.07, 0.22
        region = _connected_to_border(d < hi)
        near = _dilate(region, 2)
        alpha = np.ones_like(d)
        alpha[near] = np.clip((d[near] - lo) / (hi - lo), 0, 1)
        # Un-blend the background colour out of the soft edge (removes green / white fringes).
        safe = np.maximum(alpha, 1e-3)[..., None]
        fg = np.clip((rgb - (1 - alpha[..., None]) * bg) / safe, 0, 1)
        rgba = np.dstack([np.where(alpha[..., None] > 0, fg, 0), alpha])
    return _trim(rgba)


def _trim(rgba: np.ndarray) -> np.ndarray:
    m = rgba[..., 3] > 0.1
    rows, cols = np.where(m.any(axis=1))[0], np.where(m.any(axis=0))[0]
    if len(rows) < 8 or len(cols) < 8:
        raise CutoutError("Nothing left after removing the background. / 去掉背景后什么都没剩下。")
    rgba = rgba[rows[0]: rows[-1] + 1, cols[0]: cols[-1] + 1].copy()
    rgba[rgba[..., 3] < 0.04] = 0
    return rgba


# ---------------------------------------------------------------- poses

# action: (frames, fps, loop)
STILL_PLAN = {
    "idle": (8, 6, True),
    "thinking": (8, 5, True),
    "tool": (6, 9, True),
    "attention": (6, 8, True),
    "done": (10, 12, False),
    "error": (6, 5, True),
    "sleep": (4, 2.5, True),
    "wake": (4, 6, False),
    "squish": (4, 12, False),
    "petted": (6, 6, True),
    "annoyed": (4, 8, False),
    "walk": (6, 10, True),
    "fall": (2, 6, True),
    "dizzy": (6, 6, True),
    "cheer": (6, 8, True),
    "sulk": (4, 3, True),
}

TAU = 2 * math.pi


def _pose(action: str, i: int, n: int) -> dict:
    """sx, sy: scale; rot: degrees; dx, lift: fraction of sprite height; dark: 0..1; fx: overlay."""
    p = i / n
    s = math.sin(TAU * p)
    P = dict(sx=1.0, sy=1.0, rot=0.0, dx=0.0, lift=0.0, dark=0.0, fx=None, k=p)
    if action == "idle":
        P.update(sy=1 + 0.03 * s, sx=1 - 0.015 * s)
    elif action == "thinking":
        P.update(rot=6 * s, sy=1 + 0.015 * math.cos(TAU * p), fx="dots")
    elif action == "tool":
        h = abs(math.sin(TAU * p))
        P.update(lift=0.035 * h, sy=1 - 0.05 * (1 - h) + 0.03 * h, sx=1 + 0.03 * (1 - h), rot=2.5 * math.sin(2 * TAU * p), fx="spark")
    elif action == "attention":
        h = max(0.0, s)
        P.update(lift=0.10 * h, sy=1 + 0.05 * h - 0.06 * (h == 0), sx=1 - 0.03 * h + 0.04 * (h == 0), rot=5 * math.sin(2 * TAU * p), fx="bang")
    elif action == "done":
        keys = [(0, 0.92, 1.06, 0), (0, 0.84, 1.12, 0), (0.10, 1.12, 0.92, -4), (0.17, 1.06, 0.96, -2), (0.19, 1.0, 1.0, 0),
                (0.16, 1.02, 0.98, 2), (0.07, 1.06, 0.96, 3), (0, 0.86, 1.12, 0), (0, 1.04, 0.98, 0), (0, 1.0, 1.0, 0)]
        lift, sy, sx, rot = keys[min(i, len(keys) - 1)]
        P.update(lift=lift, sy=sy, sx=sx, rot=rot, fx="confetti" if 2 <= i <= 8 else None)
    elif action == "error":
        P.update(sy=0.93, sx=1.03, dx=0.012 * math.sin(2 * TAU * p), rot=-3, dark=0.12, fx="sweat")
    elif action == "sleep":
        P.update(rot=-9, sy=0.9 + 0.02 * s, sx=1.04, dark=0.06, fx="zzz")
    elif action == "wake":
        k = (i + 1) / n
        P.update(rot=-9 * (1 - k), sy=0.9 + 0.1 * k, sx=1.04 - 0.04 * k, dark=0.06 * (1 - k))
    elif action == "squish":
        sy, sx = [(0.68, 1.26), (0.84, 1.13), (1.07, 0.95), (1.0, 1.0)][i]
        P.update(sy=sy, sx=sx)
    elif action == "petted":
        P.update(rot=4 * s, sy=1.02 + 0.02 * math.cos(TAU * p), fx="hearts")
    elif action == "annoyed":
        P.update(dx=[0.03, -0.03, 0.02, 0][i], sy=0.97, fx="vein")
    elif action == "walk":
        P.update(lift=0.03 * abs(s), rot=5 * s)
    elif action == "fall":
        P.update(sy=1.08, sx=0.94, rot=3 if i else -3)
    elif action == "dizzy":
        P.update(rot=8 * s, dx=0.01 * math.cos(TAU * p), fx="stars")
    elif action == "cheer":
        h = abs(s)
        P.update(lift=0.08 * h, sy=1 + 0.04 * h - 0.04 * (1 - h), sx=1 - 0.02 * h + 0.03 * (1 - h), rot=4 * math.sin(2 * TAU * p), fx="confetti")
    elif action == "sulk":
        P.update(sy=0.95, rot=-4 + 1.5 * s, dark=0.08)
    return P


# ---------------------------------------------------------------- effects

def _star(d, x, y, r, fill, line, w):
    pts = []
    for j in range(10):
        a = -math.pi / 2 + j * math.pi / 5
        rr = r if j % 2 == 0 else r * 0.45
        pts.append((x + rr * math.cos(a), y + rr * math.sin(a)))
    d.polygon(pts, fill=fill, outline=line, width=w)


def _heart(d, x, y, r, fill, line, w):
    pts = []
    for j in range(40):
        t = TAU * j / 40
        hx = 16 * math.sin(t) ** 3
        hy = -(13 * math.cos(t) - 5 * math.cos(2 * t) - 2 * math.cos(3 * t) - math.cos(4 * t))
        pts.append((x + hx * r / 16, y + hy * r / 16))
    d.polygon(pts, fill=fill, outline=line, width=w)


def _fx(d: ImageDraw.ImageDraw, kind: str, P: dict, head: tuple, M: int):
    hx, top, hw = head  # head centre x, top y, sprite width
    u = M / 128  # one "frame pixel"
    ink = (40, 28, 24, 255)
    w = max(2, round(1.6 * u))
    k = P["k"]
    rx = hx + hw * 0.42
    if kind == "dots":
        n = int(k * 8) // 2 % 4
        for j in range(n):
            cx, cy = rx + j * 7 * u, top + 6 * u - j * 3 * u
            d.ellipse((cx - 3 * u, cy - 3 * u, cx + 3 * u, cy + 3 * u), fill=(255, 255, 255, 255), outline=ink, width=w)
    elif kind == "spark":
        for j, (ox, oy) in enumerate([(-0.55, 0.55), (0.55, 0.45)]):
            on = (int(k * 6) + j) % 2 == 0
            if on:
                _star(d, hx + ox * hw, top + oy * hw, 6 * u, (255, 214, 102, 255), ink, w)
    elif kind == "bang":
        sc = 1 + 0.15 * math.sin(TAU * k * 2)
        x, y = rx, top + 2 * u
        bw, bh = 5 * u * sc, 14 * u * sc
        d.rounded_rectangle((x - bw, y - bh, x + bw, y + bh * 0.35), radius=bw, fill=(255, 196, 48, 255), outline=ink, width=w)
        d.ellipse((x - bw, y + bh * 0.55, x + bw, y + bh * 0.55 + 2 * bw), fill=(255, 196, 48, 255), outline=ink, width=w)
    elif kind == "sweat":
        y = top + 8 * u + 6 * u * k
        x = rx - 2 * u
        r = 4.5 * u
        d.polygon([(x, y - 2.2 * r), (x - r * 0.9, y - 0.3 * r), (x + r * 0.9, y - 0.3 * r)], fill=(130, 200, 255, 255))
        d.ellipse((x - r, y - r, x + r, y + r), fill=(130, 200, 255, 255), outline=(40, 90, 150, 255), width=max(1, w - 1))
    elif kind == "zzz":
        n = int(k * 4) % 4 + 1
        for j in range(n):
            s = (5 + j * 2.5) * u
            x, y = rx + j * 8 * u, top + 4 * u - j * 10 * u
            pts = [(x - s / 2, y - s / 2), (x + s / 2, y - s / 2), (x - s / 2, y + s / 2), (x + s / 2, y + s / 2)]
            d.line(pts, fill=(255, 255, 255, 255), width=w + 2 * max(1, w // 2))
            d.line(pts, fill=(90, 110, 200, 255), width=w)
    elif kind == "hearts":
        for j in range(2):
            q = (k + j * 0.5) % 1
            x = hx + (-0.35 + 0.7 * j) * hw + 3 * u * math.sin(TAU * q)
            y = top + 10 * u - 22 * u * q
            if q < 0.85:
                _heart(d, x, y, 6 * u, (255, 105, 140, 255), ink, w)
    elif kind == "vein":
        x, y, r = rx, top + 6 * u, 5 * u
        for a in range(4):
            ang = a * math.pi / 2 + math.pi / 4
            cx, cy = x + r * math.cos(ang), y + r * math.sin(ang)
            d.arc((cx - r, cy - r, cx + r, cy + r), math.degrees(ang) + 100, math.degrees(ang) + 260, fill=(230, 50, 50, 255), width=w + 1)
    elif kind == "stars":
        for j in range(3):
            a = TAU * (k + j / 3)
            _star(d, hx + 0.32 * hw * math.cos(a), top - 4 * u + 4 * u * math.sin(a), 5 * u, (255, 220, 90, 255), ink, w)
    elif kind == "confetti":
        cols = [(255, 99, 132), (255, 205, 86), (75, 192, 192), (153, 102, 255), (255, 159, 64)]
        rng = np.random.default_rng(int(k * 1000) + 7)
        for j in range(9):
            x = hx + (0.35 + 0.45 * rng.random()) * hw * (1 if j % 2 else -1)
            y = top - 6 * u + rng.random() * hw * 0.7
            c = cols[j % len(cols)]
            if j % 3 == 0:
                _star(d, x, y, 4 * u, (*c, 255), ink, max(1, w - 1))
            else:
                d.rectangle((x - 2 * u, y - 1.2 * u, x + 2 * u, y + 1.2 * u), fill=(*c, 255))


# ---------------------------------------------------------------- render

def render(rgba: np.ndarray, size: int) -> tuple[dict, dict]:
    """Frames for every action in STILL_PLAN. Returns (strips, meta) where
    strips[action] is a list of RGBA images of size x size and meta[action]
    is (fps, loop)."""
    M = size * 4
    src = Image.fromarray((np.clip(rgba, 0, 1) * 255).astype(np.uint8), "RGBA")
    # Fit: leave headroom for jumps and effects.
    k = min(0.76 * M / src.height, 0.80 * M / src.width)
    sw, sh = max(1, round(src.width * k)), max(1, round(src.height * k))
    spr = src.resize((sw, sh), Image.LANCZOS).convert("RGBa")  # premultiplied
    px, py = M / 2, M - 0.02 * M  # feet
    strips, meta = {}, {}
    for action, (n, fps, loop) in STILL_PLAN.items():
        imgs = []
        for i in range(n):
            P = _pose(action, i, n)
            th = math.radians(P["rot"])
            c, s = math.cos(th), math.sin(th)
            tx, ty = px + P["dx"] * sh, py - P["lift"] * sh
            # Inverse affine: canvas -> sprite, pivot at the sprite's bottom centre.
            a_, b_ = c / P["sx"], s / P["sx"]
            d_, e_ = -s / P["sy"], c / P["sy"]
            cx0, cy0 = sw / 2, sh
            data = (a_, b_, cx0 - a_ * tx - b_ * ty, d_, e_, cy0 - d_ * tx - e_ * ty)
            fr = spr.transform((M, M), Image.AFFINE, data, resample=Image.BICUBIC, fillcolor=(0, 0, 0, 0))
            if P["dark"]:
                arr = np.asarray(fr).astype(np.float32)
                arr[..., :3] *= 1 - P["dark"]
                fr = Image.fromarray(arr.astype(np.uint8), "RGBa")
            fr = fr.convert("RGBA")
            if P["fx"]:
                layer = Image.new("RGBA", (M, M), (0, 0, 0, 0))
                head = (tx, ty - sh * P["sy"], sw * P["sx"])
                _fx(ImageDraw.Draw(layer), P["fx"], P, head, M)
                fr.alpha_composite(layer)
            imgs.append(fr.convert("RGBa").resize((size, size), Image.LANCZOS).convert("RGBA"))
        strips[action] = imgs
        meta[action] = (fps, loop)
    return strips, meta
