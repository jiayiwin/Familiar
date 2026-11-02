// Full-monitor transparent overlay (design §6): bugs crawling along the bottom
// of the screen and coin pop-ups. (Tug of war is acted out by the pets themselves.)
// Click-through everywhere except on bugs; everything pauses while an agent
// waits on the user.

import { connect, getJson, inTauri, postJson, subscribe, type Bug, type Conn, type WorldState } from "./api";
import { initLang, t as tr } from "./i18n"; // `t` is a common local name here

const q = new URLSearchParams(location.search);
const MON = { x: Number(q.get("x") ?? 0), y: Number(q.get("y") ?? 0), w: Number(q.get("w") ?? innerWidth), h: Number(q.get("h") ?? innerHeight) };
const PRIMARY = q.get("primary") === "1";
const canvas = document.getElementById("ov") as HTMLCanvasElement;
const ctx = canvas.getContext("2d")!;

const BUG = 14;
const BOSS = 30;
const GRAVITY = 1400;

interface Crawler {
  bug: Bug;
  x: number;
  y: number;
  vx: number;
  vy: number;
  falling: boolean;
  hitAt: number;
  deadAt: number;
  turnAt: number;
}

interface Popup {
  x: number;
  y: number;
  text: string;
  color: string;
  t0: number;
}

const crawlers = new Map<number, Crawler>();

/** Ball toss between two pets: positions are read from their windows. */
interface Toss { a: string; b: string; throws: number; each: number; t0: number; caught: boolean }
let toss: Toss | null = null;
const tossPos = new Map<string, { x: number; y: number } | null>();
let tossBall: { x: number; y: number } | null = null;
const popups: Popup[] = [];
let world: WorldState | null = null;
let allowed = true;
let shown = false;
let lastHits = "";
let lastT = 0;

function size() {
  const dpr = devicePixelRatio || 1;
  canvas.width = Math.round(MON.w * dpr);
  canvas.height = Math.round(MON.h * dpr);
  canvas.style.width = `${MON.w}px`;
  canvas.style.height = `${MON.h}px`;
  ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
}

const floorY = () => MON.h - 3;

function sfx(name: string) {
  if (!allowed || world?.focus) return;
  const a = new Audio(`sfx/${name}.wav`);
  a.volume = 0.5;
  a.play().catch(() => {});
}

/** Feet of a pet window in this overlay's coordinates, or null if it is on another monitor. */
async function petFeet(pet: string): Promise<{ x: number; y: number } | null> {
  if (!inTauri) return { x: MON.w * 0.8, y: MON.h - 40 };
  const { WebviewWindow } = await import("@tauri-apps/api/webviewWindow");
  const w = await WebviewWindow.getByLabel(`pet-${pet}`);
  if (!w) return null;
  const sc = await w.scaleFactor();
  const p = (await w.outerPosition()).toLogical(sc);
  const s = (await w.outerSize()).toLogical(sc);
  const x = p.x + s.width / 2 - MON.x;
  const y = p.y + s.height - 18 - MON.y;
  if (x < 0 || x >= MON.w || y < -50 || y > MON.h + 50) return null;
  return { x, y };
}

function addCrawler(bug: Bug, x: number, y: number, falling: boolean) {
  if (crawlers.has(bug.id)) return;
  crawlers.set(bug.id, {
    bug,
    x,
    y,
    vx: (Math.random() < 0.5 ? -1 : 1) * (bug.boss ? 18 : 26 + Math.random() * 22),
    vy: falling ? -180 - Math.random() * 120 : 0,
    falling,
    hitAt: 0,
    deadAt: 0,
    turnAt: performance.now() + 2000 + Math.random() * 4000,
  });
}

async function spawn(bug: Bug) {
  const feet = await petFeet(bug.pet);
  if (feet) {
    addCrawler(bug, feet.x + (Math.random() - 0.5) * 30, feet.y - 20, true);
  }
}

/** Bugs we were not told about (overlay reloaded): the overlay that holds the pet adopts them. */
async function adopt(bugs: Bug[]) {
  for (const b of bugs) {
    if (crawlers.has(b.id)) continue;
    const feet = await petFeet(b.pet);
    if (feet) addCrawler(b, Math.random() * MON.w, floorY(), false);
    else if (PRIMARY && !(await petFeet(b.pet))) {
      // The pet is gone; the primary screen takes the orphan.
      addCrawler(b, Math.random() * MON.w, floorY(), false);
    }
  }
}

function onWorld(w: WorldState) {
  world = w;
  const live = new Set(w.bugs.map((b) => b.id));
  for (const [id, c] of crawlers) {
    if (!live.has(id) && !c.deadAt) {
      // Dug into the floor (time out) or killed elsewhere.
      c.deadAt = performance.now();
    }
    const fresh = w.bugs.find((b) => b.id === id);
    if (fresh) c.bug = fresh;
  }
  void adopt(w.bugs);
}

async function squash(conn: Conn, c: Crawler) {
  if (world?.frozen || c.deadAt) return;
  c.hitAt = performance.now();
  const r = await postJson<{ ok: boolean; message?: string; result?: { hp: number; killed: boolean; boss: boolean; coins: number } }>(
    conn,
    "/v1/world/squash",
    { id: c.bug.id },
  );
  if (!r.ok || !r.result) return;
  c.bug.hp = r.result.hp;
  if (r.result.killed) {
    c.deadAt = performance.now();
    sfx(r.result.boss ? "boss-down" : "splat");
    if (r.result.coins > 0) {
      popups.push({ x: c.x, y: c.y - 20, text: `+${r.result.coins}`, color: "#f2a93b", t0: performance.now() });
      setTimeout(() => sfx("coin"), 150);
    }
    if (r.result.boss) popups.push({ x: c.x, y: c.y - 44, text: tr("Boss 倒下了！石化解除", "Boss down! Stone lifted"), color: "#e63946", t0: performance.now() });
  } else {
    sfx(c.bug.boss ? "boss-hit" : "splat");
  }
}

// ------------------------------------------------------------ drawing

function drawBug(c: Crawler, now: number) {
  const s = c.bug.boss ? BOSS : BUG;
  const frozen = !!world?.frozen;
  ctx.save();
  ctx.translate(c.x, c.y);
  if (c.deadAt) {
    const k = Math.min(1, (now - c.deadAt) / 700);
    ctx.globalAlpha = 1 - k;
    ctx.fillStyle = c.bug.boss ? "#7a1f2b" : "#4b5d2a";
    ctx.beginPath();
    ctx.ellipse(0, s * 0.1, s * (0.7 + k * 0.3), s * 0.15, 0, 0, Math.PI * 2);
    ctx.fill();
    ctx.restore();
    return;
  }
  if (frozen) ctx.globalAlpha = 0.45;
  const dir = c.vx < 0 ? -1 : 1;
  ctx.scale(dir, 1);
  const wiggle = c.falling || frozen ? 0 : Math.sin(now / 60) * 1.5;
  const hit = now - c.hitAt < 120 ? 0.8 : 1;
  ctx.scale(1, hit);
  // Legs.
  ctx.strokeStyle = "#2b1d16";
  ctx.lineWidth = c.bug.boss ? 2 : 1.2;
  for (let i = -1; i <= 1; i++) {
    const lx = i * s * 0.28;
    ctx.beginPath();
    ctx.moveTo(lx, -s * 0.2);
    ctx.lineTo(lx + (i % 2 ? wiggle : -wiggle), 0);
    ctx.stroke();
  }
  // Body and head.
  ctx.fillStyle = c.bug.boss ? "#c2283a" : "#6b8e23";
  ctx.strokeStyle = "#2b1d16";
  ctx.beginPath();
  ctx.ellipse(0, -s * 0.35, s * 0.5, s * 0.32, 0, 0, Math.PI * 2);
  ctx.fill();
  ctx.stroke();
  ctx.beginPath();
  ctx.moveTo(0, -s * 0.66);
  ctx.lineTo(0, -s * 0.05);
  ctx.stroke();
  ctx.fillStyle = "#2b1d16";
  ctx.beginPath();
  ctx.arc(s * 0.5, -s * 0.38, s * 0.18, 0, Math.PI * 2);
  ctx.fill();
  ctx.fillStyle = "#fff";
  ctx.fillRect(s * 0.52, -s * 0.46, 2, 2);
  // Antennae.
  ctx.beginPath();
  ctx.moveTo(s * 0.6, -s * 0.5);
  ctx.lineTo(s * 0.8, -s * 0.72);
  ctx.stroke();
  ctx.restore();
  if (c.bug.boss) {
    // HP bar.
    const w = BOSS * 1.4;
    ctx.fillStyle = "rgba(0,0,0,0.5)";
    ctx.fillRect(c.x - w / 2, c.y - BOSS - 10, w, 5);
    ctx.fillStyle = "#e63946";
    ctx.fillRect(c.x - w / 2, c.y - BOSS - 10, (w * c.bug.hp) / c.bug.max_hp, 5);
    ctx.fillStyle = frozen ? "rgba(43,29,22,0.5)" : "#2b1d16";
    ctx.font = "bold 11px system-ui, sans-serif";
    ctx.textAlign = "center";
    ctx.fillText("BOSS", c.x, c.y - BOSS - 14);
  }
}


function drawPopups(now: number) {
  for (let i = popups.length - 1; i >= 0; i--) {
    const p = popups[i];
    const k = (now - p.t0) / 1400;
    if (k >= 1) {
      popups.splice(i, 1);
      continue;
    }
    ctx.globalAlpha = 1 - k;
    ctx.font = "bold 15px system-ui, sans-serif";
    ctx.textAlign = "center";
    ctx.lineWidth = 3;
    ctx.strokeStyle = "rgba(255,255,255,0.9)";
    ctx.strokeText(p.text, p.x, p.y - k * 30);
    ctx.fillStyle = p.color;
    ctx.fillText(p.text, p.x, p.y - k * 30);
  }
  ctx.globalAlpha = 1;
}

/** Badminton / ping-pong between two pets: the overlay draws the flight. */
interface Rally { game: string; a: string; t0: number; each: number; hits: number; ms: number; pa: Box; pb: Box }
interface Box { x: number; y: number; bottom: number; h: number }
let rally: Rally | null = null;

async function petBox(pet: string): Promise<Box | null> {
  if (!inTauri) return null;
  const { WebviewWindow } = await import("@tauri-apps/api/webviewWindow");
  const w = await WebviewWindow.getByLabel(`pet-${pet}`);
  if (!w) return null;
  const sc = await w.scaleFactor();
  const p = (await w.outerPosition()).toLogical(sc);
  const s = (await w.outerSize()).toLogical(sc);
  return { x: p.x + s.width / 2 - MON.x, y: p.y + s.height * 0.55 - MON.y, bottom: p.y + s.height - MON.y - 2, h: s.height };
}

async function startRally(d: Record<string, unknown>) {
  const walk = Number(d.walk) || 1300;
  const recv = performance.now();
  await new Promise((r) => setTimeout(r, walk - 60));
  const [pa, pb] = [await petBox(String(d.anchor)), await petBox(String(d.mover))];
  if (!pa || !pb || pa.x < 0 || pa.x > MON.w) return;
  rally = { game: String(d.game), a: String(d.anchor), t0: recv + walk, each: Number(d.each), hits: Number(d.hits), ms: Number(d.ms), pa, pb };
  // A hidden window gets no animation frames: show it first.
  void setShown(allowed);
}

function drawRally(now: number): boolean {
  const r = rally;
  if (!r) return false;
  const t = now - r.t0;
  if (t > r.ms + 300) {
    rally = null;
    return false;
  }
  const ground = Math.max(r.pa.bottom, r.pb.bottom);
  const [L, R] = r.pa.x < r.pb.x ? [r.pa, r.pb] : [r.pb, r.pa];
  const mid = (L.x + R.x) / 2;
  ctx.save();
  if (r.game === "badminton") {
    // Net: a pole and a mesh band at chest height.
    ctx.fillStyle = "#5b3a29";
    ctx.fillRect(mid - 1.5, ground - 64, 3, 64);
    ctx.strokeStyle = "rgba(60,60,60,0.55)";
    ctx.lineWidth = 1;
    for (let y = ground - 62; y <= ground - 34; y += 5) {
      ctx.beginPath();
      ctx.moveTo(mid - 7, y);
      ctx.lineTo(mid + 7, y);
      ctx.stroke();
    }
    ctx.fillStyle = "#fff";
    ctx.fillRect(mid - 8, ground - 65, 16, 3);
  } else {
    // Table between the pets with a net in the middle.
    const tl = L.x + 34, tr = R.x - 34, ty = ground - 26;
    ctx.fillStyle = "#1d6b4f";
    ctx.fillRect(tl, ty, tr - tl, 6);
    ctx.fillStyle = "#fff";
    ctx.fillRect(tl, ty, tr - tl, 1.5);
    ctx.fillStyle = "#3b3b3b";
    ctx.fillRect(tl + 6, ty + 6, 3, 20);
    ctx.fillRect(tr - 9, ty + 6, 3, 20);
    ctx.fillStyle = "rgba(240,240,240,0.9)";
    ctx.fillRect(mid - 1, ty - 9, 2, 9);
  }
  if (t >= 0) {
    const i = Math.min(r.hits - 1, Math.floor(t / r.each));
    const from = i % 2 === 0 ? r.pa : r.pb;
    const to = i % 2 === 0 ? r.pb : r.pa;
    const last = i === r.hits - 1;
    let k = (t - i * r.each) / r.each;
    if (!last) k = Math.min(1, k);
    const dir = Math.sign(to.x - from.x);
    const fx = from.x + dir * 34, tx = to.x - dir * 30;
    let x: number, y: number;
    if (r.game === "badminton") {
      const fy = from.y - from.h * 0.12, ty2 = to.y - to.h * 0.12;
      x = fx + (tx - fx) * k;
      y = fy + (ty2 - fy) * k - Math.sin(Math.min(1, k) * Math.PI) * 110;
      if (last && k > 1) {
        // Past the racket and down to the floor.
        x = tx + dir * (k - 1) * 60;
        y = Math.min(ground - 4, ty2 + (k - 1) * (k - 1) * 900);
      }
      const ang = Math.atan2(y - (fy + (ty2 - fy) * Math.max(0, k - 0.02)), dir);
      ctx.translate(x, y);
      ctx.rotate(k < 0.5 ? (dir > 0 ? 0 : Math.PI) - 0.6 * dir : ang);
      ctx.fillStyle = "#fff";
      ctx.strokeStyle = "#999";
      ctx.lineWidth = 1;
      ctx.beginPath();
      ctx.moveTo(0, 0);
      ctx.lineTo(-11, -5);
      ctx.lineTo(-11, 5);
      ctx.closePath();
      ctx.fill();
      ctx.stroke();
      ctx.fillStyle = "#d62828";
      ctx.beginPath();
      ctx.arc(1, 0, 3, 0, Math.PI * 2);
      ctx.fill();
    } else {
      const table = ground - 27;
      const fy = from.y, ty2 = to.y;
      // One bounce on the receiver's half.
      const b = 0.62;
      x = fx + (tx - fx) * Math.min(k, 1.6);
      if (k < b) y = fy + (table - fy) * (k / b) ** 2;
      else if (k <= 1 || !last) y = table - Math.sin(((Math.min(k, 1) - b) / (1 - b)) * Math.PI * 0.5) * (table - ty2);
      else y = Math.min(ground - 3, ty2 + (k - 1) * (k - 1) * 1200);
      ctx.fillStyle = "#ff9f1c";
      ctx.strokeStyle = "#b36b00";
      ctx.lineWidth = 1;
      ctx.beginPath();
      ctx.arc(x, y, 4, 0, Math.PI * 2);
      ctx.fill();
      ctx.stroke();
    }
  }
  ctx.restore();
  return true;
}

/** Head of a pet window in this overlay's coordinates. */
async function petHead(pet: string): Promise<{ x: number; y: number } | null> {
  if (!inTauri) return null;
  const { WebviewWindow } = await import("@tauri-apps/api/webviewWindow");
  const w = await WebviewWindow.getByLabel(`pet-${pet}`);
  if (!w) return null;
  const sc = await w.scaleFactor();
  const p = (await w.outerPosition()).toLogical(sc);
  const s = (await w.outerSize()).toLogical(sc);
  return { x: p.x + s.width / 2 - MON.x, y: p.y + s.height * 0.5 - MON.y };
}

function drawToss(now: number) {
  if (!toss || toss.caught) {
    tossBall = null;
    return false;
  }
  const t = now - toss.t0;
  const i = Math.floor(t / toss.each);
  if (i >= toss.throws) {
    tossBall = null;
    return false;
  }
  const from = tossPos.get(i % 2 === 0 ? toss.a : toss.b);
  const to = tossPos.get(i % 2 === 0 ? toss.b : toss.a);
  if (!from || !to) return true;
  const k = Math.min(1, (t - i * toss.each) / (toss.each * 0.8));
  const x = from.x + (to.x - from.x) * k;
  const y = from.y + (to.y - from.y) * k - Math.sin(Math.PI * k) * 90;
  tossBall = { x, y };
  ctx.save();
  ctx.fillStyle = "rgba(0,0,0,0.15)";
  ctx.beginPath();
  ctx.ellipse(x, Math.max(from.y, to.y) + 40, 8 * (1 - Math.sin(Math.PI * k) * 0.5), 3, 0, 0, Math.PI * 2);
  ctx.fill();
  ctx.translate(x, y);
  ctx.rotate(t / 120);
  ctx.fillStyle = "#e63946";
  ctx.beginPath();
  ctx.arc(0, 0, 9, 0, Math.PI * 2);
  ctx.fill();
  ctx.strokeStyle = "#fff";
  ctx.lineWidth = 2;
  ctx.beginPath();
  ctx.arc(0, 0, 9, -0.6, 0.6);
  ctx.stroke();
  ctx.beginPath();
  ctx.arc(0, 0, 9, Math.PI - 0.6, Math.PI + 0.6);
  ctx.stroke();
  ctx.restore();
  return true;
}

async function startToss(d: { a: string; b: string; throws: number; each: number }) {
  toss = { ...d, t0: performance.now(), caught: false };
  for (const p of [d.a, d.b]) tossPos.set(p, await petHead(p));
  // Only the overlay that holds the first pet draws the ball.
  if (!tossPos.get(d.a) || tossPos.get(d.a)!.x < 0 || tossPos.get(d.a)!.x > MON.w) toss = null;
  else void setShown(allowed);
}

async function setShown(on: boolean) {
  if (on === shown || !inTauri) return;
  shown = on;
  const { getCurrentWebviewWindow } = await import("@tauri-apps/api/webviewWindow");
  const w = getCurrentWebviewWindow();
  await (on ? w.show() : w.hide());
}

async function reportHits() {
  const ball = tossBall ? [{ x: tossBall.x - 16, y: tossBall.y - 16, w: 32, h: 32 }] : [];
  const rects = world?.frozen
    ? []
    : [...ball, ...[...crawlers.values()]
        .filter((c) => !c.deadAt)
        .map((c) => {
          const s = (c.bug.boss ? BOSS : BUG) + 10;
          return { x: c.x - s / 2, y: c.y - s, w: s, h: s + 4 };
        })];
  const key = JSON.stringify(rects.map((r) => [Math.round(r.x / 4), Math.round(r.y / 4)]));
  if (key === lastHits || !inTauri) return;
  lastHits = key;
  const { invoke } = await import("@tauri-apps/api/core");
  await invoke("set_hit_rects", { rects }).catch(() => {});
}

function step(now: number) {
  const dt = Math.min(0.05, (now - (lastT || now)) / 1000);
  lastT = now;
  const frozen = !!world?.frozen;
  for (const [id, c] of crawlers) {
    if (c.deadAt) {
      if (now - c.deadAt > 800) crawlers.delete(id);
      continue;
    }
    if (c.falling) {
      c.vy += GRAVITY * dt;
      c.y += c.vy * dt;
      c.x += c.vx * dt * 2;
      if (c.y >= floorY()) {
        c.y = floorY();
        c.falling = false;
      }
    } else if (!frozen) {
      c.x += c.vx * dt;
      if (c.x < 8 || c.x > MON.w - 8) {
        c.vx = -c.vx;
        c.x = Math.max(8, Math.min(MON.w - 8, c.x));
      }
      if (now > c.turnAt) {
        c.vx = -c.vx;
        c.turnAt = now + 3000 + Math.random() * 6000;
      }
    }
  }

  ctx.clearRect(0, 0, MON.w, MON.h);
  for (const c of crawlers.values()) drawBug(c, now);
  const tossing = drawToss(now) || drawRally(now);
  drawPopups(now);
  void setShown(allowed && (crawlers.size > 0 || popups.length > 0 || tossing));
  void reportHits();
  requestAnimationFrame(step);
}

async function main() {
  size();
  const conn = await connect();
  await initLang(conn);
  onWorld(await getJson<WorldState>(conn, "/v1/world"));
  subscribe(conn, (type, data) => {
    if (type === "world.state") onWorld(data as WorldState);
    else if (type === "bug.spawn") void spawn(data as Bug);
    else if (type === "duo") {
      const d = data as Record<string, unknown>;
      if (d.game === "badminton" || d.game === "pingpong") void startRally(d);
    } else if (type === "toss") {
      const d = data as { state: string; a: string; b: string; throws: number; each: number; coins?: number };
      if (d.state === "start") void startToss(d);
      else if (toss) {
        if (d.state === "caught" && tossBall) {
          popups.push({ x: tossBall.x, y: tossBall.y - 16, text: tr(`截胡！+${d.coins ?? 0}`, `Stolen! +${d.coins ?? 0}`), color: "#e63946", t0: performance.now() });
          sfx("coin");
        }
        toss = null;
      }
    }
  });

  canvas.addEventListener("mousedown", (e) => {
    // The ball in the air first: catching it steals it.
    if (tossBall && Math.hypot(e.offsetX - tossBall.x, e.offsetY - tossBall.y) < 18) {
      void postJson(conn, "/v1/world/catch", {});
      return;
    }
    // The topmost live bug under the cursor.
    const hit = [...crawlers.values()].reverse().find((c) => {
      const s = (c.bug.boss ? BOSS : BUG) + 10;
      return !c.deadAt && Math.abs(e.offsetX - c.x) <= s / 2 && e.offsetY <= c.y + 4 && e.offsetY >= c.y - s;
    });
    if (hit) void squash(conn, hit);
  });

  if (inTauri) {
    const { getCurrentWebviewWindow } = await import("@tauri-apps/api/webviewWindow");
    const w = getCurrentWebviewWindow();
    await w.listen<boolean>("overlay:visible", (e) => {
      allowed = e.payload;
      if (!allowed) void setShown(false);
    });
    await w.listen<boolean>("boss", (e) => {
      allowed = !e.payload;
      if (!allowed) void setShown(false);
    });
    const { invoke } = await import("@tauri-apps/api/core");
    allowed = !(await invoke<boolean>("quiet").catch(() => false));
  }
  requestAnimationFrame(step);
}

main().catch((e) => console.error("overlay", e));
