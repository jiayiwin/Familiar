// Loaded pack sprites + frame drawing (tint, hue). Shared by the pet window
// and the gallery previews.

import type { Action, Pack } from "./api";

export class Sprites {
  private tinted = new Map<string, HTMLCanvasElement>();

  private constructor(
    readonly pack: Pack,
    private sheets: Map<string, HTMLImageElement>,
    readonly url: (path: string) => string,
  ) {}

  /** `fallback`: where to fetch a sheet when `url` has none (evolution folders). */
  static async load(pack: Pack, url: (path: string) => string, fallback?: (path: string) => string): Promise<Sprites> {
    const sheets = new Map<string, HTMLImageElement>();
    const files = new Set(Object.values(pack.actions).map((a) => a.sheet));
    await Promise.all(
      [...files].map(
        (f) =>
          new Promise<void>((resolve) => {
            const img = new Image();
            img.crossOrigin = "anonymous";
            let retried = false;
            img.onload = () => resolve();
            img.onerror = () => {
              if (fallback && !retried) {
                retried = true;
                img.src = fallback(f);
              } else resolve();
            };
            img.src = url(f);
            sheets.set(f, img);
          }),
      ),
    );
    return new Sprites(pack, sheets, url);
  }

  private heads = new Map<string, { x: number; top: number; w: number }>();

  /**
   * Where the head is in a frame (frame px): top-most opaque row, the centre
   * and width of the body a little below it. Used to put hats on any pack.
   */
  head(a: Action, idx: number): { x: number; top: number; w: number } {
    const key = `${a.sheet}|${idx}`;
    const hit = this.heads.get(key);
    if (hit) return hit;
    const f = this.pack.frame;
    let res = { x: f.w / 2, top: f.h * 0.1, w: f.w * 0.5 };
    const img = this.sheets.get(a.sheet);
    if (img && img.complete && img.naturalWidth) {
      try {
        const cv = document.createElement("canvas");
        cv.width = f.w;
        cv.height = f.h;
        const g = cv.getContext("2d", { willReadFrequently: true })!;
        g.drawImage(img, idx * f.w, 0, f.w, f.h, 0, 0, f.w, f.h);
        const d = g.getImageData(0, 0, f.w, f.h).data;
        const opaque = (x: number, y: number) => d[(y * f.w + x) * 4 + 3] > 128;
        // Opaque span of every row.
        const spans: [number, number][] = [];
        let widest = 0;
        for (let y = 0; y < f.h; y++) {
          let l = -1, r = -1;
          for (let x = 0; x < f.w; x++) if (opaque(x, y)) { if (l < 0) l = x; r = x; }
          spans.push([l, r]);
          if (l >= 0) widest = Math.max(widest, r - l + 1);
        }
        // The head starts at the first row that is reasonably wide: thin
        // things on top (antennae, a hair strand, a hairpin) are skipped.
        const y = spans.findIndex(([l, r]) => l >= 0 && r - l + 1 >= widest * 0.4);
        if (y >= 0) {
          const [l, r] = spans[Math.min(f.h - 1, y + Math.round(f.h * 0.04))];
          if (l >= 0) res = { x: (l + r + 1) / 2, top: y, w: r - l + 1 };
        }
      } catch {
        // Tainted canvas (no CORS): keep the estimate.
      }
    }
    this.heads.set(key, res);
    return res;
  }

  action(name: string): Action | undefined {
    return this.pack.actions[name];
  }

  /** Frame index for an action that started `elapsed` ms ago. */
  frameAt(a: Action, elapsed: number): { idx: number; finished: boolean } {
    const raw = Math.floor((elapsed / 1000) * a.fps);
    if (a.loop) return { idx: raw % a.frames, finished: false };
    return { idx: Math.min(raw, a.frames - 1), finished: raw >= a.frames };
  }

  /** Draw frame `idx` of `a` with its bottom-centre at (cx, bottom). */
  draw(c: CanvasRenderingContext2D, a: Action, idx: number, cx: number, bottom: number, scale: number, hue = 0, alpha = 1): void {
    const src = this.source(a);
    if (!src) return;
    const f = this.pack.frame;
    const dw = f.w * scale;
    const dh = f.h * scale;
    c.save();
    // Pixel art (small frames) stays crisp and blocky; drawn or photo-like
    // packs (big frames) are scaled smoothly, or they look jagged and blurry.
    const smooth = f.w >= 96 || f.h >= 96;
    c.imageSmoothingEnabled = smooth;
    if (smooth) c.imageSmoothingQuality = "high";
    c.globalAlpha = alpha;
    if (hue) c.filter = `hue-rotate(${hue}deg)`;
    c.drawImage(src, idx * f.w, 0, f.w, f.h, Math.round(cx - dw / 2), Math.round(bottom - dh), dw, dh);
    c.restore();
  }

  private source(a: Action): CanvasImageSource | null {
    const img = this.sheets.get(a.sheet);
    if (!img || !img.complete || img.naturalWidth === 0) return null;
    if (!a.tint) return img;
    const key = `${a.sheet}|${a.tint}`;
    let t = this.tinted.get(key);
    if (!t) {
      t = document.createElement("canvas");
      t.width = img.naturalWidth;
      t.height = img.naturalHeight;
      const tc = t.getContext("2d")!;
      tc.drawImage(img, 0, 0);
      tc.globalCompositeOperation = "source-atop";
      tc.globalAlpha = 0.7;
      tc.fillStyle = a.tint;
      tc.fillRect(0, 0, t.width, t.height);
      this.tinted.set(key, t);
    }
    return t;
  }
}
