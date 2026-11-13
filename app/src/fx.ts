// Effects drawn around a pet: a sweet flying into its mouth, the bite, hearts,
// confetti, a warm glow, and the Codex reset card. Positions come from the pet
// each frame.

import { worn } from "./skin";
import { drawTreat, type TreatLook } from "./treats";
import { t as tr } from "./i18n"; // `t` is a common local name here

export interface Anchor {
  cx: number; // centre of the main pet
  bottom: number; // feet line
  top: number; // top of the head
  headW: number;
  w: number; // window width
}

interface Fish { kind: "fish"; t0: number; dur: number; onArrive?: () => void; arrived?: boolean; look?: TreatLook; from?: "left" | "right" }
interface Particles { kind: "hearts" | "confetti" | "bite" | "glow"; t0: number; dur: number; seed: number }
/** Codex reset card: flies in, flips, fills the quota bar, bursts. */
interface Card { kind: "card"; t0: number; from: number; used: number }
/** A slip of 手上的事: `in` comes down into its hand; `eat`, balled up, goes into its mouth. */
interface Slip { kind: "slip"; t0: number; dur: number; mode: "in" | "eat"; onArrive?: () => void; arrived?: boolean }
type Item = Fish | Particles | Card | Slip;

/** Where a pet holds its slips: at its side, a little below the middle. */
export function slipHand(a: Anchor): { x: number; y: number } {
  return { x: a.cx + (a.headW / 0.7) * 0.36, y: a.bottom - (a.bottom - a.top) * 0.3 };
}

/** A slip of paper (folded corner, three lines of writing, a pin in the pet's colour), or `crumple` (0..1) of it balled up. */
export function drawSlip(c: CanvasRenderingContext2D, x: number, y: number, rot: number, s = 1, crumple = 0): void {
  c.save();
  c.translate(x, y);
  c.rotate(rot);
  c.scale(s, s);
  if (crumple > 0) {
    const r = 9 - crumple * 2;
    c.fillStyle = "#fbf3df";
    c.strokeStyle = "#cdbda9";
    c.lineWidth = 1;
    c.beginPath();
    for (let k = 0; k <= 10; k++) {
      const ang = (k / 10) * Math.PI * 2;
      const rr = r * (0.82 + 0.18 * Math.sin(k * 2.7));
      if (k) c.lineTo(Math.cos(ang) * rr, Math.sin(ang) * rr);
      else c.moveTo(Math.cos(ang) * rr, Math.sin(ang) * rr);
    }
    c.closePath();
    c.fill();
    c.stroke();
    c.beginPath();
    c.moveTo(-4, -2);
    c.lineTo(1, 1);
    c.lineTo(3, -3);
    c.stroke();
    c.restore();
    return;
  }
  c.shadowColor = "rgba(110,88,140,.25)";
  c.shadowBlur = 4;
  c.shadowOffsetY = 1.5;
  c.fillStyle = "#fbf3df";
  c.beginPath();
  c.moveTo(-11, -13);
  c.lineTo(6, -13);
  c.lineTo(11, -8);
  c.lineTo(11, 13);
  c.lineTo(-11, 13);
  c.closePath();
  c.fill();
  c.shadowColor = "transparent";
  c.strokeStyle = "#d8c9b3";
  c.lineWidth = 1;
  c.stroke();
  c.fillStyle = "#efe2c6";
  c.beginPath();
  c.moveTo(6, -13);
  c.lineTo(6, -8);
  c.lineTo(11, -8);
  c.closePath();
  c.fill();
  c.strokeStyle = "#9b8a9a";
  c.lineWidth = 1.2;
  c.lineCap = "round";
  for (const [ly, lw] of [[-4, 14], [1, 11], [6, 13]] as const) {
    c.beginPath();
    c.moveTo(-7, ly);
    for (let lx = -7; lx <= -7 + lw; lx += 2) c.lineTo(lx, ly + Math.sin(lx * 1.3 + ly) * 0.7);
    c.stroke();
  }
  c.fillStyle = worn().colors.accentOff;
  c.beginPath();
  c.arc(-4, -10, 2.4, 0, Math.PI * 2);
  c.fill();
  c.restore();
}

export class Fx {
  private items: Item[] = [];

  /** True while something is moving (the renderer redraws every frame). */
  busy(now: number): boolean {
    this.items = this.items.filter((it) => {
      if (it.kind === "card") return now < it.t0 + 3000;
      if (it.kind === "fish" || it.kind === "slip") return now < it.t0 + it.dur + 50;
      return now < it.t0 + it.dur;
    });
    return this.items.length > 0;
  }

  card(from: number, used: number): void {
    this.items.push({ kind: "card", t0: performance.now(), from, used });
  }

  /** A sweet flies into its mouth: `look` is the kind that came out of the jar (a wrapped sweet in its own colours when unknown). */
  fish(onArrive: () => void, look?: TreatLook | null, from?: "left" | "right"): void {
    this.items.push({ kind: "fish", t0: performance.now(), dur: 900, onArrive, look: look ?? undefined, from });
  }

  /** A slip comes down into its hand (a thing written down). */
  slipIn(onArrive?: () => void): void {
    this.items.push({ kind: "slip", t0: performance.now(), dur: 650, mode: "in", onArrive });
  }

  /** A slip, balled up, goes from its hand into its mouth (a thing crossed off). */
  slipEat(onArrive?: () => void): void {
    this.items.push({ kind: "slip", t0: performance.now(), dur: 800, mode: "eat", onArrive });
  }

  /**
   * Particles: hearts drifting up, confetti, a `bite` (crunch ring, crumbs and
   * a sparkle at the mouth) or a `glow` (a warm pulse around the pet, for a
   * secret or a letter).
   */
  burst(kind: Particles["kind"], delay = 0): void {
    const dur = kind === "confetti" ? 1800 : kind === "bite" ? 900 : kind === "glow" ? 1600 : 1500;
    this.items.push({ kind, t0: performance.now() + delay, dur, seed: Math.random() * 1000 });
  }

  draw(c: CanvasRenderingContext2D, a: Anchor, now: number): void {
    for (const it of this.items) {
      if (now < it.t0) continue;
      switch (it.kind) {
        case "card": this.drawCard(c, a, it, now); break;
        case "fish": this.drawFish(c, a, it, now); break;
        case "slip": this.drawSlipFx(c, a, it, now); break;
        default: this.drawParticles(c, a, it, now);
      }
    }
  }

  private drawCard(c: CanvasRenderingContext2D, a: Anchor, k: Card, now: number) {
    const t = now - k.t0;
    const W = 54, H = 34;
    const tx = Math.max(W / 2 + 4, Math.min(a.w - W / 2 - 4, a.cx)), ty = Math.max(H / 2 + 22, a.top - 34);
    let x = tx, y = ty, rot = 0, sx = 1, alpha = 1;
    if (t < 600) {
      const e = 1 - (1 - t / 600) ** 3;
      x = 10 + (tx - 10) * e;
      y = 4 + (ty - 4) * e;
      rot = (1 - e) * -2.4;
    }
    const flip = Math.min(1, Math.max(0, (t - 600) / 500));
    sx = Math.abs(Math.cos(flip * Math.PI));
    const front = flip >= 0.5;
    if (t > 2200) alpha = Math.max(0, 1 - (t - 2200) / 300);
    c.save();
    c.globalAlpha = alpha;
    // Glow once it is face up.
    if (front) {
      const g = c.createRadialGradient(x, y, 4, x, y, 44);
      g.addColorStop(0, `rgba(255,214,102,${0.55 + 0.25 * Math.sin(now / 90)})`);
      g.addColorStop(1, "rgba(255,214,102,0)");
      c.fillStyle = g;
      c.beginPath();
      c.arc(x, y, 44, 0, Math.PI * 2);
      c.fill();
    }
    c.translate(x, y);
    c.rotate(rot);
    c.scale(Math.max(0.04, sx), 1);
    const grad = c.createLinearGradient(-W / 2, -H / 2, W / 2, H / 2);
    if (front) {
      grad.addColorStop(0, "#ffe08a");
      grad.addColorStop(0.5, "#f5b82e");
      grad.addColorStop(1, "#e08e0b");
    } else {
      grad.addColorStop(0, "#10a37f");
      grad.addColorStop(1, "#0b6e56");
    }
    c.fillStyle = grad;
    c.strokeStyle = front ? "#8a5a00" : "#064a3a";
    c.lineWidth = 1.5;
    c.beginPath();
    c.roundRect(-W / 2, -H / 2, W, H, 5);
    c.fill();
    c.stroke();
    c.fillStyle = front ? "#5a3a00" : "#d8fff2";
    c.textAlign = "center";
    c.textBaseline = "middle";
    c.font = "bold 11px system-ui, sans-serif";
    c.fillText(front ? "RESET" : "Codex", 0, -4);
    c.font = "8px 'Microsoft YaHei', system-ui, sans-serif";
    c.fillText(front ? tr("重置卡", "card") : "♻", 0, 9);
    c.restore();
    // Quota bar drains back to empty under the card.
    if (t > 1100 && t < 2500) {
      const p = Math.min(1, (t - 1100) / 900);
      const val = k.from + (k.used - k.from) * (1 - (1 - p) ** 2);
      const bw = 70, bx = Math.max(4, Math.min(a.w - bw - 4, tx - bw / 2)), by = ty + H / 2 + 6;
      c.save();
      c.globalAlpha = alpha;
      c.fillStyle = "rgba(255,255,255,0.95)";
      c.strokeStyle = "#4a2c20";
      c.lineWidth = 1;
      c.beginPath();
      c.roundRect(bx, by, bw, 9, 4);
      c.fill();
      c.stroke();
      c.fillStyle = val > 80 ? "#e63946" : val > 50 ? "#f4a261" : "#10a37f";
      c.beginPath();
      c.roundRect(bx + 1, by + 1, Math.max(0, (bw - 2) * val / 100), 7, 3);
      c.fill();
      c.font = "bold 9px system-ui, sans-serif";
      c.fillStyle = "#2b1d16";
      c.textAlign = "center";
      c.fillText(tr(`已用 ${Math.round(val)}%`, `${Math.round(val)}% used`), bx + bw / 2, by + 19);
      c.restore();
    }
    // Sparkle burst when it is spent.
    if (t > 2200 && t < 3000) {
      const p = (t - 2200) / 800;
      c.save();
      for (let i = 0; i < 12; i++) {
        const ang = (i / 12) * Math.PI * 2;
        const r = 8 + p * 40;
        c.globalAlpha = 1 - p;
        c.fillStyle = i % 2 ? "#ffd166" : "#10a37f";
        c.beginPath();
        c.arc(tx + Math.cos(ang) * r, ty + Math.sin(ang) * r, 2.5, 0, Math.PI * 2);
        c.fill();
      }
      c.restore();
    }
  }

  private drawSlipFx(c: CanvasRenderingContext2D, a: Anchor, f: Slip, now: number) {
    const k = Math.min(1, (now - f.t0) / f.dur);
    if (k >= 1) {
      if (!f.arrived) {
        f.arrived = true;
        f.onArrive?.();
      }
      return;
    }
    const hand = slipHand(a);
    if (f.mode === "in") {
      // From above its head, a short drop with a turn, into its hand.
      const e = 1 - (1 - k) ** 3;
      const sx = a.cx, sy = Math.max(10, a.top - 24);
      drawSlip(c, sx + (hand.x - sx) * e, sy + (hand.y - sy) * e - Math.sin(k * Math.PI) * 30, 0.12 + (1 - e) * 1.2, 0.7 + 0.3 * e);
    } else {
      // Balled up on the way, a hop, into its mouth.
      const e = k < 0.5 ? 2 * k * k : 1 - (-2 * k + 2) ** 2 / 2;
      const mx = a.cx + a.headW * 0.05, my = a.top + a.headW * 0.55;
      drawSlip(c, hand.x + (mx - hand.x) * e, hand.y + (my - hand.y) * e - Math.sin(k * Math.PI) * 46, k * 6, 1, Math.min(1, k * 3));
    }
  }

  private drawFish(c: CanvasRenderingContext2D, a: Anchor, f: Fish, now: number) {
    const k = Math.min(1, (now - f.t0) / f.dur);
    if (k >= 1) {
      if (!f.arrived) {
        f.arrived = true;
        f.onArrive?.();
      }
      return;
    }
    // From the jar (top left), or tossed over by another pet standing beside it.
    const sx = f.from === "right" ? c.canvas.clientWidth - 10 : 10;
    const sy = f.from ? a.top + a.headW * 0.35 : 6;
    const ex = a.cx + a.headW * 0.05, ey = a.top + a.headW * 0.55;
    const at = (q: number) => ({ x: sx + (ex - sx) * q, y: sy + (ey - sy) * q - Math.sin(q * Math.PI) * 44 });
    const { x, y } = at(k);
    c.save();
    // A glittering wake behind it: sugar dust and a few sparkles.
    for (let i = 1; i <= 7; i++) {
      const q = k - i * 0.045;
      if (q <= 0) continue;
      const p = at(q);
      const fade = (1 - i / 8) * 0.8;
      c.globalAlpha = fade;
      c.fillStyle = i % 2 ? "#ffd9e6" : "#fff6c2";
      const r = i % 2 ? 2.2 - i * 0.15 : 3 - i * 0.25;
      if (i % 3 === 0) {
        // Four-point sparkle.
        c.beginPath();
        for (let s = 0; s < 8; s++) {
          const ang = (s / 8) * Math.PI * 2 + k * 6;
          const rr = s % 2 ? r * 0.4 : r * 1.8;
          c.lineTo(p.x + Math.cos(ang) * rr, p.y + Math.sin(ang) * rr);
        }
        c.closePath();
        c.fill();
      } else {
        c.beginPath();
        c.arc(p.x, p.y + Math.sin(i * 3 + k * 20) * 3, Math.max(0.8, r), 0, Math.PI * 2);
        c.fill();
      }
    }
    c.globalAlpha = 1;
    c.translate(x, y);
    // Nose along the arc, spinning once near the end.
    const d = at(Math.min(1, k + 0.02));
    c.rotate(Math.atan2(d.y - y, d.x - x) + (k > 0.6 ? ((k - 0.6) / 0.4) * Math.PI * 2 : 0));
    const sc = 1 + Math.sin(k * Math.PI) * 0.25;
    c.scale(sc, sc);
    c.shadowColor = "rgba(255,214,102,0.8)";
    c.shadowBlur = 8;
    // A sweet in its wrapper, in the pet's own colours.
    drawTreat(c, f.look ?? { shape: "wrapped", colors: [worn().colors.motifFill, worn().colors.accentOff] }, 0, 0, 0.78);
    c.shadowBlur = 0;
    c.restore();
  }

  private drawParticles(c: CanvasRenderingContext2D, a: Anchor, p: Particles, now: number) {
    const k = (now - p.t0) / p.dur;
    const rand = (i: number) => {
      const x = Math.sin(p.seed + i * 12.9898) * 43758.5453;
      return x - Math.floor(x);
    };
    c.save();
    c.globalAlpha = Math.max(0, 1 - k);
    if (p.kind === "bite") {
      // The bite: a ring snaps out from the mouth, crumbs scatter, a star pops.
      const mx = a.cx + a.headW * 0.05, my = a.top + a.headW * 0.55;
      c.strokeStyle = "rgba(255,255,255,0.9)";
      c.lineWidth = 2.5 * (1 - k);
      c.beginPath();
      c.arc(mx, my, 6 + k * a.headW * 0.9, 0, Math.PI * 2);
      c.stroke();
      c.strokeStyle = "rgba(255,214,102,0.9)";
      c.lineWidth = 1.5 * (1 - k);
      c.beginPath();
      c.arc(mx, my, 3 + k * a.headW * 0.6, 0, Math.PI * 2);
      c.stroke();
      for (let i = 0; i < 10; i++) {
        const ang = rand(i) * Math.PI * 2;
        const sp = 18 + rand(i + 7) * 26;
        const t = k;
        const x = mx + Math.cos(ang) * sp * t;
        const y = my + Math.sin(ang) * sp * t * 0.7 + 70 * t * t;
        c.fillStyle = i % 3 === 0 ? "#f4a261" : i % 3 === 1 ? "#ffd166" : "#fff6c2";
        c.beginPath();
        c.arc(x, y, 1.6 + rand(i + 3) * 1.4, 0, Math.PI * 2);
        c.fill();
      }
      if (k < 0.5) {
        const pop = Math.sin((k / 0.5) * Math.PI);
        c.fillStyle = "#fff6c2";
        c.beginPath();
        for (let s = 0; s < 8; s++) {
          const ang = (s / 8) * Math.PI * 2 - Math.PI / 2;
          const rr = s % 2 ? 3 * pop : 9 * pop;
          c.lineTo(mx + a.headW * 0.35 + Math.cos(ang) * rr, my - a.headW * 0.3 + Math.sin(ang) * rr);
        }
        c.closePath();
        c.fill();
      }
    } else if (p.kind === "glow") {
      // A warm pulse around the whole pet, with motes rising through it.
      const cy = (a.top + a.bottom) / 2;
      const r = a.headW * 1.6;
      const pulse = 0.5 + 0.5 * Math.sin(k * Math.PI * 3);
      const g = c.createRadialGradient(a.cx, cy, r * 0.2, a.cx, cy, r);
      g.addColorStop(0, `rgba(255,214,120,${0.35 * (1 - k) * (0.7 + 0.3 * pulse)})`);
      g.addColorStop(1, "rgba(255,214,120,0)");
      c.globalAlpha = 1;
      c.fillStyle = g;
      c.beginPath();
      c.ellipse(a.cx, cy, r, r * 1.15, 0, 0, Math.PI * 2);
      c.fill();
      for (let i = 0; i < 9; i++) {
        const q = (k + rand(i)) % 1;
        const x = a.cx + (rand(i + 4) - 0.5) * a.headW * 2.2 + Math.sin(q * 6 + i) * 4;
        const y = a.bottom - q * (a.bottom - a.top + 30);
        c.globalAlpha = Math.sin(q * Math.PI) * (1 - k);
        c.fillStyle = i % 2 ? "#fff6c2" : "#ffd166";
        c.beginPath();
        c.arc(x, y, 1.5 + rand(i + 8) * 1.5, 0, Math.PI * 2);
        c.fill();
      }
    } else if (p.kind === "hearts") {
      c.fillStyle = "#e9a3ad";
      for (let i = 0; i < 5; i++) {
        const x = a.cx + (rand(i) - 0.5) * a.headW * 1.4;
        const y = a.top + a.headW * 0.3 - k * 50 - rand(i + 9) * 20;
        const r = 4 + rand(i + 2) * 2;
        c.beginPath();
        c.moveTo(x, y + r * 0.9);
        c.bezierCurveTo(x - r * 1.6, y - r * 0.2, x - r * 0.6, y - r * 1.3, x, y - r * 0.4);
        c.bezierCurveTo(x + r * 0.6, y - r * 1.3, x + r * 1.6, y - r * 0.2, x, y + r * 0.9);
        c.fill();
      }
    } else {
      // Confetti, in the five pets' colours.
      const cols = ["#b8a4e0", "#f0d9a8", "#9db9d8", "#e9a3ad", "#a9c4a0", "#7a5fc0"];
      for (let i = 0; i < 28; i++) {
        const ang = rand(i) * Math.PI * 2;
        const sp = 40 + rand(i + 3) * 70;
        const x = a.cx + Math.cos(ang) * sp * k;
        const y = a.top + Math.sin(ang) * sp * k * 0.6 + k * k * 60 - 10;
        c.fillStyle = cols[i % cols.length];
        c.save();
        c.translate(x, y);
        c.rotate(k * 8 + i);
        c.fillRect(-2, -3, 4, 6);
        c.restore();
      }
    }
    c.restore();
  }
}
