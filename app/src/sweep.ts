// The pet sweeps the desktop: it gets on its broom, rides over to each patch
// of strewn icons, gives it a sweep, and the icons there fly off into their
// places in the columns; then it rides home. Only where the icons sit
// changes, never the files, and "put them back" undoes it all.
//
// The choreography is here; a Stage does the moving: the pet window and the
// real desktop in the app, a pretend desktop in the preview page.

/**
 * One desktop icon: its cell now (x, y) and where it goes (tx, ty), physical
 * screen px. `stow`: it goes into the 桌面收纳 folder (tx, ty is the folder's
 * cell) and leaves the desktop when it gets there.
 */
export interface DeskItem { key: string; kind: string; x: number; y: number; tx: number; ty: number; stow?: boolean }
export interface DeskPlan { items: DeskItem[]; cw: number; ch: number; strays: number }

export interface Stage {
  plan(): Promise<DeskPlan | null>;
  /** Keep every icon's place, for putting them back. False: the desktop cannot be touched. */
  begin(): Promise<boolean>;
  /** Icons on their way: [key, x, y]. Fire and forget, a frame at a time. */
  move(m: [string, number, number][]): void;
  end(moved: number): Promise<void>;
  /** Ride until the broom's bristles touch (x, y), in `ms`. */
  flyTo(x: number, y: number, ms: number): Promise<void>;
  /** Where the bristles are now. */
  where(): Promise<{ x: number; y: number }>;
  /** Ride back to where it stood. */
  home(): Promise<void>;
  /** An icon flying into the folder got there. */
  arrived?(key: string): void;
  /** The files behind the stowed icons go into the folder; false if none could. */
  stow?(keys: string[]): Promise<boolean>;
}

export type Pose = "off" | "ride" | "sweep";

/** What the window draws: the broom, which way it goes, and the dust it raises. */
export interface Broom {
  pose: Pose;
  /** Riding: 1 to the right, -1 to the left. */
  dir: 1 | -1;
  /** When the current stroke began (performance.now()). */
  at: number;
  dust: { x: number; y: number; vx: number; vy: number; t0: number; r: number }[];
  lastDust: number;
}

export const newBroom = (): Broom => ({ pose: "off", dir: -1, at: 0, dust: [], lastDust: 0 });

/** Where the bristles touch the floor while it sweeps, from its feet, in logical px at size 1. */
export const BRISTLE: [number, number] = [-76, -3];
/** One stroke of the broom, and how long it lingers at each patch. */
const STROKE_MS = 300;
const STOP_MS = 620;
/** An icon's flight to its place. */
const FLIGHT_MS = 480;
const MAX_STOPS = 7;

const wait = (ms: number) => new Promise<void>((r) => window.setTimeout(r, ms));

interface Stop { x: number; y: number; items: DeskItem[] }

/** The strewn icons in patches, in the order it reaches them from where it is: nearest next. */
export function stopsFor(moving: DeskItem[], cw: number, ch: number, from: { x: number; y: number }): Stop[] {
  let size = Math.max(cw * 2.6, 220);
  let groups: Map<string, DeskItem[]>;
  for (;;) {
    groups = new Map();
    for (const it of moving) {
      const k = `${Math.floor(it.x / size)},${Math.floor(it.y / size)}`;
      groups.set(k, [...(groups.get(k) ?? []), it]);
    }
    if (groups.size <= MAX_STOPS) break;
    size *= 1.35;
  }
  const patches: Stop[] = [...groups.values()].map((items) => {
    const cx = items.reduce((s, i) => s + i.x + cw / 2, 0) / items.length;
    const cy = items.reduce((s, i) => s + i.y + ch / 2, 0) / items.length;
    // The bristles come down on the patch's right, so the stroke goes left, towards the columns.
    return { x: cx + cw * 0.35, y: cy + ch * 0.3, items };
  });
  const out: Stop[] = [];
  let at = from;
  while (patches.length) {
    let best = 0;
    for (let i = 1; i < patches.length; i++) {
      if (Math.hypot(patches[i].x - at.x, patches[i].y - at.y) < Math.hypot(patches[best].x - at.x, patches[best].y - at.y)) best = i;
    }
    const [s] = patches.splice(best, 1);
    out.push(s);
    at = s;
  }
  return out;
}

export interface Hooks {
  /** The pet's body goes with the broom: leaning into a ride, or into a stroke. */
  pose(p: Pose, dir: 1 | -1): void;
}

/**
 * Sweep the desktop. Null: there is no desktop to sweep (icons hidden or
 * auto-arranged, or not on Windows); 0: it was tidy already; otherwise how
 * many icons went back in line.
 */
export async function sweep(stage: Stage, broom: Broom, hooks: Hooks): Promise<number | null> {
  const plan = await stage.plan();
  if (!plan) return null;
  const { cw, ch } = plan;
  const moving = plan.items.filter((i) => Math.abs(i.x - i.tx) > cw / 2 || Math.abs(i.y - i.ty) > ch / 2);
  if (!moving.length) return 0;
  if (!(await stage.begin())) return null;

  const start = await stage.where();
  const stops = stopsFor(moving, cw, ch, start);
  // Each icon flies along a low arc, landing a touch past its place and settling.
  const flights: { it: DeskItem; t0: number; done: boolean }[] = [];
  const ticker = window.setInterval(() => {
    const now = performance.now();
    const batch: [string, number, number][] = [];
    for (const f of flights) {
      if (f.done || now < f.t0) continue;
      const k = Math.min(1, (now - f.t0) / FLIGHT_MS);
      const e = 1 - Math.pow(1 - k, 3);
      const lift = Math.sin(Math.PI * k) * Math.min(110, Math.hypot(f.it.tx - f.it.x, f.it.ty - f.it.y) * 0.22);
      batch.push([f.it.key, Math.round(f.it.x + (f.it.tx - f.it.x) * e), Math.round(f.it.y + (f.it.ty - f.it.y) * e - lift)]);
      if (k >= 1) {
        f.done = true;
        if (f.it.stow) stage.arrived?.(f.it.key);
      }
    }
    if (batch.length) stage.move(batch);
  }, 33);

  try {
    let at = start;
    for (const s of stops) {
      const d = Math.hypot(s.x - at.x, s.y - at.y);
      broom.dir = s.x < at.x ? -1 : 1;
      broom.pose = "ride";
      hooks.pose("ride", broom.dir);
      await stage.flyTo(s.x, s.y, Math.max(260, Math.min(760, d / 1.6)));
      at = s;
      broom.pose = "sweep";
      broom.at = performance.now();
      hooks.pose("sweep", -1);
      // The stroke reaches them in turn, right to left.
      const order = [...s.items].sort((a, b) => b.x - a.x);
      order.forEach((it, k) => flights.push({ it, t0: performance.now() + 150 + k * 45, done: false }));
      await wait(STOP_MS);
    }
    // The last ones land while it straightens up.
    const until = performance.now() + 1500;
    while (flights.some((f) => !f.done) && performance.now() < until) await wait(60);
  } finally {
    window.clearInterval(ticker);
    // Wherever the flights got to, every icon ends exactly in its place.
    stage.move(moving.map((i) => [i.key, i.tx, i.ty]));
  }
  // What flew into the folder goes into it for real.
  const stowed = moving.filter((i) => i.stow).map((i) => i.key);
  if (stowed.length && stage.stow) await stage.stow(stowed);
  broom.pose = "ride";
  broom.dir = start.x < (await stage.where()).x ? -1 : 1;
  hooks.pose("ride", broom.dir);
  await stage.home();
  broom.pose = "off";
  hooks.pose("off", -1);
  await stage.end(moving.length);
  return moving.length;
}

// ------------------------------------------------------------ drawing

const WOOD = "#d2ab83";
const WOOD_DARK = "#b88d66";
const STRAW = "#f3dca6";
const STRAW_DARK = "#dcb878";
const LINE = "rgba(125,102,114,.9)";

/** A broom from the end of its handle (hx, hy) to where the straw is bound (bx, by), the straw going on `len` beyond. */
function drawBroomAt(ctx: CanvasRenderingContext2D, hx: number, hy: number, bx: number, by: number, len: number, band: string, splay: number) {
  const a = Math.atan2(by - hy, bx - hx);
  ctx.save();
  ctx.lineCap = ctx.lineJoin = "round";
  // The handle: a round stick with a light edge.
  ctx.strokeStyle = LINE;
  ctx.lineWidth = 7.5;
  ctx.beginPath();
  ctx.moveTo(hx, hy);
  ctx.lineTo(bx, by);
  ctx.stroke();
  ctx.strokeStyle = WOOD;
  ctx.lineWidth = 5;
  ctx.stroke();
  ctx.strokeStyle = "rgba(255,246,232,.7)";
  ctx.lineWidth = 1.4;
  ctx.beginPath();
  ctx.moveTo(hx + Math.sin(a) * 1.2, hy - Math.cos(a) * 1.2);
  ctx.lineTo(bx + Math.sin(a) * 1.2, by - Math.cos(a) * 1.2);
  ctx.stroke();
  // The straw: a fan, splayed a little more as it pushes.
  ctx.translate(bx, by);
  ctx.rotate(a);
  const w0 = 6, w1 = 15 + splay * 4;
  ctx.beginPath();
  ctx.moveTo(-2, -w0);
  ctx.quadraticCurveTo(len * 0.55, -w1 * 0.9, len, -w1);
  ctx.quadraticCurveTo(len + 5, 0, len, w1);
  ctx.quadraticCurveTo(len * 0.55, w1 * 0.9, -2, w0);
  ctx.closePath();
  ctx.fillStyle = STRAW;
  ctx.fill();
  ctx.strokeStyle = LINE;
  ctx.lineWidth = 1.6;
  ctx.stroke();
  ctx.save();
  ctx.clip();
  ctx.strokeStyle = STRAW_DARK;
  ctx.lineWidth = 1.2;
  for (let i = -3; i <= 3; i++) {
    ctx.beginPath();
    ctx.moveTo(4, i * 1.6);
    ctx.lineTo(len + 4, i * (w1 / 3.2));
    ctx.stroke();
  }
  ctx.restore();
  // The binding, in the pet's own colour.
  ctx.fillStyle = band;
  ctx.strokeStyle = LINE;
  ctx.lineWidth = 1.4;
  ctx.beginPath();
  ctx.roundRect(-4, -w0 - 1.5, 8, (w0 + 1.5) * 2, 2.5);
  ctx.fill();
  ctx.stroke();
  ctx.restore();
  // A knot of darker wood at the handle's end.
  ctx.fillStyle = WOOD_DARK;
  ctx.strokeStyle = LINE;
  ctx.lineWidth = 1.4;
  ctx.beginPath();
  ctx.arc(hx, hy, 3.6, 0, Math.PI * 2);
  ctx.fill();
  ctx.stroke();
}

/** Strokes to the left and back: the straw drags along the floor, the handle (behind the pet) leans with it. */
function sweepBroom(ctx: CanvasRenderingContext2D, b: Broom, now: number, band: string) {
  const p = (now - b.at) / STROKE_MS;
  const s = Math.abs(Math.sin(p * Math.PI));
  const tip: [number, number] = [BRISTLE[0] + 12 - s * 22, BRISTLE[1]];
  const top: [number, number] = [-4 - s * 8, -62];
  const len = 34;
  const L = Math.hypot(tip[0] - top[0], tip[1] - top[1]);
  const bind: [number, number] = [top[0] + (tip[0] - top[0]) * (1 - len / L), top[1] + (tip[1] - top[1]) * (1 - len / L)];
  drawBroomAt(ctx, top[0], top[1], bind[0], bind[1], len, band, s);
  // Each stroke throws up a little dust where the straw leaves off.
  if (Math.floor(p) !== Math.floor((b.lastDust - b.at) / STROKE_MS) || b.lastDust < b.at) {
    b.lastDust = now;
    for (let i = 0; i < 5; i++) {
      b.dust.push({ x: tip[0] - 6, y: tip[1] - 3, vx: -(30 + Math.random() * 60), vy: -(16 + Math.random() * 30), t0: now, r: 2.8 + Math.random() * 3 });
    }
  }
}

/** Its body rocks with each stroke (degrees), while it sweeps. */
export function sweepLean(b: Broom, now: number): number {
  return b.pose === "sweep" ? -6 - Math.abs(Math.sin(((now - b.at) / STROKE_MS) * Math.PI)) * 6 : 0;
}

/**
 * The broom and its dust, with the pet's feet at (fx, fy) and `k` its size.
 * `back` is drawn before the pet (the broom it sweeps with, held at its side,
 * the handle behind it); `front` after (the stick it rides on, and the dust).
 */
export function drawBroom(ctx: CanvasRenderingContext2D, b: Broom, fx: number, fy: number, k: number, now: number, band: string, layer: "back" | "front") {
  ctx.save();
  ctx.translate(fx, fy);
  ctx.scale(k, k);
  if (layer === "back") {
    if (b.pose === "sweep") sweepBroom(ctx, b, now, band);
    ctx.restore();
    return;
  }
  if (b.pose === "ride") {
    // Side-saddle on the stick, the straw trailing; a bob as it goes, and a wisp of dust behind.
    const bob = Math.sin(now / 95) * 1.6;
    ctx.save();
    ctx.translate(0, -10 + bob);
    ctx.scale(b.dir, 1);
    ctx.rotate(-0.07);
    drawBroomAt(ctx, 60, 0, -24, 0, 34, band, 0);
    ctx.restore();
    if (now - b.lastDust > 110) {
      b.lastDust = now;
      b.dust.push({ x: -b.dir * 60, y: -10 + bob, vx: -b.dir * (20 + Math.random() * 20), vy: 6 + Math.random() * 10, t0: now, r: 1.6 + Math.random() * 1.6 });
    }
  }
  // The dust: soft paper puffs, rising a little and fading.
  for (let i = b.dust.length - 1; i >= 0; i--) {
    const d = b.dust[i];
    const t = (now - d.t0) / 1000;
    if (t > 0.75) {
      b.dust.splice(i, 1);
      continue;
    }
    const q = t / 0.75;
    ctx.globalAlpha = 0.95 * (1 - q);
    ctx.fillStyle = "#f8f0e6";
    ctx.strokeStyle = "#c9b9b4";
    ctx.lineWidth = 1;
    ctx.beginPath();
    ctx.arc(d.x + d.vx * t, d.y + d.vy * t, d.r * (1 + q * 1.4), 0, Math.PI * 2);
    ctx.fill();
    ctx.stroke();
  }
  ctx.globalAlpha = 1;
  ctx.restore();
}
