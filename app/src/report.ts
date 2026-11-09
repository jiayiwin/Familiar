// Daily report card (design §7.2), drawn like a holographic trading card: a
// slowly turning foil border, a rainbow sheen that sweeps across, sparkles in
// the header, the pets on duty animated in their outfits, and the day's
// numbers. Everything is on one canvas so it can be copied as an image.

import { connect, fileUrl, getJson, inTauri, RARITY_COLORS, type CampDay, type Conn, type Pack, type PetView, type Report } from "./api";
import { drawHat, drawWearBehind, drawWearFront, fitOf } from "./decor";
import { toast } from "./preview";
import { Sprites } from "./sprites";
import { initLang, t as tr } from "./i18n"; // `t` is a common local name here

const W = 400;
const canvas = document.getElementById("card") as HTMLCanvasElement;
const ctx = canvas.getContext("2d")!;
let conn: Conn;
let names: Record<string, string> = {};
let day = new Date();
let report: Report | null = null;
let H = 600;

const ymd = (d: Date) => `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, "0")}-${String(d.getDate()).padStart(2, "0")}`;

function eff(d?: CampDay): number {
  if (!d || d.turns < 3) return 0;
  return (d.clean - d.errors) / d.turns;
}

function rrect(x: number, y: number, w: number, h: number, r: number) {
  ctx.beginPath();
  ctx.roundRect(x, y, w, h, r);
}

function text(t: string, x: number, y: number, size: number, color: string, weight = "normal", align: CanvasTextAlign = "left", maxW?: number) {
  ctx.font = weight === "serif" ? `bold ${size}px Bahnschrift, "Segoe UI", "Microsoft YaHei UI", sans-serif` : `${weight} ${size}px "Segoe UI", "Microsoft YaHei UI", "PingFang SC", system-ui, sans-serif`;
  ctx.fillStyle = color;
  ctx.textAlign = align;
  ctx.textBaseline = "alphabetic";
  if (maxW) ctx.fillText(t, x, y, maxW);
  else ctx.fillText(t, x, y);
}

/**
 * A big number with small units ("1 小时 5 分" → big 1, small 小时, big 5…),
 * shrunk to fit its column so wide values never run into the next one.
 */
function stat(label: string, value: string, x: number, y: number, maxW = 104) {
  const parts = value.match(/[+\-<]?\d+(?:\.\d+)?|[^\d+\-<]+/g) ?? [value];
  const big = (p: string) => /\d/.test(p);
  const font = (p: string, k: number) => (big(p) ? `bold ${Math.round(24 * k)}px Bahnschrift, "Segoe UI", sans-serif` : `600 ${Math.round(11.5 * k)}px "Segoe UI", "Microsoft YaHei UI", sans-serif`);
  const width = (k: number) =>
    parts.reduce((w, p) => {
      ctx.font = font(p.trim() || p, k);
      return w + ctx.measureText(p.trim()).width + (big(p) ? 0 : 3 * k);
    }, 0);
  const k = Math.min(1, maxW / Math.max(1, width(1)));
  let cx = x;
  ctx.textAlign = "left";
  ctx.textBaseline = "alphabetic";
  for (const p of parts) {
    const t = p.trim();
    if (!t) continue;
    ctx.font = font(t, k);
    ctx.fillStyle = big(t) ? "#1f2245" : "#4a4e72";
    if (!big(t)) cx += 2 * k;
    ctx.fillText(t, cx, y);
    cx += ctx.measureText(t).width + (big(t) ? 0 : 1 * k);
  }
  text(label, x, y + 17, 10.5, "#8b8ca6", "600");
}

/** Work time for the report: "<1 分钟", "25 分钟", "1 小时 5 分". */
function workTime(ms: number): string {
  const m = Math.floor(ms / 60000);
  if (m < 1) return tr("<1 分钟", "<1 min");
  if (m < 60) return tr(`${m} 分钟`, `${m} min`);
  return tr(`${Math.floor(m / 60)} 小时 ${m % 60} 分`, `${Math.floor(m / 60)} h ${m % 60} min`);
}

function fishIcon(x: number, y: number, color: string, s = 1) {
  ctx.fillStyle = color;
  ctx.beginPath();
  ctx.ellipse(x, y, 9 * s, 5 * s, 0, 0, Math.PI * 2);
  ctx.fill();
  ctx.beginPath();
  ctx.moveTo(x - 8 * s, y);
  ctx.lineTo(x - 14 * s, y - 5 * s);
  ctx.lineTo(x - 14 * s, y + 5 * s);
  ctx.fill();
  ctx.fillStyle = "#fff";
  ctx.fillRect(x + 3 * s, y - 2 * s, 2 * s, 2 * s);
}

function star(x: number, y: number, r: number) {
  ctx.beginPath();
  for (let i = 0; i < 8; i++) {
    const a = (i / 8) * Math.PI * 2 - Math.PI / 2;
    const rr = i % 2 ? r * 0.38 : r;
    ctx.lineTo(x + Math.cos(a) * rr, y + Math.sin(a) * rr);
  }
  ctx.closePath();
  ctx.fill();
}

const GAME_NAMES: Record<string, () => string> = {
  tug: () => tr("拔河", "tug"),
  badminton: () => tr("羽毛球", "badminton"),
  pingpong: () => tr("乒乓球", "ping-pong"),
  armwrestle: () => tr("掰手腕", "arm wrestling"),
  race: () => tr("赛跑", "race"),
  duel: () => tr("擂台", "duel"),
  rps: () => tr("猜拳", "RPS"),
  rps_user: () => tr("和你猜拳", "RPS with you"),
};
const CAMP_NAMES: Record<string, string> = { "claude-code": "Claude Code", codex: "Codex" };
const RARITY_RANK: Record<string, number> = { legendary: 3, epic: 2, rare: 1, common: 0 };

interface Games {
  affinity: number;
  packs: { "claude-code": string | null; codex: string | null };
}
let games: Games | null = null;
const photos: HTMLImageElement[] = [];
/** Animated sprites and outfits of the pets on duty, by pack. */
const sprites = new Map<string, Sprites>();
const outfits = new Map<string, { hat?: string; wear: Record<string, string> }>();
const pointer = { x: -1, y: -1, on: false };
const t0 = performance.now();

/** A pet's display name: the duty list knows camps and packs; otherwise the id. */
function petName(r: Report, id: string): string {
  const d = r.duty.find((d) => d.camp === id || d.pack === id);
  return d ? d.name : id;
}

/** Card height for this report (the fish row is capped, so it stays on one screen). */
function heightOf(r: Report): number {
  const detail = r.whips || r.games ? 16 : 0;
  const photo = (games?.affinity ?? 0) >= 80 && photos.length === 2 ? 96 : 0;
  return 4 + 80 + 10 + 84 + 12 + 234 + 10 + detail + 62 + 78 + photo + 26 + 4;
}

function draw(r: Report, now: number) {
  const t = (now - t0) / 1000;
  const dpr = devicePixelRatio || 1;
  if (canvas.width !== W * dpr || canvas.height !== H * dpr) {
    canvas.width = W * dpr;
    canvas.height = H * dpr;
    canvas.style.width = `${W}px`;
    canvas.style.height = `${H}px`;
  }
  ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
  ctx.clearRect(0, 0, W, H);

  // Foil border: a conic rainbow that slowly turns, brightest near the pointer.
  const cg = ctx.createConicGradient(t * 0.6, W / 2, H / 2);
  ["#ff7a9c", "#ffd166", "#8be3b4", "#7fb0ec", "#c9a6ff", "#ff7a9c"].forEach((c, i, a) => cg.addColorStop(i / (a.length - 1), c));
  rrect(0, 0, W, H, 24);
  ctx.fillStyle = cg;
  ctx.fill();
  ctx.save();
  rrect(4, 4, W - 8, H - 8, 20);
  ctx.clip();
  ctx.fillStyle = "#f6f1e7";
  ctx.fillRect(0, 0, W, H);

  // Header: indigo with bold shapes and twinkling sparkles.
  const hg = ctx.createLinearGradient(0, 0, W, 0);
  hg.addColorStop(0, "#2b3275");
  hg.addColorStop(1, "#3b44a0");
  ctx.fillStyle = hg;
  ctx.fillRect(0, 0, W, 84);
  ctx.save();
  ctx.beginPath();
  ctx.rect(0, 0, W, 84);
  ctx.clip();
  const disc = (x: number, y: number, rad: number, c: string) => {
    ctx.fillStyle = c;
    ctx.beginPath();
    ctx.arc(x, y, rad, 0, Math.PI * 2);
    ctx.fill();
  };
  disc(W - 40, 88 + Math.sin(t * 0.8) * 3, 46, "#ee5b3c");
  disc(W - 112, 18 + Math.sin(t * 1.1 + 1) * 2, 18, "#f4b63f");
  disc(W + 4, 22, 26, "#6fc3a0");
  ctx.fillStyle = "#ad98e6";
  ctx.beginPath();
  ctx.arc(W - 150, 84, 22, Math.PI, 0);
  ctx.fill();
  for (let i = 0; i < 14; i++) {
    const sx = 150 + ((i * 97) % 240), sy = 8 + ((i * 53) % 70);
    const tw = 0.5 + 0.5 * Math.sin(t * (1.5 + (i % 3) * 0.7) + i * 1.9);
    ctx.globalAlpha = tw * 0.9;
    ctx.fillStyle = i % 3 ? "#fff6c2" : "#ffd166";
    star(sx, sy, 1.5 + tw * 2.5);
  }
  ctx.globalAlpha = 1;
  ctx.restore();
  text(tr("今日报告", "Daily Report"), 22, 40, 22, "#ffffff", "serif");
  text(r.day, 22, 62, 11, "rgba(255,255,255,0.65)");
  const coin = tr(`金币 ${r.balance}`, `Coins ${r.balance}`);
  ctx.font = `bold 13px Bahnschrift, "Segoe UI", "Microsoft YaHei UI", sans-serif`;
  const cw = ctx.measureText(coin).width + 22;
  ctx.fillStyle = "#ffffff";
  rrect(W - 20 - cw - 96, 28, cw, 26, 13);
  ctx.fill();
  text(coin, W - 20 - 96 - cw / 2, 46, 13, "#2b3275", "serif", "center");

  // On duty: the pets that stood for the agents, alive and dressed.
  let y = 94;
  ctx.fillStyle = "#ffffff";
  rrect(12, y, W - 24, 84, 14);
  ctx.fill();
  ctx.save();
  rrect(12, y, W - 24, 84, 14);
  ctx.clip();
  disc(W - 26, y + 4, 26, "#e8e2fa");
  ctx.restore();
  text(tr("今日值班", "On duty today"), W - 24, y + 16, 11, "#8b8ca6", "600", "right");
  if (!r.duty.length) text(tr("今天还没有宠物上岗", "No pet has clocked in yet today"), 22, y + 52, 12, "#8b8ca6");
  r.duty.slice(0, 2).forEach((d, i) => {
    const x = 20 + i * ((W - 24) / 2);
    const sp = sprites.get(d.pack);
    if (sp) {
      const a = sp.action("idle") ?? Object.values(sp.pack.actions)[0];
      const f = sp.pack.frame;
      const fr = sp.frameAt(a, now - t0 + i * 400);
      const sc = f.h >= 96 ? 66 / f.h : Math.max(1, Math.floor(68 / f.h));
      const cx = x + 34, bottom = y + 80;
      const fit = fitOf(sp.body(a, fr.idx), f.w, f.h, cx, bottom, sc);
      const o = outfits.get(d.pack);
      drawWearBehind(ctx, o?.wear, fit, now);
      sp.draw(ctx, a, fr.idx, cx, bottom, sc);
      drawWearFront(ctx, o?.wear, fit, now);
      if (o?.hat) {
        const hd = sp.head(a, fr.idx);
        drawHat(ctx, o.hat, cx + (hd.x - f.w / 2) * sc, bottom - (f.h - hd.top) * sc, hd.w * sc, now);
      }
    } else {
      ctx.fillStyle = "#efe8dc";
      rrect(x + 6, y + 22, 56, 56, 10);
      ctx.fill();
    }
    const color = d.camp === "codex" ? "#2f9a8a" : "#e0794f";
    text(d.name, x + 74, y + 40, 13, "#1f2245", "bold", "left", 108);
    text(CAMP_NAMES[d.camp] ?? d.camp, x + 74, y + 55, 10.5, color, "bold", "left", 108);
    text(
      d.turns ? tr(`${d.turns} 轮 · ${workTime(d.work_ms)}`, `${d.turns} turns · ${workTime(d.work_ms)}`) : tr("在岗，还没完成任务", "On duty, nothing finished yet"),
      x + 74, y + 69, 10.5, "#8b8ca6", "normal", "left", 108,
    );
  });

  // Twelve tiles, three a row.
  y = 190;
  const tints = ["#fde4d6", "#dcf1e6", "#e8e2fa", "#dde9fa", "#fdf0c8", "#fde4d6", "#fde4d6", "#dde9fa", "#e8e2fa", "#dcf1e6", "#fdf0c8", "#dde9fa"];
  for (let i = 0; i < 12; i++) {
    const x = 12 + (i % 3) * 128;
    const ty = y + Math.floor(i / 3) * 60;
    ctx.fillStyle = "#ffffff";
    rrect(x, ty, 120, 54, 12);
    ctx.fill();
    ctx.save();
    rrect(x, ty, 120, 54, 12);
    ctx.clip();
    disc(x + 112, ty + 6, 20, tints[i]);
    ctx.restore();
  }
  const fish = r.catches.filter((c) => c.outcome !== "boot" && c.outcome !== "escape");
  const put = (i: number, label: string, value: string) => stat(label, value, 22 + (i % 3) * 128, y + 28 + Math.floor(i / 3) * 60);
  put(0, tr("agent 工作时长", "Agent work time"), r.work_ms ? workTime(r.work_ms) : "0");
  put(1, tr("完成轮数", "Turns done"), String(r.turns));
  put(2, tr("一次通过", "First-try passes"), String(r.clean));
  put(3, tr("钓到的鱼", "Fish caught"), String(fish.length));
  put(4, tr("拍死的虫", "Bugs squashed"), `${r.bugs}${r.bosses ? ` + ${r.bosses} Boss` : ""}`);
  put(5, tr("今日金币", "Coins today"), `+${r.coins_earned}`);
  put(6, tr("挨的鞭子", "Lashes taken"), tr(`${r.whips} 下`, `${r.whips}`));
  put(7, tr("玩的游戏", "Games played"), tr(`${r.games} 局`, `${r.games}`));
  put(8, tr("听电台", "Radio time"), r.radio_s ? workTime(r.radio_s * 1000) : "0");
  put(9, tr("提肛", "Kegels"), r.kegel_reps ? tr(`${r.kegel_reps} 组`, `${r.kegel_reps} reps`) : "0");
  put(10, tr("收到的信", "Letters"), tr(`${r.letters} 封`, `${r.letters}`));
  put(11, tr("最多同时在线 agent", "Peak agents online"), tr(`${r.peak_agents} 个`, `${r.peak_agents}`));
  y += 234 + 10;
  const detail: string[] = [];
  if (r.whips) detail.push(Object.entries(r.whipped).sort((a, b) => b[1] - a[1]).map(([id, n]) => `${petName(r, id)} ${tr(`挨了 ${n} 下`, `took ${n}`)}`).join(" · "));
  if (r.games) detail.push(Object.entries(r.games_by).sort((a, b) => b[1] - a[1]).map(([g, n]) => `${GAME_NAMES[g]?.() ?? g} ${n}`).join(" · "));
  if (detail.length) {
    text(detail.join("   |   "), W / 2, y + 8, 10.5, "#8b8ca6", "normal", "center", W - 40);
    y += 16;
  }

  // Tug of war as one bar: orange for Claude Code, teal for Codex.
  const cc = r.by_camp["claude-code"];
  const cx = r.by_camp["codex"];
  ctx.fillStyle = "#e6ddcc";
  ctx.fillRect(20, y, W - 40, 1);
  text(tr("今日拔河", "Today's tug-of-war"), 20, y + 20, 12.5, "#8b8ca6", "bold");
  if (cc || cx) {
    const a = eff(cc), b = eff(cx);
    const verdict = !cc || !cx ? tr("只有一方上场", "Only one team played") : a > b ? tr("Claude Code 领先", "Claude Code leads") : b > a ? tr("Codex 领先", "Codex leads") : tr("平局", "Tie");
    text(verdict, W - 20, y + 20, 12.5, "#1f2245", "bold", "right");
    const ta = cc?.turns ?? 0, tb = cx?.turns ?? 0;
    const share = ta + tb ? ta / (ta + tb) : 0.5;
    const bx = 20, bw = W - 40, by = y + 30;
    ctx.save();
    rrect(bx, by, bw, 10, 5);
    ctx.clip();
    ctx.fillStyle = "#2f9a8a";
    ctx.fillRect(bx, by, bw, 10);
    ctx.fillStyle = "#e0794f";
    ctx.fillRect(bx, by, bw * share, 10);
    ctx.fillStyle = "rgba(255,255,255,0.85)";
    ctx.fillRect(bx + bw * share - 1, by, 2, 10);
    ctx.restore();
    const line = (d: CampDay | undefined) => (d ? tr(`${d.turns} 轮 · 通过 ${d.clean} · 错 ${d.errors} · 效率 ${eff(d).toFixed(2)}`, `${d.turns} turns · ${d.clean} clean · ${d.errors} err · ${eff(d).toFixed(2)}`) : tr("没上场", "sat out"));
    text(`Claude Code · ${line(cc)}`, 20, y + 54, 10, "#e0794f", "600", "left", (W - 50) / 2);
    text(`${line(cx)} · Codex`, W - 20, y + 54, 10, "#2f9a8a", "600", "right", (W - 50) / 2);
  } else {
    text(tr("今天还没有完成的回合", "No finished turns yet today"), W - 20, y + 20, 12, "#8b8ca6", "normal", "right");
  }
  y += 62;

  // Fish basket: one fish per species with a count, biggest catches first.
  ctx.fillStyle = "#e6ddcc";
  ctx.fillRect(20, y, W - 40, 1);
  const coins = fish.reduce((s, f) => s + (f.coins || 0), 0);
  text(fish.length ? tr(`鱼篓 · ${fish.length} 条${coins ? ` · +${coins} 金币` : ""}`, `Fish basket · ${fish.length}${coins ? ` · +${coins} coins` : ""}`) : tr("鱼篓", "Fish basket"), 20, y + 20, 12.5, "#8b8ca6", "bold");
  if (!fish.length) {
    text(tr("今天一条鱼都没钓到", "Not a single fish today"), W - 20, y + 20, 12, "#8b8ca6", "normal", "right");
  } else {
    const groups = new Map<string, { n: number; rarity: string }>();
    for (const f of fish) {
      const g = groups.get(f.fish_id) ?? { n: 0, rarity: f.rarity };
      g.n++;
      groups.set(f.fish_id, g);
    }
    const list = [...groups.entries()].sort((a, b) => (RARITY_RANK[b[1].rarity] ?? 0) - (RARITY_RANK[a[1].rarity] ?? 0) || b[1].n - a[1].n);
    const shown = list.slice(0, 7);
    const cw2 = (W - 40) / 7;
    shown.forEach(([id, g], i) => {
      const x = 20 + cw2 * i + cw2 / 2;
      const bob = Math.sin(t * 2 + i) * 1.5;
      ctx.fillStyle = "#ffffff";
      rrect(x - cw2 / 2 + 3, y + 28, cw2 - 6, 44, 10);
      ctx.fill();
      fishIcon(x + 3, y + 42 + bob, RARITY_COLORS[g.rarity] ?? "#8a9bb0", 1.1);
      text(names[id] ?? id, x, y + 64, 9, "#4a4e72", "600", "center", cw2 - 10);
      if (g.n > 1) {
        ctx.fillStyle = "#ee5b3c";
        rrect(x + cw2 / 2 - 20, y + 29, 17, 13, 6);
        ctx.fill();
        text(`×${g.n}`, x + cw2 / 2 - 11.5, y + 39, 9, "#ffffff", "bold", "center");
      }
    });
    if (list.length > 7) text(tr(`+${list.length - 7} 种`, `+${list.length - 7} more`), W - 20, y + 20, 10.5, "#8b8ca6", "normal", "right");
  }
  y += 78;

  if ((games?.affinity ?? 0) >= 80 && photos.length === 2) {
    // Affinity 80+: the two pets in one picture.
    ctx.fillStyle = "#e6ddcc";
    ctx.fillRect(20, y, W - 40, 1);
    text(tr(`合照 · 好感度 ${games!.affinity}`, `Photo · affinity ${games!.affinity}`), 20, y + 18, 12.5, "#8b8ca6", "bold");
    ctx.imageSmoothingEnabled = false;
    ctx.drawImage(photos[0], W / 2 - 76, y + 22, 72, 72);
    ctx.drawImage(photos[1], W / 2 + 4, y + 22, 72, 72);
    y += 96;
  }
  text(
    tr("战绩仅供娱乐，任务难度不同，不代表模型能力", "Just for fun: tasks differ in difficulty, so this says nothing about model ability"),
    W / 2, H - 14, 9.5, "#a9a8bd", "normal", "center", W - 24,
  );

  // Holographic sheen: a rainbow band sweeping across, plus a glare under the pointer.
  ctx.save();
  ctx.globalCompositeOperation = "soft-light";
  const k = ((t * 0.18) % 1.6) - 0.3;
  const sg = ctx.createLinearGradient(k * W - 160, 0, k * W + 160, H);
  sg.addColorStop(0, "rgba(255,255,255,0)");
  sg.addColorStop(0.35, "rgba(255,120,160,0.85)");
  sg.addColorStop(0.5, "rgba(255,240,180,1)");
  sg.addColorStop(0.65, "rgba(120,200,255,0.85)");
  sg.addColorStop(1, "rgba(255,255,255,0)");
  ctx.fillStyle = sg;
  ctx.fillRect(0, 0, W, H);
  if (pointer.on) {
    ctx.globalCompositeOperation = "screen";
    const gl = ctx.createRadialGradient(pointer.x, pointer.y, 0, pointer.x, pointer.y, 170);
    gl.addColorStop(0, "rgba(255,255,255,0.35)");
    gl.addColorStop(1, "rgba(255,255,255,0)");
    ctx.fillStyle = gl;
    ctx.fillRect(0, 0, W, H);
  }
  ctx.restore();
  ctx.restore();
}

function loadImage(pack: string): Promise<HTMLImageElement | null> {
  return new Promise((resolve) => {
    const img = new Image();
    img.crossOrigin = "anonymous";
    img.onload = () => resolve(img.naturalWidth ? img : null);
    img.onerror = () => resolve(null);
    img.src = fileUrl(conn, pack, "preview.png");
  });
}

/** Sprites and the outfit (shop + season hat) of a pack on duty. */
async function loadPet(pack: string): Promise<void> {
  if (sprites.has(pack)) return;
  try {
    const p = await getJson<Pack>(conn, `/v1/packs/${encodeURIComponent(pack)}`);
    sprites.set(pack, await Sprites.load(p, (f) => fileUrl(conn, pack, f)));
  } catch {
    return;
  }
  const o: { hat?: string; wear: Record<string, string> } = { wear: {} };
  const shop = await getJson<{ equipped: { pack: string; slot: string; item: string }[] }>(conn, "/v1/shop").catch(() => null);
  for (const e of shop?.equipped ?? []) {
    if (e.pack !== pack) continue;
    if (e.slot === "hat") o.hat = e.item;
    else if (e.slot !== "rope") o.wear[e.slot] = e.item;
  }
  const res = await getJson<PetView[] | { pets: PetView[] }>(conn, "/v1/pets").catch(() => []);
  const views = Array.isArray(res) ? res : (res.pets ?? []);
  const v = views.find((v) => v.pack === pack && v.hat);
  if (v?.hat) o.hat = v.hat; // the season crown / paper hat wins
  outfits.set(pack, o);
}

async function load() {
  const r = await getJson<Report>(conn, `/v1/report?day=${ymd(day)}`);
  await Promise.all(r.duty.map((d) => loadPet(d.pack)));
  report = r;
  H = heightOf(r);
  // Fit the window to the card (no empty space under it, no scrollbar).
  if (inTauri) {
    const { getCurrentWindow, LogicalSize } = await import("@tauri-apps/api/window");
    await getCurrentWindow().setSize(new LogicalSize(460, H + 86)).catch(() => {});
  }
}

function loop(now: number) {
  requestAnimationFrame(loop);
  if (report) draw(report, now);
}

async function main() {
  conn = await connect();
  await initLang(conn);
  const stats = await getJson<{ fish: { id: string; name: string }[] }>(conn, "/v1/stats");
  names = Object.fromEntries(stats.fish.map((f) => [f.id, f.name]));
  games = await getJson<Games>(conn, "/v1/games").catch(() => null);
  if (games && games.affinity >= 80 && games.packs["claude-code"] && games.packs.codex) {
    for (const p of [games.packs["claude-code"], games.packs.codex]) {
      const img = await loadImage(p!);
      if (img) photos.push(img);
    }
  }
  await load();
  requestAnimationFrame(loop);

  // Tilt toward the pointer like a card in hand; the glare follows it on the canvas.
  canvas.onmousemove = (e) => {
    const b = canvas.getBoundingClientRect();
    pointer.x = ((e.clientX - b.left) / b.width) * W;
    pointer.y = ((e.clientY - b.top) / b.height) * H;
    pointer.on = true;
    const dx = pointer.x / W - 0.5, dy = pointer.y / H - 0.5;
    canvas.style.transform = `perspective(900px) rotateX(${(-dy * 7).toFixed(2)}deg) rotateY(${(dx * 9).toFixed(2)}deg)`;
  };
  canvas.onmouseleave = () => {
    pointer.on = false;
    canvas.style.transform = "";
  };
  canvas.onclick = async () => {
    if (!inTauri) return;
    const { getCurrentWindow } = await import("@tauri-apps/api/window");
    await getCurrentWindow().close();
  };
  document.getElementById("prev")!.onclick = () => {
    day = new Date(day.getTime() - 86_400_000);
    void load();
  };
  document.getElementById("next")!.onclick = () => {
    day = new Date(day.getTime() + 86_400_000);
    void load();
  };
  document.getElementById("copy")!.onclick = () => {
    canvas.toBlob(async (blob) => {
      if (!blob) return;
      try {
        await navigator.clipboard.write([new ClipboardItem({ "image/png": blob })]);
        toast(tr("已复制，可以直接粘贴到聊天里", "Copied — paste it straight into a chat"));
      } catch (e) {
        toast(tr(`复制失败：${String(e)}`, `Copy failed: ${String(e)}`));
      }
    }, "image/png");
  };
}

main().catch((e) => {
  document.body.innerHTML = `<div class="banner">${tr("报告加载失败：", "Failed to load the report: ")}${String(e)}</div>`;
});
