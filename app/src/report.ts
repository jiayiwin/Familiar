// The daily report: one card on the pet's paper. 紫梦 sits at the top; under it
// the day in numbers (the agents' time, turns, what went through first time,
// what got stuck, fish, bugs, the radio, kegel sets, letters) and the day's
// catch. Everything is drawn on one canvas so the card can be copied as an
// image.

import { closeSelf, connect, getJson, SOURCE_NAMES, type CampDay, type Conn, type Report } from "./api";
import { toast } from "./toast";
import { drawTreat, type TreatLook } from "./treats";
import { initLang, t as tr } from "./i18n"; // `t` is a common local name here
import { loadRig, type RigPet } from "./rig/rig.js";
import { applySkin, followSkin, skinFor, worn } from "./skin";

/** The card's width: a tall card on its own, a wide one when the window has the room (as a tab). */
let W = 400;
const PAD = 26;
const UI = '"Segoe UI Variable Text", "Segoe UI", "Microsoft YaHei UI", "PingFang SC", system-ui, sans-serif';
const HEAD_H = 156;

const canvas = document.getElementById("card") as HTMLCanvasElement;
const ctx = canvas.getContext("2d")!;
let conn: Conn;
let names: Record<string, string> = {};
let looks: Record<string, TreatLook> = {};
let day = new Date();
let report: Report | null = null;
/** Your own day at the desk; only known for today. */
let desk: { ms: number; breaks: number } | null = null;
let pet: RigPet | null = null;
let H = 560;
let lastT = 0;

const ymd = (d: Date) => `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, "0")}-${String(d.getDate()).padStart(2, "0")}`;
const isToday = () => ymd(day) === ymd(new Date());

function text(t: string, x: number, y: number, size: number, color: string, weight = 400, align: CanvasTextAlign = "left", maxW?: number) {
  ctx.font = `${weight} ${size}px ${UI}`;
  ctx.fillStyle = color;
  ctx.textAlign = align;
  ctx.textBaseline = "alphabetic";
  if (maxW) {
    let s = t;
    while (ctx.measureText(s).width > maxW && s.length > 1) s = s.slice(0, -2) + "…";
    ctx.fillText(s, x, y);
  } else ctx.fillText(t, x, y);
}

/** "2 小时 14 分" / "14 分" */
function span(ms: number): string {
  const m = Math.floor(ms / 60000);
  const h = Math.floor(m / 60);
  if (h === 0) return tr(`${m} 分`, `${m}m`);
  return m % 60 ? tr(`${h} 小时 ${m % 60} 分`, `${h}h ${m % 60}m`) : tr(`${h} 小时`, `${h}h`);
}

/** The line under the date: what kind of day it was, in the pet's words. Only what the numbers say. */
function summary(r: Report): string {
  if (r.turns === 0) return isToday() ? tr("今天还没开工。", "Nothing run yet today.") : tr("那天没开工。", "Nothing was run that day.");
  if (r.errors === 0) return tr(`${r.turns} 轮，没卡住。`, `${r.turns} turns, none stuck.`);
  return tr(`${r.turns} 轮，卡住 ${r.errors} 次。`, `${r.turns} turns, stuck ${r.errors}×.`);
}

/** The numbers under the header: [label, value], only the ones that happened. */
function facts(r: Report): [string, string][] {
  const fish = r.catches.filter((c) => c.outcome === "fish").length;
  const out: [string, string][] = [];
  if (desk && desk.ms >= 60000) out.push([tr("你在桌前", "You, at the desk"), span(desk.ms)]);
  if (desk && desk.ms >= 60000) out.push([tr("中间起身", "Breaks taken"), tr(`${desk.breaks} 次`, `${desk.breaks}`)]);
  out.push([tr("agent 的一天", "The agents' day"), r.work_ms >= 60000 ? span(r.work_ms) : "—"]);
  out.push([tr("一次通过", "Clean first time"), tr(`${r.clean} 轮`, `${r.clean}`)]);
  if (r.peak_agents > 1) out.push([tr("同时在跑最多", "Most running at once"), tr(`${r.peak_agents} 个`, `${r.peak_agents}`)]);
  if (fish) out.push([tr("得到的糖果", "Sweets earned"), tr(`${fish} 颗`, `${fish}`)]);
  if (r.bugs) out.push([tr("拍掉的虫", "Bugs squashed"), tr(`${r.bugs} 只`, `${r.bugs}`)]);
  if (r.radio_s >= 60) out.push([tr("听电台", "Radio"), span(r.radio_s * 1000)]);
  if (r.kegel_sets) out.push([tr("提肛", "Kegel sets"), tr(`${r.kegel_sets} 组`, `${r.kegel_sets}`)]);
  if (r.letters) out.push([tr("收到的信", "Letters"), tr(`${r.letters} 封`, `${r.letters}`)]);
  return out;
}

const agents = (r: Report): [string, CampDay][] => Object.entries(r.by_camp).filter(([, d]) => d.turns > 0);

/** What went into the jar today, rarest first, counted by kind. */
function basket(r: Report): { id: string; rarity: string; n: number }[] {
  const rank: Record<string, number> = { legendary: 3, epic: 2, rare: 1, common: 0 };
  const by = new Map<string, { id: string; rarity: string; n: number }>();
  for (const c of r.catches) {
    if (c.outcome !== "fish") continue;
    const e = by.get(c.fish_id) ?? { id: c.fish_id, rarity: c.rarity, n: 0 };
    e.n++;
    by.set(c.fish_id, e);
  }
  return [...by.values()].sort((a, b) => (rank[b.rarity] ?? 0) - (rank[a.rarity] ?? 0) || b.n - a.n).slice(0, 6);
}

/** Wide when there is room for it: the pet and the day on the left, the numbers on the right. */
const wide = () => innerWidth >= 700;
/** Width of the left panel in the wide card. */
const left = () => Math.max(260, Math.min(320, Math.round(W * 0.34)));

function heightOf(r: Report): number {
  if (wide()) {
    const rows = Math.ceil(facts(r).length / 3);
    const lower = Math.max(agents(r).length, basket(r).length);
    return Math.max(380, 44 + rows * 64 + (lower ? 32 + 18 + lower * 26 : 0) + 44);
  }
  const rows = Math.ceil(facts(r).length / 2);
  const a = agents(r).length;
  const b = basket(r).length;
  return HEAD_H + 22 + rows * 58 + (a ? 20 + a * 26 : 0) + (b ? 34 + Math.ceil(b / 2) * 26 : 0) + 40;
}

/** A small drawing of one kind in the jar. */
function fishIcon(x: number, y: number, id: string) {
  drawTreat(ctx, looks[id] ?? {}, x - 1, y, 0.52);
}

function draw(r: Report, now: number) {
  // The card is on the main pet's paper, in its colours.
  const { paper: PAPER, ink: INK, ink2: INK2, muted: MUTED, line: LINE, rule: RULE, room: ROOM, cushion: CUSHION, accentOff: FAINT } = worn().colors;
  const dpr = devicePixelRatio || 1;
  if (canvas.width !== Math.round(W * dpr) || canvas.height !== Math.round(H * dpr)) {
    canvas.width = Math.round(W * dpr);
    canvas.height = Math.round(H * dpr);
    fit();
  }
  ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
  ctx.clearRect(0, 0, W, H);
  if (wide()) return drawWide(r, now, worn().colors);

  // The sheet.
  ctx.save();
  ctx.beginPath();
  ctx.roundRect(0.5, 0.5, W - 1, H - 1, 24);
  ctx.fillStyle = PAPER;
  ctx.fill();
  ctx.clip();

  // Where it sits: the colour of its room, the cushion, and the date beside it.
  ctx.fillStyle = ROOM;
  ctx.fillRect(0, 0, W, HEAD_H);
  const d = new Date(`${r.day}T12:00:00`);
  const date = isNaN(d.getTime()) ? r.day : tr(`${d.getMonth() + 1} 月 ${d.getDate()} 日，周${"日一二三四五六"[d.getDay()]}`, d.toLocaleDateString("en-US", { weekday: "long", month: "long", day: "numeric" }));
  text(isToday() ? tr("今天", "Today") : tr("那一天", "That day"), PAD, 52, 26, INK, 800);
  text(date, PAD, 76, 13, MUTED, 700);
  text(summary(r), PAD, 112, 14, INK, 700, "left", 190);
  seatPet(W - 104, HEAD_H - 20, 138, 118, CUSHION, now);
  lastT = now;

  // The day in numbers, two to a row.
  let y = HEAD_H + 22;
  const fs = facts(r);
  const colW = (W - PAD * 2 - 12) / 2;
  fs.forEach(([label, value], i) => {
    const x = PAD + (i % 2) * (colW + 12);
    const top = y + Math.floor(i / 2) * 58;
    text(label, x, top + 14, 12, MUTED, 700);
    text(value, x, top + 40, 22, INK, 800, "left", colW);
  });
  y += Math.ceil(fs.length / 2) * 58;

  // Each agent's share.
  const as = agents(r);
  if (as.length) {
    ctx.fillStyle = RULE;
    ctx.fillRect(PAD, y + 4, W - PAD * 2, 1);
    y += 20;
    for (const [source, a] of as) {
      ctx.fillStyle = source === "claude-code" ? "#e0794f" : source === "codex" ? "#2f9a8a" : "#8d7fb5";
      ctx.beginPath();
      ctx.arc(PAD + 4, y + 9, 3.5, 0, Math.PI * 2);
      ctx.fill();
      text(SOURCE_NAMES[source] ?? source, PAD + 15, y + 14, 13, INK, 700);
      text(a.errors ? tr(`${a.turns} 轮 · 卡住 ${a.errors}`, `${a.turns} turns · stuck ${a.errors}`) : tr(`${a.turns} 轮`, `${a.turns} turns`), W - PAD, y + 14, 13, INK2, 400, "right");
      y += 26;
    }
  }

  // The catch.
  const bs = basket(r);
  if (bs.length) {
    ctx.fillStyle = RULE;
    ctx.fillRect(PAD, y + 4, W - PAD * 2, 1);
    text(tr("糖罐", "The jar"), PAD, y + 28, 12, MUTED, 700);
    y += 34;
    bs.forEach((f, i) => {
      const x = PAD + (i % 2) * (colW + 12);
      const top = y + Math.floor(i / 2) * 26;
      fishIcon(x + 14, top + 9, f.id);
      text(`${names[f.id] ?? f.id}${f.n > 1 ? ` ×${f.n}` : ""}`, x + 28, top + 14, 13, INK, 400, "left", colW - 30);
    });
    y += Math.ceil(bs.length / 2) * 26;
  }

  text(tr("睿洱", "Rill"), W - PAD, H - 16, 11, FAINT, 700, "right");
  ctx.restore();
  // The same warm-grey line the pet is drawn with.
  ctx.strokeStyle = LINE;
  ctx.lineWidth = 1;
  ctx.beginPath();
  ctx.roundRect(0.5, 0.5, W - 1, H - 1, 24);
  ctx.stroke();
}

/** The pet on its cushion, feet at (px, floor), within a box of bw × bh. */
function seatPet(px: number, floor: number, bw: number, bh: number, cushion: string, now: number) {
  ctx.fillStyle = cushion;
  ctx.beginPath();
  ctx.ellipse(px, floor - 2, bw * 0.55, 12, 0, 0, Math.PI * 2);
  ctx.fill();
  if (!pet) return;
  pet.update(Math.min(0.1, (now - (lastT || now)) / 1000));
  const [x0, y0, x1, y1] = pet.rig.bbox;
  const s = Math.min(bw / (x1 - x0), bh / (y1 - y0));
  ctx.save();
  ctx.translate(px - pet.foot[0] * s, floor - pet.foot[1] * s);
  pet.draw(ctx, s, null);
  ctx.restore();
}

/** The wide card: the day and the pet on the left, in its room's colour; the numbers, the agents and the jar on the right. */
function drawWide(r: Report, now: number, c: ReturnType<typeof worn>["colors"]) {
  const { paper: PAPER, ink: INK, ink2: INK2, muted: MUTED, line: LINE, rule: RULE, room: ROOM, cushion: CUSHION, accentOff: FAINT } = c;
  const L = left();
  ctx.save();
  ctx.beginPath();
  ctx.roundRect(0.5, 0.5, W - 1, H - 1, 24);
  ctx.fillStyle = PAPER;
  ctx.fill();
  ctx.clip();
  ctx.fillStyle = ROOM;
  ctx.fillRect(0, 0, L, H);

  const d = new Date(`${r.day}T12:00:00`);
  const date = isNaN(d.getTime()) ? r.day : tr(`${d.getMonth() + 1} 月 ${d.getDate()} 日，周${"日一二三四五六"[d.getDay()]}`, d.toLocaleDateString("en-US", { weekday: "long", month: "long", day: "numeric" }));
  text(isToday() ? tr("今天", "Today") : tr("那一天", "That day"), PAD + 2, 60, 30, INK, 800);
  text(date, PAD + 2, 88, 13, MUTED, 700);
  text(summary(r), PAD + 2, 124, 15, INK, 700, "left", L - PAD * 2);
  seatPet(L / 2, H - 46, Math.min(236, L - 44), Math.min(214, H - 196), CUSHION, now);
  lastT = now;

  // The day in numbers, three to a row.
  const x0 = L + 34, RW = W - x0 - PAD;
  const fs = facts(r);
  const gap = 18, colW = (RW - gap * 2) / 3;
  let y = 44;
  fs.forEach(([label, value], i) => {
    const x = x0 + (i % 3) * (colW + gap);
    const top = y + Math.floor(i / 3) * 64;
    text(label, x, top + 14, 12, MUTED, 700, "left", colW);
    text(value, x, top + 44, 24, INK, 800, "left", colW);
  });
  y += Math.ceil(fs.length / 3) * 64;

  // Below: each agent's share, and what went into the jar, side by side.
  const as = agents(r), bs = basket(r);
  if (as.length || bs.length) {
    ctx.fillStyle = RULE;
    ctx.fillRect(x0, y + 6, RW, 1);
    y += 32;
    const half = (RW - 28) / 2;
    const cols = [{ x: x0, w: half }, { x: x0 + half + 28, w: half }];
    let ci = 0;
    if (as.length) {
      const { x, w } = cols[ci++];
      text(tr("各个 agent", "The agents"), x, y, 12, MUTED, 700);
      as.forEach(([source, a], i) => {
        const top = y + 10 + i * 26;
        ctx.fillStyle = source === "claude-code" ? "#e0794f" : source === "codex" ? "#2f9a8a" : "#8d7fb5";
        ctx.beginPath();
        ctx.arc(x + 4, top + 9, 3.5, 0, Math.PI * 2);
        ctx.fill();
        text(SOURCE_NAMES[source] ?? source, x + 15, top + 14, 13, INK, 700, "left", w * 0.55);
        text(a.errors ? tr(`${a.turns} 轮 · 卡住 ${a.errors}`, `${a.turns} turns · stuck ${a.errors}`) : tr(`${a.turns} 轮`, `${a.turns} turns`), x + w, top + 14, 13, INK2, 400, "right");
      });
    }
    if (bs.length) {
      const { x, w } = cols[ci];
      text(tr("糖罐", "The jar"), x, y, 12, MUTED, 700);
      bs.forEach((f, i) => {
        const top = y + 10 + i * 26;
        fishIcon(x + 12, top + 9, f.id);
        text(`${names[f.id] ?? f.id}${f.n > 1 ? ` ×${f.n}` : ""}`, x + 28, top + 14, 13, INK, 400, "left", w - 30);
      });
    }
  }

  text(tr("睿洱", "Rill"), W - PAD, H - 18, 11, FAINT, 700, "right");
  ctx.restore();
  ctx.strokeStyle = LINE;
  ctx.lineWidth = 1;
  ctx.beginPath();
  ctx.roundRect(0.5, 0.5, W - 1, H - 1, 24);
  ctx.stroke();
}

/** Pick the card's width for the window, and show it whole: scaled down to fit, never scrolled. */
function layout() {
  W = wide() ? Math.round(Math.min(960, innerWidth - 48)) : 400;
  if (report) H = heightOf(report);
  fit();
}
function fit() {
  const k = Math.min(1, (innerHeight - 96) / H, (innerWidth - 28) / W);
  canvas.style.width = `${Math.round(W * k)}px`;
  canvas.style.height = `${Math.round(H * k)}px`;
}

async function load() {
  const [r, today] = await Promise.all([
    getJson<Report>(conn, `/v1/report?day=${ymd(day)}`),
    isToday() ? getJson<{ desk_ms: number; breaks: number }>(conn, "/v1/today").catch(() => null) : Promise.resolve(null),
  ]);
  desk = today ? { ms: today.desk_ms, breaks: today.breaks } : null;
  report = r;
  if (pet) pet.expr = r.turns === 0 ? "sleepy" : r.errors > r.clean ? "worried" : "happy";
  layout();
  (document.getElementById("next") as HTMLButtonElement).disabled = isToday();
  // 收工曲 needs a day with some work in it.
  (document.getElementById("song") as HTMLButtonElement).disabled = r.turns < 6;
}

function loop(now: number) {
  requestAnimationFrame(loop);
  if (report) draw(report, now);
}

async function main() {
  conn = await connect();
  await initLang(conn);
  const sk = applySkin((await skinFor(conn)).skin.id);
  followSkin(conn);
  const [stats, rig] = await Promise.all([
    getJson<{ fish: { id: string; name: string; shape?: string; colors?: string[] }[] }>(conn, "/v1/stats").catch(() => ({ fish: [] })),
    loadRig(sk.rig ?? "rig/zimeng/").catch(() => null),
  ]);
  names = Object.fromEntries(stats.fish.map((f) => [f.id, f.name]));
  looks = Object.fromEntries(stats.fish.map((f) => [f.id, f]));
  pet = rig;
  await load();
  requestAnimationFrame(loop);
  addEventListener("resize", layout);

  canvas.onclick = () => void closeSelf();
  document.getElementById("prev")!.onclick = () => {
    day = new Date(day.getTime() - 86_400_000);
    void load();
  };
  document.getElementById("next")!.onclick = () => {
    if (isToday()) return;
    day = new Date(day.getTime() + 86_400_000);
    void load();
  };
  // This day's 收工曲, in a tab of its own (on its own page outside the window).
  document.getElementById("song")!.onclick = () => {
    if (window.parent !== window) window.parent.postMessage({ type: "open-tab", payload: { kind: "song", tab: ymd(day) } }, location.origin);
    else location.href = `song.html${location.search ? `${location.search}&` : "?"}day=${ymd(day)}`;
  };
  document.getElementById("copy")!.onclick = () => {
    canvas.toBlob(async (blob) => {
      if (!blob) return;
      try {
        await navigator.clipboard.write([new ClipboardItem({ "image/png": blob })]);
        toast(tr("复制好了，可以直接贴进聊天。", "Copied. Paste it straight into a chat."));
      } catch (e) {
        toast(tr(`没复制上：${String(e)}`, `Copy failed: ${String(e)}`));
      }
    }, "image/png");
  };
}

main().catch((e) => {
  document.body.textContent = tr("报告加载失败：", "Failed to load the report: ") + String(e);
});
