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
      el.innerHTML = `<div class="greet">${tr("新年快乐 · 恭喜发财", "Happy Lunar New Year")}</div>
        <div class="sub">${EN ? `${zodiac} · day ${day + 1} of the new year` : `${ny.year} ${zodiac} · 今天是正月<b>${DAY_ZH[day] ?? ""}</b>`}</div>`;
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
    el.innerHTML = `
      <div class="sub">${eveToday ? tr("今天就是除夕！距离正月初一还有", "It's New Year's Eve! The new year starts in") : EN ? `Lunar New Year (${zodiac}) in` : `距离 <b>${ny.year} ${zodiac}</b>春节还有`}</div>
      <div class="digits">
        <span class="cell big" id="c0"></span><span class="unit">${tr("天", "days")}</span>
        <span class="cell" id="c1"></span><span class="unit">${tr("时", "h")}</span>
        <span class="cell" id="c2"></span><span class="unit">${tr("分", "m")}</span>
        <span class="cell" id="c3"></span><span class="unit">${tr("秒", "s")}</span>
      </div>
      <div class="eve">${tr("除夕", "New Year's Eve")}：${dateLabel(ny.eve)} · ${tr("正月初一", "New Year's Day")}：${dateLabel(ny.date)}</div>`;
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
      const palette = [["#ffd36b", "#fff1b8"], ["#ff5a3c", "#ffd36b"], ["#ff9de2", "#fff"], ["#7ee8fa", "#fff"]][Math.floor(Math.random() * 4)];
      burst(W * (0.15 + Math.random() * 0.7), H * (0.15 + Math.random() * 0.4), palette, 46, 240, 120);
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
    burst(d.x, d.y, ["#ffcc33", "#ff5a1f", "#fff", "#888"], 40, 340);
    floats.push({ x: d.x, y: d.y, text: tr("嘭！-10", "Bang! -10"), color: "#ffe1a0", t: now });
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
    burst(d.x, d.y, ["#ffd36b", "#fff1b8", "#f5a623"], 34, 300);
  } else {
    bgm.ding(1 + Math.min(combo, 12) * 0.03);
    burst(d.x, d.y, ["#ffd36b", "#f5a623", "#ff5a3c"], 16, 200);
  }
  floats.push({ x: d.x, y: d.y, text: `+${gain}${combo >= 5 ? ` ${tr("连击", "combo")}×${combo}` : ""}`, color: d.kind === "ingot" ? "#fff1b8" : "#ffd36b", t: now });
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
  const g = ctx.createLinearGradient(0, -h / 2, 0, h / 2);
  g.addColorStop(0, "#f2433a");
  g.addColorStop(1, "#b8121a");
  ctx.fillStyle = g;
  rr(-w / 2, -h / 2, w, h, s * 0.12);
  ctx.fill();
  ctx.strokeStyle = "#ffd36b";
  ctx.lineWidth = 1.5;
  ctx.stroke();
  // flap
  ctx.fillStyle = "#d42a24";
  ctx.beginPath();
  ctx.moveTo(-w / 2, -h / 2 + s * 0.1);
  ctx.quadraticCurveTo(0, -h / 2 + s * 0.55, w / 2, -h / 2 + s * 0.1);
  ctx.lineTo(w / 2, -h / 2 + s * 0.05);
  ctx.lineTo(-w / 2, -h / 2 + s * 0.05);
  ctx.fill();
  // gold seal with 福
  ctx.fillStyle = "#ffd36b";
  ctx.beginPath();
  ctx.arc(0, -h / 2 + s * 0.42, s * 0.2, 0, Math.PI * 2);
  ctx.fill();
  ctx.fillStyle = "#b8121a";
  ctx.font = `bold ${Math.round(s * 0.26)}px 'Microsoft YaHei', sans-serif`;
  ctx.textAlign = "center";
  ctx.textBaseline = "middle";
  ctx.fillText("福", 0, -h / 2 + s * 0.43);
}

function drawIngot(s: number, now: number) {
  const g = ctx.createLinearGradient(0, -s / 2, 0, s / 2);
  g.addColorStop(0, "#fff1b8");
  g.addColorStop(0.5, "#ffc93c");
  g.addColorStop(1, "#c98a00");
  ctx.fillStyle = g;
  ctx.beginPath();
  ctx.moveTo(-s * 0.75, -s * 0.15);
  ctx.quadraticCurveTo(-s * 0.55, s * 0.45, 0, s * 0.42);
  ctx.quadraticCurveTo(s * 0.55, s * 0.45, s * 0.75, -s * 0.15);
  ctx.quadraticCurveTo(s * 0.4, s * 0.02, 0, s * 0.02);
  ctx.quadraticCurveTo(-s * 0.4, s * 0.02, -s * 0.75, -s * 0.15);
  ctx.fill();
  ctx.beginPath();
  ctx.ellipse(0, -s * 0.05, s * 0.3, s * 0.24, 0, Math.PI, 0);
  ctx.fill();
  // twinkle
  const tw = (Math.sin(now / 120) + 1) / 2;
  ctx.fillStyle = `rgba(255,255,255,${0.4 + 0.6 * tw})`;
  ctx.font = `${Math.round(s * 0.4)}px sans-serif`;
  ctx.textAlign = "center";
  ctx.textBaseline = "middle";
  ctx.fillText("✦", s * 0.45, -s * 0.35);
}

function drawCracker(s: number, now: number) {
  const w = s * 0.5;
  const h = s * 1.2;
  const g = ctx.createLinearGradient(-w / 2, 0, w / 2, 0);
  g.addColorStop(0, "#8c0d12");
  g.addColorStop(0.45, "#e8322a");
  g.addColorStop(1, "#8c0d12");
  ctx.fillStyle = g;
  rr(-w / 2, -h / 2, w, h, 4);
  ctx.fill();
  ctx.fillStyle = "#ffd36b";
  ctx.fillRect(-w / 2, -h / 2 + h * 0.18, w, 3);
  ctx.fillRect(-w / 2, h / 2 - h * 0.22, w, 3);
  // fuse with a fizzing spark
  ctx.strokeStyle = "#5a3b1a";
  ctx.lineWidth = 2;
  ctx.beginPath();
  ctx.moveTo(0, -h / 2);
  ctx.quadraticCurveTo(s * 0.25, -h / 2 - s * 0.2, s * 0.1, -h / 2 - s * 0.38);
  ctx.stroke();
  const f = 0.6 + 0.4 * Math.sin(now / 40);
  ctx.fillStyle = `rgba(255,${180 + 60 * f},60,${f})`;
  ctx.beginPath();
  ctx.arc(s * 0.1, -h / 2 - s * 0.38, 3 + 3 * f, 0, Math.PI * 2);
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
    ctx.font = "bold 20px 'Microsoft YaHei', sans-serif";
    ctx.lineWidth = 4;
    ctx.strokeStyle = "rgba(90,0,0,0.7)";
    ctx.strokeText(f.text, f.x, f.y - 40 * k);
    ctx.fillStyle = f.color;
    ctx.fillText(f.text, f.x, f.y - 40 * k);
  }
  ctx.globalAlpha = 1;
  ctx.restore();

  // HUD
  if (running) {
    const left = Math.max(0, ROUND_MS - (now - roundStart));
    ctx.fillStyle = "rgba(0,0,0,0.25)";
    rr(14, 12, W - 28, 8, 4);
    ctx.fill();
    ctx.fillStyle = left < 5000 ? "#ff7a59" : "#ffd36b";
    rr(14, 12, (W - 28) * (left / ROUND_MS), 8, 4);
    ctx.fill();
    ctx.font = "bold 22px 'Microsoft YaHei', sans-serif";
    ctx.textAlign = "left";
    ctx.textBaseline = "top";
    ctx.fillStyle = "#ffd36b";
    ctx.fillText(`🧧 ${score}`, 16, 28);
    ctx.textAlign = "right";
    ctx.fillStyle = left < 5000 ? "#ffb199" : "#fff4dc";
    ctx.fillText(`${Math.ceil(left / 1000)}s`, W - 16, 28);
    if (combo >= 3) {
      ctx.textAlign = "center";
      ctx.font = "bold 18px 'Microsoft YaHei', sans-serif";
      ctx.fillStyle = "#fff1b8";
      ctx.fillText(tr(`连击 ×${combo}`, `Combo ×${combo}`), W / 2, 28);
    }
  } else if (Math.random() < dt * 0.6) {
    // idle: a slow drift of packets behind the start card
    drops.push({ kind: "packet", x: 30 + Math.random() * (W - 60), y: -50, vy: 60 + Math.random() * 30, phase: Math.random() * 6, size: 26 + Math.random() * 10, value: 0, spin: 0.4, gone: 0 });
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
  const note = bgm.on ? `<span class="note">🎵</span> ` : "🎵 ";
  const m = info?.music;
  $("song").innerHTML = m
    ? `${note}<b>${m.name.replace(/[<>&]/g, "")}</b>`
    : `${note}<b>${tr("锣鼓小调", "Gong & drum tune")}</b> ${tr("· 把《恭喜发财》放进音乐文件夹就会播放它", "· put your own song in the music folder to play it")}`;
  $("reset").hidden = !m?.picked;
  $("mute").textContent = bgm.on ? "🔊" : "🔇";
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
