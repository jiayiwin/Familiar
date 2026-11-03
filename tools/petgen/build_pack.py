"""Make a Familiar pet pack from one picture, or from AI action videos.

One picture (no AI needed):

    python tools/petgen/build_pack.py <workdir> --image me.png --id my-pet --name "My Pet"

The picture should show the whole character on a transparent or plain
one-colour background. Every action is animated from it (see still.py).

Action videos:

    python tools/petgen/build_pack.py <workdir> --id my-pet --name "My Pet"

<workdir> holds one video per action, named after the action: idle.mp4,
thinking.mp4, tool.mp4, attention.mp4, done.mp4, error.mp4, sleep.mp4 (any
subset; idle is required). Videos are expected to share one fixed camera and
a solid green background (the generation prompts ask for both).

Output: <workdir>/pack/<id>/ (pet.json, sprites/, sounds/, preview.png) and
<workdir>/contact.png for a quick look. Requires Pillow and numpy; the video
route also needs ffmpeg on PATH.
"""

import argparse
import json
import shutil
import subprocess
import tempfile
from pathlib import Path

import numpy as np
from PIL import Image, ImageDraw

import still

ROOT = Path(__file__).resolve().parents[2]

# action: (frames in the strip, fps, loop?, which part of the clip to search)
PLAN = {
    "idle": (6, 5, True, (0.0, 1.0)),
    "thinking": (6, 5, True, (0.25, 1.0)),
    "tool": (6, 8, True, (0.2, 1.0)),
    "attention": (4, 6, True, (0.25, 1.0)),
    "done": (8, 10, False, (0.0, 1.0)),
    "error": (6, 5, True, (0.3, 1.0)),
    "sleep": (4, 2.5, True, (0.6, 1.0)),
}
EXTRACT_FPS = 12


def extract(video: Path, out: Path):
    out.mkdir(parents=True, exist_ok=True)
    subprocess.run(
        ["ffmpeg", "-loglevel", "error", "-i", str(video), "-vf", f"fps={EXTRACT_FPS}", str(out / "f%04d.png")],
        check=True,
    )
    return sorted(out.glob("f*.png"))


def key_green(img: Image.Image) -> np.ndarray:
    """Chroma-key a green-screen frame to RGBA (float arrays 0..1), with despill."""
    a = np.asarray(img.convert("RGB")).astype(np.float32) / 255.0
    r, g, b = a[..., 0], a[..., 1], a[..., 2]
    green = g - np.maximum(r, b)
    # Fully transparent above ~0.28 "greenness", fully opaque below ~0.08.
    alpha = np.clip(1.0 - (green - 0.08) / 0.20, 0.0, 1.0)
    # Despill: pull green down to the other channels on edge pixels.
    g2 = np.where(green > 0, np.maximum(r, b) + np.clip(green, 0, 0.06), g)
    rgba = np.dstack([r, g2, b, alpha])
    rgba[alpha < 0.08] = 0
    return rgba


def bbox(alpha: np.ndarray, thr=0.5):
    """Box around the character, ignoring stray keyed pixels: rows / columns
    need a few solid pixels, and a thin border of the video is skipped."""
    m = alpha > thr
    b = max(3, m.shape[0] // 100)
    m[:b, :] = m[-b:, :] = False
    m[:, :b] = m[:, -b:] = False
    rows = np.where(m.sum(axis=1) >= 3)[0]
    cols = np.where(m.sum(axis=0) >= 3)[0]
    if len(rows) == 0 or len(cols) == 0:
        return None
    return cols.min(), rows.min(), cols.max() + 1, rows.max() + 1


def small(rgba: np.ndarray) -> np.ndarray:
    """Downscaled premultiplied colour, for comparing frames."""
    im = Image.fromarray((rgba * 255).astype(np.uint8), "RGBA").resize((48, 48))
    a = np.asarray(im).astype(np.float32) / 255.0
    return a[..., :3] * a[..., 3:4]


def pick(frames: list, n: int, loop: bool, part):
    """Indices of n frames. Loops: the window whose first and last frame match best."""
    total = len(frames)
    lo, hi = int(part[0] * total), max(int(part[1] * total), int(part[0] * total) + n)
    hi = min(hi, total)
    if not loop or hi - lo <= n + 2:
        return list(np.linspace(lo, hi - 1, n).round().astype(int))
    smalls = {i: small(frames[i]) for i in range(lo, hi)}
    best = None
    min_len = max(n, int(EXTRACT_FPS * 1.2))
    for s in range(lo, hi - min_len):
        for e in range(s + min_len, hi):
            d = float(np.abs(smalls[s] - smalls[e]).mean())
            score = d - 0.0005 * (e - s)  # prefer longer windows when equally seamless
            if best is None or score < best[0]:
                best = (score, s, e)
    _, s, e = best
    # e matches s, so stop just before it: the strip wraps back to s.
    return list(np.linspace(s, e - 1, n).round().astype(int))


def to_image(rgba: np.ndarray) -> Image.Image:
    return Image.fromarray((np.clip(rgba, 0, 1) * 255).astype(np.uint8), "RGBA")


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("workdir", type=Path)
    ap.add_argument("--id", required=True)
    ap.add_argument("--name", required=True)
    ap.add_argument("--size", type=int, default=128, help="frame size in px (square)")
    ap.add_argument("--scale", type=float, default=1.0)
    ap.add_argument("--tags", help="comma-separated (default: 自制 for --image, AI 生成 for videos)")
    ap.add_argument("--bubbles", type=Path, help="JSON file with bubble lines")
    ap.add_argument("--name-en", help="English display name (pet.json name_en)")
    ap.add_argument("--bubbles-en", type=Path, help="JSON file with English bubble lines (pet.json bubbles_en)")
    ap.add_argument("--image", type=Path, help="make every action from this one picture (no videos needed)")
    args = ap.parse_args()
    work = args.workdir
    work.mkdir(parents=True, exist_ok=True)

    if args.image:
        try:
            rgba = still.cutout(args.image)
        except still.CutoutError as e:
            raise SystemExit(str(e))
        strips, meta = still.render(rgba, args.size)
        print(f"{len(strips)} actions animated from {args.image.name}")
        description = ("由 tools/petgen 从一张图片生成", "Generated by tools/petgen from one picture")
    else:
        strips, meta = from_videos(work, args.size)
        description = ("由 tools/petgen 从参考图和 AI 视频生成", "Generated by tools/petgen from reference art and AI video")
    write_pack(work, args, strips, meta, description)


def from_videos(work: Path, size: int):
    tmp = Path(tempfile.mkdtemp(prefix="petgen-"))
    keyed: dict[str, list] = {}
    for action in PLAN:
        video = work / f"{action}.mp4"
        if not video.exists():
            continue
        paths = extract(video, tmp / action)
        keyed[action] = [key_green(Image.open(p)) for p in paths]
        print(f"{action}: {len(paths)} frames extracted")
    if "idle" not in keyed:
        raise SystemExit("idle.mp4 is required")

    chosen: dict[str, list] = {}
    for action, frames in keyed.items():
        n, _, loop, part = PLAN[action]
        idx = pick(frames, n, loop, part)
        chosen[action] = [frames[i] for i in idx]
        print(f"{action}: frames {idx}")
    # Waking up = the start of the sleep clip (sitting down) played backwards.
    if "sleep" in keyed:
        frames = keyed["sleep"]
        cut = max(4, int(len(frames) * 0.45))
        idx = list(np.linspace(0, cut, 4).round().astype(int))[::-1]
        chosen["wake"] = [frames[i] for i in idx]

    # One crop for every action: union bbox, square, feet on the bottom edge.
    boxes = [bbox(f[..., 3]) for frames in chosen.values() for f in frames]
    boxes = [b for b in boxes if b]
    # A frame with background noise at the edges would blow the box up to the
    # whole video: drop boxes much wider than the typical one.
    med = float(np.median([b[2] - b[0] for b in boxes]))
    boxes = [b for b in boxes if b[2] - b[0] <= med * 1.5] or boxes
    x0 = min(b[0] for b in boxes)
    y0 = min(b[1] for b in boxes)
    x1 = max(b[2] for b in boxes)
    y1 = max(b[3] for b in boxes)
    side = int(max(x1 - x0, y1 - y0) * 1.04)
    cx = (x0 + x1) // 2
    crop = (cx - side // 2, y1 - side, cx - side // 2 + side, y1)
    print(f"crop {crop} -> {size}px")

    def frame_img(rgba):
        im = to_image(rgba)
        # Pad so crops outside the video stay transparent.
        pad = Image.new("RGBA", (im.width + 2 * side, im.height + 2 * side), (0, 0, 0, 0))
        pad.paste(im, (side, side))
        c = pad.crop((crop[0] + side, crop[1] + side, crop[2] + side, crop[3] + side))
        # Resize in premultiplied space so edges do not grow dark halos.
        return c.convert("RGBa").resize((size, size), Image.LANCZOS).convert("RGBA")

    strips = {action: [frame_img(f) for f in frames] for action, frames in chosen.items()}
    meta = {action: (PLAN[action][1], PLAN[action][2]) for action in strips if action in PLAN}
    if "wake" in strips:
        meta["wake"] = (6, False)
    shutil.rmtree(tmp, ignore_errors=True)
    return strips, meta


def write_pack(work: Path, args, strips: dict, meta: dict, description: tuple):
    out = work / "pack" / args.id
    if out.exists():
        shutil.rmtree(out)
    (out / "sprites").mkdir(parents=True)
    (out / "sounds").mkdir()
    for action, imgs in strips.items():
        strip = Image.new("RGBA", (args.size * len(imgs), args.size), (0, 0, 0, 0))
        for i, im in enumerate(imgs):
            strip.paste(im, (i * args.size, 0))
        strip.save(out / "sprites" / f"{action}.png")

    # Sounds: the built-in chimes (generated in code, no licensing questions).
    for s in ("attention", "done", "error", "squish1", "squish2", "squish3"):
        src = ROOT / "pets" / "pixel-cat" / "sounds" / f"{s}.wav"
        if src.exists():
            shutil.copy(src, out / "sounds" / src.name)

    actions = {}
    for action, imgs in strips.items():
        fps, loop = meta.get(action, (6, False))
        a = {"sheet": f"sprites/{action}.png", "frames": len(imgs), "fps": fps, "loop": loop}
        if not loop:
            a["then"] = "idle"
        actions[action] = a
    for k, snd in (("attention", "attention"), ("done", "done"), ("error", "error")):
        if k in actions:
            actions[k]["sound"] = f"sounds/{snd}.wav"
    actions["stone"] = {"fallback": "error", "tint": "#9aa0a6"}
    actions["aborted"] = {"fallback": "error"}

    bubbles = json.loads(args.bubbles.read_text(encoding="utf-8")) if args.bubbles else {}
    bubbles_en = json.loads(args.bubbles_en.read_text(encoding="utf-8")) if args.bubbles_en else None
    manifest = {
        "schema": 1,
        "id": args.id,
        "name": args.name,
        "name_en": args.name_en,
        "version": "0.1.0",
        "author": "Familiar petgen",
        "description": description[0],
        "description_en": description[1],
        "frame": {"w": args.size, "h": args.size, "scale": args.scale},
        "actions": actions,
        "bubbles": bubbles,
        "bubbles_en": bubbles_en,
        "sounds": {"squish": [f"sounds/squish{i}.wav" for i in (1, 2, 3) if (out / "sounds" / f"squish{i}.wav").exists()]},
        "tags": [t.strip() for t in (args.tags or ("自制" if args.image else "AI 生成")).split(",") if t.strip()],
    }
    manifest = {k: v for k, v in manifest.items() if v is not None}
    (out / "pet.json").write_text(json.dumps(manifest, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
    strips["idle"][0].resize((256, 256), Image.LANCZOS).save(out / "preview.png")

    # Same checks as the app's importer: strip sizes and referenced files.
    for action, a in actions.items():
        if "sheet" in a:
            w, h = Image.open(out / a["sheet"]).size
            assert (w, h) == (a["frames"] * args.size, args.size), (action, w, h)
        if "sound" in a:
            assert (out / a["sound"]).exists(), a["sound"]

    # Contact sheet on a checkerboard so transparency problems show.
    rows = list(strips.items())
    cols = max(len(v) for _, v in rows)
    sheet = Image.new("RGBA", (cols * args.size + 90, len(rows) * args.size), (255, 255, 255, 255))
    for y in range(0, sheet.height, 16):
        for x in range(90, sheet.width, 16):
            if (x // 16 + y // 16) % 2:
                sheet.paste((230, 230, 230, 255), (x, y, x + 16, y + 16))
    d = ImageDraw.Draw(sheet)
    for r, (action, imgs) in enumerate(rows):
        d.text((6, r * args.size + args.size // 2 - 6), action, fill=(0, 0, 0, 255))
        for c, im in enumerate(imgs):
            sheet.alpha_composite(im, (90 + c * args.size, r * args.size))
    sheet.save(work / "contact.png")
    print(f"pack written to {out}")


if __name__ == "__main__":
    main()
