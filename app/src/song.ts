// 收工曲: the five play a day back, and a strip under them shows which
// stretch of the day is sounding. It opens as a tab of the window, for today
// (a pet asked before off time: `?play=1` starts it at once) or for any day of
// the report (`?day=YYYY-MM-DD`). The day's turns come from petd; for
// recording, `window.__DAY` may be set before it loads, and in a browser with
// no petd a made-up day plays. `renderAt(sec)` draws the page at one moment;
// `offline()` renders the audio and returns a WAV as base64.

import { compose, type Inst, type Song, type Turn } from "./song/compose";
import { createPerformer, SAMPLES, CELLO_SAMPLES, FLUTE_SAMPLES, XYLOPHONE_SAMPLES, HARP_SAMPLES, type Performer, type Score } from "./rig/perform.js";
import { loadRig } from "./rig/rig.js";
import { applySkin, skinFor, SKINS } from "./skin";
import { connect, getJson } from "./api";
import { initLang, t } from "./i18n";

declare global {
  interface Window { __DAY?: { day: string; turns: Turn[] }; renderAt: (s: number) => void; offline: () => Promise<string>; songLength: number; ready: boolean }
}

const SETS: Record<Inst, string[]> = { piano: SAMPLES, cello: CELLO_SAMPLES, flute: FLUTE_SAMPLES, xylophone: XYLOPHONE_SAMPLES, harp: HARP_SAMPLES };
const LOOK: Record<Inst, { attack: number; release: number; volume: number }> = {
  piano: { attack: 0, release: 1.4, volume: 0 },
  cello: { attack: 0.06, release: 0.5, volume: -1 },
  flute: { attack: 0.03, release: 0.35, volume: -5 },
  xylophone: { attack: 0, release: 1.1, volume: -9 },
  harp: { attack: 0, release: 1.8, volume: -3 },
};
const PETS: { inst: Inst; skin: keyof typeof SKINS; zh: string; en: string; plays: [string, string] }[] = [
  { inst: "harp", skin: "rongrong", zh: "茸茸", en: "Rongrong", plays: ["竖琴 · 这一天的屋子", "Harp · the room all day"] },
  { inst: "cello", skin: "mashu", zh: "麻薯", en: "Mashu", plays: ["大提琴 · Codex", "Cello · Codex"] },
  { inst: "piano", skin: "zimeng", zh: "紫梦", en: "Zimeng", plays: ["钢琴 · Claude Code", "Piano · Claude Code"] },
  { inst: "flute", skin: "paopao", zh: "泡泡", en: "Paopao", plays: ["长笛 · 做完一大件", "Flute · a big job done"] },
  { inst: "xylophone", skin: "gugu", zh: "菇菇", en: "Gugu", plays: ["木琴 · 每一轮做完", "Xylophone · every turn done"] },
];
const COLOR: Record<string, string> = { "claude-code": "#e0794f", codex: "#2f9a8a" };
const LABEL: Record<string, string> = { "claude-code": "Claude Code", codex: "Codex", gemini: "Gemini", cursor: "Cursor", opencode: "OpenCode" };

/** A made-up day, when none is given: two agents, a stuck afternoon, a long job at the end. */
function madeUp(): Turn[] {
  const d0 = new Date();
  d0.setHours(9, 0, 0, 0);
  const at = (h: number) => d0.getTime() + h * 3_600_000;
  const out: Turn[] = [];
  for (let h = 0.2; h < 9; h += 0.22) {
    const stuck = h > 4.6 && h < 5.6;
    if (h > 3 && h < 3.9) continue; // lunch
    out.push({ source: "claude-code", started_at: at(h), ended_at: at(h + 0.15), result: stuck ? "error" : "ok", tool_calls: 6 + Math.round(8 * Math.sin(h)), tool_errors: stuck ? 2 : 0 });
  }
  for (let h = 1; h < 8.5; h += 0.9) out.push({ source: "codex", started_at: at(h), ended_at: at(h + 0.5), result: "ok", tool_calls: 10, tool_errors: 0 });
  out.push({ source: "codex", started_at: at(7.4), ended_at: at(8.2), result: "ok", tool_calls: 30, tool_errors: 0 });
  return out;
}

const query = new URLSearchParams(location.search);
const ymd = (x: Date) => `${x.getFullYear()}-${String(x.getMonth() + 1).padStart(2, "0")}-${String(x.getDate()).padStart(2, "0")}`;
/** Fewer turns than this and there is nothing to play (petd asks from the same number). */
const MIN_TURNS = 6;

/** The day to play: given, or petd's, or (a browser with no petd) a made-up one. */
async function theDay(): Promise<{ day: string; turns: Turn[] }> {
  if (window.__DAY) return window.__DAY;
  const want = query.get("day") ?? ymd(new Date());
  try {
    const conn = await connect();
    await initLang(conn);
    applySkin((await skinFor(conn, query.get("pet"))).skin.id);
    return await getJson<{ day: string; turns: Turn[] }>(conn, `/v1/song?day=${want}`);
  } catch {
    applySkin("zimeng");
    return { day: want, turns: madeUp() };
  }
}

const day = await theDay();
const seed = Number(day.day.replace(/-/g, "")) || 1;
const empty = day.turns.length < MIN_TURNS;
const song: Song = compose(day.turns, seed);
window.songLength = song.end;
const d = new Date(`${day.day}T12:00:00`);
document.getElementById("date")!.textContent = t(
  `${d.getMonth() + 1} 月 ${d.getDate()} 日，周${"日一二三四五六"[d.getDay()]}`,
  d.toLocaleDateString("en-GB", { weekday: "long", day: "numeric", month: "long" }),
);

// ---- the five, each with its instrument out, playing its own part of the day
const band = document.getElementById("band")!;
const seats = new Map<Inst, HTMLElement>();
interface Player { inst: Inst; cv: HTMLCanvasElement; perf: Performer; foot: [number, number]; box: [number, number, number, number] }
const players: Player[] = [];
/** Each instrument's notes as its player reads them: the piano's in two hands, the cello bowed, the rest on themselves. */
function partOf(inst: Inst): Score {
  const ev = song.notes.filter((n) => n.inst === inst).map((n) => {
    const base = { t: n.t, note: n.note, dur: n.dur, vel: n.vel };
    if (inst === "piano") return { ...base, hand: (/[0-3]$/.test(n.note) ? "l" : "r") as "l" | "r", tap: true };
    if (inst === "cello") return { ...base, bow: true };
    return { ...base, own: true };
  });
  return { ev, end: song.end };
}
const loading = PETS.map(async (p) => {
  const el = document.createElement("div");
  el.className = "seat";
  el.innerHTML = `<div class="glow"></div><canvas></canvas><div class="cushion"></div><b>${t(p.zh, p.en)}</b><small>${t(...p.plays)}</small>`;
  band.append(el);
  seats.set(p.inst, el);
  const rig = await loadRig(SKINS[p.skin].rig ?? "rig/zimeng/");
  rig.expr = "calm";
  // Silent: the page plays the sound itself, so the five stay on one clock.
  const perf = createPerformer(null, rig, { instrument: p.inst });
  await perf.start("play", { silent: true, score: partOf(p.inst) });
  players.push({ inst: p.inst, cv: el.querySelector("canvas")!, perf, foot: rig.foot as [number, number], box: rig.rig.bbox as [number, number, number, number] });
});

/** Draw every player at music time `sec` (`dt` since the last frame). */
function drawPlayers(sec: number, dt: number) {
  const dpr = devicePixelRatio || 1;
  for (const p of players) {
    const W = p.cv.clientWidth, H = p.cv.clientHeight;
    if (p.cv.width !== Math.round(W * dpr)) {
      p.cv.width = Math.round(W * dpr);
      p.cv.height = Math.round(H * dpr);
    }
    p.perf.tick(dt, Math.max(0, sec));
    const g = p.cv.getContext("2d")!;
    g.setTransform(dpr, 0, 0, dpr, 0, 0);
    g.clearRect(0, 0, W, H);
    // Room around the body for the instrument: the keys float in front, the cello stands beside.
    const [x0, y0, x1, y1] = p.box;
    const s = Math.min(W / ((x1 - x0) * 1.5), (H - 14) / ((y1 - y0) * 1.32));
    // The harp and the cello stand at the player's left: move those two right a little so their instruments stay in the frame.
    const shift = p.inst === "harp" ? 0.16 : p.inst === "cello" ? 0.07 : 0;
    g.translate(W / 2 + W * shift - p.foot[0] * s, H - 14 - p.foot[1] * s);
    p.perf.draw(g, s);
  }
}

// ---- the day under them
const cv = document.getElementById("day") as HTMLCanvasElement;
const ctx = cv.getContext("2d")!;
const sources = [...new Set(day.turns.map((t) => t.source))].sort((a, b) => (a === "claude-code" ? -1 : b === "claude-code" ? 1 : a.localeCompare(b)));
const peak: Record<string, number> = {};
for (const s of sources) peak[s] = Math.max(1e-6, ...song.bars.map((b) => b.work[s] ?? 0));
const hm = (ms: number) => {
  const x = new Date(ms);
  return `${String(x.getHours()).padStart(2, "0")}:${String(x.getMinutes()).padStart(2, "0")}`;
};

function drawDay(sec: number) {
  const dpr = devicePixelRatio || 1;
  const W = cv.clientWidth, H = cv.clientHeight;
  if (cv.width !== Math.round(W * dpr)) {
    cv.width = Math.round(W * dpr);
    cv.height = Math.round(H * dpr);
  }
  ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
  ctx.clearRect(0, 0, W, H);
  const L = 104, R = W - 12, laneH = 34, top = 10;
  const n = song.bars.length;
  const bw = (R - L) / n;
  // Where the music is: which bar, and how far into it.
  const prog = Math.max(0, Math.min(n, (sec - song.start) / song.barSec));
  ctx.font = "700 12px 'Segoe UI', 'Microsoft YaHei UI', sans-serif";
  sources.forEach((s, k) => {
    const y = top + k * (laneH + 8);
    ctx.fillStyle = "#776775";
    ctx.textBaseline = "middle";
    ctx.fillText(LABEL[s] ?? s, 0, y + laneH / 2);
    ctx.fillStyle = "rgba(91,74,87,0.06)";
    ctx.beginPath();
    ctx.roundRect(L, y, R - L, laneH, 8);
    ctx.fill();
    song.bars.forEach((b, i) => {
      const x = L + i * bw;
      const sounded = i < prog;
      const w = Math.min(1, (b.work[s] ?? 0) / peak[s]);
      if (w > 0.01) {
        ctx.fillStyle = COLOR[s] ?? "#8d7fb5";
        ctx.globalAlpha = sounded ? 0.85 : 0.25;
        const h = Math.max(3, w * (laneH - 8));
        ctx.beginPath();
        ctx.roundRect(x + 2, y + laneH - 4 - h, bw - 4, h, 3);
        ctx.fill();
      }
      ctx.globalAlpha = sounded ? 1 : 0.35;
      // A failure that stood: a small red dot; the turn that resolved it: a green one.
      if ((b.errors[s] ?? 0) > (b.done[s] ?? 0)) {
        ctx.fillStyle = "#c24e38";
        ctx.beginPath();
        ctx.arc(x + bw / 2, y + 7, 3.2, 0, Math.PI * 2);
        ctx.fill();
      }
      if (b.resolves.includes(s)) {
        ctx.fillStyle = "#5a9a78";
        ctx.beginPath();
        ctx.arc(x + bw / 2, y + 7, 3.2, 0, Math.PI * 2);
        ctx.fill();
      }
      ctx.globalAlpha = 1;
    });
  });
  // Hours along the bottom.
  const yb = top + sources.length * (laneH + 8) + 4;
  ctx.fillStyle = "#a8989f";
  ctx.font = "600 11px 'Segoe UI', sans-serif";
  ctx.textBaseline = "top";
  for (let i = 0; i <= n; i += Math.ceil(n / 6)) {
    const ms = song.from + ((song.to - song.from) * i) / n;
    ctx.fillText(hm(ms), L + i * bw - 14, yb);
  }
  // The playhead, and the time of day it is at.
  if (sec > 0 && prog < n) {
    const x = L + prog * bw;
    ctx.strokeStyle = "#7a5fc0";
    ctx.lineWidth = 2;
    ctx.beginPath();
    ctx.moveTo(x, top - 4);
    ctx.lineTo(x, yb - 2);
    ctx.stroke();
    const label = hm(song.from + ((song.to - song.from) * prog) / n);
    ctx.fillStyle = "#7a5fc0";
    ctx.font = "800 12px 'Segoe UI', sans-serif";
    ctx.fillText(label, Math.min(R - 34, x + 4), yb);
  }
}

/** What that stretch of the day was, in a few words: so you can hear what is what. */
function caption(sec: number): string {
  const i = Math.floor((sec - song.start) / song.barSec);
  const b = song.bars[i];
  if (!b) return t("最后一起。", "All together, to end.");
  const who = (s: string) => LABEL[s] ?? s;
  if (b.resolves.length) return t(`${who(b.resolves[0])} 解开了。`, `${who(b.resolves[0])} got unstuck.`);
  const stuck = sources.find((s) => (b.errors[s] ?? 0) > (b.done[s] ?? 0));
  if (stuck) return t(`这里 ${who(stuck)} 卡住了。`, `Here ${who(stuck)} got stuck.`);
  const done = Object.values(b.done).reduce((s, n) => s + n, 0);
  if (done >= 3) return t("一下做完好几件。", "Several done at once.");
  if (b.quiet) return t("这会儿没人干活。", "Nobody working just now.");
  const busiest = sources.filter((s) => (b.work[s] ?? 0) > 0).sort((x, y) => (b.work[y] ?? 0) / peak[y] - (b.work[x] ?? 0) / peak[x]);
  if (busiest.length > 1) return t("两边都在忙。", "Both busy.");
  if (busiest.length === 1) return t(`${who(busiest[0])} 在忙。`, `${who(busiest[0])} at work.`);
  return "……";
}

/** Who sounds at `sec`: their seat lifts and glows. */
function light(sec: number) {
  for (const [inst, el] of seats) el.classList.toggle("on", song.notes.some((nt) => nt.inst === inst && sec >= nt.t && sec < nt.t + Math.min(0.32, nt.dur)));
}

let lastSec = 0;
window.renderAt = (sec: number) => {
  drawDay(sec);
  light(sec);
  drawPlayers(sec, Math.max(0, Math.min(0.1, sec - lastSec)) || 1 / 30);
  lastSec = sec;
  const said = document.getElementById("said")!;
  said.textContent = empty
    ? t("那天没怎么开工，没什么可弹的。", "Hardly any work that day: nothing to play.")
    : sec < song.start
      ? t("这一天的，弹给你听。", "This day, played back.")
      : sec > song.end - 4.6
        ? t("就到这儿。明天见。", "That is all. See you tomorrow.")
        : caption(sec);
};
window.renderAt(0);
// Before anything plays they idle on their instruments.
const idle = (t: number) => {
  if (!playingLive && !capturing) drawPlayers(0, 1 / 60);
  requestAnimationFrame(idle);
  void t;
};
let playingLive = false;
let capturing = false;
requestAnimationFrame(idle);

// ---- sound
async function samplers(Tone: typeof import("tone")) {
  const rev = new Tone.Reverb({ decay: 2.6, wet: 0.22 }).toDestination();
  await rev.ready;
  const out: Partial<Record<Inst, import("tone").Sampler>> = {};
  for (const inst of Object.keys(SETS) as Inst[]) {
    out[inst] = new Tone.Sampler({
      urls: Object.fromEntries(SETS[inst].map((n) => [n, n.replace("#", "s") + ".mp3"])),
      baseUrl: `audio/${inst}/`,
      ...LOOK[inst],
    }).connect(rev);
  }
  await Tone.loaded();
  return out as Record<Inst, import("tone").Sampler>;
}

function schedule(s: Record<Inst, import("tone").Sampler>, t0: number) {
  for (const nt of song.notes) s[nt.inst].triggerAttackRelease(nt.note, nt.dur, t0 + nt.t, nt.vel);
}

/** The whole piece rendered offline, as a WAV in base64. */
window.offline = async () => {
  const Tone = await import("tone");
  const buf = await Tone.Offline(async () => {
    const s = await samplers(Tone);
    schedule(s, 0.05);
  }, song.end + 0.5, 2, 44100);
  const a = buf.get()!;
  const ch = [a.getChannelData(0), a.getChannelData(1)];
  const n = a.length;
  const data = new DataView(new ArrayBuffer(44 + n * 4));
  const str = (o: number, s: string) => [...s].forEach((c, i) => data.setUint8(o + i, c.charCodeAt(0)));
  str(0, "RIFF"); data.setUint32(4, 36 + n * 4, true); str(8, "WAVE"); str(12, "fmt ");
  data.setUint32(16, 16, true); data.setUint16(20, 1, true); data.setUint16(22, 2, true); data.setUint32(24, 44100, true);
  data.setUint32(28, 44100 * 4, true); data.setUint16(32, 4, true); data.setUint16(34, 16, true); str(36, "data"); data.setUint32(40, n * 4, true);
  for (let i = 0; i < n; i++) for (let c = 0; c < 2; c++) data.setInt16(44 + i * 4 + c * 2, Math.max(-1, Math.min(1, ch[c][i])) * 0x7fff, true);
  const bytes = new Uint8Array(data.buffer);
  let bin = "";
  for (let i = 0; i < bytes.length; i += 0x8000) bin += String.fromCharCode(...bytes.subarray(i, i + 0x8000));
  return btoa(bin);
};

// Live: press play (or it starts by itself, when a pet's offer was accepted).
const btn = document.getElementById("play") as HTMLButtonElement;
btn.disabled = empty;
document.getElementById("hint")!.textContent = empty
  ? ""
  : t(
      `${day.turns.length} 轮 · ${Math.round(song.end)} 秒 · ${sources.map((s) => LABEL[s] ?? s).join("、")}`,
      `${day.turns.length} turns · ${Math.round(song.end)} s · ${sources.map((s) => LABEL[s] ?? s).join(", ")}`,
    );
let kit: Record<Inst, import("tone").Sampler> | null = null;
btn.onclick = async () => {
  if (empty) return;
  btn.disabled = true;
  btn.textContent = t("在弹……", "Playing…");
  playingLive = true;
  const Tone = await import("tone");
  await Tone.start();
  const s = (kit ??= await samplers(Tone));
  const t0 = Tone.now() + 0.2;
  schedule(s, t0);
  const begin = performance.now() + 200;
  const tick = () => {
    const sec = (performance.now() - begin) / 1000;
    window.renderAt(sec);
    if (sec < song.end) requestAnimationFrame(tick);
    else {
      btn.disabled = false;
      btn.textContent = t("再听一遍", "Play it again");
      playingLive = false;
    }
  };
  requestAnimationFrame(tick);
};
void Promise.all(loading).then(() => {
  window.ready = true;
  if (query.get("play") === "1" && !empty) btn.click();
});
/** For recording: stop the idle loop drawing over the frames. */
(window as unknown as { capture: () => void }).capture = () => (capturing = true);
