"""Generate the built-in packs (pixel-cat, pixel-bot) and the app icons.

    python tools/gen-builtin-pets.py

Output: pets/<id>/ (pet.json, sprites, sounds, preview) and
app/src-tauri/icons/. Requires Pillow. Everything is drawn in code, so the
built-in art has no licensing questions.
"""

import json
import math
import struct
import wave
from pathlib import Path

from PIL import Image

ROOT = Path(__file__).resolve().parent.parent
W = H = 32  # frame size in pixels
SCALE = 3

OUTLINE = (74, 44, 32, 255)
FUR = (244, 166, 97, 255)
FUR_DARK = (214, 128, 66, 255)
BELLY = (255, 230, 196, 255)
EYE = (40, 30, 40, 255)
PINK = (240, 128, 150, 255)
WHITE = (255, 255, 255, 255)
RED = (230, 57, 70, 255)
BLUE = (110, 170, 240, 255)
GREY = (120, 130, 150, 255)
SCREEN = (140, 220, 255, 255)
YELLOW = (255, 214, 80, 255)


class Canvas:
    def __init__(self):
        self.px = [[None] * W for _ in range(H)]

    def set(self, x, y, c):
        if 0 <= x < W and 0 <= y < H:
            self.px[y][x] = c

    def get(self, x, y):
        return self.px[y][x] if 0 <= x < W and 0 <= y < H else None

    def ellipse(self, cx, cy, rx, ry, c):
        for y in range(H):
            for x in range(W):
                if ((x + 0.5 - cx) / rx) ** 2 + ((y + 0.5 - cy) / ry) ** 2 <= 1:
                    self.set(x, y, c)

    def tri(self, a, b, c, col):
        def s(p1, p2, p3):
            return (p1[0] - p3[0]) * (p2[1] - p3[1]) - (p2[0] - p3[0]) * (p1[1] - p3[1])

        for y in range(H):
            for x in range(W):
                p = (x + 0.5, y + 0.5)
                d1, d2, d3 = s(p, a, b), s(p, b, c), s(p, c, a)
                neg = d1 < 0 or d2 < 0 or d3 < 0
                pos = d1 > 0 or d2 > 0 or d3 > 0
                if not (neg and pos):
                    self.set(x, y, col)

    def rect(self, x0, y0, x1, y1, c):
        for y in range(y0, y1 + 1):
            for x in range(x0, x1 + 1):
                self.set(x, y, c)

    def outline(self):
        """Outline every filled region against transparency."""
        add = []
        for y in range(H):
            for x in range(W):
                if self.px[y][x] is None:
                    for dx, dy in ((1, 0), (-1, 0), (0, 1), (0, -1)):
                        n = self.get(x + dx, y + dy)
                        if n is not None and n != OUTLINE:
                            add.append((x, y))
                            break
        for x, y in add:
            self.set(x, y, OUTLINE)

    def image(self):
        img = Image.new("RGBA", (W, H), (0, 0, 0, 0))
        for y in range(H):
            for x in range(W):
                if self.px[y][x]:
                    img.putpixel((x, y), self.px[y][x])
        return img


def cat(dy=0, eyes="open", paws="down", tail=0, squash=0, lying=False, mouth=None):
    """Draw the cat. dy moves it up (negative) for jumps; squash flattens the body."""
    c = Canvas()
    base = 31 + dy  # feet line
    if lying:
        # Loaf pose: wide low body, head resting on the front.
        c.ellipse(16, base - 4.5, 11, 4.5, FUR)
        c.ellipse(16, base - 3.5, 7, 2.5, BELLY)
        tx = 27
        c.ellipse(tx + 1, base - 3, 2.5, 1.6, FUR_DARK)
        hy = base - 9
    else:
        by = base - 6 + squash * 0.5
        c.ellipse(16, by, 8.5, 6.5 - squash, FUR)
        c.ellipse(16, by + 1, 5, 4.2 - squash, BELLY)
        # Tail sways between three positions.
        t = [(25, by - 1), (26, by - 3), (25, by - 5)][tail % 3]
        c.ellipse(t[0], t[1], 2, 3.2, FUR_DARK)
        c.ellipse(t[0] - 1, t[1] + 2, 2, 1.5, FUR)
        hy = by - 9 + squash * 0.5
    # Head and ears.
    c.tri((8.5, hy - 2), (10, hy - 9.5), (14, hy - 4.5), FUR)
    c.tri((23.5, hy - 2), (22, hy - 9.5), (18, hy - 4.5), FUR)
    c.tri((10.2, hy - 4), (10.8, hy - 7.6), (12.6, hy - 4.8), PINK)
    c.tri((21.8, hy - 4), (21.2, hy - 7.6), (19.4, hy - 4.8), PINK)
    c.ellipse(16, hy, 8, 6.5, FUR)
    c.ellipse(16, hy + 2.5, 4, 2.5, BELLY)
    c.outline()

    ey = int(round(hy))
    if eyes == "open":
        for ex in (12, 19):
            c.rect(ex, ey - 1, ex + 1, ey, EYE)
            c.set(ex, ey - 1, WHITE)
    elif eyes == "up":
        for ex in (12, 19):
            c.rect(ex, ey - 2, ex + 1, ey - 1, EYE)
            c.set(ex + 1, ey - 2, WHITE)
    elif eyes == "down":
        for ex in (12, 19):
            c.rect(ex, ey, ex + 1, ey + 1, EYE)
    elif eyes == "closed":
        for ex in (12, 19):
            c.rect(ex, ey, ex + 1, ey, EYE)
    elif eyes == "happy":
        for ex in (12, 19):
            c.set(ex, ey, EYE)
            c.set(ex + 1, ey - 1, EYE)
            c.set(ex + 2, ey, EYE)
    elif eyes == "x":
        for ex in (12, 19):
            c.set(ex, ey - 1, EYE)
            c.set(ex + 1, ey, EYE)
            c.set(ex, ey + 1, EYE)
            c.set(ex + 2, ey - 1, EYE)
            c.set(ex + 2, ey + 1, EYE)
    elif eyes == "wide":
        for ex in (11, 19):
            c.rect(ex, ey - 2, ex + 2, ey, EYE)
            c.set(ex, ey - 2, WHITE)
    elif eyes == "sad":
        for ex in (12, 19):
            c.rect(ex, ey, ex + 1, ey, EYE)
        c.set(11, ey - 1, EYE)
        c.set(21, ey - 1, EYE)
    # Nose and mouth.
    c.set(15, ey + 2, PINK)
    c.set(16, ey + 2, PINK)
    if mouth == "open":
        c.rect(15, ey + 3, 16, ey + 4, (160, 50, 60, 255))
    elif mouth == "frown":
        c.set(14, ey + 4, EYE)
        c.set(15, ey + 3, EYE)
        c.set(16, ey + 3, EYE)
        c.set(17, ey + 4, EYE)
    else:
        c.set(14, ey + 3, EYE)
        c.set(17, ey + 3, EYE)

    if not lying:
        py = base - 1
        left = {"down": 0, "up1": -3, "up2": 0, "raised": -9, "chin": -8, "both": -9, "raisedR": 0,
                "walk1": -1, "walk2": 0, "rod": -6}[paws]
        right = {"down": 0, "up1": 0, "up2": -3, "raised": 0, "chin": 0, "both": -9, "raisedR": -9,
                 "walk1": 0, "walk2": -1, "rod": -6}[paws]
        lx = 6 if paws in ("raised", "both") else 11
        rx = 25 if paws in ("both", "raisedR") else 20
        for px, off in ((lx, left), (rx, right)):
            c.rect(px, py + off - 1, px + 1, py + off, FUR)
            c.set(px, py + off + 1, OUTLINE)
            c.set(px + 1, py + off + 1, OUTLINE)
            c.set(px - 1, py + off - 1, OUTLINE)
            c.set(px + 2, py + off - 1, OUTLINE)
            c.set(px - 1, py + off, OUTLINE)
            c.set(px + 2, py + off, OUTLINE)
    return c


def laptop(c, glow):
    c.rect(9, 26, 22, 27, GREY)
    c.rect(10, 20, 21, 25, OUTLINE)
    c.rect(11, 21, 20, 24, SCREEN if glow else BLUE)
    c.set(12, 22, WHITE)
    c.set(14, 22, WHITE)
    c.set(13, 23, WHITE)


def book(c):
    c.rect(9, 23, 15, 28, (90, 130, 200, 255))
    c.rect(16, 23, 22, 28, (90, 130, 200, 255))
    c.rect(10, 24, 14, 27, WHITE)
    c.rect(17, 24, 21, 27, WHITE)
    for y in (25, 26):
        c.rect(11, y, 13, y, GREY)
        c.rect(18, y, 20, y, GREY)


def exclaim(c, dy=0):
    c.rect(27, 2 + dy, 28, 6 + dy, RED)
    c.rect(27, 8 + dy, 28, 9 + dy, RED)


def dots(c, n):
    for i in range(n):
        c.rect(24 + i * 3, 4, 25 + i * 3, 5, OUTLINE)


def zzz(c, phase):
    y = 7 - phase * 2
    x = 24 + phase
    for dx in range(4):
        c.set(x + dx, y, BLUE)
        c.set(x + dx, y + 3, BLUE)
    c.set(x + 2, y + 1, BLUE)
    c.set(x + 1, y + 2, BLUE)


def sparkle(c, x, y):
    c.set(x, y, YELLOW)
    for dx, dy in ((1, 0), (-1, 0), (0, 1), (0, -1)):
        c.set(x + dx, y + dy, YELLOW)


def sweat(c, dy):
    c.set(24, 8 + dy, BLUE)
    c.rect(23, 9 + dy, 25, 10 + dy, BLUE)


def strip(frames):
    img = Image.new("RGBA", (W * len(frames), H), (0, 0, 0, 0))
    for i, f in enumerate(frames):
        img.paste(f.image(), (i * W, 0))
    return img


def build_frames():
    a = {}
    a["idle"] = [cat(tail=0), cat(tail=1, squash=0.5), cat(tail=2), cat(tail=1, eyes="closed")]
    th = []
    for i in range(6):
        f = cat(eyes="up", paws="chin", tail=i % 3)
        dots(f, i % 4)
        th.append(f)
    a["thinking"] = th
    tl = []
    for i in range(4):
        f = cat(eyes="down", paws="up1" if i % 2 == 0 else "up2", tail=i % 3)
        laptop(f, i % 2 == 0)
        tl.append(f)
    a["tool"] = tl
    rd = []
    for i in range(4):
        f = cat(eyes="down" if i != 3 else "closed", paws="down", tail=i % 3)
        book(f)
        rd.append(f)
    a["reading"] = rd
    at = []
    for i in range(2):
        f = cat(dy=-2 if i == 0 else 0, eyes="wide", paws="raised", mouth="open")
        exclaim(f, -1 if i == 0 else 0)
        at.append(f)
    a["attention"] = at
    dn = []
    for i, dy in enumerate([0, -3, -6, -7, -6, -3, 0, 0]):
        f = cat(dy=dy, eyes="happy", paws="raised" if dy < -2 else "down", mouth="open", squash=1.5 if i in (0, 7) else 0)
        if i in (2, 3, 4):
            sparkle(f, 4, 6 + i)
            sparkle(f, 27, 4 + i)
        dn.append(f)
    a["done"] = dn
    er = []
    for i in range(4):
        f = cat(eyes="sad" if i % 2 else "x", mouth="frown", squash=1)
        sweat(f, i % 2)
        er.append(f)
    a["error"] = er
    sl = []
    for i in range(4):
        f = cat(lying=True, eyes="closed", dy=0)
        zzz(f, i % 3)
        sl.append(f)
    a["sleep"] = sl
    a["wake"] = [cat(squash=2, eyes="closed", mouth="open"), cat(dy=-1, paws="raised", eyes="happy", mouth="open")]
    a["squish"] = [cat(squash=3, eyes="closed"), cat(dy=-1, squash=-1, eyes="happy"), cat(squash=1)]
    return a


def tone(path, notes, rate=22050, vol=0.35):
    """notes: list of (freq_hz, seconds)."""
    frames = bytearray()
    for freq, dur in notes:
        n = int(rate * dur)
        for i in range(n):
            env = min(1.0, i / (rate * 0.01)) * max(0.0, 1 - i / n) ** 1.5
            v = math.sin(2 * math.pi * freq * i / rate) * 0.8 + math.sin(4 * math.pi * freq * i / rate) * 0.2
            frames += struct.pack("<h", int(v * env * vol * 32767))
    with wave.open(str(path), "wb") as w:
        w.setnchannels(1)
        w.setsampwidth(2)
        w.setframerate(rate)
        w.writeframes(bytes(frames))




def add_cat_wave(a):
    wv = []
    for i in range(4):
        f = cat(dy=-1 if i % 2 == 0 else 0, eyes="happy", paws="raised", tail=i % 3, mouth="open" if i % 2 else None)
        # Motion marks beside the waving paw.
        if i % 2 == 0:
            f.set(4, 18, OUTLINE)
            f.set(4, 20, OUTLINE)
        else:
            f.set(9, 17, OUTLINE)
            f.set(9, 19, OUTLINE)
        wv.append(f)
    a["wave"] = wv


# ---------------------------------------------------------------- pixel-bot

METAL = (186, 200, 214, 255)
METAL_DARK = (128, 146, 166, 255)
TEAL = (52, 178, 168, 255)
FACE_BG = (28, 38, 54, 255)
FACE_FG = (120, 244, 224, 255)
FACE_RED = (255, 110, 110, 255)
TREAD = (70, 78, 92, 255)
BULB = (255, 196, 64, 255)


def bot(dy=0, face="open", arms="down", blink=False, squash=0, lying=False, fg=FACE_FG):
    """Draw the robot. dy moves it up (negative) for jumps."""
    c = Canvas()
    base = 31 + dy
    if lying:
        # Powered down: sunk onto its treads, head resting on the body.
        c.rect(8, base - 3, 23, base, TREAD)
        c.rect(9, base - 8, 22, base - 4, METAL)
        hy = base - 12
    else:
        c.rect(9, base - 3, 22, base, TREAD)
        sq = int(squash)
        c.rect(10, base - 11 + sq, 21, base - 4, METAL)
        c.rect(13, base - 9 + sq, 18, base - 6, TEAL)
        hy = base - 17 + sq
    # Head with cut corners.
    c.rect(8, hy - 5, 23, hy + 4, METAL)
    for x, y in ((8, hy - 5), (23, hy - 5), (8, hy + 4), (23, hy + 4)):
        c.set(x, y, None)
    # Antenna.
    c.rect(15, hy - 8, 16, hy - 6, METAL_DARK)
    c.rect(14, hy - 10, 17, hy - 9, BULB if not blink else FACE_RED)
    # Arms.
    if not lying:
        shoulder = base - 11 + int(squash)
        pos = {
            "down": ((6, shoulder + 1), (24, shoulder + 1)),
            "type1": ((7, shoulder + 3), (23, shoulder + 5)),
            "type2": ((7, shoulder + 5), (23, shoulder + 3)),
            "raised": ((5, shoulder - 6), (24, shoulder + 1)),
            "both": ((5, shoulder - 6), (25, shoulder - 6)),
            "wave1": ((6, shoulder + 1), (25, shoulder - 7)),
            "wave2": ((6, shoulder + 1), (23, shoulder - 6)),
            "chin": ((9, hy + 5), (24, shoulder + 1)),
        }[arms]
        for ax, ay in pos:
            c.rect(ax, ay, ax + 1, ay + 4, METAL_DARK)
    c.outline()
    # Wheels on the tread (after outline so they stay inside).
    for wx in (11, 15, 19):
        c.set(wx, base - 1, METAL_DARK)
        c.set(wx + 1, base - 1, METAL_DARK)
    # Face screen.
    c.rect(10, hy - 3, 21, hy + 2, FACE_BG)
    fy = hy - 1
    if face == "open":
        for ex in (12, 18):
            c.rect(ex, fy - 1, ex + 1, fy, fg)
    elif face == "up":
        for ex in (12, 18):
            c.rect(ex, fy - 2, ex + 1, fy - 1, fg)
    elif face == "down":
        for ex in (12, 18):
            c.rect(ex, fy, ex + 1, fy + 1, fg)
    elif face == "closed":
        for ex in (12, 18):
            c.rect(ex, fy, ex + 1, fy, fg)
    elif face == "happy":
        for ex in (12, 18):
            c.set(ex - 1, fy, fg)
            c.set(ex, fy - 1, fg)
            c.set(ex + 1, fy - 1, fg)
            c.set(ex + 2, fy, fg)
    elif face == "x":
        for ex in (12, 18):
            c.set(ex - 1, fy - 1, fg)
            c.set(ex + 1, fy - 1, fg)
            c.set(ex, fy, fg)
            c.set(ex - 1, fy + 1, fg)
            c.set(ex + 1, fy + 1, fg)
    elif face == "wide":
        for ex in (11, 18):
            c.rect(ex, fy - 2, ex + 2, fy, fg)
    elif face == "sad":
        for ex in (12, 18):
            c.rect(ex, fy, ex + 1, fy, fg)
        c.set(11, fy - 1, fg)
        c.set(20, fy - 1, fg)
    return c


def build_bot_frames():
    a = {}
    a["idle"] = [bot(), bot(blink=True), bot(), bot(face="closed")]
    th = []
    for i in range(6):
        f = bot(face="up", arms="chin", blink=i % 2 == 0)
        dots(f, i % 4)
        th.append(f)
    a["thinking"] = th
    tl = []
    for i in range(4):
        f = bot(face="down", arms="type1" if i % 2 == 0 else "type2", blink=i % 2 == 1)
        laptop(f, i % 2 == 0)
        tl.append(f)
    a["tool"] = tl
    rd = []
    for i in range(4):
        f = bot(face="down" if i != 3 else "closed")
        book(f)
        rd.append(f)
    a["reading"] = rd
    at = []
    for i in range(2):
        f = bot(dy=-2 if i == 0 else 0, face="wide", arms="raised", blink=i == 0, fg=FACE_RED if i == 0 else FACE_FG)
        exclaim(f, -1 if i == 0 else 0)
        at.append(f)
    a["attention"] = at
    dn = []
    for i, dy in enumerate([0, -3, -6, -7, -6, -3, 0, 0]):
        f = bot(dy=dy, face="happy", arms="both" if dy < -2 else "down", squash=1 if i in (0, 7) else 0, blink=i % 2 == 0)
        if i in (2, 3, 4):
            sparkle(f, 4, 6 + i)
            sparkle(f, 27, 4 + i)
        dn.append(f)
    a["done"] = dn
    er = []
    for i in range(4):
        f = bot(face="x" if i % 2 == 0 else "sad", squash=1, fg=FACE_RED, blink=True)
        sweat(f, i % 2)
        er.append(f)
    a["error"] = er
    sl = []
    for i in range(4):
        f = bot(lying=True, face="closed", fg=(60, 110, 110, 255))
        zzz(f, i % 3)
        sl.append(f)
    a["sleep"] = sl
    a["wake"] = [bot(squash=2, face="closed"), bot(dy=-1, arms="both", face="happy", blink=True)]
    a["squish"] = [bot(squash=3, face="closed"), bot(dy=-1, face="happy"), bot(squash=1)]
    a["wave"] = [bot(arms="wave1", face="happy"), bot(arms="wave2", face="happy", blink=True)] * 2
    return a


# ---------------------------------------------------------------- output

ONE_SHOT = ("done", "wake", "squish", "wave")
FPS = {"idle": 3, "thinking": 6, "tool": 8, "reading": 3, "attention": 4, "done": 12, "error": 4,
       "sleep": 2, "wake": 2.5, "squish": 12, "wave": 6.7}


def actions_for(frames):
    actions = {}
    for n, fr in frames.items():
        a = {"sheet": f"sprites/{n}.png", "frames": len(fr), "fps": FPS[n], "loop": n not in ONE_SHOT}
        if n in ONE_SHOT:
            a["then"] = "idle"
        actions[n] = a
    actions["attention"]["sound"] = "sounds/attention.wav"
    actions["done"]["sound"] = "sounds/done.wav"
    actions["error"]["sound"] = "sounds/error.wav"
    actions["stone"] = {"fallback": "error", "tint": "#9aa0a6"}
    actions["aborted"] = {"fallback": "error"}
    return actions


def write_pack(pid, name, frames, bubbles, tags, sounds, description="", en=None):
    """`en`: optional {"name", "description", "bubbles", "tags"} written as the *_en fields."""
    en = en or {}
    out = ROOT / "pets" / pid
    (out / "sprites").mkdir(parents=True, exist_ok=True)
    (out / "sounds").mkdir(parents=True, exist_ok=True)
    for n, fr in frames.items():
        strip(fr).save(out / "sprites" / f"{n}.png")
    for n, (notes, vol) in sounds.items():
        tone(out / "sounds" / f"{n}.wav", notes, vol=vol)
    manifest = {
        "schema": 1,
        "id": pid,
        "name": name,
        "name_en": en.get("name"),
        "version": "1.0.0",
        "author": "Familiar",
        "description": description,
        "description_en": en.get("description"),
        "frame": {"w": W, "h": H, "scale": SCALE, "anchor": [16, 31]},
        "actions": actions_for(frames),
        "bubbles": bubbles,
        "bubbles_en": en.get("bubbles"),
        "tags": tags,
        "tags_en": en.get("tags"),
    }
    manifest = {k: v for k, v in manifest.items() if v is not None}
    (out / "pet.json").write_text(json.dumps(manifest, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
    frames["idle"][0].image().resize((256, 256), Image.NEAREST).save(out / "preview.png")


def main():
    cat_frames = build_frames()
    add_cat_wave(cat_frames)
    write_pack(
        "pixel-cat", "像素猫", cat_frames,
        {
            "attention": ["轮到你啦！", "等你确认～", "喵？要你点一下"],
            "done": ["搞定！", "做完啦～", "喵～收工"],
            "error": ["哎呀出错了", "翻车了…", "再试试？"],
            "stone": ["石化了…", "连错三次，让我缓缓"],
        },
        ["猫", "可爱", "像素"],
        {
            "attention": ([(880, 0.12), (1318.5, 0.22)], 0.35),
            "done": ([(659.3, 0.08), (784, 0.08), (1046.5, 0.18)], 0.25),
            "error": ([(220, 0.12), (196, 0.2)], 0.25),
        },
        description="橘色小猫，敲电脑、翻书、跳起来庆祝。",
        en={
            "name": "Pixel Cat",
            "description": "A little orange cat that types, reads and jumps for joy.",
            "bubbles": {
                "attention": [
                    "Your turn!",
                    "Waiting on you~",
                    "Mew? Need a click"
                ],
                "done": [
                    "Done!",
                    "All finished~",
                    "Purr~ clocking out"
                ],
                "error": [
                    "Oops, an error",
                    "Crashed and burned…",
                    "Try again?"
                ],
                "stone": [
                    "Turned to stone…",
                    "Three fails in a row, gimme a sec"
                ]
            },
            "tags": [
                "cat",
                "cute",
                "pixel"
            ]
        },
    )

    bot_frames = build_bot_frames()
    write_pack(
        "pixel-bot", "像素机器人", bot_frames,
        {
            "attention": ["需要授权！", "等待人类输入…", "滴——请确认"],
            "done": ["任务完成。", "执行完毕", "滴滴，收工"],
            "error": ["检测到错误", "异常！正在冷却", "重试中…"],
            "stone": ["系统过热，休眠中", "连续失败 3 次，冷却"],
        },
        ["机器人", "极客", "像素"],
        {
            "attention": ([(1046.5, 0.08), (1046.5, 0.08), (1568, 0.16)], 0.3),
            "done": ([(523.3, 0.06), (784, 0.06), (1046.5, 0.06), (1568, 0.14)], 0.22),
            "error": ([(330, 0.08), (247, 0.08), (196, 0.18)], 0.25),
        },
        description="履带小机器人，屏幕脸，天线会闪。",
        en={
            "name": "Pixel Bot",
            "description": "A tiny tracked robot with a screen face and a blinking antenna.",
            "bubbles": {
                "attention": [
                    "Authorization required!",
                    "Awaiting human input…",
                    "Beep — please confirm"
                ],
                "done": [
                    "Task complete.",
                    "Execution finished",
                    "Beep boop, job done"
                ],
                "error": [
                    "Error detected",
                    "Exception! Cooling down",
                    "Retrying…"
                ],
                "stone": [
                    "System overheated, sleeping",
                    "3 failures in a row, cooling off"
                ]
            },
            "tags": [
                "robot",
                "geek",
                "pixel"
            ]
        },
    )

    icons = ROOT / "app" / "src-tauri" / "icons"
    icons.mkdir(parents=True, exist_ok=True)
    head = cat_frames["idle"][0].image().crop((4, 4, 28, 28)).resize((240, 240), Image.NEAREST)
    big = Image.new("RGBA", (256, 256), (0, 0, 0, 0))
    big.paste(head, (8, 8))
    big.save(icons / "icon.png")
    for s in (32, 128):
        big.resize((s, s), Image.NEAREST).save(icons / f"{s}x{s}.png")
    big.resize((256, 256), Image.NEAREST).save(icons / "128x128@2x.png")
    big.save(icons / "icon.ico", sizes=[(16, 16), (24, 24), (32, 32), (48, 48), (64, 64), (128, 128), (256, 256)])

    main_m3(cat_frames, bot_frames)

    # Contact sheet of every action of both packs, for eyeballing.
    rows = [strip(fr) for frames in (cat_frames, bot_frames) for fr in frames.values()]
    wmax = max(r.width for r in rows)
    sheet = Image.new("RGBA", (wmax, H * len(rows)), (255, 255, 255, 255))
    for i, r in enumerate(rows):
        sheet.paste(r, (0, i * H), r)
    (ROOT / "target").mkdir(exist_ok=True)
    sheet.resize((wmax * 3, H * len(rows) * 3), Image.NEAREST).save(ROOT / "target" / "pets-contact-sheet.png")
    print("pixel-cat, pixel-bot and icons written; contact sheet in target/")



# ---------------------------------------------------------------- M3/M4 actions
#
# Interaction and gameplay actions shared by both packs: walk, fall, dizzy,
# petted, annoyed, sulk, fish, fish_reel, cheer, taunt, comfort, peek.

PINK_HEART = (240, 90, 130, 255)
WATER = (90, 160, 230, 255)
ROD = (120, 80, 50, 255)
FISHC = (250, 170, 60, 255)
CLOUD = (150, 158, 170, 255)
ANGER = (225, 40, 50, 255)


def heart(c, x, y):
    for dx, dy in ((0, 0), (2, 0), (-1, 1), (0, 1), (1, 1), (2, 1), (3, 1), (0, 2), (1, 2), (2, 2), (1, 3)):
        c.set(x + dx, y + dy, PINK_HEART)


def stars(c, phase, cy):
    # Three stars circling over the head.
    for i in range(3):
        a = phase * 0.9 + i * 2.09
        x = int(round(16 + math.cos(a) * 7))
        y = int(round(cy + math.sin(a) * 2))
        sparkle(c, x, y)


def anger(c, x, y):
    for dx, dy in ((0, 0), (2, 0), (1, 1), (0, 2), (2, 2)):
        c.set(x + dx, y + dy, ANGER)


def cloud(c, x, y):
    for dx in range(5):
        c.set(x + dx, y, CLOUD)
        c.set(x + dx, y + 1, CLOUD)
    c.set(x + 1, y - 1, CLOUD)
    c.set(x + 3, y - 1, CLOUD)
    for i in range(3):
        c.set(x + i * 2, y + 3, BLUE)


def rod(c, phase, bite=False):
    """Fishing rod held to the right, line dropping off the bottom edge."""
    tipx, tipy = 29, 12 - (2 if bite else 0)
    for i in range(10):
        c.set(20 + i, 22 - i, ROD)
    dip = [0, 1, 0, -1][phase % 4] + (2 if bite else 0)
    for y in range(tipy + 1, 30):
        c.set(tipx + (1 if y > 24 else 0), y, (230, 230, 230, 255))
    c.set(tipx + 1, 29 + min(0, dip), RED)
    c.set(tipx + 1, 30, WATER)
    c.set(tipx, 31, WATER)
    c.set(tipx + 2, 31, WATER)


def fish_icon(c, x, y):
    c.rect(x, y, x + 4, y + 2, FISHC)
    c.set(x + 5, y, FISHC)
    c.set(x + 5, y + 2, FISHC)
    c.set(x + 1, y + 1, EYE)


def m3_actions(draw, kind):
    """draw(**kw) returns a Canvas for this pack; kind is 'cat' or 'bot'."""
    cat_ = kind == "cat"
    a = {}

    def arms(name):
        if cat_:
            return {"both": "both", "raised": "raised", "raisedR": "raisedR", "walk1": "walk1", "walk2": "walk2",
                    "rod": "rod", "down": "down", "chin": "chin", "type1": "up1", "type2": "up2"}[name]
        return {"both": "both", "raised": "raised", "raisedR": "wave1", "walk1": "type1", "walk2": "type2",
                "rod": "type1", "down": "down", "chin": "chin", "type1": "type1", "type2": "type2"}[name]

    def face(name):
        if cat_:
            return {"open": "open", "happy": "happy", "x": "x", "wide": "wide", "sad": "sad", "closed": "closed",
                    "down": "down", "up": "up"}[name]
        return name

    def kw(**k):
        out = {}
        if "eyes" in k:
            out["eyes" if cat_ else "face"] = face(k.pop("eyes"))
        if "arms" in k:
            out["paws" if cat_ else "arms"] = arms(k.pop("arms"))
        if not cat_:
            k.pop("tail", None)
            k.pop("mouth", None)
        out.update(k)
        return out

    a["walk"] = [draw(**kw(dy=-(i % 2), arms="walk1" if i % 2 == 0 else "walk2", tail=i % 3)) for i in range(4)]
    a["fall"] = [draw(**kw(eyes="wide", arms="both", mouth="open", tail=i)) for i in range(2)]
    dz = []
    for i in range(4):
        f = draw(**kw(eyes="x", arms="down", squash=1, mouth="open", tail=i % 3))
        stars(f, i, 3)
        dz.append(f)
    a["dizzy"] = dz
    pt = []
    for i in range(4):
        f = draw(**kw(eyes="happy", arms="down", squash=0.5 if i % 2 else 0, tail=i % 3))
        heart(f, 25, 6 - i % 2 * 2)
        if i >= 2:
            heart(f, 3, 9 - i % 2)
        pt.append(f)
    a["petted"] = pt
    an = []
    for i in range(3):
        f = draw(**kw(eyes="sad", arms="both" if i == 1 else "down", mouth="frown", dy=-2 if i == 1 else 0))
        anger(f, 24, 4 + (i % 2))
        an.append(f)
    a["annoyed"] = an
    sk = []
    for i in range(4):
        f = draw(**kw(eyes="down", arms="down", mouth="frown", squash=1, tail=0))
        cloud(f, 23, 3 + (i % 2))
        sk.append(f)
    a["sulk"] = sk
    fs = []
    for i in range(4):
        f = draw(**kw(eyes="closed" if i == 3 else "open", arms="rod", tail=i % 3))
        rod(f, i)
        fs.append(f)
    a["fish"] = fs
    fr = []
    for i, dy in enumerate([0, 0, -2, -5, -6, -3]):
        f = draw(**kw(dy=dy, eyes="wide" if i < 2 else "happy", arms="rod" if i < 2 else "both", mouth="open"))
        if i < 2:
            rod(f, 0, bite=True)
        else:
            fish_icon(f, 24, 4 + i)
            sparkle(f, 4, 8)
        fr.append(f)
    a["fish_reel"] = fr
    ch = []
    for i in range(4):
        f = draw(**kw(dy=-2 if i % 2 == 0 else 0, eyes="happy", arms="both" if i % 2 == 0 else "raised", mouth="open", tail=i % 3))
        sparkle(f, 3 + (i % 2) * 2, 6)
        sparkle(f, 27, 5 + (i % 2))
        ch.append(f)
    a["cheer"] = ch
    tn = []
    for i in range(4):
        f = draw(**kw(eyes="happy" if i % 2 else "closed", arms="raisedR" if i % 2 == 0 else "down", mouth="open", dy=-(i % 2)))
        tn.append(f)
    a["taunt"] = tn
    cm = []
    for i in range(4):
        f = draw(**kw(eyes="happy", arms="type1" if i % 2 == 0 else "type2", tail=i % 3))
        heart(f, 25, 7 - (i % 2))
        cm.append(f)
    a["comfort"] = cm
    a["peek"] = [draw(**kw(dy=d, eyes="up" if d > 0 else "happy")) for d in (14, 9, 4, 0)]

    # Eating a fish: chomp, the fish gets smaller, then a happy face.
    my = 20 if cat_ else 17
    eat = []
    for i, size in enumerate([5, 5, 3, 3, 1, 0]):
        f = draw(**kw(eyes="closed" if i % 2 else "happy", arms="chin", mouth="open" if i % 2 == 0 else None))
        if size:
            c = f
            c.rect(17, my, 16 + size, my + 2, FISHC)
            c.set(17, my + 1, EYE)
        if i >= 4:
            heart(f, 24, 6 + (i % 2))
        eat.append(f)
    a["eat"] = eat

    # Packing the bag before going home.
    bag = []
    for i in range(4):
        f = draw(**kw(eyes="down" if i % 2 == 0 else "happy", arms="type1" if i % 2 == 0 else "type2", tail=i % 3))
        f.rect(22, 22, 29, 29, ROD)
        f.rect(23, 23, 28, 28, (160, 110, 70, 255))
        f.rect(24, 20, 27, 21, ROD)
        if i % 2 == 0:
            f.rect(24, 21, 27, 23, BLUE)  # the laptop going in
        bag.append(f)
    a["pack_bag"] = bag
    return a


M3_FPS = {"walk": 8, "fall": 6, "dizzy": 6, "petted": 5, "annoyed": 6, "sulk": 3, "fish": 2.5, "fish_reel": 7,
          "cheer": 6, "taunt": 5, "comfort": 4, "peek": 8, "eat": 5, "pack_bag": 3}
# One-shot actions return to `then` (idle) when done.
M3_ONE_SHOT = ("fish_reel", "peek", "annoyed", "eat")


def chirp(path, f0, f1, dur, vol=0.3, rate=22050):
    """Pitch sweep for squish / squash sounds."""
    frames = bytearray()
    n = int(rate * dur)
    ph = 0.0
    for i in range(n):
        t = i / n
        f = f0 + (f1 - f0) * t
        ph += 2 * math.pi * f / rate
        env = min(1.0, i / (rate * 0.005)) * (1 - t) ** 2
        frames += struct.pack("<h", int(math.sin(ph) * env * vol * 32767))
    with wave.open(str(path), "wb") as w:
        w.setnchannels(1)
        w.setsampwidth(2)
        w.setframerate(rate)
        w.writeframes(bytes(frames))


def purr(path, dur=1.2, rate=22050, vol=0.25):
    import random
    rnd = random.Random(7)
    frames = bytearray()
    n = int(rate * dur)
    for i in range(n):
        t = i / rate
        amp = 0.5 + 0.5 * math.sin(2 * math.pi * 24 * t)
        v = (rnd.random() * 2 - 1) * 0.5 + math.sin(2 * math.pi * 60 * t) * 0.5
        env = min(1.0, i / (rate * 0.05)) * min(1.0, (n - i) / (rate * 0.1))
        frames += struct.pack("<h", int(v * amp * env * vol * 32767))
    with wave.open(str(path), "wb") as w:
        w.setnchannels(1)
        w.setsampwidth(2)
        w.setframerate(rate)
        w.writeframes(bytes(frames))


def write_m3(pid, frames_m3, squish_pitch, petted_sound):
    """Add the M3/M4 sheets and sounds to an existing pack and patch pet.json."""
    out = ROOT / "pets" / pid
    for n, fr in frames_m3.items():
        strip(fr).save(out / "sprites" / f"{n}.png")
    for i, (f0, f1) in enumerate(squish_pitch):
        chirp(out / "sounds" / f"squish{i + 1}.wav", f0, f1, 0.16)
    petted_sound(out / "sounds" / "petted.wav")
    manifest = json.loads((out / "pet.json").read_text(encoding="utf-8"))
    for n, fr in frames_m3.items():
        act = {"sheet": f"sprites/{n}.png", "frames": len(fr), "fps": M3_FPS[n], "loop": n not in M3_ONE_SHOT}
        if n in M3_ONE_SHOT:
            act["then"] = "idle"
        manifest["actions"][n] = act
    manifest["actions"]["petted"]["sound"] = "sounds/petted.wav"
    manifest["sounds"] = {"squish": [f"sounds/squish{i + 1}.wav" for i in range(len(squish_pitch))]}
    (out / "pet.json").write_text(json.dumps(manifest, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
    return frames_m3


def system_sounds():
    """Sounds that belong to the system overlay, not to a pack."""
    out = ROOT / "app" / "public" / "sfx"
    out.mkdir(parents=True, exist_ok=True)
    chirp(out / "splat.wav", 420, 90, 0.12, vol=0.35)
    chirp(out / "boss-hit.wav", 260, 120, 0.09, vol=0.35)
    tone(out / "coin.wav", [(1318.5, 0.06), (1760, 0.12)], vol=0.22)
    tone(out / "boss-down.wav", [(523.3, 0.08), (659.3, 0.08), (784, 0.08), (1046.5, 0.25)], vol=0.25)
    tone(out / "catch.wav", [(784, 0.07), (1046.5, 0.07), (1318.5, 0.16)], vol=0.22)
    tone(out / "splash.wav", [(220, 0.05), (180, 0.08)], vol=0.15)


def main_m3(cat_frames, bot_frames):
    cat_m3 = write_m3("pixel-cat", m3_actions(cat, "cat"), [(700, 300), (820, 380), (600, 260)], purr)
    bot_m3 = write_m3("pixel-bot", m3_actions(bot, "bot"), [(900, 500), (1100, 600), (760, 420)],
                      lambda p: tone(p, [(523.3, 0.1), (659.3, 0.1), (523.3, 0.1), (659.3, 0.15)], vol=0.18))
    system_sounds()
    rows = [strip(fr) for frames in (cat_m3, bot_m3) for fr in frames.values()]
    wmax = max(r.width for r in rows)
    sheet = Image.new("RGBA", (wmax, H * len(rows)), (255, 255, 255, 255))
    for i, r in enumerate(rows):
        sheet.paste(r, (0, i * H), r)
    sheet.resize((wmax * 3, H * len(rows) * 3), Image.NEAREST).save(ROOT / "target" / "pets-m3-sheet.png")


if __name__ == "__main__":
    main()
