// One picture -> a pet pack, in the browser: cut the character out, then
// animate every action by squashing, tilting and hopping, with small drawn
// effects (thought dots, "!", sweat drop, Zzz, hearts, stars, confetti).
// Same idea as tools/petgen/still.py, so nobody needs Python.

export class CutoutError extends Error {}

const MAX_SIDE = 720;

/** How the character was separated from the background. */
export type CutMethod = "alpha" | "flat" | "ai";

/**
 * The character cut out of `img`, trimmed to its bounding box.
 * Transparent pictures are used as they are, a plain one-colour background
 * is keyed out, anything else goes through a small local segmentation model.
 */
export async function cutout(img: CanvasImageSource & { width: number; height: number }): Promise<{ canvas: HTMLCanvasElement; method: CutMethod }> {
  const k = Math.min(1, MAX_SIDE / Math.max(img.width, img.height));
  const w = Math.max(1, Math.round(img.width * k)), h = Math.max(1, Math.round(img.height * k));
  const cv = document.createElement("canvas");
  cv.width = w; cv.height = h;
  const g = cv.getContext("2d", { willReadFrequently: true })!;
  g.imageSmoothingQuality = "high";
  g.drawImage(img, 0, 0, w, h);
  const id = g.getImageData(0, 0, w, h);
  const d = id.data;
  const n = w * h;

  let transparent = 0;
  for (let i = 0; i < n; i++) if (d[i * 4 + 3] < 230) transparent++;
  let method: CutMethod = "alpha";
  if (transparent / n > 0.02) {
    for (let i = 0; i < n; i++) if (d[i * 4 + 3] < 10) d[i * 4 + 3] = 0;
  } else if (keyBackground(d, w, h)) {
    method = "flat";
  } else {
    method = "ai";
    const m = await segment(cv);
    for (let i = 0; i < n; i++) d[i * 4 + 3] = m[i];
  }
  g.putImageData(id, 0, 0);
  return { canvas: trim(cv, d, w, h), method };
}

// ---------------------------------------------------------------- AI cut-out

// U-2-Net (u2netp, Apache-2.0, public/models), run locally with
// onnxruntime-web. Loaded on first use only.
const SEG = 320;
let session: Promise<import("onnxruntime-web").InferenceSession> | null = null;
async function seg() {
  if (!session) {
    session = (async () => {
      const ort = await import("onnxruntime-web/wasm");
      ort.env.wasm.numThreads = 1; // no cross-origin isolation, so no threads
      return ort.InferenceSession.create("/models/u2netp.onnx", { executionProviders: ["wasm"] });
    })();
    session.catch(() => (session = null));
  }
  return session;
}

/** Foreground alpha (0..255) for every pixel of `cv`. */
async function segment(cv: HTMLCanvasElement): Promise<Uint8ClampedArray> {
  const ort = await import("onnxruntime-web/wasm");
  const s = await seg();
  const small = document.createElement("canvas");
  small.width = small.height = SEG;
  const sg = small.getContext("2d", { willReadFrequently: true })!;
  sg.imageSmoothingQuality = "high";
  sg.drawImage(cv, 0, 0, SEG, SEG);
  const px = sg.getImageData(0, 0, SEG, SEG).data;
  let max = 1;
  for (let i = 0; i < px.length; i++) if (i % 4 !== 3 && px[i] > max) max = px[i];
  const mean = [0.485, 0.456, 0.406], std = [0.229, 0.224, 0.225];
  const input = new Float32Array(3 * SEG * SEG);
  for (let i = 0; i < SEG * SEG; i++) for (let c = 0; c < 3; c++) input[c * SEG * SEG + i] = (px[i * 4 + c] / max - mean[c]) / std[c];
  const out = await s.run({ [s.inputNames[0]]: new ort.Tensor("float32", input, [1, 3, SEG, SEG]) });
  const raw = out[s.outputNames[0]].data as Float32Array;
  let lo = Infinity, hi = -Infinity;
  for (let i = 0; i < SEG * SEG; i++) { if (raw[i] < lo) lo = raw[i]; if (raw[i] > hi) hi = raw[i]; }
  // Normalise, then a soft threshold that drops the faint haze around the subject.
  const mask = sg.createImageData(SEG, SEG);
  for (let i = 0; i < SEG * SEG; i++) {
    const v = (raw[i] - lo) / Math.max(hi - lo, 1e-6);
    const a = Math.min(1, Math.max(0, (v - 0.15) / 0.5));
    mask.data[i * 4 + 3] = Math.round(a * a * (3 - 2 * a) * 255);
  }
  sg.putImageData(mask, 0, 0);
  const big = document.createElement("canvas");
  big.width = cv.width; big.height = cv.height;
  const bg = big.getContext("2d", { willReadFrequently: true })!;
  bg.imageSmoothingQuality = "high";
  bg.drawImage(small, 0, 0, cv.width, cv.height);
  const up = bg.getImageData(0, 0, cv.width, cv.height).data;
  const res = new Uint8ClampedArray(cv.width * cv.height);
  for (let i = 0; i < res.length; i++) res[i] = up[i * 4 + 3];
  return res;
}

/** Key out a plain one-colour background. False when the background is busy. */
function keyBackground(d: Uint8ClampedArray, w: number, h: number): boolean {
  // Background colour = median of a thin border.
  const border: number[][] = [];
  const add = (x: number, y: number) => { const i = (y * w + x) * 4; border.push([d[i] / 255, d[i + 1] / 255, d[i + 2] / 255]); };
  for (let b = 0; b < 3; b++) {
    for (let x = 0; x < w; x++) { add(x, b); add(x, h - 1 - b); }
    for (let y = 0; y < h; y++) { add(b, y); add(w - 1 - b, y); }
  }
  const med = [0, 1, 2].map((c) => { const v = border.map((p) => p[c]).sort((a, b) => a - b); return v[v.length >> 1]; });
  const dist = (r: number, g: number, b: number) => Math.hypot(r - med[0], g - med[1], b - med[2]);
  const spread = border.map((p) => dist(p[0], p[1], p[2])).sort((a, b) => a - b)[Math.floor(border.length * 0.85)];
  if (spread > 0.12) return false;
  const n = w * h;

  if (med[1] - Math.max(med[0], med[2]) > 0.3) {
    // Green screen: key by greenness everywhere (gaps between arms and body too).
    for (let i = 0; i < n; i++) {
      const r = d[i * 4] / 255, g = d[i * 4 + 1] / 255, b = d[i * 4 + 2] / 255;
      const green = g - Math.max(r, b);
      const a = Math.min(1, Math.max(0, 1 - (green - 0.08) / 0.2));
      if (green > 0) d[i * 4 + 1] = Math.round((Math.max(r, b) + Math.min(green, 0.06)) * 255);
      d[i * 4 + 3] = a < 0.08 ? 0 : Math.round(a * 255);
    }
    return true;
  }

  // Plain colour: flood fill from the edges so the same colour inside the
  // character survives, then a soft edge with the background un-blended.
  const lo = 0.07, hi = 0.22;
  const dd = new Float32Array(n);
  for (let i = 0; i < n; i++) dd[i] = dist(d[i * 4] / 255, d[i * 4 + 1] / 255, d[i * 4 + 2] / 255);
  const region = new Uint8Array(n);
  const queue = new Int32Array(n);
  let qh = 0, qt = 0;
  const seed = (i: number) => { if (!region[i] && dd[i] < hi) { region[i] = 1; queue[qt++] = i; } };
  for (let x = 0; x < w; x++) { seed(x); seed((h - 1) * w + x); }
  for (let y = 0; y < h; y++) { seed(y * w); seed(y * w + w - 1); }
  while (qh < qt) {
    const i = queue[qh++], x = i % w;
    if (x > 0) seed(i - 1);
    if (x < w - 1) seed(i + 1);
    if (i >= w) seed(i - w);
    if (i < n - w) seed(i + w);
  }
  // Pixels within 2 px of the background region get a soft alpha.
  let near = region.slice();
  for (let r = 0; r < 2; r++) {
    const nx = near.slice();
    for (let i = 0; i < n; i++) {
      if (near[i]) continue;
      const x = i % w;
      if ((x > 0 && near[i - 1]) || (x < w - 1 && near[i + 1]) || (i >= w && near[i - w]) || (i < n - w && near[i + w])) nx[i] = 1;
    }
    near = nx;
  }
  for (let i = 0; i < n; i++) {
    if (!near[i]) continue;
    const a = Math.min(1, Math.max(0, (dd[i] - lo) / (hi - lo)));
    if (a <= 0) { d[i * 4 + 3] = 0; continue; }
    for (let c = 0; c < 3; c++) {
      const v = (d[i * 4 + c] / 255 - (1 - a) * med[c]) / a;
      d[i * 4 + c] = Math.round(Math.min(1, Math.max(0, v)) * 255);
    }
    d[i * 4 + 3] = Math.round(a * 255);
  }
  return true;
}

function trim(cv: HTMLCanvasElement, d: Uint8ClampedArray, w: number, h: number): HTMLCanvasElement {
  let x0 = w, y0 = h, x1 = -1, y1 = -1;
  for (let y = 0; y < h; y++) for (let x = 0; x < w; x++) {
    if (d[(y * w + x) * 4 + 3] > 25) { if (x < x0) x0 = x; if (x > x1) x1 = x; if (y < y0) y0 = y; if (y > y1) y1 = y; }
  }
  if (x1 - x0 < 8 || y1 - y0 < 8) throw new CutoutError("empty");
  const out = document.createElement("canvas");
  out.width = x1 - x0 + 1; out.height = y1 - y0 + 1;
  out.getContext("2d")!.drawImage(cv, -x0, -y0);
  return out;
}

// ---------------------------------------------------------------- poses

/** action: [frames, fps, loop] */
export const PLAN: Record<string, [number, number, boolean]> = {
  idle: [8, 6, true], thinking: [8, 5, true], tool: [6, 9, true], attention: [6, 8, true],
  done: [10, 12, false], error: [6, 5, true], sleep: [4, 2.5, true], wake: [4, 6, false],
  squish: [4, 12, false], petted: [6, 6, true], annoyed: [4, 8, false], walk: [6, 10, true],
  fall: [2, 6, true], dizzy: [6, 6, true], cheer: [6, 8, true], sulk: [4, 3, true],
};

type Fx = "dots" | "spark" | "bang" | "sweat" | "zzz" | "hearts" | "vein" | "stars" | "confetti" | null;
interface Pose { sx: number; sy: number; rot: number; dx: number; lift: number; dark: number; fx: Fx; k: number }
const TAU = Math.PI * 2;

function pose(action: string, i: number, n: number): Pose {
  const p = i / n, s = Math.sin(TAU * p);
  const P: Pose = { sx: 1, sy: 1, rot: 0, dx: 0, lift: 0, dark: 0, fx: null, k: p };
  const set = (o: Partial<Pose>) => Object.assign(P, o);
  switch (action) {
    case "idle": set({ sy: 1 + 0.03 * s, sx: 1 - 0.015 * s }); break;
    case "thinking": set({ rot: 6 * s, sy: 1 + 0.015 * Math.cos(TAU * p), fx: "dots" }); break;
    case "tool": { const h = Math.abs(s); set({ lift: 0.035 * h, sy: 1 - 0.05 * (1 - h) + 0.03 * h, sx: 1 + 0.03 * (1 - h), rot: 2.5 * Math.sin(2 * TAU * p), fx: "spark" }); break; }
    case "attention": { const h = Math.max(0, s), g = h === 0 ? 1 : 0; set({ lift: 0.1 * h, sy: 1 + 0.05 * h - 0.06 * g, sx: 1 - 0.03 * h + 0.04 * g, rot: 5 * Math.sin(2 * TAU * p), fx: "bang" }); break; }
    case "done": {
      const keys = [[0, 0.92, 1.06, 0], [0, 0.84, 1.12, 0], [0.1, 1.12, 0.92, -4], [0.17, 1.06, 0.96, -2], [0.19, 1, 1, 0],
        [0.16, 1.02, 0.98, 2], [0.07, 1.06, 0.96, 3], [0, 0.86, 1.12, 0], [0, 1.04, 0.98, 0], [0, 1, 1, 0]];
      const [lift, sy, sx, rot] = keys[Math.min(i, keys.length - 1)];
      set({ lift, sy, sx, rot, fx: i >= 2 && i <= 8 ? "confetti" : null }); break;
    }
    case "error": set({ sy: 0.93, sx: 1.03, dx: 0.012 * Math.sin(2 * TAU * p), rot: -3, dark: 0.12, fx: "sweat" }); break;
    case "sleep": set({ rot: -9, sy: 0.9 + 0.02 * s, sx: 1.04, dark: 0.06, fx: "zzz" }); break;
    case "wake": { const k = (i + 1) / n; set({ rot: -9 * (1 - k), sy: 0.9 + 0.1 * k, sx: 1.04 - 0.04 * k, dark: 0.06 * (1 - k) }); break; }
    case "squish": { const [sy, sx] = [[0.68, 1.26], [0.84, 1.13], [1.07, 0.95], [1, 1]][i]; set({ sy, sx }); break; }
    case "petted": set({ rot: 4 * s, sy: 1.02 + 0.02 * Math.cos(TAU * p), fx: "hearts" }); break;
    case "annoyed": set({ dx: [0.03, -0.03, 0.02, 0][i], sy: 0.97, fx: "vein" }); break;
    case "walk": set({ lift: 0.03 * Math.abs(s), rot: 5 * s }); break;
    case "fall": set({ sy: 1.08, sx: 0.94, rot: i ? 3 : -3 }); break;
    case "dizzy": set({ rot: 8 * s, dx: 0.01 * Math.cos(TAU * p), fx: "stars" }); break;
    case "cheer": { const h = Math.abs(s); set({ lift: 0.08 * h, sy: 1 + 0.04 * h - 0.04 * (1 - h), sx: 1 - 0.02 * h + 0.03 * (1 - h), rot: 4 * Math.sin(2 * TAU * p), fx: "confetti" }); break; }
    case "sulk": set({ sy: 0.95, rot: -4 + 1.5 * s, dark: 0.08 }); break;
  }
  return P;
}

// ---------------------------------------------------------------- effects

const INK = "#28181a";
function star(g: CanvasRenderingContext2D, x: number, y: number, r: number, fill: string, lw: number) {
  g.beginPath();
  for (let j = 0; j < 10; j++) {
    const a = -Math.PI / 2 + (j * Math.PI) / 5, rr = j % 2 ? r * 0.45 : r;
    g.lineTo(x + rr * Math.cos(a), y + rr * Math.sin(a));
  }
  g.closePath(); g.fillStyle = fill; g.fill(); g.lineWidth = lw; g.strokeStyle = INK; g.stroke();
}
function heart(g: CanvasRenderingContext2D, x: number, y: number, r: number, lw: number) {
  g.beginPath();
  for (let j = 0; j < 40; j++) {
    const t = (TAU * j) / 40;
    g.lineTo(x + (16 * Math.sin(t) ** 3 * r) / 16, y - ((13 * Math.cos(t) - 5 * Math.cos(2 * t) - 2 * Math.cos(3 * t) - Math.cos(4 * t)) * r) / 16);
  }
  g.closePath(); g.fillStyle = "#ff698c"; g.fill(); g.lineWidth = lw; g.strokeStyle = INK; g.stroke();
}
// Small deterministic random for confetti.
const rand = (seed: number) => { const x = Math.sin(seed * 127.1 + 311.7) * 43758.5453; return x - Math.floor(x); };

function drawFx(g: CanvasRenderingContext2D, P: Pose, hx: number, top: number, hw: number, M: number) {
  const u = M / 128, lw = Math.max(2, 1.6 * u), k = P.k, rx = hx + hw * 0.42;
  g.save();
  g.lineJoin = "round"; g.lineCap = "round";
  switch (P.fx) {
    case "dots": {
      const cnt = Math.floor((k * 8) / 2) % 4;
      for (let j = 0; j < cnt; j++) {
        g.beginPath(); g.arc(rx + j * 7 * u, top + 6 * u - j * 3 * u, 3 * u, 0, TAU);
        g.fillStyle = "#fff"; g.fill(); g.lineWidth = lw; g.strokeStyle = INK; g.stroke();
      }
      break;
    }
    case "spark":
      [[-0.55, 0.55], [0.55, 0.45]].forEach(([ox, oy], j) => { if ((Math.floor(k * 6) + j) % 2 === 0) star(g, hx + ox * hw, top + oy * hw, 6 * u, "#ffd666", lw); });
      break;
    case "bang": {
      const sc = 1 + 0.15 * Math.sin(TAU * k * 2), bw = 5 * u * sc, bh = 14 * u * sc, x = rx, y = top + 2 * u;
      g.fillStyle = "#ffc430"; g.strokeStyle = INK; g.lineWidth = lw;
      g.beginPath(); g.roundRect(x - bw, y - bh, bw * 2, bh * 1.35, bw); g.fill(); g.stroke();
      g.beginPath(); g.arc(x, y + bh * 0.55 + bw, bw, 0, TAU); g.fill(); g.stroke();
      break;
    }
    case "sweat": {
      const y = top + 8 * u + 6 * u * k, x = rx - 2 * u, r = 4.5 * u;
      g.fillStyle = "#82c8ff";
      g.beginPath(); g.moveTo(x, y - 2.2 * r); g.lineTo(x - r * 0.9, y - 0.3 * r); g.lineTo(x + r * 0.9, y - 0.3 * r); g.fill();
      g.beginPath(); g.arc(x, y, r, 0, TAU); g.fill(); g.lineWidth = lw * 0.7; g.strokeStyle = "#285a96"; g.stroke();
      break;
    }
    case "zzz": {
      const cnt = (Math.floor(k * 4) % 4) + 1;
      for (let j = 0; j < cnt; j++) {
        const s = (5 + j * 2.5) * u, x = rx + j * 8 * u, y = top + 4 * u - j * 10 * u;
        g.beginPath(); g.moveTo(x - s / 2, y - s / 2); g.lineTo(x + s / 2, y - s / 2); g.lineTo(x - s / 2, y + s / 2); g.lineTo(x + s / 2, y + s / 2);
        g.lineWidth = lw * 2.4; g.strokeStyle = "#fff"; g.stroke();
        g.lineWidth = lw; g.strokeStyle = "#5a6ec8"; g.stroke();
      }
      break;
    }
    case "hearts":
      for (let j = 0; j < 2; j++) {
        const q = (k + j * 0.5) % 1;
        if (q < 0.85) heart(g, hx + (-0.35 + 0.7 * j) * hw + 3 * u * Math.sin(TAU * q), top + 10 * u - 22 * u * q, 6 * u, lw);
      }
      break;
    case "vein": {
      const x = rx, y = top + 6 * u, r = 5 * u;
      g.strokeStyle = "#e63232"; g.lineWidth = lw + 1;
      for (let a = 0; a < 4; a++) {
        const ang = (a * Math.PI) / 2 + Math.PI / 4;
        g.beginPath(); g.arc(x + r * Math.cos(ang), y + r * Math.sin(ang), r, ang + Math.PI * 0.55, ang + Math.PI * 1.45); g.stroke();
      }
      break;
    }
    case "stars":
      for (let j = 0; j < 3; j++) { const a = TAU * (k + j / 3); star(g, hx + 0.32 * hw * Math.cos(a), top - 4 * u + 4 * u * Math.sin(a), 5 * u, "#ffdc5a", lw); }
      break;
    case "confetti": {
      const cols = ["#ff6384", "#ffcd56", "#4bc0c0", "#9966ff", "#ff9f40"];
      for (let j = 0; j < 9; j++) {
        const seed = Math.floor(k * 1000) + j * 13;
        const x = hx + (0.35 + 0.45 * rand(seed)) * hw * (j % 2 ? 1 : -1), y = top - 6 * u + rand(seed + 5) * hw * 0.7;
        if (j % 3 === 0) star(g, x, y, 4 * u, cols[j % 5], lw * 0.7);
        else { g.fillStyle = cols[j % 5]; g.fillRect(x - 2 * u, y - 1.2 * u, 4 * u, 2.4 * u); }
      }
      break;
    }
  }
  g.restore();
}

// ---------------------------------------------------------------- render

export interface Built { strips: Record<string, HTMLCanvasElement>; size: number }

/** Frames for every action in PLAN, as one horizontal strip per action. */
export function render(sprite: HTMLCanvasElement, size = 128): Built {
  const M = size * 4;
  const k = Math.min((0.76 * M) / sprite.height, (0.8 * M) / sprite.width);
  const sw = sprite.width * k, sh = sprite.height * k;
  const px = M / 2, py = M - 0.02 * M;
  const big = document.createElement("canvas"); big.width = big.height = M;
  const g = big.getContext("2d")!;
  const half = document.createElement("canvas"); half.width = half.height = M / 2;
  const hg = half.getContext("2d")!;
  const strips: Record<string, HTMLCanvasElement> = {};
  for (const [action, [n]] of Object.entries(PLAN)) {
    const strip = document.createElement("canvas");
    strip.width = size * n; strip.height = size;
    const sg = strip.getContext("2d")!;
    for (let i = 0; i < n; i++) {
      const P = pose(action, i, n);
      const tx = px + P.dx * sh, ty = py - P.lift * sh;
      g.setTransform(1, 0, 0, 1, 0, 0);
      g.clearRect(0, 0, M, M);
      g.imageSmoothingQuality = "high";
      g.filter = P.dark ? `brightness(${1 - P.dark})` : "none";
      g.translate(tx, ty); g.rotate((P.rot * Math.PI) / 180); g.scale(P.sx, P.sy);
      g.drawImage(sprite, -sw / 2, -sh, sw, sh);
      g.setTransform(1, 0, 0, 1, 0, 0);
      g.filter = "none";
      if (P.fx) drawFx(g, P, tx, ty - sh * P.sy, sw * P.sx, M);
      // Two halving steps keep the downscale smooth.
      hg.clearRect(0, 0, M / 2, M / 2);
      hg.imageSmoothingQuality = "high";
      hg.drawImage(big, 0, 0, M / 2, M / 2);
      sg.imageSmoothingQuality = "high";
      sg.drawImage(half, i * size, 0, size, size);
    }
    strips[action] = strip;
  }
  return { strips, size };
}

export interface PackMeta {
  id: string; name: string; name_en?: string; scale: number;
  bubbles: Record<string, string[]>; bubbles_en?: Record<string, string[]>;
}

/** pet.json for a built pack. `sounds` lists the sound files that will be written. */
export function manifest(b: Built, m: PackMeta, sounds: string[]): object {
  const actions: Record<string, object> = {};
  for (const [action, [n, fps, loop]] of Object.entries(PLAN)) {
    const a: Record<string, unknown> = { sheet: `sprites/${action}.png`, frames: n, fps, loop };
    if (!loop) a.then = "idle";
    if (["attention", "done", "error"].includes(action) && sounds.includes(`${action}.wav`)) a.sound = `sounds/${action}.wav`;
    actions[action] = a;
  }
  actions.stone = { fallback: "error", tint: "#9aa0a6" };
  actions.aborted = { fallback: "error" };
  const out: Record<string, unknown> = {
    schema: 1, id: m.id, name: m.name, version: "0.1.0", author: "Familiar",
    description: "用一张图片在图鉴里做的宠物", description_en: "Made from one picture in the gallery",
    frame: { w: b.size, h: b.size, scale: m.scale }, actions, bubbles: m.bubbles,
    sounds: { squish: sounds.filter((s) => s.startsWith("squish")).map((s) => `sounds/${s}`) },
    tags: ["自制"],
  };
  if (m.name_en) out.name_en = m.name_en;
  if (m.bubbles_en) out.bubbles_en = m.bubbles_en;
  return out;
}

export const canvasPng = (c: HTMLCanvasElement): Promise<Uint8Array> =>
  new Promise((res, rej) => c.toBlob((b) => (b ? b.arrayBuffer().then((a) => res(new Uint8Array(a)), rej) : rej(new Error("toBlob"))), "image/png"));
