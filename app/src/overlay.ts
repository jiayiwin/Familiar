// Full-monitor transparent overlay: bugs crawling along the bottom of the
// screen. Click-through everywhere except on bugs; everything pauses while an agent waits on the user.
//
// The pets go after the bugs themselves: a bug that has landed is chased by
// its own pet, which runs along the floor and pounces (you can still get there
// first with a click). A boss pins its pet in place; then the other pets that
// are up come over and swat it, one swat at a time, and your clicks count too.

import { connect, getJson, inTauri, postJson, subscribe, type Bug, type Conn, type WorldState } from "./api";
import { initLang, t as tr } from "./i18n"; // `t` is a common local name here
import { linesOf } from "./voice";

const q = new URLSearchParams(location.search);
const MON = { x: Number(q.get("x") ?? 0), y: Number(q.get("y") ?? 0), w: Number(q.get("w") ?? innerWidth), h: Number(q.get("h") ?? innerHeight) };
const PRIMARY = q.get("primary") === "1";
const canvas = document.getElementById("ov") as HTMLCanvasElement;
const ctx = canvas.getContext("2d")!;

const BUG = 24;
const BOSS = 46;
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
  /** When it came to rest on the floor (its pet goes for it a moment later). */
  landedAt: number;
  /** A Boss: which side of its pinned pet it landed on (helpers come at it from that side, not over the pet). */
  away?: number;
}

interface Popup {
  x: number;
  y: number;
  text: string;
  color: string;
  t0: number;
}

const crawlers = new Map<number, Crawler>();

/** The pets as the daemon last described them: which of the five each is, and what its agent is doing. */
const pets = new Map<string, { skin: string; state: string }>();
/** A pet out after a bug: which one, when it set off, when it may next swat. */
const hunts = new Map<string, { bug: number; since: number; aimAt: number; nextSwat: number; aimX: number }>();
/** When each pet last said something about bugs, so it does not chatter. */
const saidAt = new Map<string, number>();
/** A bug lies this long on the floor before its pet goes for it (time enough for you to click it first). */
const CHASE_AFTER = 1100;
/** How close (px) a pet's feet must be to a bug to get it; and how often a helper swats a boss. */
const REACH = 30;
const SWAT_EVERY = 900;

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

let _modelToastEl: HTMLElement | null = null;
let _modelToastTimer: ReturnType<typeof setTimeout> | null = null;
function showModelToast(text: string, kind: "progress" | "success" | "error") {
  if (!_modelToastEl) {
    _modelToastEl = document.createElement("div");
    Object.assign(_modelToastEl.style, {
      position: "fixed", bottom: "28px", left: "50%", transform: "translateX(-50%)",
      background: "rgba(20,20,20,0.88)", color: "#fff", padding: "7px 22px",
      borderRadius: "20px", fontSize: "13px", fontFamily: "system-ui, sans-serif",
      pointerEvents: "none", zIndex: "9999", transition: "opacity 0.4s", opacity: "0",
      backdropFilter: "blur(6px)", boxShadow: "0 2px 12px rgba(0,0,0,0.3)",
    });
    document.body.appendChild(_modelToastEl);
  }
  if (_modelToastTimer) { clearTimeout(_modelToastTimer); _modelToastTimer = null; }
  _modelToastEl.textContent = text;
  _modelToastEl.style.opacity = "1";
  if (kind !== "progress") {
    _modelToastTimer = setTimeout(() => { if (_modelToastEl) _modelToastEl.style.opacity = "0"; }, 3500);
  }
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
    vx: (Math.random() < 0.5 ? -1 : 1) * (bug.boss ? 6 : 26 + Math.random() * 22),
    vy: falling ? -180 - Math.random() * 120 : 0,
    falling,
    hitAt: 0,
    deadAt: 0,
    turnAt: performance.now() + 2000 + Math.random() * 4000,
    landedAt: falling ? Infinity : performance.now(),
  });
}

async function spawn(bug: Bug) {
  const feet = await petFeet(bug.pet);
  if (feet) {
    // A Boss lands beside its pet (towards the middle of the screen), where it can be seen and hit,
    // not under it; small ones drop at its feet.
    const off = bug.boss ? (feet.x > MON.w / 2 ? -1 : 1) * 96 : (Math.random() - 0.5) * 30;
    addCrawler(bug, feet.x + off, feet.y - 20, true);
    const c = crawlers.get(bug.id);
    if (c && bug.boss) c.away = Math.sign(off);
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

/** A pet says one of its bug lines, now and then. */
async function petSays(pet: string, key: string, gap = 6000) {
  const now = performance.now();
  if (now - (saidAt.get(pet) ?? -1e9) < gap || !inTauri) return;
  saidAt.set(pet, now);
  const all = linesOf(pets.get(pet)?.skin ?? "zimeng", key);
  if (!all.length) return;
  const { emitTo } = await import("@tauri-apps/api/event");
  await emitTo(`pet-${pet}`, "say", { text: all[Math.floor(Math.random() * all.length)] });
}

async function chaseTo(pet: string, x: number | null) {
  if (!inTauri) return;
  const { invoke } = await import("@tauri-apps/api/core");
  await invoke("pet_chase", { pet, x: x == null ? null : MON.x + x }).catch(() => {});
}

/** Free to go after a bug: up, not waiting on you, not pinned, and standing on this screen's floor. */
async function free(pet: string): Promise<{ x: number; y: number } | null> {
  const st = pets.get(pet)?.state;
  if (!st || st === "attention" || st === "stone" || st === "offline") return null;
  const feet = await petFeet(pet);
  return feet && Math.abs(feet.y - floorY()) < 46 ? feet : null;
}

/** Pets from another screen called over to help with a Boss on this one. */
const summoned = new Set<string>();

/** A Boss here, a free pet on another screen: it walks over to the pinned one, onto this floor. */
async function summon(bosses: Crawler[]) {
  if (!bosses.length) {
    // The Boss is gone. One that came over and is not chasing anything here goes home (the hunt takes the rest back).
    for (const pet of summoned) {
      if (!hunts.has(pet)) void import("@tauri-apps/api/core").then(({ invoke }) => invoke("pet_visit", { pet, target: null }).catch(() => {}));
    }
    return summoned.clear();
  }
  for (const [pet, p] of pets) {
    if (summoned.has(pet) || bosses.some((b) => b.bug.pet === pet)) continue;
    if (!p.state || ["attention", "stone", "offline", "sleeping"].includes(p.state)) continue;
    // Already on this screen: the hunt below sends it.
    if (await petFeet(pet)) continue;
    summoned.add(pet);
    void petSays(pet, "bug.help", 20_000);
    const { invoke } = await import("@tauri-apps/api/core");
    await invoke("pet_visit", { pet, target: bosses[0].bug.pet }).catch(() => {});
  }
}

let hunting = false;
/** Twice a second: send pets after bugs, steer them as the bugs crawl, and swat what they reach. */
async function hunt(conn: Conn) {
  if (hunting || !allowed || world?.frozen || world?.focus) return;
  hunting = true;
  try {
    const now = performance.now();
    const live = [...crawlers.values()].filter((c) => !c.deadAt && !c.falling);
    // A boss pins its own pet; every other pet that is free is a helper.
    const bosses = live.filter((c) => c.bug.boss);
    if (inTauri) await summon(bosses);
    for (const [pet] of pets) {
      const mine = hunts.get(pet);
      let target: Crawler | undefined;
      if (mine) target = crawlers.get(mine.bug);
      if (!target || target.deadAt) {
        const own = live.filter((c) => !c.bug.boss && c.bug.pet === pet && now - c.landedAt > CHASE_AFTER);
        const help = bosses.find((c) => c.bug.pet !== pet);
        target = help ?? own.sort((a, b) => a.bug.id - b.bug.id)[0];
        if (!target) {
          if (mine) {
            hunts.delete(pet);
            // Nothing left to get: back to where it was.
            window.setTimeout(() => !hunts.has(pet) && void chaseTo(pet, null), 1200);
          }
          continue;
        }
      }
      const feet = await free(pet);
      if (!feet) {
        hunts.delete(pet);
        continue;
      }
      let h = hunts.get(pet);
      if (!h || h.bug !== target.bug.id) {
        h = { bug: target.bug.id, since: now, aimAt: 0, nextSwat: 0, aimX: NaN };
        hunts.set(pet, h);
        void petSays(pet, target.bug.boss ? "bug.help" : "bug.chase", target.bug.boss ? 20_000 : 9000);
      }
      // A Boss is hit from its far side, so the helper does not stand on the pet it pins.
      const spot = target.bug.boss && target.away ? target.x + target.away * 54 : target.x;
      const gap = Math.abs(feet.x - spot);
      if (gap <= REACH) {
        if (now >= h.nextSwat) {
          h.nextSwat = now + SWAT_EVERY;
          const last = !target.bug.boss || target.bug.hp <= 1;
          const { emitTo } = inTauri ? await import("@tauri-apps/api/event") : { emitTo: async () => {} };
          await emitTo(`pet-${pet}`, "swat", { last });
          // The paw comes down a moment after the pounce starts.
          window.setTimeout(() => void squash(conn, target!, pet), 160);
        }
      } else if (now - h.aimAt > 450 || Math.abs(h.aimX - target.x) > 40) {
        // Run to where the bug will be by the time it gets there.
        const lead = spot + target.vx * Math.min(1.2, gap / 330);
        h.aimAt = now;
        h.aimX = target.x;
        void chaseTo(pet, Math.max(10, Math.min(MON.w - 10, lead)));
      }
    }
  } finally {
    hunting = false;
  }
}

async function squash(conn: Conn, c: Crawler, by?: string) {
  if (world?.frozen || c.deadAt) return;
  c.hitAt = performance.now();
  const r = await postJson<{ ok: boolean; message?: string; result?: { hp: number; killed: boolean; boss: boolean } }>(
    conn,
    "/v1/world/squash",
    { id: c.bug.id },
  );
  if (!r.ok || !r.result) return;
  c.bug.hp = r.result.hp;
  if (r.result.killed) {
    c.deadAt = performance.now();
    sfx(r.result.boss ? "boss-down" : "splat");
    if (r.result.boss) {
      popups.push({ x: c.x, y: c.y - 44, text: tr("拍掉了。它能动了。", "Squashed. It can move again."), color: "#5b4a57", t0: performance.now() });
      void petSays(c.bug.pet, "bug.freed", 0);
    } else if (by && ![...crawlers.values()].some((o) => !o.deadAt && o.bug.pet === by && !o.bug.boss)) void petSays(by, "bug.done", 15_000);
  } else {
    sfx(c.bug.boss ? "boss-hit" : "splat");
  }
}

// ------------------------------------------------------------ drawing

/**
 * A bug in the pets' own hand: a round little beetle in a pastel shell, a soft
 * warm-grey line, two dot eyes and a blush, legs that patter. Each one gets one
 * of three shells by its id; a Boss is a bigger, plum one with a frown, its
 * strength a small pill above it. Squashed, it goes up in a puff.
 */
const SHELLS = [
  { body: "#cfe2bf", spot: "#a6c595", head: "#b9d4a9" }, // sage
  { body: "#f6d5bb", spot: "#e7ae8c", head: "#efc3a4" }, // apricot
  { body: "#dcd3ef", spot: "#b6a8db", head: "#cbbfe7" }, // lavender
];
const BOSS_SHELL = { body: "#ecb8c2", spot: "#cf8193", head: "#e3a3b1" };
const BUG_LINE = "#7d6672";
const BUG_EYE = "#4a3a44";

function drawBug(c: Crawler, now: number) {
  const s = c.bug.boss ? BOSS : BUG;
  const look = c.bug.boss ? BOSS_SHELL : SHELLS[c.bug.id % SHELLS.length];
  const frozen = !!world?.frozen;
  ctx.save();
  ctx.translate(c.x, c.y);
  ctx.lineCap = "round";
  ctx.lineJoin = "round";
  if (c.deadAt) {
    // A soft puff: little paper clouds going out, and two glints of its colour.
    const k = Math.min(1, (now - c.deadAt) / 700);
    ctx.globalAlpha = 1 - k;
    for (let i = 0; i < 6; i++) {
      const a = (i / 6) * Math.PI * 2 + 0.3;
      const d = s * (0.25 + k * 0.75);
      const r = s * (0.24 - k * 0.1);
      ctx.beginPath();
      ctx.arc(Math.cos(a) * d, -s * 0.4 + Math.sin(a) * d * 0.7, Math.max(1, r), 0, Math.PI * 2);
      ctx.fillStyle = "#fffaf2";
      ctx.fill();
      ctx.strokeStyle = "#d9cbc8";
      ctx.lineWidth = 1;
      ctx.stroke();
    }
    ctx.fillStyle = look.spot;
    for (const [dx, dy] of [[-0.6, -0.9], [0.7, -0.75]]) {
      const x = dx * s * (0.6 + k), y = dy * s * (0.6 + k * 0.6), r = s * 0.16;
      ctx.beginPath();
      ctx.moveTo(x, y - r);
      ctx.quadraticCurveTo(x, y, x + r, y);
      ctx.quadraticCurveTo(x, y, x, y + r);
      ctx.quadraticCurveTo(x, y, x - r, y);
      ctx.quadraticCurveTo(x, y, x, y - r);
      ctx.fill();
    }
    ctx.restore();
    return;
  }
  if (frozen) ctx.globalAlpha = 0.45;
  const dir = c.vx < 0 ? -1 : 1;
  ctx.scale(dir, 1);
  const step = c.falling || frozen ? 0 : now / 70;
  const hit = now - c.hitAt < 120 ? 0.78 : 1;
  ctx.scale(1 + (1 - hit) * 0.5, hit);
  const lw = c.bug.boss ? 2.2 : 1.6;

  // A soft shadow on the floor.
  ctx.fillStyle = "rgba(91, 74, 87, 0.14)";
  ctx.beginPath();
  ctx.ellipse(0, 0.5, s * 0.62, s * 0.11, 0, 0, Math.PI * 2);
  ctx.fill();

  // Three pairs of short legs, pattering.
  ctx.strokeStyle = BUG_LINE;
  ctx.lineWidth = lw;
  for (let i = 0; i < 3; i++) {
    const lx = (i - 1) * s * 0.3 - s * 0.05;
    const lift = Math.sin(step + i * 2.1) * s * 0.07;
    ctx.beginPath();
    ctx.moveTo(lx, -s * 0.16);
    ctx.lineTo(lx + s * 0.06 + lift, -Math.max(0, lift) * 0.6);
    ctx.stroke();
  }

  // The shell: round, with a parting down the middle and a few spots.
  const bob = c.falling || frozen ? 0 : Math.abs(Math.sin(step)) * s * 0.03;
  ctx.translate(0, -bob);
  ctx.fillStyle = look.body;
  ctx.beginPath();
  ctx.ellipse(-s * 0.08, -s * 0.42, s * 0.56, s * 0.42, 0, 0, Math.PI * 2);
  ctx.fill();
  ctx.save();
  ctx.clip();
  ctx.fillStyle = look.spot;
  for (const [x, y, r] of [[-0.42, -0.5, 0.11], [-0.12, -0.66, 0.09], [0.1, -0.36, 0.1], [-0.3, -0.24, 0.08]]) {
    ctx.beginPath();
    ctx.arc(x * s, y * s, r * s, 0, Math.PI * 2);
    ctx.fill();
  }
  // A highlight on top, like the pets' fur catching the light.
  ctx.fillStyle = "rgba(255, 255, 255, 0.45)";
  ctx.beginPath();
  ctx.ellipse(-s * 0.26, -s * 0.66, s * 0.2, s * 0.08, -0.3, 0, Math.PI * 2);
  ctx.fill();
  ctx.restore();
  ctx.strokeStyle = BUG_LINE;
  ctx.lineWidth = lw;
  ctx.beginPath();
  ctx.ellipse(-s * 0.08, -s * 0.42, s * 0.56, s * 0.42, 0, 0, Math.PI * 2);
  ctx.stroke();
  ctx.beginPath();
  ctx.moveTo(s * 0.06, -s * 0.82);
  ctx.quadraticCurveTo(-s * 0.06, -s * 0.45, s * 0.02, -s * 0.04);
  ctx.stroke();

  // The head, a little round one in front.
  const hx = s * 0.5, hy = -s * 0.36, hr = s * 0.25;
  ctx.fillStyle = look.head;
  ctx.beginPath();
  ctx.arc(hx, hy, hr, 0, Math.PI * 2);
  ctx.fill();
  ctx.stroke();
  // Antennae with round tips.
  for (const [ax, ay] of [[0.62, -0.86], [0.86, -0.74]]) {
    const tx = ax * s + Math.sin(step * 0.5) * s * 0.03;
    ctx.beginPath();
    ctx.moveTo(hx + hr * 0.2, hy - hr * 0.8);
    ctx.quadraticCurveTo(hx + hr * 0.4, ay * s + s * 0.1, tx, ay * s);
    ctx.stroke();
    ctx.fillStyle = BUG_LINE;
    ctx.beginPath();
    ctx.arc(tx, ay * s, lw * 1.1, 0, Math.PI * 2);
    ctx.fill();
  }
  // The eye, a highlight, a blush.
  ctx.fillStyle = BUG_EYE;
  ctx.beginPath();
  ctx.arc(hx + hr * 0.3, hy - hr * 0.12, Math.max(1.4, s * 0.075), 0, Math.PI * 2);
  ctx.fill();
  ctx.fillStyle = "#fff";
  ctx.beginPath();
  ctx.arc(hx + hr * 0.36, hy - hr * 0.22, Math.max(0.6, s * 0.028), 0, Math.PI * 2);
  ctx.fill();
  ctx.fillStyle = "rgba(240, 140, 150, 0.5)";
  ctx.beginPath();
  ctx.ellipse(hx + hr * 0.35, hy + hr * 0.42, hr * 0.28, hr * 0.15, 0, 0, Math.PI * 2);
  ctx.fill();
  if (c.bug.boss) {
    // A Boss frowns.
    ctx.strokeStyle = BUG_EYE;
    ctx.lineWidth = 1.6;
    ctx.beginPath();
    ctx.moveTo(hx + hr * 0.05, hy - hr * 0.5);
    ctx.lineTo(hx + hr * 0.6, hy - hr * 0.32);
    ctx.stroke();
  }
  ctx.restore();

  if (c.bug.boss) {
    // Its strength: a small pill above it, in its own colour.
    const w = BOSS * 1.5, h = 7, x = c.x - w / 2, y = c.y - BOSS - 14;
    ctx.save();
    if (frozen) ctx.globalAlpha = 0.45;
    ctx.fillStyle = "rgba(255, 250, 242, 0.95)";
    ctx.strokeStyle = "#d9cbc8";
    ctx.lineWidth = 1;
    ctx.beginPath();
    ctx.roundRect(x, y, w, h, h / 2);
    ctx.fill();
    ctx.stroke();
    const f = Math.max(0, c.bug.hp / c.bug.max_hp);
    if (f > 0) {
      ctx.fillStyle = BOSS_SHELL.spot;
      ctx.beginPath();
      ctx.roundRect(x + 1.5, y + 1.5, Math.max(h - 3, (w - 3) * f), h - 3, (h - 3) / 2);
      ctx.fill();
    }
    ctx.restore();
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

async function setShown(on: boolean) {
  if (on === shown || !inTauri) return;
  shown = on;
  const { getCurrentWebviewWindow } = await import("@tauri-apps/api/webviewWindow");
  const w = getCurrentWebviewWindow();
  await (on ? w.show() : w.hide());
}

async function reportHits() {
  const rects = world?.frozen
    ? []
    : [...crawlers.values()]
        .filter((c) => !c.deadAt)
        .map((c) => {
          const s = (c.bug.boss ? BOSS : BUG) + 10;
          return { x: c.x - s / 2, y: c.y - s, w: s, h: s + 4 };
        });
  const key = JSON.stringify(rects.map((r) => [Math.round(r.x / 4), Math.round(r.y / 4)]));
  if (key === lastHits || !inTauri) return;
  lastHits = key;
  const { invoke } = await import("@tauri-apps/api/core");
  await invoke("set_hit_rects", { rects }).catch(() => {});
}

let conn: Conn | null = null;
let lastHunt = 0;

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
        c.landedAt = now;
        // A boss lands on its pet: it says so.
        if (c.bug.boss) void petSays(c.bug.pet, "bug.pinned", 0);
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
  drawPopups(now);
  void setShown(allowed && (crawlers.size > 0 || popups.length > 0));
  void reportHits();
  if (conn && now - lastHunt > 250) {
    lastHunt = now;
    void hunt(conn);
  }
  requestAnimationFrame(step);
}

async function main() {
  size();
  const c = await connect();
  conn = c;
  await initLang(c);
  onWorld(await getJson<WorldState>(c, "/v1/world"));
  const list = await getJson<{ pets: { id: string; skin?: string; state: string }[] }>(c, "/v1/pets").catch(() => ({ pets: [] }));
  for (const p of list.pets) pets.set(p.id, { skin: p.skin ?? "zimeng", state: p.state });
  subscribe(c, (type, data) => {
    if (type === "pet.state") {
      const p = data as { id: string; skin?: string; state: string };
      if (p?.id) pets.set(p.id, { skin: p.skin ?? "zimeng", state: p.state });
    } else if (type === "world.state") onWorld(data as WorldState);
    else if (type === "model_download") {
      const d = data as { done?: number; total?: number; ready?: boolean; error?: string };
      if (d.error) {
        showModelToast(tr("模型下载失败", "Model download failed"), "error");
      } else if (d.ready) {
        showModelToast(tr("本地模型好了。", "Local model's ready."), "success");
      } else if (d.done != null && d.total != null) {
        const pct = Math.round((d.done / d.total) * 100);
        showModelToast(tr(`在下载模型… ${pct}%`, `Downloading the model… ${pct}%`), "progress");
      }
    } else if (type === "bug.spawn") void spawn(data as Bug);
  });

  canvas.addEventListener("mousedown", (e) => {
    // The topmost live bug under the cursor.
    const hit = [...crawlers.values()].reverse().find((c) => {
      const s = (c.bug.boss ? BOSS : BUG) + 10;
      return !c.deadAt && Math.abs(e.offsetX - c.x) <= s / 2 && e.offsetY <= c.y + 4 && e.offsetY >= c.y - s;
    });
    if (hit) void squash(c, hit);
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
