// Animated pack previews for the gallery and the first-run chooser.

import { fileUrl, getJson, type Conn, type Pack } from "./api";
import { Sprites } from "./sprites";

interface Item {
  canvas: HTMLCanvasElement;
  sprites: Sprites;
  action: string;
  start: number;
  hue: number;
}

export class Previews {
  private items: Item[] = [];
  private cache = new Map<string, Promise<Sprites>>();

  constructor(private conn: Conn) {
    const loop = (t: number) => {
      for (const it of this.items) this.draw(it, t);
      requestAnimationFrame(loop);
    };
    requestAnimationFrame(loop);
  }

  sprites(id: string, rev: number): Promise<Sprites> {
    const key = `${id}@${rev}`;
    let p = this.cache.get(key);
    if (!p) {
      p = getJson<Pack>(this.conn, `/v1/packs/${encodeURIComponent(id)}`).then((pack) =>
        Sprites.load(pack, (f) => fileUrl(this.conn, id, f, rev)),
      );
      this.cache.set(key, p);
    }
    return p;
  }

  clear(): void {
    this.items = [];
  }

  /** A canvas that plays `idle`, and `wave` (or `done`) while hovered. */
  async mount(id: string, rev: number, size = 112, hue = 0): Promise<HTMLCanvasElement> {
    const canvas = document.createElement("canvas");
    const dpr = window.devicePixelRatio || 1;
    canvas.width = size * dpr;
    canvas.height = size * dpr;
    canvas.style.width = `${size}px`;
    canvas.style.height = `${size}px`;
    const sprites = await this.sprites(id, rev);
    const it: Item = { canvas, sprites, action: "idle", start: performance.now(), hue };
    canvas.addEventListener("mouseenter", () => {
      it.action = sprites.action("wave") ? "wave" : sprites.action("done") ? "done" : "idle";
      it.start = performance.now();
    });
    canvas.addEventListener("mouseleave", () => {
      it.action = "idle";
      it.start = performance.now();
    });
    this.items.push(it);
    return canvas;
  }

  private draw(it: Item, now: number): void {
    const c = it.canvas.getContext("2d")!;
    const dpr = window.devicePixelRatio || 1;
    const size = it.canvas.width / dpr;
    c.setTransform(dpr, 0, 0, dpr, 0, 0);
    c.clearRect(0, 0, size, size);
    let a = it.sprites.action(it.action) ?? it.sprites.action("idle")!;
    let fr = it.sprites.frameAt(a, now - it.start);
    if (fr.finished) {
      // Keep looping the hover action.
      it.start = now;
      fr = it.sprites.frameAt(a, 0);
    }
    const f = it.sprites.pack.frame;
    // Whole multiples keep pixel art crisp; a frame bigger than the canvas is scaled down to fit.
    const raw = Math.min(size / f.w, size / f.h);
    const scale = raw >= 1 ? Math.floor(raw) : raw;
    if (!a) a = it.sprites.action("idle")!;
    it.sprites.draw(c, a, fr.idx, size / 2, size - (size - f.h * scale) / 2, scale, it.hue);
  }
}

export function toast(text: string): void {
  let el = document.querySelector<HTMLDivElement>(".toast");
  if (!el) {
    el = document.createElement("div");
    el.className = "toast";
    document.body.appendChild(el);
  }
  el.textContent = text;
  el.classList.add("show");
  clearTimeout(Number(el.dataset.t));
  el.dataset.t = String(window.setTimeout(() => el!.classList.remove("show"), 2600));
}
