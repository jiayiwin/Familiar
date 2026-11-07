// 小皮鞭: the lash itself, drawn on the overlay canvas. Pure canvas maths with
// no Tauri in it, so the same code runs in a preview page.
//
// The lash is a rope of 26 points. The grip and the tip follow scripted paths
// (the tip sweeps back and low, rises behind the whipper, rolls over the top
// and comes down on the target); everything in between just follows, so the
// rope bows, loops overhead and straightens on its own, like a real bullwhip.
// At the hit the tip is let go and springs back. The impact is drawn big:
// hit-stop, flash, a comic burst with the word, shock rings, sparks, a welt.

export interface WhipBox { x: number; y: number; top: number; h: number }

export interface WhipOpts {
  /** The pet holding the whip; null: the user's fist comes in from above. */
  from: WhipBox | null;
  to: WhipBox;
  /** When the stroke starts (performance.now clock) and how long until it lands. */
  t0: number;
  hit: number;
  screenW: number;
  /** The ground: the lash never falls below it. */
  floor: number;
  /** The word in the burst. */
  text: string;
  sfx: (name: string) => void;
}

interface Pt { x: number; y: number; px: number; py: number }
interface Spark { x: number; y: number; vx: number; vy: number; c: string; r: number }

const N = 26;
/** Share of the stroke spent winding up. */
const WIND = 0.4;
/** The scene lingers this long after the hit (recoil, rings, welt), fading out. */
const FADE = 950;
const SPARK_COLORS = ["#fff3b0", "#ffd23a", "#ff8c1a", "#e63946"];

const clamp = (v: number, lo: number, hi: number) => Math.max(lo, Math.min(hi, v));
const easeInOut = (u: number) => (u < 0.5 ? 2 * u * u : 1 - (-2 * u + 2) ** 2 / 2);
const bez2 = (a: Pt2, c: Pt2, b: Pt2, u: number): Pt2 => {
  const v = 1 - u;
  return { x: v * v * a.x + 2 * v * u * c.x + u * u * b.x, y: v * v * a.y + 2 * v * u * c.y + u * u * b.y };
};
const bez3 = (a: Pt2, c1: Pt2, c2: Pt2, b: Pt2, u: number): Pt2 => {
  const v = 1 - u;
  return {
    x: v * v * v * a.x + 3 * v * v * u * c1.x + 3 * v * u * u * c2.x + u * u * u * b.x,
    y: v * v * v * a.y + 3 * v * v * u * c1.y + 3 * v * u * u * c2.y + u * u * u * b.y,
  };
};
type Pt2 = { x: number; y: number };

export class Whip {
  private readonly dir: number;
  /** Everything scales with the victim's size. */
  private readonly S: number;
  private readonly hx: number;
  private readonly hy: number;
  private readonly tx: number;
  private readonly ty: number;
  private readonly reach: number;
  private readonly seg: number;
  private readonly pts: Pt[];
  private readonly trail: Pt2[][] = [];
  private readonly sparks: Spark[] = [];
  private lastT = 0;
  private cracked = false;
  private swished = false;
  private markAng = 0;
  private shakeSeed = 0;

  constructor(private readonly o: WhipOpts) {
    const { from, to } = o;
    this.dir = from ? Math.sign(to.x - from.x) || 1 : to.x < o.screenW / 2 ? -1 : 1;
    const S = (this.S = clamp(to.h / 120, 0.8, 1.8));
    if (from) {
      this.hx = from.x + this.dir * from.h * 0.2;
      this.hy = from.y - from.h * 0.12;
    } else {
      this.hx = to.x - this.dir * 150 * S;
      this.hy = Math.max(30, to.top - 110 * S);
    }
    this.tx = to.x - this.dir * to.h * 0.08;
    this.ty = to.y - to.h * 0.05;
    const d = Math.hypot(this.tx - this.hx, this.ty - this.hy);
    this.reach = clamp(d * 0.95, 150 * S, 440);
    // Rope length: a touch more than the furthest the tip ever gets from the hand.
    let max = 0;
    for (let i = 0; i <= 48; i++) {
      const h = this.hand(i / 48), t = this.tip(i / 48);
      max = Math.max(max, Math.hypot(t.x - h.x, t.y - h.y));
    }
    const len = max * 1.08;
    this.seg = len / (N - 1);
    // Start coiled: a bow from the hand to the resting tip, slack hanging behind.
    const h0 = this.hand(0), t0 = this.tip(0);
    const chord = Math.hypot(t0.x - h0.x, t0.y - h0.y);
    const nx = -this.dir * 0.7, ny = 0.7; // bow back and down
    let amp = 0;
    for (let tries = 0; tries < 30; tries++) {
      let arc = 0;
      let prev: Pt2 | null = null;
      for (let i = 0; i < N; i++) {
        const p = this.bowPoint(h0, t0, nx, ny, amp, i / (N - 1));
        if (prev) arc += Math.hypot(p.x - prev.x, p.y - prev.y);
        prev = p;
      }
      if (arc >= len) break;
      amp += Math.max(2, (len - arc) * 0.5);
    }
    if (chord > len) amp = 0;
    this.pts = Array.from({ length: N }, (_, i) => {
      const p = this.bowPoint(h0, t0, nx, ny, amp, i / (N - 1));
      return { x: p.x, y: p.y, px: p.x, py: p.y };
    });
  }

  private bowPoint(a: Pt2, b: Pt2, nx: number, ny: number, amp: number, s: number): Pt2 {
    const w = Math.sin(Math.PI * s) * amp;
    return { x: a.x + (b.x - a.x) * s + nx * w, y: a.y + (b.y - a.y) * s + ny * w };
  }

  /** Where the grip is at stroke time `k` (0 start, 1 hit, >1 after). */
  private hand(k: number): Pt2 {
    const { hx, hy, dir, reach: R } = this;
    if (k < WIND) {
      const u = easeInOut(k / WIND);
      return { x: hx - dir * 24 * u, y: hy - 0.26 * R * u };
    }
    if (k <= 1) {
      const u = (k - WIND) / (1 - WIND);
      const e = 1 - (1 - u) ** 3;
      return { x: hx - dir * 24 + dir * 58 * e, y: hy - 0.26 * R + 0.34 * R * e };
    }
    const u = Math.min(1, (k - 1) * 1.6);
    const e = easeInOut(u);
    return { x: hx + dir * 34 - dir * 34 * e, y: hy + 0.08 * R - 0.08 * R * e };
  }

  /** Where the tip is driven to at stroke time `k` (pinned until the hit). */
  private tip(k: number): Pt2 {
    const { hx, hy, dir, reach: R, tx, ty } = this;
    const p0 = { x: hx - dir * 12, y: hy + R * 0.5 }; // hanging by the side
    const c0 = { x: hx - dir * R * 0.95, y: hy + R * 0.3 }; // sweeps back low…
    const p1 = { x: hx - dir * R * 0.62, y: hy - R * 0.82 }; // …then up behind
    if (k < WIND) return bez2(p0, c0, p1, easeInOut(k / WIND));
    const c1 = { x: hx + dir * R * 0.22, y: hy - R * 1.3 }; // over the top
    const c2 = { x: tx - dir * R * 0.5, y: ty - R * 0.95 }; // and down onto the target
    const u = clamp((k - WIND) / (1 - WIND), 0, 1);
    return bez3(p1, c1, c2, { x: tx, y: ty }, u ** 1.7);
  }

  /** Advance the rope. Returns false once the scene is over. */
  step(now: number): boolean {
    const t = now - this.o.t0;
    if (t > this.o.hit + FADE) return false;
    if (t < 0) return true;
    const dt = this.lastT ? clamp((now - this.lastT) / 1000, 0.004, 0.033) : 0.016;
    this.lastT = now;
    const k = t / this.o.hit;
    const { pts, seg, S } = this;
    const g = 1500 * dt * dt;
    for (const p of pts) {
      const vx = (p.x - p.px) * 0.985, vy = (p.y - p.py) * 0.985;
      p.px = p.x;
      p.py = p.y;
      p.x += vx;
      p.y += vy + g;
    }
    const h = this.hand(k);
    const pinTip = t < this.o.hit;
    const tp = pinTip ? this.tip(k) : null;
    const last = pts[N - 1];
    for (let it = 0; it < 14; it++) {
      pts[0].x = h.x;
      pts[0].y = h.y;
      if (tp) {
        last.x = tp.x;
        last.y = tp.y;
      }
      for (let i = 0; i < N - 1; i++) {
        const a = pts[i], b = pts[i + 1];
        const dx = b.x - a.x, dy = b.y - a.y;
        const d = Math.hypot(dx, dy) || 0.001;
        const diff = (d - seg) / d;
        const wa = i === 0 ? 0 : i + 1 === N - 1 && tp ? 1 : 0.5;
        const wb = 1 - wa;
        a.x += dx * diff * wa;
        a.y += dy * diff * wa;
        b.x -= dx * diff * wb;
        b.y -= dy * diff * wb;
      }
    }
    pts[0].x = h.x;
    pts[0].y = h.y;
    for (const p of pts) {
      if (p.y > this.o.floor) {
        p.y = this.o.floor;
        p.px = p.x + (p.x - p.px) * 0.3; // friction on the ground
      }
    }
    // Motion trail through the strike.
    if (k > WIND && t < this.o.hit + 120) {
      this.trail.push(pts.map((p) => ({ x: p.x, y: p.y })));
      if (this.trail.length > 7) this.trail.shift();
    } else if (this.trail.length) this.trail.shift();
    // The swish in the sound leads the crack by 150 ms.
    if (!this.swished && t >= this.o.hit - 150) {
      this.swished = true;
      this.o.sfx("whip");
    }
    if (!this.cracked && t >= this.o.hit) {
      this.cracked = true;
      const prev = pts[N - 2];
      this.markAng = Math.atan2(last.y - prev.y, last.x - prev.x);
      // Let go: the tip springs back the way it came.
      last.px = last.x + this.dir * 6 * S;
      last.py = last.y + 9 * S;
      for (let i = 0; i < 18; i++) {
        const ang = Math.random() * Math.PI * 2;
        const sp = (160 + Math.random() * 300) * S;
        this.sparks.push({ x: this.tx, y: this.ty, vx: Math.cos(ang) * sp, vy: Math.sin(ang) * sp - 120 * S, c: SPARK_COLORS[i % SPARK_COLORS.length], r: (1.4 + Math.random() * 1.8) * S });
      }
      setTimeout(() => this.o.sfx("ouch"), 170);
      if (this.o.from) setTimeout(() => this.o.sfx("hmph"), 720);
    }
    for (const s of this.sparks) {
      s.x += s.vx * dt;
      s.y += s.vy * dt;
      s.vy += 900 * dt;
      s.vx *= 0.985;
    }
    return true;
  }

  draw(ctx: CanvasRenderingContext2D, now: number): void {
    const t = now - this.o.t0;
    if (t < 0 || t > this.o.hit + FADE) return;
    const { S, pts } = this;
    const post = t - this.o.hit;
    ctx.save();
    // Hit-stop: the whole scene shudders for a few frames.
    if (post >= 0 && post < 150) {
      this.shakeSeed += 1;
      const a = (1 - post / 150) * 6 * S;
      ctx.translate(Math.sin(this.shakeSeed * 2.7) * a, Math.cos(this.shakeSeed * 1.9) * a);
    }
    const alpha = post > 400 ? Math.max(0, 1 - (post - 400) / (FADE - 400)) : 1;
    ctx.globalAlpha = alpha;
    ctx.lineCap = "round";
    ctx.lineJoin = "round";

    // Ghosts of the last few frames: speed.
    const n = this.trail.length;
    this.trail.forEach((tr, i) => {
      const a = ((i + 1) / (n + 1)) * 0.4;
      this.rope(ctx, tr, a * alpha, 0.85, "#6b4a3a", null);
    });
    // The lash: dark leather with a lighter seam, white-hot along the end as it lands.
    const hot = post >= -90 && post < 60 ? 1 - Math.abs(post + 15) / 75 : 0;
    this.rope(ctx, pts, alpha, 1, "#1f120c", "#8a5a3c");
    if (hot > 0) {
      ctx.save();
      ctx.globalAlpha = alpha * hot * 0.9;
      ctx.strokeStyle = "#fff6c8";
      ctx.lineWidth = 2.2 * S;
      ctx.beginPath();
      const start = Math.floor(N * 0.55);
      ctx.moveTo(pts[start].x, pts[start].y);
      for (let i = start + 1; i < N; i++) ctx.lineTo(pts[i].x, pts[i].y);
      ctx.stroke();
      ctx.restore();
    }
    // Tassel at the tip: a red knot and three strands trailing the motion.
    const tip = pts[N - 1], pre = pts[N - 2];
    const ta = Math.atan2(tip.y - pre.y, tip.x - pre.x);
    ctx.strokeStyle = "#d8203a";
    ctx.lineWidth = 1.6 * S;
    for (let i = -1; i <= 1; i++) {
      const a = ta + i * 0.45;
      ctx.beginPath();
      ctx.moveTo(tip.x, tip.y);
      ctx.lineTo(tip.x + Math.cos(a) * 9 * S, tip.y + Math.sin(a) * 9 * S);
      ctx.stroke();
    }
    ctx.fillStyle = "#d8203a";
    ctx.beginPath();
    ctx.arc(tip.x, tip.y, 2.8 * S, 0, Math.PI * 2);
    ctx.fill();

    // Handle in the hand, pointing away from the lash; the user's fist and sleeve behind it.
    const h = pts[0], h2 = pts[2];
    let ux = h.x - h2.x, uy = h.y - h2.y;
    const ul = Math.hypot(ux, uy) || 1;
    ux /= ul;
    uy /= ul;
    const hl = 30 * S;
    const ex = h.x + ux * hl, ey = h.y + uy * hl;
    if (!this.o.from) {
      // Sleeve, then the fist.
      ctx.strokeStyle = "#c0182e";
      ctx.lineWidth = 17 * S;
      ctx.beginPath();
      ctx.moveTo(ex + ux * 8 * S, ey + uy * 8 * S);
      ctx.lineTo(ex + ux * 60 * S, ey + uy * 60 * S);
      ctx.stroke();
      ctx.strokeStyle = "#1f120c";
      ctx.lineWidth = 3 * S;
      ctx.beginPath();
      ctx.moveTo(ex + ux * 9 * S - uy * 9 * S, ey + uy * 9 * S + ux * 9 * S);
      ctx.lineTo(ex + ux * 9 * S + uy * 9 * S, ey + uy * 9 * S - ux * 9 * S);
      ctx.stroke();
      ctx.fillStyle = "#f3c9a4";
      ctx.strokeStyle = "#a86f4a";
      ctx.lineWidth = 1.6 * S;
      ctx.beginPath();
      ctx.arc(ex - ux * 4 * S, ey - uy * 4 * S, 10 * S, 0, Math.PI * 2);
      ctx.fill();
      ctx.stroke();
    }
    ctx.strokeStyle = "#141414";
    ctx.lineWidth = 8 * S;
    ctx.beginPath();
    ctx.moveTo(h.x, h.y);
    ctx.lineTo(ex, ey);
    ctx.stroke();
    // Red ribbon wound round the grip, a gold pommel, a red bow where the lash starts.
    ctx.strokeStyle = "#d8203a";
    ctx.lineWidth = 2.4 * S;
    for (const f of [0.3, 0.5, 0.7]) {
      const cx = h.x + ux * hl * f, cy = h.y + uy * hl * f;
      ctx.beginPath();
      ctx.moveTo(cx - uy * 5 * S - ux * 2 * S, cy + ux * 5 * S - uy * 2 * S);
      ctx.lineTo(cx + uy * 5 * S + ux * 2 * S, cy - ux * 5 * S + uy * 2 * S);
      ctx.stroke();
    }
    ctx.fillStyle = "#e0b02a";
    ctx.strokeStyle = "#8a5a00";
    ctx.lineWidth = 1.2 * S;
    ctx.beginPath();
    ctx.arc(ex, ey, 5 * S, 0, Math.PI * 2);
    ctx.fill();
    ctx.stroke();
    ctx.fillStyle = "#d8203a";
    for (const sgn of [-1, 1]) {
      ctx.beginPath();
      ctx.ellipse(h.x - uy * sgn * 4.5 * S, h.y + ux * sgn * 4.5 * S, 4.6 * S, 3 * S, Math.atan2(uy, ux) + (Math.PI / 2) * sgn, 0, Math.PI * 2);
      ctx.fill();
    }

    if (post >= 0) this.impact(ctx, post, alpha);
    ctx.restore();
  }

  /** A tapered, smoothed stroke through the rope points. */
  private rope(ctx: CanvasRenderingContext2D, p: Pt2[], alpha: number, scale: number, base: string, seam: string | null): void {
    const { S } = this;
    ctx.save();
    ctx.globalAlpha = alpha;
    const passes: [string, number][] = seam ? [[base, 1], [seam, 0.38]] : [[base, 1]];
    for (const [color, wf] of passes) {
      ctx.strokeStyle = color;
      for (let i = 0; i < N - 1; i++) {
        const w = (7.5 - 5.3 * (i / (N - 2))) * S * scale * wf;
        ctx.lineWidth = Math.max(0.8, w);
        const a = i === 0 ? p[0] : { x: (p[i - 1].x + p[i].x) / 2, y: (p[i - 1].y + p[i].y) / 2 };
        const b = i + 1 === N - 1 ? p[N - 1] : { x: (p[i].x + p[i + 1].x) / 2, y: (p[i].y + p[i + 1].y) / 2 };
        ctx.beginPath();
        ctx.moveTo(a.x, a.y);
        ctx.quadraticCurveTo(p[i].x, p[i].y, b.x, b.y);
        ctx.stroke();
      }
    }
    ctx.restore();
  }

  private impact(ctx: CanvasRenderingContext2D, post: number, alpha: number): void {
    const { S, dir, tx, ty } = this;
    // Welt: a red streak where it landed, along the lash's final line.
    ctx.save();
    ctx.translate(tx, ty);
    ctx.rotate(this.markAng);
    ctx.globalAlpha = alpha * 0.9;
    ctx.strokeStyle = "#d8203a";
    ctx.lineWidth = 4.2 * S;
    ctx.beginPath();
    ctx.moveTo(-20 * S, 0);
    ctx.lineTo(16 * S, 0);
    ctx.stroke();
    ctx.strokeStyle = "rgba(255,120,140,0.8)";
    ctx.lineWidth = 2 * S;
    ctx.beginPath();
    ctx.moveTo(-14 * S, 5 * S);
    ctx.lineTo(10 * S, 5 * S);
    ctx.stroke();
    ctx.restore();
    // Flash.
    if (post < 160) {
      const r = 1 - post / 160;
      const rad = (40 + 50 * (1 - r)) * S;
      const g = ctx.createRadialGradient(tx, ty, 0, tx, ty, rad);
      g.addColorStop(0, `rgba(255,255,240,${0.95 * r})`);
      g.addColorStop(0.4, `rgba(255,220,80,${0.6 * r})`);
      g.addColorStop(1, "rgba(255,200,60,0)");
      ctx.fillStyle = g;
      ctx.beginPath();
      ctx.arc(tx, ty, rad, 0, Math.PI * 2);
      ctx.fill();
    }
    // Shock rings.
    for (const [start, color, max] of [[0, "#ffd23a", 120], [90, "#e63946", 95]] as [number, string, number][]) {
      const u = (post - start) / 420;
      if (u < 0 || u > 1) continue;
      const e = 1 - (1 - u) ** 2;
      ctx.globalAlpha = alpha * (1 - u) * 0.85;
      ctx.strokeStyle = color;
      ctx.lineWidth = Math.max(0.5, 7 * (1 - u)) * S;
      ctx.beginPath();
      ctx.arc(tx, ty, (10 + max * e) * S, 0, Math.PI * 2);
      ctx.stroke();
    }
    // Speed lines.
    if (post < 240) {
      const u = post / 240;
      ctx.globalAlpha = alpha * (1 - u);
      ctx.strokeStyle = "#fff1a8";
      ctx.lineWidth = (3 - 2 * u) * S;
      for (let i = 0; i < 12; i++) {
        const a = (i / 12) * Math.PI * 2 + 0.26;
        const r0 = (16 + 30 * u) * S, r1 = (32 + 58 * u) * S;
        ctx.beginPath();
        ctx.moveTo(tx + Math.cos(a) * r0, ty + Math.sin(a) * r0);
        ctx.lineTo(tx + Math.cos(a) * r1, ty + Math.sin(a) * r1);
        ctx.stroke();
      }
    }
    // Sparks with little tails.
    ctx.globalAlpha = alpha * Math.max(0, 1 - post / 650);
    for (const s of this.sparks) {
      ctx.strokeStyle = s.c;
      ctx.lineWidth = s.r;
      ctx.beginPath();
      ctx.moveTo(s.x - s.vx * 0.035, s.y - s.vy * 0.035);
      ctx.lineTo(s.x, s.y);
      ctx.stroke();
    }
    // The comic burst with the word, popping in with an overshoot.
    if (post < 560) {
      const pop = post < 120 ? 0.35 + 0.95 * (post / 120) : 1.3 - 0.3 * Math.min(1, (post - 120) / 140);
      const fade = post > 400 ? 1 - (post - 400) / 160 : 1;
      const bx = tx + dir * 16 * S, by = ty - 66 * S;
      ctx.save();
      ctx.globalAlpha = alpha * fade;
      ctx.translate(bx, by);
      ctx.rotate(-0.2 * dir + Math.sin(post / 40) * 0.03);
      ctx.scale(pop, pop);
      ctx.beginPath();
      const spikes = 14, ro = 48 * S, ri = 31 * S;
      for (let i = 0; i < spikes * 2; i++) {
        const r = i % 2 ? ri : ro;
        const a = (i / (spikes * 2)) * Math.PI * 2;
        ctx.lineTo(Math.cos(a) * r, Math.sin(a) * r);
      }
      ctx.closePath();
      ctx.fillStyle = "#ffe14a";
      ctx.strokeStyle = "#d8203a";
      ctx.lineWidth = 3 * S;
      ctx.fill();
      ctx.stroke();
      ctx.font = `900 ${Math.round(28 * S)}px 'Microsoft YaHei', system-ui, sans-serif`;
      ctx.textAlign = "center";
      ctx.textBaseline = "middle";
      ctx.lineWidth = 5 * S;
      ctx.strokeStyle = "#fff";
      ctx.strokeText(this.o.text, 0, 1);
      ctx.fillStyle = "#d8203a";
      ctx.fillText(this.o.text, 0, 1);
      ctx.restore();
    }
    ctx.globalAlpha = alpha;
  }
}
