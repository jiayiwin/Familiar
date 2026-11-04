// Spring Festival page: a countdown to 正月初一, a 30-second "抢红包" game
// that pays a few coins, and festive background music.

import { connect, getJson, postJson, type Conn } from "./api";
import { Bgm } from "./bgm";
import { EN, initLang, t as tr } from "./i18n"; // `t` is a common local name here

interface NewYear { date: string; eve: string; year: number; zodiac: string; zodiac_en: string }
interface Info { next: NewYear | null; rounds: number; paid_rounds: number; round_max: number; music: { name: string; picked: boolean; lyrics: boolean } | null }

const $ = (id: string) => document.getElementById(id)!;
const bgm = new Bgm();
let conn: Conn | null = null;
let info: Info | null = null;

// ---------------------------------------------------------------- countdown

const WEEK_ZH = "日一二三四五六";
const WEEK_EN = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];
const DAY_ZH = ["初一", "初二", "初三", "初四", "初五", "初六", "初七"];

/** Local midnight of "YYYY-MM-DD". */
function midnight(ymd: string): Date {
  const [y, m, d] = ymd.split("-").map(Number);
  return new Date(y, m - 1, d);
}

function dateLabel(ymd: string): string {
  const d = midnight(ymd);
  return EN
    ? `${WEEK_EN[d.getDay()]} ${d.toLocaleDateString("en", { month: "short", day: "numeric", year: "numeric" })}`
    : `${d.getFullYear()} 年 ${d.getMonth() + 1} 月 ${d.getDate()} 日 星期${WEEK_ZH[d.getDay()]}`;
}

/** Colours from the page's CSS tokens (light or dark). */
const PAL = { ink: "#161514", muted: "#8c857b", gold: "#a88454", red: "#a8382b", panel: "#faf8f4", line: "#e2ddd4" };
function readPalette() {
  const cs = getComputedStyle(document.documentElement);
  const v = (n: string, d: string) => cs.getPropertyValue(n).trim() || d;
  Object.assign(PAL, { ink: v("--ink", PAL.ink), muted: v("--muted", PAL.muted), gold: v("--accent", PAL.gold), red: v("--red", PAL.red), panel: v("--panel", PAL.panel), line: v("--line", PAL.line) });
}
readPalette();
matchMedia("(prefers-color-scheme: dark)").addEventListener("change", readPalette);
const SERIF = getComputedStyle(document.documentElement).getPropertyValue("--serif").trim() || "Georgia, serif";

const STEMS = "甲乙丙丁戊己庚辛壬癸";
const BRANCHES = "子丑寅卯辰巳午未申酉戌亥";
const ganzhi = (y: number) => STEMS[(y - 4) % 10] + BRANCHES[(y - 4) % 12];

let lastSec = -1;
let celebrated = false;

function renderCount() {
  const el = $("count");
  const ny = info?.next;
  if (!ny) {
    el.innerHTML = `<div class="greet">${tr("恭喜发财", "Happy New Year")}</div>`;
    return;
  }
  const now = new Date();
  const left = midnight(ny.date).getTime() - now.getTime();
  const zodiac = EN ? `Year of the ${ny.zodiac_en}` : `${ny.zodiac}年`;
  if (left <= 0) {
    // During the festival (through 初七).
    const day = Math.floor((now.getTime() - midnight(ny.date).getTime()) / 86_400_000);
    if (el.dataset.mode !== `fest${day}`) {
      el.dataset.mode = `fest${day}`;
      el.innerHTML = `<div class="kicker">${EN ? `${ny.year} · ${zodiac}` : `${ny.year} · ${ganzhi(ny.year)}${zodiac}`}</div>
        <div class="greet">${tr("新年快乐", "Happy Lunar New Year")}</div>
        <div class="eve">${EN ? `Day ${day + 1} of the new year` : `正月${DAY_ZH[day] ?? ""} · 恭喜发财`}</div>`;
    }
    if (!celebrated) {
      celebrated = true;
      fireworks(18);
    }
    return;
  }
  const s = Math.floor(left / 1000);
  if (s === lastSec) return;
  lastSec = s;
  const parts = [Math.floor(s / 86400), Math.floor(s / 3600) % 24, Math.floor(s / 60) % 60, s % 60];
  if (el.dataset.mode !== "count") {
    el.dataset.mode = "count";
    const eveToday = new Date().toDateString() === midnight(ny.eve).toDateString();
    const cell = (i: number, label: string) => `<div class="cell"><span class="num" id="c${i}"></span><span class="unit">${label}</span></div>`;
    el.innerHTML = `
      <div class="kicker">${EN ? `${ny.year} · Year of the ${ny.zodiac_en}` : `${ny.year} · ${ganzhi(ny.year)}${zodiac}`}</div>
      <div class="sub">${eveToday ? tr("今夜除夕，距离正月初一", "New Year's Eve — the new year begins in") : tr("距离春节还有", "Lunar New Year begins in")}</div>
      <div class="digits">${cell(0, tr("天", "Days"))}${cell(1, tr("时", "Hours"))}${cell(2, tr("分", "Minutes"))}${cell(3, tr("秒", "Seconds"))}</div>
      <div class="eve"><span>${tr("除夕", "Eve")} · ${dateLabel(ny.eve)}</span><i></i><span>${tr("正月初一", "New Year's Day")} · ${dateLabel(ny.date)}</span></div>`;
  }
  parts.forEach((v, i) => {
    const c = $(`c${i}`);
    const txt = i === 0 ? String(v) : String(v).padStart(2, "0");
    if (c.textContent !== txt) {
      c.textContent = txt;
      c.classList.remove("tick");
      void c.offsetWidth;
      c.classList.add("tick");
    }
  });
  // The last ten seconds: count out loud with a firework each.
  if (s <= 10 && s > 0) fireworks(1);
}

// ---------------------------------------------------------------- game

const canvas = $("game") as HTMLCanvasElement;
const ctx = canvas.getContext("2d")!;
let W = 0;
let H = 0;

function resize() {
  const r = canvas.getBoundingClientRect();
  const dpr = devicePixelRatio || 1;
  W = r.width;
  H = r.height;
  canvas.width = Math.round(W * dpr);
  canvas.height = Math.round(H * dpr);
  ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
}

type Kind = "packet" | "ingot" | "cracker";
interface Drop { kind: Kind; x: number; y: number; vy: number; phase: number; size: number; value: number; spin: number; gone: number }
interface Spark { x: number; y: number; vx: number; vy: number; life: number; t: number; color: string; size: number; gravity: number }
interface Float { x: number; y: number; text: string; color: string; t: number }

const ROUND_MS = 30_000;
let drops: Drop[] = [];
let sparks: Spark[] = [];
let floats: Float[] = [];
let running = false;
let roundStart = 0;
let nextSpawn = 0;
let score = 0;
let grabbed = 0;
let combo = 0;
let lastGrab = 0;
let shake = 0;
let last = performance.now();

function spawn(now: number) {
  const k = (now - roundStart) / ROUND_MS; // 0 → 1: faster and busier
  const r = Math.random();
  const kind: Kind = r < 0.06 ? "ingot" : r < 0.06 + 0.1 + 0.08 * k ? "cracker" : "packet";
  const size = kind === "packet" ? 30 + Math.random() * 16 : kind === "ingot" ? 34 : 30;
  drops.push({
    kind,
    x: 30 + Math.random() * (W - 60),
    y: -60,
    vy: (kind === "ingot" ? 190 : 110) + Math.random() * 60 + 120 * k,
    phase: Math.random() * Math.PI * 2,
    size,
    value: kind === "ingot" ? 20 : kind === "cracker" ? -10 : Math.round(size / 10) - 1, // 2–4
    spin: (Math.random() - 0.5) * 0.6,
    gone: 0,
  });
  nextSpawn = now + Math.max(170, 420 - 230 * k) * (0.6 + Math.random() * 0.8);
}

function burst(x: number, y: number, colors: string[], n: number, speed = 260, gravity = 500) {
  for (let i = 0; i < n; i++) {
    const a = Math.random() * Math.PI * 2;
    const v = speed * (0.3 + Math.random() * 0.7);
    sparks.push({ x, y, vx: Math.cos(a) * v, vy: Math.sin(a) * v - speed * 0.3, life: 0.6 + Math.random() * 0.6, t: 0, color: colors[i % colors.length], size: 2 + Math.random() * 3, gravity });
  }
}

/** Fireworks over the game area (festival, last seconds of the countdown). */
function fireworks(n: number) {
  for (let i = 0; i < n; i++) {
    setTimeout(() => {
      const palette = [[PAL.gold, PAL.gold, PAL.red], [PAL.red, PAL.gold]][Math.floor(Math.random() * 2)];
      burst(W * (0.15 + Math.random() * 0.7), H * (0.15 + Math.random() * 0.4), palette, 40, 220, 90);
    }, i * 380);
  }
}

function grab(d: Drop, now: number) {
  d.gone = now;
  if (d.kind === "cracker") {
    score = Math.max(0, score + d.value);
    combo = 0;
    shake = 14;
    bgm.boom();
    burst(d.x, d.y, [PAL.ink, PAL.red, PAL.muted], 34, 320);
    floats.push({ x: d.x, y: d.y, text: tr("嘭 −10", "Bang −10"), color: PAL.red, t: now });
    return;
  }
  combo = now - lastGrab < 1200 ? combo + 1 : 1;
  lastGrab = now;
  const bonus = Math.floor(combo / 5);
  const gain = d.value + bonus;
  score += gain;
  grabbed++;
  if (d.kind === "ingot") {
    bgm.chime();
    burst(d.x, d.y, [PAL.gold, PAL.gold, PAL.red], 30, 280);
  } else {
    bgm.ding(1 + Math.min(combo, 12) * 0.03);
    burst(d.x, d.y, [PAL.gold, PAL.red], 12, 180);
  }
  floats.push({ x: d.x, y: d.y, text: `+${gain}${combo >= 5 ? ` ${tr("连击", "combo")}×${combo}` : ""}`, color: d.kind === "ingot" ? PAL.gold : PAL.ink, t: now });
}

canvas.addEventListener("pointerdown", (ev) => {
  bgm.wake();
  if (!running) return;
  const r = canvas.getBoundingClientRect();
  const x = ev.clientX - r.left;
  const y = ev.clientY - r.top;
  const now = performance.now();
  // Top-most first; generous hit box.
  for (let i = drops.length - 1; i >= 0; i--) {
    const d = drops[i];
    if (d.gone) continue;
    const pad = 10;
    if (Math.abs(x - d.x) < d.size * 0.6 + pad && Math.abs(y - d.y) < d.size * 0.75 + pad) {
      grab(d, now);
      return;
    }
  }
  combo = 0;
});

function startRound() {
  bgm.wake();
  $("intro").hidden = true;
  $("result").hidden = true;
  drops = [];
  floats = [];
  score = 0;
  grabbed = 0;
  combo = 0;
  running = true;
  roundStart = performance.now();
  nextSpawn = roundStart;
}

async function endRound() {
  running = false;
  for (const d of drops) if (!d.gone) d.gone = performance.now();
  fireworks(3);
  $("r-title").textContent = score >= 150 ? tr("红包大王！", "Red-envelope king!") : score >= 80 ? tr("手气不错！", "Nice haul!") : tr("时间到！", "Time's up!");
  $("r-score").textContent = String(score);
  $("r-line").textContent = tr(`抢到 ${grabbed} 个红包`, `${grabbed} red envelopes grabbed`);
  $("r-coins").textContent = "";
  $("result").hidden = false;
  if (!conn) return;
  const r = await postJson<{ ok: boolean; coins?: number; left?: number; message?: string }>(conn, "/v1/spring/hongbao", { score, grabbed });
  if (!r.ok) $("r-coins").textContent = r.message ?? "";
  else if ((r.coins ?? 0) > 0) $("r-coins").textContent = tr(`+${r.coins} 金币 · 今天还能领 ${r.left} 次`, `+${r.coins} coins · ${r.left} paid rounds left today`);
  else $("r-coins").textContent = tr("今天的红包金币领完了，玩得开心就好～", "No more coins today — play just for fun!");
  void refreshInfo();
}

// ---------------------------------------------------------------- drawing

function rr(x: number, y: number, w: number, h: number, r: number) {
  ctx.beginPath();
  ctx.roundRect(x, y, w, h, r);
}

function drawPacket(s: number) {
  const w = s;
  const h = s * 1.3;
  ctx.fillStyle = PAL.red;
  rr(-w / 2, -h / 2, w, h, 2);
  ctx.fill();
  ctx.strokeStyle = PAL.gold;
  ctx.lineWidth = 1;
  // flap line and a small seal
  ctx.beginPath();
  ctx.moveTo(-w / 2, -h / 2 + s * 0.12);
  ctx.quadraticCurveTo(0, -h / 2 + s * 0.5, w / 2, -h / 2 + s * 0.12);
  ctx.stroke();
  ctx.beginPath();
  ctx.arc(0, -h / 2 + s * 0.44, s * 0.15, 0, Math.PI * 2);
  ctx.fillStyle = PAL.gold;
  ctx.fill();
  ctx.fillStyle = PAL.red;
  ctx.font = `${Math.round(s * 0.2)}px ${SERIF}`;
  ctx.textAlign = "center";
  ctx.textBaseline = "middle";
  ctx.fillText("福", 0, -h / 2 + s * 0.45);
}

function drawIngot(s: number, now: number) {
  ctx.fillStyle = PAL.gold;
  ctx.beginPath();
  ctx.moveTo(-s * 0.7, -s * 0.12);
  ctx.quadraticCurveTo(-s * 0.5, s * 0.38, 0, s * 0.36);
  ctx.quadraticCurveTo(s * 0.5, s * 0.38, s * 0.7, -s * 0.12);
  ctx.quadraticCurveTo(s * 0.36, s * 0.04, 0, s * 0.04);
  ctx.quadraticCurveTo(-s * 0.36, s * 0.04, -s * 0.7, -s * 0.12);
  ctx.fill();
  ctx.beginPath();
  ctx.ellipse(0, -s * 0.02, s * 0.26, s * 0.2, 0, Math.PI, 0);
  ctx.fill();
  // a slow glint
  const tw = (Math.sin(now / 260) + 1) / 2;
  ctx.strokeStyle = `rgba(255,255,255,${0.25 + 0.5 * tw})`;
  ctx.lineWidth = 1;
  ctx.beginPath();
  ctx.moveTo(-s * 0.12, -s * 0.16);
  ctx.quadraticCurveTo(0, -s * 0.2, s * 0.12, -s * 0.16);
  ctx.stroke();
}

function drawCracker(s: number, now: number) {
  const w = s * 0.4;
  const h = s * 1.15;
  ctx.fillStyle = PAL.ink;
  rr(-w / 2, -h / 2, w, h, 2);
  ctx.fill();
  ctx.fillStyle = PAL.red;
  ctx.fillRect(-w / 2, -h / 2 + h * 0.2, w, 2);
  ctx.fillRect(-w / 2, h / 2 - h * 0.24, w, 2);
  // fuse and spark
  ctx.strokeStyle = PAL.muted;
  ctx.lineWidth = 1;
  ctx.beginPath();
  ctx.moveTo(0, -h / 2);
  ctx.quadraticCurveTo(s * 0.22, -h / 2 - s * 0.18, s * 0.08, -h / 2 - s * 0.34);
  ctx.stroke();
  const f = 0.5 + 0.5 * Math.sin(now / 45);
  ctx.fillStyle = PAL.gold;
  ctx.globalAlpha *= 0.5 + 0.5 * f;
  ctx.beginPath();
  ctx.arc(s * 0.08, -h / 2 - s * 0.34, 2 + 2 * f, 0, Math.PI * 2);
  ctx.fill();
}

function frame(now: number) {
  const dt = Math.min(0.05, (now - last) / 1000);
  last = now;
  renderCount();
  renderLyric();

  if (running) {
    if (now >= nextSpawn) spawn(now);
    if (now - roundStart >= ROUND_MS) void endRound();
  }
  ctx.clearRect(0, 0, W, H);
  ctx.save();
  if (shake > 0) {
    ctx.translate((Math.random() - 0.5) * shake, (Math.random() - 0.5) * shake);
    shake = Math.max(0, shake - dt * 60);
  }

  // falling things
  drops = drops.filter((d) => (d.gone ? now - d.gone < 250 : d.y < H + 80));
  for (const d of drops) {
    if (!d.gone) {
      d.y += d.vy * dt;
      d.phase += dt * 2;
    }
    const sway = Math.sin(d.phase) * 14;
    ctx.save();
    ctx.translate(d.x + sway, d.y);
    ctx.rotate(Math.sin(d.phase) * d.spin);
    if (d.gone) {
      const k = (now - d.gone) / 250;
      ctx.globalAlpha = 1 - k;
      ctx.scale(1 + k * 0.6, 1 + k * 0.6);
    }
    if (d.kind === "packet") drawPacket(d.size);
    else if (d.kind === "ingot") drawIngot(d.size, now);
    else drawCracker(d.size, now);
    ctx.restore();
    if (!d.gone) d.x = Math.min(W - 24, Math.max(24, d.x)); // keep inside after sway
  }

  // sparks
  sparks = sparks.filter((p) => p.t < p.life);
  for (const p of sparks) {
    p.t += dt;
    p.vy += p.gravity * dt;
    p.x += p.vx * dt;
    p.y += p.vy * dt;
    ctx.globalAlpha = Math.max(0, 1 - p.t / p.life);
    ctx.fillStyle = p.color;
    ctx.beginPath();
    ctx.arc(p.x, p.y, p.size, 0, Math.PI * 2);
    ctx.fill();
  }
  ctx.globalAlpha = 1;

  // floating "+N"
  floats = floats.filter((f) => now - f.t < 900);
  ctx.textAlign = "center";
  ctx.textBaseline = "middle";
  for (const f of floats) {
    const k = (now - f.t) / 900;
    ctx.globalAlpha = 1 - k;
    ctx.font = `italic 18px ${SERIF}`;
    ctx.fillStyle = f.color;
    ctx.fillText(f.text, f.x, f.y - 40 * k);
  }
  ctx.globalAlpha = 1;
  ctx.restore();

  // HUD: a hairline timer, the score in serif
  if (running) {
    const left = Math.max(0, ROUND_MS - (now - roundStart));
    ctx.fillStyle = PAL.line;
    ctx.fillRect(0, 0, W, 2);
    ctx.fillStyle = left < 5000 ? PAL.red : PAL.gold;
    ctx.fillRect(0, 0, W * (left / ROUND_MS), 2);
    ctx.textBaseline = "top";
    ctx.textAlign = "left";
    ctx.fillStyle = PAL.ink;
    ctx.font = `28px ${SERIF}`;
    ctx.fillText(String(score), 20, 18);
    ctx.fillStyle = PAL.muted;
    ctx.font = "10px 'Segoe UI', sans-serif";
    ctx.fillText(tr("得分", "SCORE"), 22, 52);
    ctx.textAlign = "right";
    ctx.fillStyle = left < 5000 ? PAL.red : PAL.ink;
    ctx.font = `28px ${SERIF}`;
    ctx.fillText(String(Math.ceil(left / 1000)), W - 20, 18);
    ctx.fillStyle = PAL.muted;
    ctx.font = "10px 'Segoe UI', sans-serif";
    ctx.fillText(tr("秒", "SEC"), W - 22, 52);
    if (combo >= 3) {
      ctx.textAlign = "center";
      ctx.font = `italic 18px ${SERIF}`;
      ctx.fillStyle = PAL.gold;
      ctx.fillText(tr(`连击 × ${combo}`, `combo × ${combo}`), W / 2, 24);
    }
  } else if (Math.random() < dt * 0.6) {
    // idle: a slow drift of packets behind the start card
    drops.push({ kind: "packet", x: 30 + Math.random() * (W - 60), y: -50, vy: 40 + Math.random() * 20, phase: Math.random() * 6, size: 18 + Math.random() * 8, value: 0, spin: 0.3, gone: 0 });
  }
  requestAnimationFrame(frame);
}

// ---------------------------------------------------------------- music

const PREF = "spring-music";
interface Pref { on: boolean; vol: number }

function readPref(): Pref {
  try {
    return { on: true, vol: 0.6, ...JSON.parse(localStorage.getItem(PREF) ?? "{}") };
  } catch {
    return { on: true, vol: 0.6 };
  }
}

function writePref(p: Pref) {
  try {
    localStorage.setItem(PREF, JSON.stringify(p));
  } catch {
    /* private window */
  }
}

let pref = readPref();

/** The user's song in ~/.familiar/music, served by petd. */
function songUrl(): string | null {
  return conn && info?.music ? `${conn.base}/v1/spring/music?token=${conn.token}&t=${Date.now()}` : null;
}

function songLabel() {
  const note = `<span class="eq${bgm.on ? " on" : ""}"><i></i><i></i><i></i></span>`;
  const m = info?.music;
  $("song").innerHTML = m
    ? `${note}<b>${m.name.replace(/[<>&]/g, "")}</b>`
    : `${note}<b>${tr("锣鼓小调", "Gong & drum tune")}</b> ${tr("· 把《恭喜发财》放进音乐文件夹就会播放它", "· put your own song in the music folder to play it")}`;
  $("reset").hidden = !m?.picked;
  $("mute").textContent = bgm.on ? tr("音乐 开", "Music on") : tr("音乐 关", "Music off");
}

function play() {
  if (pref.on) bgm.start(songUrl());
  else bgm.stop();
  songLabel();
  void loadLyrics();
}

// ---- lyrics (.lrc next to the song), shown in time with it

let lyrics: { t: number; text: string }[] = [];
let lyricAt = -2;

/** "[01:14.27]text" lines → sorted cues; lines with several stamps repeat. */
function parseLrc(src: string): { t: number; text: string }[] {
  const out: { t: number; text: string }[] = [];
  for (const line of src.split(/\r?\n/)) {
    const stamps = [...line.matchAll(/\[(\d+):(\d+(?:\.\d+)?)\]/g)];
    if (!stamps.length) continue;
    const text = line.replace(/\[[^\]]*\]/g, "").trim();
    for (const m of stamps) out.push({ t: Number(m[1]) * 60 + Number(m[2]), text });
  }
  return out.sort((a, b) => a.t - b.t);
}

async function loadLyrics() {
  lyrics = [];
  lyricAt = -2;
  if (conn && info?.music?.lyrics) {
    const r = await fetch(`${conn.base}/v1/spring/lyrics`, { headers: { "X-Familiar-Token": conn.token } }).catch(() => null);
    if (r?.ok) lyrics = parseLrc(await r.text());
  }
  $("lyric").hidden = !lyrics.length;
}

function renderLyric() {
  if (!lyrics.length) return;
  const t = bgm.songTime();
  let i = -1;
  if (t !== null) while (i + 1 < lyrics.length && lyrics[i + 1].t <= t + 0.15) i++;
  if (i === lyricAt) return;
  lyricAt = i;
  const cur = $("ly-cur");
  cur.textContent = i >= 0 ? lyrics[i].text : t === null ? "" : "♪ ♪ ♪";
  $("ly-nxt").textContent = lyrics[i + 1]?.text ?? "";
  cur.classList.remove("pop");
  void cur.offsetWidth;
  cur.classList.add("pop");
}

async function setupMusic() {
  ($("vol") as HTMLInputElement).value = String(Math.round(pref.vol * 100));
  bgm.setVolume(pref.vol);
  play();
  // Audio may only start after the first click in the window.
  addEventListener("pointerdown", () => bgm.wake(), { once: true });
  $("mute").onclick = () => {
    pref.on = !pref.on;
    writePref(pref);
    play();
  };
  $("vol").oninput = (e) => {
    pref.vol = Number((e.target as HTMLInputElement).value) / 100;
    bgm.setVolume(pref.vol);
    writePref(pref);
  };
  $("folder").onclick = async () => {
    const { invoke } = await import("@tauri-apps/api/core");
    await invoke("menu_pick", { id: "musicdir" }).catch(() => {});
  };
  // Back from the folder: pick up a song dropped in there.
  addEventListener("focus", async () => {
    const before = info?.music?.name;
    await refreshInfo();
    if (info?.music?.name !== before) play();
  });
  $("pick").onclick = () => ($("file") as HTMLInputElement).click();
  $("file").onchange = async (e) => {
    const input = e.target as HTMLInputElement;
    const f = input.files?.[0];
    input.value = "";
    if (!f || !conn) return;
    $("song").textContent = tr("正在保存…", "Saving…");
    const name = f.name.replace(/\.[^.]+$/, "");
    const ext = (f.name.match(/\.([^.]+)$/)?.[1] ?? "mp3").toLowerCase();
    const r = await fetch(`${conn.base}/v1/spring/music?name=${encodeURIComponent(name)}&ext=${encodeURIComponent(ext)}`, {
      method: "POST",
      headers: { "X-Familiar-Token": conn.token, "Content-Type": "application/octet-stream" },
      body: f,
    })
      .then((r) => r.json() as Promise<{ ok: boolean; message?: string }>)
      .catch(() => ({ ok: false, message: tr("保存失败", "Could not save") }));
    if (!r.ok) {
      $("song").textContent = r.message ?? "";
      return;
    }
    await refreshInfo();
    pref.on = true;
    writePref(pref);
    play();
  };
  $("reset").onclick = async () => {
    if (!conn) return;
    await fetch(`${conn.base}/v1/spring/music`, { method: "DELETE", headers: { "X-Familiar-Token": conn.token } }).catch(() => {});
    await refreshInfo();
    play();
  };
}

// ---------------------------------------------------------------- start

async function refreshInfo() {
  if (!conn) return;
  info = await getJson<Info>(conn, "/v1/spring").catch(() => info);
  if (!info) return;
  const left = Math.max(0, info.paid_rounds - info.rounds);
  $("quota").textContent = left
    ? tr(`每局最多 ${info.round_max} 金币 · 今天还能领 ${left} 次`, `Up to ${info.round_max} coins a round · ${left} paid rounds left today`)
    : tr("今天的金币领完了，还能接着玩", "No more coins today, but you can keep playing");
}

async function main() {
  resize();
  addEventListener("resize", resize);
  $("start").onclick = startRound;
  $("again").onclick = startRound;
  try {
    conn = await connect();
    await initLang(conn);
  } catch {
    conn = null; // opened outside the app: play without coins
  }
  await refreshInfo();
  await setupMusic();
  requestAnimationFrame(frame);
}

void main();
