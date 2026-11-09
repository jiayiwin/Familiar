// Cosmetics drawn in code so they fit any pack: the season crown / loser hat
// and everything from the shop. Hats take the head (`cx` / `top` = top-centre
// of the head, `w` its width); the rest of the outfit takes a `Fit`, the
// frame's landmarks (head, neck, feet, box) mapped to canvas px.

import type { Body } from "./sprites";
import { t } from "./i18n";

export const HAT_NAMES: Record<string, string> = {
  get crown() { return t("赛季皇冠", "Season crown"); },
  get loser_hat() { return t("纸帽", "Paper hat"); },
  get flower() { return t("小红花", "Red flower"); },
  get bow() { return t("蝴蝶结", "Bow"); },
  get straw_hat() { return t("草帽", "Straw hat"); },
  get party_hat() { return t("派对帽", "Party hat"); },
  get beanie() { return t("毛线帽", "Beanie"); },
  get wizard_hat() { return t("法师帽", "Wizard hat"); },
  get halo() { return t("光环", "Halo"); },
};

/** Slots a pet wears things in, in the order the shop lists them. */
export const SLOTS = ["hat", "face", "neck", "back", "feet", "hand", "aura"] as const;
export type Slot = (typeof SLOTS)[number];

export const SLOT_NAMES: Record<string, () => string> = {
  hat: () => t("帽子", "Hats"),
  face: () => t("脸上", "Face"),
  neck: () => t("脖子", "Neck"),
  back: () => t("背后", "Back"),
  feet: () => t("脚上", "Feet"),
  hand: () => t("手里", "In hand"),
  aura: () => t("光效", "Aura"),
  rope: () => t("拔河绳", "Rope"),
  quips: () => t("台词", "Lines"),
};

/** Items that move on their own, so the window keeps redrawing while worn. */
const ANIMATED = new Set([
  "crown", "halo", "headphones",
  "cape_red", "wings_angel", "wings_bat", "jetpack",
  "balloon", "flag", "coffee", "wand",
  "hearts", "sparkles", "rain_cloud", "fireflies", "flames", "rainbow", "bell_collar",
]);

export function animatedOutfit(hat: string | undefined, wear: Record<string, string> | undefined): boolean {
  if (hat && ANIMATED.has(hat)) return true;
  return !!wear && Object.values(wear).some((id) => ANIMATED.has(id));
}

const OUT = "#3a2418";

function outline(c: CanvasRenderingContext2D, w: number) {
  c.lineWidth = Math.max(1.5, w * 0.05);
  c.strokeStyle = OUT;
  c.stroke();
}

// ------------------------------------------------------------------- hats

export function drawHat(c: CanvasRenderingContext2D, id: string, cx: number, top: number, w: number, now = 0): void {
  const u = Math.max(8, w); // head width
  c.save();
  c.lineJoin = "round";
  switch (id) {
    case "crown": {
      const hw = u * 0.42, h = u * 0.32, y = top + u * 0.06;
      c.beginPath();
      c.moveTo(cx - hw, y);
      c.lineTo(cx - hw, y - h * 0.55);
      c.lineTo(cx - hw * 0.5, y - h * 0.25);
      c.lineTo(cx, y - h);
      c.lineTo(cx + hw * 0.5, y - h * 0.25);
      c.lineTo(cx + hw, y - h * 0.55);
      c.lineTo(cx + hw, y);
      c.closePath();
      c.fillStyle = "#f4c430";
      c.fill();
      outline(c, u);
      for (const [dx, col] of [[-hw * 0.5, "#e63946"], [0, "#3f8efc"], [hw * 0.5, "#2a9d8f"]] as const) {
        c.beginPath();
        c.arc(cx + dx, y - h * 0.18, u * 0.045, 0, Math.PI * 2);
        c.fillStyle = col;
        c.fill();
      }
      // Sparkle so champions stand out.
      const t = (now / 400) % (Math.PI * 2);
      c.globalAlpha = 0.5 + 0.5 * Math.sin(t);
      c.fillStyle = "#fff6c2";
      c.fillRect(cx + hw * 0.7, y - h * 1.05, u * 0.05, u * 0.05);
      break;
    }
    case "loser_hat": {
      // Dunce cap, tilted.
      c.translate(cx, top + u * 0.05);
      c.rotate(0.25);
      c.beginPath();
      c.moveTo(-u * 0.2, 0);
      c.lineTo(u * 0.02, -u * 0.55);
      c.lineTo(u * 0.22, 0);
      c.closePath();
      c.fillStyle = "#e9e4da";
      c.fill();
      outline(c, u);
      c.fillStyle = OUT;
      c.font = `bold ${Math.round(u * 0.14)}px system-ui, sans-serif`;
      c.textAlign = "center";
      c.fillText(t("输", "L"), u * 0.01, -u * 0.12);
      break;
    }
    case "flower": {
      const x = cx + u * 0.28, y = top + u * 0.08, r = u * 0.08;
      for (let i = 0; i < 5; i++) {
        const a = (i / 5) * Math.PI * 2;
        c.beginPath();
        c.arc(x + Math.cos(a) * r, y + Math.sin(a) * r, r * 0.9, 0, Math.PI * 2);
        c.fillStyle = "#e63946";
        c.fill();
      }
      c.beginPath();
      c.arc(x, y, r * 0.7, 0, Math.PI * 2);
      c.fillStyle = "#ffd166";
      c.fill();
      break;
    }
    case "bow": {
      const x = cx, y = top + u * 0.04, s = u * 0.2;
      c.fillStyle = "#ff6fa8";
      for (const d of [-1, 1]) {
        c.beginPath();
        c.moveTo(x, y);
        c.lineTo(x + d * s, y - s * 0.6);
        c.lineTo(x + d * s, y + s * 0.6);
        c.closePath();
        c.fill();
        outline(c, u);
      }
      c.beginPath();
      c.arc(x, y, s * 0.28, 0, Math.PI * 2);
      c.fillStyle = "#ff3d8b";
      c.fill();
      outline(c, u);
      break;
    }
    case "straw_hat": {
      const y = top + u * 0.1;
      c.beginPath();
      c.ellipse(cx, y, u * 0.62, u * 0.13, 0, 0, Math.PI * 2);
      c.fillStyle = "#e9c46a";
      c.fill();
      outline(c, u);
      c.beginPath();
      c.ellipse(cx, y - u * 0.1, u * 0.32, u * 0.17, 0, Math.PI, 0);
      c.closePath();
      c.fillStyle = "#f2d57e";
      c.fill();
      outline(c, u);
      c.fillStyle = "#e63946";
      c.fillRect(cx - u * 0.32, y - u * 0.06, u * 0.64, u * 0.05);
      break;
    }
    case "party_hat": {
      const y = top + u * 0.05;
      c.beginPath();
      c.moveTo(cx - u * 0.2, y);
      c.lineTo(cx, y - u * 0.5);
      c.lineTo(cx + u * 0.2, y);
      c.closePath();
      c.fillStyle = "#7b6cd9";
      c.fill();
      outline(c, u);
      c.fillStyle = "#ffd166";
      for (let i = 1; i <= 3; i++) {
        c.beginPath();
        c.arc(cx + (i % 2 ? -1 : 1) * u * 0.05, y - u * 0.12 * i, u * 0.03, 0, Math.PI * 2);
        c.fill();
      }
      c.beginPath();
      c.arc(cx, y - u * 0.52, u * 0.06, 0, Math.PI * 2);
      c.fillStyle = "#ff6fa8";
      c.fill();
      break;
    }
    case "beanie": {
      const y = top + u * 0.14;
      c.beginPath();
      c.ellipse(cx, y, u * 0.4, u * 0.3, 0, Math.PI, 0);
      c.closePath();
      c.fillStyle = "#3f8efc";
      c.fill();
      outline(c, u);
      c.fillStyle = "#e8f0ff";
      c.fillRect(cx - u * 0.42, y - u * 0.04, u * 0.84, u * 0.1);
      c.beginPath();
      c.arc(cx, y - u * 0.32, u * 0.08, 0, Math.PI * 2);
      c.fillStyle = "#ffffff";
      c.fill();
      outline(c, u);
      break;
    }
    case "cat_ears": {
      for (const d of [-1, 1]) {
        const bx = cx + d * u * 0.28, by = top + u * 0.14;
        c.beginPath();
        c.moveTo(bx - u * 0.15, by);
        c.lineTo(bx + d * u * 0.06, by - u * 0.42);
        c.lineTo(bx + u * 0.15, by);
        c.closePath();
        c.fillStyle = "#f4a261";
        c.fill();
        outline(c, u);
        c.beginPath();
        c.moveTo(bx - u * 0.07, by - u * 0.02);
        c.lineTo(bx + d * u * 0.04, by - u * 0.26);
        c.lineTo(bx + u * 0.07, by - u * 0.02);
        c.closePath();
        c.fillStyle = "#ffb4c6";
        c.fill();
      }
      break;
    }
    case "chef_hat": {
      const y = top + u * 0.08;
      c.fillStyle = "#ffffff";
      for (const [dx, dy, r] of [[-0.22, -0.1, 0.17], [0.22, -0.1, 0.17], [0, -0.22, 0.21]] as const) {
        c.beginPath();
        c.arc(cx + dx * u, y + dy * u, r * u, 0, Math.PI * 2);
        c.fill();
        outline(c, u);
      }
      c.beginPath();
      c.roundRect(cx - u * 0.3, y - u * 0.04, u * 0.6, u * 0.16, u * 0.03);
      c.fill();
      outline(c, u);
      break;
    }
    case "headphones": {
      const y = top + u * 0.4;
      c.beginPath();
      c.ellipse(cx, y, u * 0.52, u * 0.5, 0, Math.PI, 0);
      c.lineWidth = Math.max(2, u * 0.07);
      c.strokeStyle = "#2b2d6e";
      c.stroke();
      c.lineJoin = "round";
      for (const d of [-1, 1]) {
        c.beginPath();
        c.roundRect(cx + d * u * 0.52 - u * 0.1, y - u * 0.14, u * 0.2, u * 0.3, u * 0.05);
        c.fillStyle = "#e63946";
        c.fill();
        outline(c, u);
      }
      // A little music.
      c.fillStyle = "#2b2d6e";
      c.globalAlpha = 0.5 + 0.5 * Math.sin(now / 350);
      c.font = `${Math.round(u * 0.22)}px system-ui, sans-serif`;
      c.textAlign = "center";
      c.fillText("♪", cx + u * 0.72, y - u * 0.3 - Math.sin(now / 500) * u * 0.05);
      break;
    }
    case "wizard_hat": {
      const y = top + u * 0.1;
      c.beginPath();
      c.ellipse(cx, y, u * 0.5, u * 0.1, 0, 0, Math.PI * 2);
      c.fillStyle = "#2b2d6e";
      c.fill();
      outline(c, u);
      c.beginPath();
      c.moveTo(cx - u * 0.26, y - u * 0.02);
      c.quadraticCurveTo(cx - u * 0.02, y - u * 0.5, cx + u * 0.22, y - u * 0.7);
      c.lineTo(cx + u * 0.26, y - u * 0.02);
      c.closePath();
      c.fill();
      outline(c, u);
      c.fillStyle = "#ffd166";
      c.font = `${Math.round(u * 0.16)}px system-ui, sans-serif`;
      c.textAlign = "center";
      c.fillText("★", cx, y - u * 0.18);
      break;
    }
    case "top_hat": {
      const y = top + u * 0.1;
      c.fillStyle = "#1f1f2e";
      c.beginPath();
      c.roundRect(cx - u * 0.3, y - u * 0.56, u * 0.6, u * 0.58, u * 0.04);
      c.fill();
      outline(c, u);
      c.beginPath();
      c.ellipse(cx, y, u * 0.5, u * 0.09, 0, 0, Math.PI * 2);
      c.fill();
      outline(c, u);
      c.fillStyle = "#e63946";
      c.fillRect(cx - u * 0.3, y - u * 0.16, u * 0.6, u * 0.1);
      break;
    }
    case "halo": {
      const y = top - u * 0.12 + Math.sin(now / 500) * u * 0.03;
      c.beginPath();
      c.ellipse(cx, y, u * 0.3, u * 0.08, 0, 0, Math.PI * 2);
      c.lineWidth = Math.max(2, u * 0.06);
      c.strokeStyle = "#ffd166";
      c.shadowColor = "#fff3b0";
      c.shadowBlur = u * 0.15;
      c.stroke();
      break;
    }
  }
  c.restore();
}

// ------------------------------------------------------------- the outfit

/** A frame's landmarks placed on a canvas: bottom-centre at (cx, bottom), scale `sc`. */
export interface Fit {
  cx: number;
  bottom: number;
  sc: number;
  fw: number;
  fh: number;
  b: Body;
}

export function fitOf(b: Body, fw: number, fh: number, cx: number, bottom: number, sc: number): Fit {
  return { cx, bottom, sc, fw, fh, b };
}

/** Canvas px of the landmarks that items hang on. */
function marks(fit: Fit) {
  const X = (fx: number) => fit.cx + (fx - fit.fw / 2) * fit.sc;
  const Y = (fy: number) => fit.bottom - (fit.fh - fy) * fit.sc;
  const b = fit.b;
  const u = Math.max(8, b.head.w * fit.sc);
  return {
    X, Y, u,
    hx: X(b.head.x), ht: Y(b.head.top),
    bx: X(b.cx), bl: X(b.left), br: X(b.right), bt: Y(b.top), bb: Y(b.bottom),
    bw: (b.right - b.left) * fit.sc, bh: (b.bottom - b.top) * fit.sc,
    nx: X(b.neck.x), ny: Y(b.neck.y), nw: Math.max(u * 0.5, b.neck.w * fit.sc),
    fy: Y(b.feet.y), fl: X(b.feet.l), fr: X(b.feet.r),
    // Eyes sit a little under the top of the head; the hand at the right flank.
    ey: Y(b.head.top) + u * 0.3,
    handX: X(b.right) - u * 0.08, handY: Y(b.neck.y) + (b.bottom - b.neck.y) * fit.sc * 0.25,
  };
}

type M = ReturnType<typeof marks>;

function heart(c: CanvasRenderingContext2D, x: number, y: number, s: number) {
  c.beginPath();
  c.moveTo(x, y + s * 0.6);
  c.bezierCurveTo(x - s, y - s * 0.2, x - s * 0.5, y - s, x, y - s * 0.35);
  c.bezierCurveTo(x + s * 0.5, y - s, x + s, y - s * 0.2, x, y + s * 0.6);
  c.closePath();
  c.fill();
}

function star(c: CanvasRenderingContext2D, x: number, y: number, r: number) {
  c.beginPath();
  for (let i = 0; i < 8; i++) {
    const a = (i / 8) * Math.PI * 2 - Math.PI / 2;
    const rr = i % 2 ? r * 0.35 : r;
    c.lineTo(x + Math.cos(a) * rr, y + Math.sin(a) * rr);
  }
  c.closePath();
  c.fill();
}

/** Seeded, smooth pseudo-random drift for floating things. */
const drift = (seed: number, t: number) => Math.sin(t * 0.9 + seed * 1.7) * 0.6 + Math.sin(t * 1.7 + seed * 3.1) * 0.4;

/** Items that go behind the sprite: wings, capes, packs and the big auras. */
export function drawWearBehind(c: CanvasRenderingContext2D, wear: Record<string, string> | undefined, fit: Fit, now = 0): void {
  if (!wear) return;
  const m = marks(fit);
  c.save();
  c.lineJoin = "round";
  if (wear.aura) drawAuraBehind(c, wear.aura, m, now);
  if (wear.back) drawBack(c, wear.back, m, now);
  c.restore();
}

/** Items that go over the sprite: feet, neck, face, hand and the small auras. */
export function drawWearFront(c: CanvasRenderingContext2D, wear: Record<string, string> | undefined, fit: Fit, now = 0): void {
  if (!wear) return;
  const m = marks(fit);
  c.save();
  c.lineJoin = "round";
  if (wear.back === "backpack") drawStraps(c, m);
  if (wear.feet) drawFeet(c, wear.feet, m);
  if (wear.neck) drawNeck(c, wear.neck, m, now);
  if (wear.face) drawFace(c, wear.face, m);
  if (wear.hand) drawHand(c, wear.hand, m, now);
  if (wear.aura) drawAuraFront(c, wear.aura, m, now);
  c.restore();
}

function drawFace(c: CanvasRenderingContext2D, id: string, m: M): void {
  const { u, hx, ey } = m;
  const dx = u * 0.2, r = u * 0.14;
  switch (id) {
    case "glasses_round":
      c.lineWidth = Math.max(1.2, u * 0.045);
      c.strokeStyle = "#2b2d6e";
      c.fillStyle = "rgba(220,235,255,0.35)";
      for (const d of [-1, 1]) {
        c.beginPath();
        c.arc(hx + d * dx, ey, r, 0, Math.PI * 2);
        c.fill();
        c.stroke();
      }
      c.beginPath();
      c.moveTo(hx - dx + r, ey);
      c.lineTo(hx + dx - r, ey);
      c.stroke();
      break;
    case "sunglasses":
      c.fillStyle = "#1f1f2e";
      for (const d of [-1, 1]) {
        c.beginPath();
        c.roundRect(hx + d * dx - r * 1.1, ey - r * 0.8, r * 2.2, r * 1.6, r * 0.5);
        c.fill();
      }
      c.fillRect(hx - dx + r, ey - u * 0.025, dx * 2 - r * 2, u * 0.05);
      c.fillStyle = "rgba(255,255,255,0.55)";
      for (const d of [-1, 1]) c.fillRect(hx + d * dx - r * 0.7, ey - r * 0.5, r * 0.5, r * 0.3);
      break;
    case "eyepatch":
      c.fillStyle = "#1f1f2e";
      c.beginPath();
      c.ellipse(hx + dx, ey, r * 1.05, r * 0.95, 0, 0, Math.PI * 2);
      c.fill();
      c.strokeStyle = "#1f1f2e";
      c.lineWidth = Math.max(1, u * 0.035);
      c.beginPath();
      c.moveTo(hx + dx - r, ey - r * 0.3);
      c.lineTo(hx - u * 0.52, ey - u * 0.3);
      c.moveTo(hx + dx + r * 0.6, ey - r * 0.7);
      c.lineTo(hx + u * 0.5, ey - u * 0.32);
      c.stroke();
      break;
    case "blush":
      c.fillStyle = "rgba(255,110,140,0.65)";
      for (const d of [-1, 1]) {
        c.beginPath();
        c.ellipse(hx + d * u * 0.3, ey + u * 0.2, u * 0.11, u * 0.07, 0, 0, Math.PI * 2);
        c.fill();
      }
      break;
    case "monocle": {
      c.lineWidth = Math.max(1.4, u * 0.05);
      c.strokeStyle = "#d4a017";
      c.fillStyle = "rgba(220,235,255,0.35)";
      c.beginPath();
      c.arc(hx + dx, ey, r * 1.1, 0, Math.PI * 2);
      c.fill();
      c.stroke();
      c.lineWidth = Math.max(1, u * 0.025);
      c.beginPath();
      c.moveTo(hx + dx + r * 0.9, ey + r * 0.6);
      c.quadraticCurveTo(hx + dx + r * 2.2, ey + u * 0.5, hx + dx + r * 1.2, m.ny + u * 0.1);
      c.stroke();
      break;
    }
  }
}

function drawNeck(c: CanvasRenderingContext2D, id: string, m: M, now: number): void {
  const { u, nx, ny, nw } = m;
  const h = u * 0.16;
  switch (id) {
    case "scarf_red": {
      c.fillStyle = "#e63946";
      c.beginPath();
      c.roundRect(nx - nw * 0.6, ny - h * 0.4, nw * 1.2, h, h * 0.4);
      c.fill();
      outline(c, u);
      // The tail hangs off one side and sways.
      const sway = Math.sin(now / 600) * u * 0.04;
      c.beginPath();
      c.moveTo(nx + nw * 0.25, ny + h * 0.5);
      c.lineTo(nx + nw * 0.55 + sway, ny + h * 0.5);
      c.lineTo(nx + nw * 0.5 + sway * 2, ny + h * 0.5 + u * 0.45);
      c.lineTo(nx + nw * 0.2 + sway * 2, ny + h * 0.5 + u * 0.42);
      c.closePath();
      c.fill();
      outline(c, u);
      c.fillStyle = "#ffd166";
      for (let i = 0; i < 3; i++) c.fillRect(nx + nw * 0.22 + sway * 2 + i * u * 0.09, ny + h * 0.5 + u * 0.38, u * 0.04, u * 0.08);
      break;
    }
    case "bowtie": {
      const s = u * 0.16;
      c.fillStyle = "#2b2d6e";
      for (const d of [-1, 1]) {
        c.beginPath();
        c.moveTo(nx, ny);
        c.lineTo(nx + d * s, ny - s * 0.55);
        c.lineTo(nx + d * s, ny + s * 0.55);
        c.closePath();
        c.fill();
        outline(c, u);
      }
      c.fillStyle = "#3f8efc";
      c.beginPath();
      c.arc(nx, ny, s * 0.25, 0, Math.PI * 2);
      c.fill();
      break;
    }
    case "tie": {
      const len = Math.min(u * 0.75, (m.bb - ny) * 0.55);
      c.fillStyle = "#e63946";
      c.beginPath();
      c.moveTo(nx - u * 0.07, ny);
      c.lineTo(nx + u * 0.07, ny);
      c.lineTo(nx + u * 0.1, ny + len * 0.8);
      c.lineTo(nx, ny + len);
      c.lineTo(nx - u * 0.1, ny + len * 0.8);
      c.closePath();
      c.fill();
      outline(c, u);
      c.fillStyle = "#ffd166";
      for (let i = 1; i <= 2; i++) c.fillRect(nx - u * 0.08, ny + len * 0.3 * i, u * 0.16, u * 0.03);
      break;
    }
    case "bell_collar": {
      c.fillStyle = "#b5362f";
      c.beginPath();
      c.roundRect(nx - nw * 0.55, ny - h * 0.3, nw * 1.1, h * 0.7, h * 0.3);
      c.fill();
      outline(c, u);
      const swing = Math.sin(now / 420) * u * 0.02;
      c.fillStyle = "#f4c430";
      c.beginPath();
      c.arc(nx + swing, ny + h * 0.75, u * 0.09, 0, Math.PI * 2);
      c.fill();
      outline(c, u);
      c.fillStyle = OUT;
      c.fillRect(nx + swing - u * 0.012, ny + h * 0.75, u * 0.024, u * 0.07);
      break;
    }
    case "pearls": {
      c.fillStyle = "#fff7ea";
      const n = 7, r = u * 0.045;
      for (let i = 0; i < n; i++) {
        const k = i / (n - 1) - 0.5;
        const x = nx + k * nw * 1.05;
        const y = ny + h * 0.2 + (1 - (k * 2) ** 2) * u * 0.14;
        c.beginPath();
        c.arc(x, y, r, 0, Math.PI * 2);
        c.fill();
        c.lineWidth = 1;
        c.strokeStyle = "#c9b8a0";
        c.stroke();
      }
      break;
    }
  }
}

function drawBack(c: CanvasRenderingContext2D, id: string, m: M, now: number): void {
  const { u, bx, bt, bb, bh, bw, nx, ny } = m;
  const back = -1; // the pet faces the viewer; "behind" leans to its left side
  switch (id) {
    case "cape_red": {
      const w = Math.sin(now / 700) * u * 0.08;
      const top = ny - u * 0.05;
      c.fillStyle = "#c1272d";
      c.beginPath();
      c.moveTo(nx - bw * 0.45, top);
      c.lineTo(nx + bw * 0.45, top);
      c.quadraticCurveTo(nx + bw * 0.7 + w, (top + bb) / 2, nx + bw * 0.62 + w * 2, bb - u * 0.05);
      c.lineTo(nx - bw * 0.62 + w * 2, bb - u * 0.05);
      c.quadraticCurveTo(nx - bw * 0.7 + w, (top + bb) / 2, nx - bw * 0.45, top);
      c.closePath();
      c.fill();
      outline(c, u);
      c.fillStyle = "#ffd166";
      c.beginPath();
      c.arc(nx, top + u * 0.03, u * 0.06, 0, Math.PI * 2);
      c.fill();
      break;
    }
    case "wings_angel":
    case "wings_bat": {
      const flap = Math.sin(now / 320) * 0.18;
      const span = Math.max(u * 0.9, bw * 0.95);
      const y0 = ny + bh * 0.12;
      for (const d of [-1, 1]) {
        c.save();
        c.translate(bx + d * bw * 0.25, y0);
        c.rotate(-d * flap);
        c.beginPath();
        if (id === "wings_angel") {
          c.moveTo(0, 0);
          c.quadraticCurveTo(d * span * 0.5, -span * 0.75, d * span, -span * 0.55);
          c.quadraticCurveTo(d * span * 0.7, -span * 0.2, d * span * 0.72, span * 0.05);
          c.quadraticCurveTo(d * span * 0.45, span * 0.1, d * span * 0.4, span * 0.3);
          c.quadraticCurveTo(d * span * 0.2, span * 0.15, 0, span * 0.12);
          c.closePath();
          c.fillStyle = "#fffaf0";
          c.fill();
          c.lineWidth = Math.max(1.2, u * 0.04);
          c.strokeStyle = "#cfc3ad";
          c.stroke();
          c.beginPath();
          c.moveTo(d * span * 0.15, span * 0.02);
          c.quadraticCurveTo(d * span * 0.5, -span * 0.3, d * span * 0.8, -span * 0.4);
          c.stroke();
        } else {
          c.moveTo(0, 0);
          c.quadraticCurveTo(d * span * 0.4, -span * 0.7, d * span, -span * 0.6);
          c.lineTo(d * span * 0.85, -span * 0.22);
          c.lineTo(d * span * 0.95, span * 0.05);
          c.lineTo(d * span * 0.6, -span * 0.05);
          c.lineTo(d * span * 0.62, span * 0.25);
          c.lineTo(d * span * 0.3, span * 0.08);
          c.lineTo(d * span * 0.2, span * 0.3);
          c.closePath();
          c.fillStyle = "#4b2a6b";
          c.fill();
          outline(c, u);
          c.strokeStyle = "#6e4594";
          c.lineWidth = Math.max(1, u * 0.03);
          for (const k of [0.35, 0.65]) {
            c.beginPath();
            c.moveTo(0, 0);
            c.lineTo(d * span * k, -span * (0.1 + k * 0.4));
            c.stroke();
          }
        }
        c.restore();
      }
      break;
    }
    case "backpack": {
      const w = bw * 0.62, h = bh * 0.42;
      const x = bx + back * bw * 0.22 - w / 2, y = ny - u * 0.02;
      c.fillStyle = "#3f8efc";
      c.beginPath();
      c.roundRect(x, y, w, h, u * 0.12);
      c.fill();
      outline(c, u);
      c.fillStyle = "#2b6fd1";
      c.beginPath();
      c.roundRect(x + w * 0.18, y + h * 0.5, w * 0.64, h * 0.38, u * 0.08);
      c.fill();
      outline(c, u);
      c.fillStyle = "#ffd166";
      c.fillRect(x + w * 0.42, y + h * 0.5, w * 0.16, h * 0.1);
      break;
    }
    case "jetpack": {
      const w = bw * 0.26, h = bh * 0.5;
      const y = ny;
      for (const d of [-1, 1]) {
        const x = bx + d * bw * 0.32 - w / 2;
        c.fillStyle = "#9aa3ad";
        c.beginPath();
        c.roundRect(x, y, w, h, w * 0.4);
        c.fill();
        outline(c, u);
        c.fillStyle = "#e63946";
        c.fillRect(x + w * 0.2, y + h * 0.2, w * 0.6, h * 0.12);
        // Flame, flickering.
        const fl = (0.8 + 0.4 * Math.sin(now / 60 + d)) * u * 0.4;
        const g = c.createLinearGradient(0, y + h, 0, y + h + fl);
        g.addColorStop(0, "#ffd166");
        g.addColorStop(0.5, "#f4a261");
        g.addColorStop(1, "rgba(230,57,70,0)");
        c.fillStyle = g;
        c.beginPath();
        c.moveTo(x + w * 0.2, y + h);
        c.lineTo(x + w / 2, y + h + fl);
        c.lineTo(x + w * 0.8, y + h);
        c.closePath();
        c.fill();
      }
      // Fin connecting the two.
      c.fillStyle = "#6b7480";
      c.fillRect(bx - bw * 0.2, ny + bh * 0.1, bw * 0.4, u * 0.08);
      break;
    }
  }
  void bt;
}

/** The backpack's straps go over the shoulders, in front. */
function drawStraps(c: CanvasRenderingContext2D, m: M): void {
  const { u, nx, ny, nw, bb } = m;
  c.strokeStyle = "#2b6fd1";
  c.lineWidth = Math.max(1.5, u * 0.06);
  c.lineCap = "round";
  for (const d of [-1, 1]) {
    c.beginPath();
    c.moveTo(nx + d * nw * 0.32, ny - u * 0.02);
    c.lineTo(nx + d * nw * 0.26, ny + (bb - ny) * 0.4);
    c.stroke();
  }
}

function drawFeet(c: CanvasRenderingContext2D, id: string, m: M): void {
  const { u, fy, fl, fr } = m;
  const span = Math.max(u * 0.6, fr - fl);
  const fw = Math.min(u * 0.34, span * 0.42); // one foot
  const xs = [fl + span * 0.25, fl + span * 0.75];
  switch (id) {
    case "socks_stripe":
      xs.forEach((x, i) => {
        const h = u * 0.26;
        c.fillStyle = "#ffffff";
        c.beginPath();
        c.roundRect(x - fw / 2, fy - h, fw, h, u * 0.04);
        c.fill();
        c.save();
        c.clip();
        c.fillStyle = i ? "#2a9d8f" : "#e63946";
        for (let k = 0; k < 3; k++) c.fillRect(x - fw / 2, fy - h + k * h * 0.4, fw, h * 0.18);
        c.restore();
        outline(c, u);
      });
      break;
    case "sneakers":
      for (const x of xs) {
        const h = u * 0.2;
        c.fillStyle = "#ffffff";
        c.beginPath();
        c.roundRect(x - fw / 2, fy - h, fw * 1.1, h, [u * 0.05, u * 0.1, u * 0.03, u * 0.03]);
        c.fill();
        outline(c, u);
        c.fillStyle = "#3f8efc";
        c.fillRect(x - fw / 2, fy - h * 0.35, fw * 1.1, h * 0.2);
        c.fillStyle = "#e63946";
        c.fillRect(x - fw * 0.1, fy - h * 0.9, fw * 0.3, h * 0.12);
      }
      break;
    case "rain_boots":
      for (const x of xs) {
        const h = u * 0.4;
        c.fillStyle = "#f4c430";
        c.beginPath();
        c.roundRect(x - fw / 2, fy - h, fw, h, [u * 0.04, u * 0.04, u * 0.08, u * 0.04]);
        c.fill();
        outline(c, u);
        c.fillStyle = "#1f1f2e";
        c.fillRect(x - fw / 2, fy - h * 0.18, fw * 1.05, h * 0.18);
        c.fillStyle = "rgba(255,255,255,0.5)";
        c.fillRect(x - fw * 0.3, fy - h * 0.85, fw * 0.14, h * 0.5);
      }
      break;
    case "skates":
      for (const x of xs) {
        const h = u * 0.2;
        c.fillStyle = "#ff6fa8";
        c.beginPath();
        c.roundRect(x - fw / 2, fy - h - u * 0.1, fw * 1.05, h, u * 0.05);
        c.fill();
        outline(c, u);
        c.fillStyle = "#1f1f2e";
        for (const k of [0.25, 0.75]) {
          c.beginPath();
          c.arc(x - fw / 2 + fw * 1.05 * k, fy - u * 0.04, u * 0.055, 0, Math.PI * 2);
          c.fill();
        }
      }
      break;
  }
}

function drawHand(c: CanvasRenderingContext2D, id: string, m: M, now: number): void {
  const { u, handX, handY, hx, ht } = m;
  switch (id) {
    case "balloon": {
      const bob = Math.sin(now / 900) * u * 0.08;
      const bx = handX + u * 0.5 + Math.sin(now / 1300) * u * 0.1, by = ht - u * 0.55 + bob;
      c.strokeStyle = "#6b6b6b";
      c.lineWidth = 1;
      c.beginPath();
      c.moveTo(handX, handY);
      c.quadraticCurveTo((handX + bx) / 2 + u * 0.1, (handY + by) / 2, bx, by + u * 0.32);
      c.stroke();
      c.fillStyle = "#e63946";
      c.beginPath();
      c.ellipse(bx, by, u * 0.26, u * 0.32, 0, 0, Math.PI * 2);
      c.fill();
      outline(c, u);
      c.fillStyle = OUT;
      c.beginPath();
      c.moveTo(bx - u * 0.05, by + u * 0.36);
      c.lineTo(bx + u * 0.05, by + u * 0.36);
      c.lineTo(bx, by + u * 0.3);
      c.closePath();
      c.fill();
      c.fillStyle = "rgba(255,255,255,0.6)";
      c.beginPath();
      c.ellipse(bx - u * 0.09, by - u * 0.12, u * 0.06, u * 0.1, -0.4, 0, Math.PI * 2);
      c.fill();
      break;
    }
    case "lollipop": {
      c.strokeStyle = "#fff";
      c.lineWidth = Math.max(1.5, u * 0.05);
      c.beginPath();
      c.moveTo(handX, handY);
      c.lineTo(handX + u * 0.15, handY - u * 0.45);
      c.stroke();
      c.fillStyle = "#ff6fa8";
      c.beginPath();
      c.arc(handX + u * 0.17, handY - u * 0.58, u * 0.16, 0, Math.PI * 2);
      c.fill();
      outline(c, u);
      c.strokeStyle = "#fff";
      c.lineWidth = Math.max(1, u * 0.03);
      c.beginPath();
      for (let a = 0; a < Math.PI * 4; a += 0.3) {
        const r = (a / (Math.PI * 4)) * u * 0.12;
        c.lineTo(handX + u * 0.17 + Math.cos(a) * r, handY - u * 0.58 + Math.sin(a) * r);
      }
      c.stroke();
      break;
    }
    case "coffee": {
      const w = u * 0.26, h = u * 0.3;
      const x = handX - w * 0.2, y = handY - h * 0.6;
      c.fillStyle = "#fffaf0";
      c.beginPath();
      c.roundRect(x, y, w, h, [u * 0.03, u * 0.03, u * 0.08, u * 0.08]);
      c.fill();
      outline(c, u);
      c.fillStyle = "#e63946";
      c.fillRect(x, y + h * 0.35, w, h * 0.25);
      c.strokeStyle = OUT;
      c.lineWidth = Math.max(1.2, u * 0.04);
      c.beginPath();
      c.arc(x + w, y + h * 0.45, w * 0.3, -Math.PI / 2, Math.PI / 2);
      c.stroke();
      // Steam.
      c.strokeStyle = "rgba(120,120,120,0.55)";
      c.lineWidth = 1;
      for (const k of [0.3, 0.7]) {
        const ph = now / 500 + k * 4;
        c.beginPath();
        c.moveTo(x + w * k, y - u * 0.02);
        c.quadraticCurveTo(x + w * k + Math.sin(ph) * u * 0.08, y - u * 0.15, x + w * k + Math.sin(ph + 1) * u * 0.06, y - u * 0.3);
        c.stroke();
      }
      break;
    }
    case "flag": {
      c.strokeStyle = "#8d5a3b";
      c.lineWidth = Math.max(1.5, u * 0.05);
      c.beginPath();
      c.moveTo(handX, handY);
      c.lineTo(handX + u * 0.1, handY - u * 0.75);
      c.stroke();
      const wave = Math.sin(now / 250) * u * 0.06;
      c.fillStyle = "#ee5b3c";
      c.beginPath();
      c.moveTo(handX + u * 0.1, handY - u * 0.75);
      c.quadraticCurveTo(handX + u * 0.35, handY - u * 0.72 + wave, handX + u * 0.55, handY - u * 0.62 + wave);
      c.quadraticCurveTo(handX + u * 0.35, handY - u * 0.5 - wave, handX + u * 0.1, handY - u * 0.45);
      c.closePath();
      c.fill();
      outline(c, u);
      break;
    }
    case "wand": {
      c.strokeStyle = "#2b2d6e";
      c.lineWidth = Math.max(1.5, u * 0.05);
      c.beginPath();
      c.moveTo(handX, handY);
      c.lineTo(handX + u * 0.2, handY - u * 0.6);
      c.stroke();
      c.fillStyle = "#ffd166";
      star(c, handX + u * 0.22, handY - u * 0.68, u * 0.12);
      c.globalAlpha = 0.6 + 0.4 * Math.sin(now / 200);
      c.fillStyle = "#fff6c2";
      star(c, handX + u * 0.4, handY - u * 0.85, u * 0.05);
      star(c, handX + u * 0.05, handY - u * 0.9, u * 0.04);
      c.globalAlpha = 1;
      break;
    }
    case "umbrella": {
      const top = ht - u * 0.5, r = Math.max(u * 0.7, m.bw * 0.6);
      c.strokeStyle = "#8d5a3b";
      c.lineWidth = Math.max(1.5, u * 0.05);
      c.beginPath();
      c.moveTo(handX, handY);
      c.lineTo(hx, top + u * 0.05);
      c.stroke();
      c.fillStyle = "#3f8efc";
      c.beginPath();
      c.moveTo(hx - r, top + u * 0.15);
      c.quadraticCurveTo(hx, top - r * 0.55, hx + r, top + u * 0.15);
      for (let i = 3; i >= -3; i -= 2) c.quadraticCurveTo(hx + (i * r) / 3, top + u * 0.28, hx + ((i - 1) * r) / 3, top + u * 0.15);
      c.closePath();
      c.fill();
      outline(c, u);
      c.fillStyle = "#ffd166";
      for (const k of [-1, 1]) {
        c.beginPath();
        c.moveTo(hx + k * r * 0.33, top + u * 0.15);
        c.quadraticCurveTo(hx + k * r * 0.45, top - r * 0.3, hx + k * r * 0.66, top + u * 0.15);
        c.lineTo(hx + k * r * 0.33, top + u * 0.15);
        c.fill();
      }
      break;
    }
  }
}

function drawAuraBehind(c: CanvasRenderingContext2D, id: string, m: M, now: number): void {
  const { u, bx, bb, bw, bh, ht } = m;
  switch (id) {
    case "rainbow": {
      const r0 = Math.max(bw * 0.7, u * 0.9), bands = ["#e63946", "#f4a261", "#e9c46a", "#2a9d8f", "#3f8efc", "#7b6cd9"];
      const cy = bb - u * 0.1, step = u * 0.09;
      c.globalAlpha = 0.85 + 0.15 * Math.sin(now / 900);
      bands.forEach((col, i) => {
        c.strokeStyle = col;
        c.lineWidth = step;
        c.beginPath();
        c.arc(bx, cy, r0 + (bands.length - i) * step, Math.PI, 0);
        c.stroke();
      });
      c.globalAlpha = 1;
      break;
    }
    case "flames": {
      // A wall of fire wider than the pet, tallest at the sides where it shows.
      const n = 8, span = Math.max(bw * 1.7, u * 2.2);
      for (let i = 0; i < n; i++) {
        const k = (i + 0.5) / n;
        const x = bx - span / 2 + span * k;
        const side = 0.55 + 0.45 * Math.abs(k - 0.5) * 2;
        const h = (0.45 + 0.3 * Math.abs(Math.sin(now / 90 + i * 1.9))) * bh * side;
        const g = c.createLinearGradient(0, bb, 0, bb - h);
        g.addColorStop(0, "#ffd166");
        g.addColorStop(0.35, "#f4a261");
        g.addColorStop(0.75, "#e63946");
        g.addColorStop(1, "rgba(230,57,70,0)");
        c.fillStyle = g;
        c.beginPath();
        c.moveTo(x - span / n * 0.7, bb);
        c.quadraticCurveTo(x - span / n * 0.4, bb - h * 0.5, x + Math.sin(now / 130 + i) * u * 0.08, bb - h);
        c.quadraticCurveTo(x + span / n * 0.4, bb - h * 0.5, x + span / n * 0.7, bb);
        c.closePath();
        c.fill();
      }
      break;
    }
  }
  void ht;
}

function drawAuraFront(c: CanvasRenderingContext2D, id: string, m: M, now: number): void {
  const { u, bx, bb, bw, bh, hx, ht } = m;
  const t = now / 1000;
  switch (id) {
    case "hearts":
      c.fillStyle = "#ff6fa8";
      for (let i = 0; i < 3; i++) {
        const k = ((t * 0.5 + i / 3) % 1);
        const x = hx + (i - 1) * u * 0.35 + drift(i, t) * u * 0.1;
        const y = ht - u * 0.1 - k * u * 0.8;
        c.globalAlpha = (1 - k) * 0.9;
        heart(c, x, y, u * 0.09 * (0.7 + 0.3 * (1 - k)));
      }
      c.globalAlpha = 1;
      break;
    case "sparkles":
      for (let i = 0; i < 6; i++) {
        const a = (i / 6) * Math.PI * 2 + t * 0.3;
        const x = bx + Math.cos(a) * bw * 0.72, y = bb - bh * 0.5 + Math.sin(a) * bh * 0.5;
        const tw = 0.5 + 0.5 * Math.sin(t * 5 + i * 1.3);
        c.globalAlpha = 0.35 + 0.65 * tw;
        c.fillStyle = i % 2 ? "#ffd166" : "#fff6c2";
        star(c, x, y, u * (0.05 + 0.06 * tw));
      }
      c.globalAlpha = 1;
      break;
    case "rain_cloud": {
      const cx = hx + Math.sin(t * 0.7) * u * 0.06, cy = ht - u * 0.55;
      c.fillStyle = "#9aa3ad";
      for (const [dx, dy, r] of [[-0.22, 0.05, 0.17], [0, -0.08, 0.22], [0.24, 0.04, 0.17], [0.05, 0.1, 0.18]] as const) {
        c.beginPath();
        c.arc(cx + dx * u, cy + dy * u, r * u, 0, Math.PI * 2);
        c.fill();
      }
      c.strokeStyle = "#3f8efc";
      c.lineWidth = Math.max(1, u * 0.035);
      c.lineCap = "round";
      for (let i = 0; i < 4; i++) {
        const k = (t * 1.6 + i / 4) % 1;
        const x = cx - u * 0.25 + i * u * 0.17, y = cy + u * 0.25 + k * u * 0.5;
        c.globalAlpha = 1 - k;
        c.beginPath();
        c.moveTo(x, y);
        c.lineTo(x - u * 0.02, y + u * 0.1);
        c.stroke();
      }
      c.globalAlpha = 1;
      break;
    }
    case "fireflies":
      for (let i = 0; i < 6; i++) {
        const x = bx + drift(i, t * 0.8) * bw * 0.7, y = bb - bh * 0.55 + drift(i + 7, t * 0.6) * bh * 0.55;
        const glow = 0.4 + 0.6 * (0.5 + 0.5 * Math.sin(t * 2.5 + i * 2.1));
        c.globalAlpha = glow * 0.35;
        c.fillStyle = "#e9ff8a";
        c.beginPath();
        c.arc(x, y, u * 0.1, 0, Math.PI * 2);
        c.fill();
        c.globalAlpha = glow;
        c.beginPath();
        c.arc(x, y, u * 0.035, 0, Math.PI * 2);
        c.fill();
      }
      c.globalAlpha = 1;
      break;
  }
}

/** Rope colours for tug of war. */
export function ropeStyle(c: CanvasRenderingContext2D, rope: string | null | undefined, x0: number, x1: number): string | CanvasGradient {
  if (rope === "rope_gold") return "#f4c430";
  if (rope === "rope_rainbow") {
    const g = c.createLinearGradient(x0, 0, x1, 0);
    ["#e63946", "#f4a261", "#e9c46a", "#2a9d8f", "#3f8efc", "#7b6cd9"].forEach((col, i, a) => g.addColorStop(i / (a.length - 1), col));
    return g;
  }
  return "#a0703a";
}
