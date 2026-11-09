// Effects drawn around a pet: rock-paper-scissors hands, a fish flying into
// its mouth, hearts, confetti, coins, badges (race flag, bet chips, betting
// tickets) and a race progress bar. Positions come from the pet each frame.

import { t as tr } from "./i18n"; // `t` is a common local name here

export interface Anchor {
  cx: number; // centre of the main pet
  bottom: number; // feet line
  top: number; // top of the head
  headW: number;
  w: number; // window width
}

const HANDS = ["✊", "✌️", "✋"];
const SHAKE_MS = 1200;

interface Hand { kind: "hand"; t0: number; sign: number; reveal: number; until: number; side: "pet" | "user" }
interface Fish { kind: "fish"; t0: number; dur: number; onArrive?: () => void; arrived?: boolean }
interface Particles { kind: "hearts" | "confetti" | "coins" | "bite" | "glow"; t0: number; dur: number; seed: number; text?: string }
interface Chip { kind: "chip"; id: string; text: string; color: string; t0: number; end?: { t0: number; how: "win" | "lose" | "shatter"; text?: string } }
interface Progress { kind: "progress"; id: string; value: number; target: number }
/** You vs the pet: a little card over its head. */
interface Versus { kind: "versus"; t0: number; user: number; pet: number; reveal: number; until: number; result: string; color: string }
/** Codex reset card: flies in, flips, fills the quota bar, bursts. */
interface Card { kind: "card"; t0: number; from: number; used: number }
type Item = Hand | Fish | Particles | Chip | Progress | Versus | Card;

export class Fx {
  private items: Item[] = [];

  /** True while something is moving (the renderer redraws every frame). */
  busy(now: number): boolean {
    this.items = this.items.filter((it) => {
      if (it.kind === "hand" || it.kind === "versus") return now < it.until;
      if (it.kind === "card") return now < it.t0 + 3000;
      if (it.kind === "fish") return now < it.t0 + it.dur + 50;
      if (it.kind === "hearts" || it.kind === "confetti" || it.kind === "coins" || it.kind === "bite" || it.kind === "glow") return now < it.t0 + it.dur;
      if (it.kind === "chip") return !it.end || now < it.end.t0 + 1600;
      return true;
    });
    return this.items.some((it) => it.kind !== "progress" || Math.abs(it.value - it.target) > 0.005) || this.items.some((it) => it.kind === "chip");
  }

  /** A hand that shakes, then shows `sign` (0 rock, 1 scissors, 2 paper). */
  hand(sign: number, side: "pet" | "user", delay = 0, hold = 1800): void {
    const t0 = performance.now() + delay;
    this.items.push({ kind: "hand", t0, sign, reveal: t0 + SHAKE_MS, until: t0 + SHAKE_MS + hold, side });
  }

  /** Rock-paper-scissors against the user: "你 ✊ VS ✌ 我", then the result. */
  versus(user: number, pet: number, result: string, color: string): void {
    this.items = this.items.filter((it) => it.kind !== "versus");
    const t0 = performance.now();
    this.items.push({ kind: "versus", t0, user, pet, reveal: t0 + SHAKE_MS, until: t0 + SHAKE_MS + 2600, result, color });
  }

  card(from: number, used: number): void {
    this.items.push({ kind: "card", t0: performance.now(), from, used });
  }

  fish(onArrive: () => void): void {
    this.items.push({ kind: "fish", t0: performance.now(), dur: 900, onArrive });
  }

  /**
   * Particles: hearts drifting up, confetti, coins with an amount, a `bite`
   * (crunch ring, crumbs and a sparkle at the mouth) or a `glow` (a warm
   * pulse around the pet, for a secret or a letter).
   */
  burst(kind: Particles["kind"], text?: string, delay = 0): void {
    const dur = kind === "confetti" ? 1800 : kind === "bite" ? 900 : kind === "glow" ? 1600 : 1500;
    this.items.push({ kind, t0: performance.now() + delay, dur, seed: Math.random() * 1000, text });
  }

  chip(id: string, text: string, color: string): void {
    this.items = this.items.filter((it) => !(it.kind === "chip" && it.id === id));
    this.items.push({ kind: "chip", id, text, color, t0: performance.now() });
  }

  /** End a chip: it flies off as a win, falls as a loss, or shatters. */
  endChip(id: string, how: "win" | "lose" | "shatter", text?: string): void {
    for (const it of this.items) if (it.kind === "chip" && it.id === id && !it.end) it.end = { t0: performance.now(), how, text };
  }

  endChips(prefix: string, how: "win" | "lose" | "shatter", text?: string): void {
    for (const it of this.items) if (it.kind === "chip" && it.id.startsWith(prefix) && !it.end) it.end = { t0: performance.now(), how, text };
  }

  hasChip(prefix: string): boolean {
    return this.items.some((it) => it.kind === "chip" && it.id.startsWith(prefix) && !it.end);
  }

  progress(id: string, value: number): void {
    const p = this.items.find((it): it is Progress => it.kind === "progress" && it.id === id);
    if (p) p.target = value;
    else this.items.push({ kind: "progress", id, value: 0, target: value });
  }

  dropProgress(id: string): void {
    this.items = this.items.filter((it) => !(it.kind === "progress" && it.id === id));
  }

  draw(c: CanvasRenderingContext2D, a: Anchor, now: number): void {
    let chipRow = 0;
    for (const it of this.items) {
      if (now < ("t0" in it ? it.t0 : 0)) continue;
      switch (it.kind) {
        case "hand": this.drawHand(c, a, it, now); break;
        case "versus": this.drawVersus(c, a, it, now); break;
        case "card": this.drawCard(c, a, it, now); break;
        case "fish": this.drawFish(c, a, it, now); break;
        case "hearts": case "confetti": case "coins": case "bite": case "glow": this.drawParticles(c, a, it, now); break;
        case "chip": this.drawChip(c, a, it, now, chipRow++); break;
        case "progress": this.drawProgress(c, a, it); break;
      }
    }
  }

  private drawHand(c: CanvasRenderingContext2D, a: Anchor, h: Hand, now: number) {
    const size = Math.max(22, a.headW * 0.55);
    const shaking = now < h.reveal;
    const k = (now - h.t0) / SHAKE_MS;
    const bob = shaking ? Math.abs(Math.sin(k * Math.PI * 3)) * size * 0.35 : 0;
    const x = a.cx;
    const y = a.top - size * 0.2 - bob;
    c.save();
    c.font = `${Math.round(size)}px "Segoe UI Emoji", "Apple Color Emoji", sans-serif`;
    c.textAlign = "center";
    c.textBaseline = "bottom";
    if (!shaking) {
      // Pop on reveal.
      const p = Math.min(1, (now - h.reveal) / 150);
      c.translate(x, y);
      c.scale(0.7 + 0.3 * p + (p < 1 ? 0.2 : 0), 0.7 + 0.3 * p + (p < 1 ? 0.2 : 0));
      c.fillText(HANDS[h.sign % 3], 0, 0);
    } else {
      c.fillText(HANDS[0], x, y);
    }
    c.restore();
    if (h.side === "user") {
      c.font = "bold 10px 'Microsoft YaHei', system-ui, sans-serif";
      c.textAlign = "center";
      c.fillStyle = "#4a2c20";
      c.fillText(tr("你", "You"), x, a.top + 4);
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

  private drawVersus(c: CanvasRenderingContext2D, a: Anchor, v: Versus, now: number) {
    const W = 136, H = 64;
    const x0 = Math.max(2, Math.min(a.w - W - 2, a.cx - W / 2));
    const y0 = Math.max(2, a.top - H - 8);
    const shaking = now < v.reveal;
    const fade = Math.min(1, (v.until - now) / 300);
    c.save();
    c.globalAlpha = Math.max(0, fade);
    c.fillStyle = "rgba(255,255,255,0.96)";
    c.strokeStyle = "#4a2c20";
    c.lineWidth = 1.5;
    c.beginPath();
    c.roundRect(x0, y0, W, H, 8);
    c.fill();
    c.stroke();
    // Title: the chant, then the result.
    c.textAlign = "center";
    c.textBaseline = "middle";
    c.font = "bold 12px 'Microsoft YaHei', system-ui, sans-serif";
    c.fillStyle = shaking ? "#4a2c20" : v.color;
    c.fillText(shaking ? tr("石头、剪刀、布！", "Rock, paper, scissors!") : v.result, x0 + W / 2, y0 + 12, W - 8);
    // Hands: yours on the left, the pet's on the right.
    const k = (now - v.t0) / SHAKE_MS;
    const bob = shaking ? Math.abs(Math.sin(k * Math.PI * 3)) * 6 : 0;
    const pop = shaking ? 1 : 1 + 0.25 * Math.max(0, 1 - (now - v.reveal) / 150);
    const hand = (sign: number, cx: number) => {
      c.save();
      c.translate(cx, y0 + 36 - bob);
      c.scale(pop, pop);
      c.font = `24px "Segoe UI Emoji", "Apple Color Emoji", sans-serif`;
      c.fillText(HANDS[shaking ? 0 : sign % 3], 0, 0);
      c.restore();
    };
    hand(v.user, x0 + 32);
    hand(v.pet, x0 + W - 32);
    c.font = "bold 11px system-ui, sans-serif";
    c.fillStyle = "#c0392b";
    c.fillText("VS", x0 + W / 2, y0 + 36);
    c.font = "10px 'Microsoft YaHei', system-ui, sans-serif";
    c.fillStyle = "#6b5040";
    c.fillText(tr("你", "You"), x0 + 32, y0 + H - 8);
    c.fillText(tr("我", "Me"), x0 + W - 32, y0 + H - 8);
    c.restore();
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
    const sx = 10, sy = 6;
    const ex = a.cx + a.headW * 0.05, ey = a.top + a.headW * 0.55;
    const at = (q: number) => ({ x: sx + (ex - sx) * q, y: sy + (ey - sy) * q - Math.sin(q * Math.PI) * 44 });
    const { x, y } = at(k);
    c.save();
    // A glittering wake behind the fish: droplets and a few sparkles.
    for (let i = 1; i <= 7; i++) {
      const q = k - i * 0.045;
      if (q <= 0) continue;
      const p = at(q);
      const fade = (1 - i / 8) * 0.8;
      c.globalAlpha = fade;
      c.fillStyle = i % 2 ? "#9ad7ff" : "#fff6c2";
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
    c.fillStyle = "#f4a261";
    c.beginPath();
    c.ellipse(0, 0, 9, 5, 0, 0, Math.PI * 2);
    c.fill();
    c.beginPath();
    c.moveTo(-8, 0);
    c.lineTo(-14, -5 - Math.sin(k * 40) * 2);
    c.lineTo(-14, 5 + Math.sin(k * 40) * 2);
    c.fill();
    c.shadowBlur = 0;
    c.fillStyle = "rgba(255,255,255,0.75)";
    c.beginPath();
    c.ellipse(-1, -1.5, 5, 1.5, 0, 0, Math.PI * 2);
    c.fill();
    c.fillStyle = "#2b1d16";
    c.fillRect(3, -2, 2, 2);
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
      c.font = "14px 'Segoe UI Emoji', sans-serif";
      c.textAlign = "center";
      for (let i = 0; i < 5; i++) {
        const x = a.cx + (rand(i) - 0.5) * a.headW * 1.4;
        const y = a.top + a.headW * 0.3 - k * 50 - rand(i + 9) * 20;
        c.fillText("💗", x, y);
      }
    } else if (p.kind === "confetti") {
      const cols = ["#e63946", "#f4a261", "#e9c46a", "#2a9d8f", "#3f8efc", "#7b6cd9"];
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
    } else {
      // Coins spring up and fall, with the amount floating above.
      for (let i = 0; i < 8; i++) {
        const vx = (rand(i) - 0.5) * 90;
        const vy = -90 - rand(i + 5) * 60;
        const t = k * 1.2;
        const x = a.cx + vx * t;
        const y = a.top + a.headW * 0.4 + vy * t + 160 * t * t;
        c.fillStyle = "#f4c430";
        c.strokeStyle = "#a8781a";
        c.lineWidth = 1.5;
        c.beginPath();
        c.ellipse(x, y, 5, 5 * Math.abs(Math.cos(k * 10 + i)) + 1, 0, 0, Math.PI * 2);
        c.fill();
        c.stroke();
      }
      if (p.text) {
        c.font = "bold 16px system-ui, sans-serif";
        c.textAlign = "center";
        c.lineWidth = 3;
        c.strokeStyle = "rgba(255,255,255,0.9)";
        const y = a.top - 10 - k * 30;
        c.strokeText(p.text, a.cx, y);
        c.fillStyle = "#c8901a";
        c.fillText(p.text, a.cx, y);
      }
    }
    c.restore();
  }

  private drawChip(c: CanvasRenderingContext2D, a: Anchor, ch: Chip, now: number, row: number) {
    c.save();
    c.font = "bold 10px 'Microsoft YaHei', system-ui, sans-serif";
    const text = ch.end?.text ?? ch.text;
    const tw = c.measureText(text).width + 12;
    let x = Math.min(a.w - tw - 2, a.cx + a.headW * 0.45);
    let y = a.top + 4 + row * 17;
    let alpha = Math.min(1, (now - ch.t0) / 200);
    let rot = 0;
    if (ch.end) {
      const k = Math.min(1, (now - ch.end.t0) / 1600);
      if (ch.end.how === "win") {
        y -= k * 30;
        alpha = 1 - k * k;
      } else if (ch.end.how === "lose") {
        y += k * k * 60;
        rot = k * 0.8;
        alpha = 1 - k;
      } else {
        // Shatter: two halves drop apart.
        alpha = 1 - k;
        c.globalAlpha = alpha;
        for (const d of [-1, 1]) {
          c.save();
          c.translate(x + tw / 2 + d * k * 14, y + 7 + k * k * 50);
          c.rotate(d * k * 1.2);
          c.beginPath();
          c.rect(d < 0 ? -tw / 2 : 0, -7, tw / 2, 14);
          c.clip();
          this.chipBox(c, -tw / 2, -7, tw, text, ch.color);
          c.restore();
        }
        c.restore();
        return;
      }
    }
    c.globalAlpha = Math.max(0, alpha);
    c.translate(x + tw / 2, y + 7);
    c.rotate(rot);
    this.chipBox(c, -tw / 2, -7, tw, text, ch.end?.how === "win" ? "#2a9d8f" : ch.end?.how === "lose" ? "#9aa0a6" : ch.color);
    c.restore();
    x = 0;
  }

  private chipBox(c: CanvasRenderingContext2D, x: number, y: number, w: number, text: string, color: string) {
    c.fillStyle = color;
    c.beginPath();
    c.roundRect(x, y, w, 14, 7);
    c.fill();
    c.fillStyle = "#fff";
    c.textAlign = "center";
    c.textBaseline = "middle";
    c.fillText(text, x + w / 2, y + 7.5);
  }

  private drawProgress(c: CanvasRenderingContext2D, a: Anchor, p: Progress) {
    p.value += (p.target - p.value) * 0.15;
    const w = Math.max(40, a.headW * 1.3);
    const x = a.cx - w / 2;
    const y = a.bottom - 4;
    c.fillStyle = "rgba(43,29,22,0.35)";
    c.beginPath();
    c.roundRect(x, y, w, 4, 2);
    c.fill();
    c.fillStyle = "#e63946";
    c.beginPath();
    c.roundRect(x, y, Math.max(2, w * Math.min(1, p.value)), 4, 2);
    c.fill();
    c.font = "10px 'Segoe UI Emoji', sans-serif";
    c.textAlign = "left";
    c.textBaseline = "middle";
    c.fillText("🏁", x + w + 2, y + 2);
  }
}
