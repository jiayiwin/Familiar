// The radio: stations from all over the world via the Radio Browser
// directory (radio-browser.info), played straight from each station's
// stream. Nothing goes online until this window is opened. Closing the window
// only hides it, so the station keeps playing. The pet listens along and says
// what it knows about the station: whether you have had it on before.

import { connect, inTauri, onHost, postJson, type Conn } from "./api";
import { EN, initLang, t as tr } from "./i18n";
import { mountPortrait } from "./rig/portrait";
import type { RigPet } from "./rig/rig.js";
import { applySkin, followSkin, skinFor } from "./skin";
import { line } from "./voice";

interface Station {
  stationuuid: string;
  name: string;
  url: string;
  url_resolved: string;
  favicon: string;
  countrycode: string;
  country: string;
  tags: string;
  codec: string;
  bitrate: number;
  hls: number;
  clickcount: number;
}
interface Country { name: string; iso_3166_1: string; stationcount: number }
type Tab = "often" | "up" | "calm" | "xs" | "world";
/** Tab names from before the shelves were renamed still arrive (saved, in links, from the tray). */
const asTab = (v: unknown): Tab | null => (v === "hot" || v === "fav" ? "often" : v === "often" || v === "up" || v === "calm" || v === "xs" || v === "world" ? v : null);

const SERVERS = ["de1", "fi1", "de2", "nl1", "at1"].map((s) => `https://${s}.api.radio-browser.info`);
const $ = <T extends HTMLElement>(id: string) => document.getElementById(id) as T;
const list = $("list"), player = $("player"), playBtn = $<HTMLButtonElement>("play"), favBtn = $<HTMLButtonElement>("fav");
const statusEl = $("status"), nameEl = $("name"), metaEl = $("meta"), lineEl = $("line");
const vol = $<HTMLInputElement>("vol"), q = $<HTMLInputElement>("q");

const PLAY = '<svg viewBox="0 0 24 24"><path d="M8 5.5v13a1 1 0 0 0 1.5.86l10.5-6.5a1 1 0 0 0 0-1.72L9.5 4.64A1 1 0 0 0 8 5.5z"/></svg>';
const PAUSE = '<svg viewBox="0 0 24 24"><rect x="6" y="5" width="4" height="14" rx="1.2"/><rect x="14" y="5" width="4" height="14" rx="1.2"/></svg>';

// ---------------------------------------------------------------- storage

function load<T>(key: string, dflt: T): T {
  try {
    const v = localStorage.getItem(key);
    return v ? (JSON.parse(v) as T) : dflt;
  } catch {
    return dflt;
  }
}
function save(key: string, v: unknown) {
  try {
    localStorage.setItem(key, JSON.stringify(v));
  } catch {
    /* private mode */
  }
}
const slim = (s: Station): Station => ({
  stationuuid: s.stationuuid, name: s.name.trim(), url: s.url, url_resolved: s.url_resolved, favicon: s.favicon, countrycode: s.countrycode,
  country: s.country, tags: s.tags, codec: s.codec, bitrate: s.bitrate, hls: s.hls, clickcount: s.clickcount,
});
let favs: Station[] = load("radio.favs", []);
let cur: Station | null = load("radio.last", null);

/** What has been on before: how often, and when (this time and the time before). */
interface Heard { s: Station; n: number; last: number; prev: number }
let heard: Record<string, Heard> = load("radio.heard", {});
function noteHeard(s: Station) {
  const h = heard[s.stationuuid];
  heard[s.stationuuid] = { s: slim(s), n: (h?.n ?? 0) + 1, last: Date.now(), prev: h?.last ?? 0 };
  // Keep the sixty most recent.
  const keep = Object.values(heard).sort((a, b) => b.last - a.last).slice(0, 60);
  heard = Object.fromEntries(keep.map((x) => [x.s.stationuuid, x]));
  save("radio.heard", heard);
}

// ---------------------------------------------------------------- directory

let server = 0;
async function api<T>(path: string): Promise<T> {
  let last: unknown;
  for (let i = 0; i < SERVERS.length; i++) {
    const base = SERVERS[(server + i) % SERVERS.length];
    try {
      const ctl = new AbortController();
      const timer = setTimeout(() => ctl.abort(), 9000);
      const r = await fetch(base + path, { signal: ctl.signal });
      clearTimeout(timer);
      if (!r.ok) throw new Error(String(r.status));
      server = (server + i) % SERVERS.length;
      return (await r.json()) as T;
    } catch (e) {
      last = e;
    }
  }
  throw last;
}
const OK = "hidebroken=true&order=clickcount&reverse=true";
const stations = (path: string, limit = 80) => api<Station[]>(`/json/stations/${path}${path.includes("?") ? "&" : "?"}${OK}&limit=${limit}`);
const dedupe = (v: Station[]) => {
  const seen = new Set<string>();
  return v.filter((s) => s.url_resolved && !seen.has(s.stationuuid) && seen.add(s.stationuuid));
};

const cache = new Map<string, Promise<Station[]>>();
function cached(key: string, f: () => Promise<Station[]>): Promise<Station[]> {
  if (!cache.has(key)) cache.set(key, f().catch((e) => { cache.delete(key); throw e; }));
  return cache.get(key)!;
}
const hot = () => cached("hot", async () => dedupe(await (EN ? stations("search?", 80) : stations("bycountrycodeexact/CN", 80))));
const crosstalk = () =>
  cached("xs", async () => {
    const parts = await Promise.allSettled(["bytag/crosstalk", "byname/相声", "byname/评书", "byname/曲艺", "byname/文艺广播", "bytag/comedy"].map((p) => stations(encodeURI(p), 40)));
    const all = parts.flatMap((p) => (p.status === "fulfilled" ? p.value : []));
    // Chinese 相声 / 评书 first, then comedy from elsewhere.
    const zh = (s: Station) => (/相声|评书|曲艺|crosstalk/i.test(s.name + s.tags) ? 0 : /文艺/.test(s.name) ? 1 : 2);
    return dedupe(all).sort((a, b) => zh(a) - zh(b) || b.clickcount - a.clickcount);
  });
/** Several tags as one shelf, the most listened-to first. */
const byTags = (key: string, tags: string[]) =>
  cached(key, async () => {
    const parts = await Promise.allSettled(tags.map((t) => stations(`bytagexact/${encodeURIComponent(t)}`, 30)));
    return dedupe(parts.flatMap((p) => (p.status === "fulfilled" ? p.value : []))).sort((a, b) => b.clickcount - a.clickcount);
  });
const upbeat = () => byTags("up", ["dance", "funk", "disco", "hits"]);
const quiet = () => byTags("calm", ["ambient", "chillout", "classical", "piano"]);
/** Favourites first, then what has been on most. */
const regulars = (): Station[] => dedupe([...favs, ...Object.values(heard).filter((h) => h.n >= 2).sort((a, b) => b.n - a.n || b.last - a.last).map((h) => h.s)]);
let countries: Country[] | null = null;

// ---------------------------------------------------------------- names

let regionNames: Intl.DisplayNames | null = null;
function countryName(code: string, fallback = ""): string {
  try {
    regionNames ??= new Intl.DisplayNames([EN ? "en" : "zh-CN"], { type: "region" });
    return (code && regionNames.of(code.toUpperCase())) || fallback || code;
  } catch {
    return fallback || code;
  }
}
const TAG_ZH: Record<string, string> = {
  crosstalk: "相声", storytelling: "评书", comedy: "喜剧", news: "新闻", music: "音乐", entertainment: "娱乐", culture: "文化", opera: "戏曲",
  pop: "流行", talk: "谈话", sports: "体育", classical: "古典", jazz: "爵士", rock: "摇滚", traffic: "交通", information: "资讯",
  "easy listening": "轻音乐", oldies: "老歌", literature: "文学", tv: "电视伴音", country: "乡村", electronic: "电子", dance: "舞曲",
  hits: "热歌", chillout: "放松", lounge: "休闲", ambient: "氛围", folk: "民谣", "hip hop": "嘻哈", kids: "儿童", religion: "宗教",
};
const tagLine = (s: Station) =>
  [...new Set(s.tags.split(",").map((t) => t.trim()).filter((t) => t && !/^\d/.test(t)).map((t) => (EN ? t : TAG_ZH[t.toLowerCase()] ?? t)))].slice(0, 3).join(" · ");

// ---------------------------------------------------------------- playback

const audio = new Audio();
audio.preload = "none";
audio.volume = load("radio.vol", 70) / 100;
vol.value = String(Math.round(audio.volume * 100));
// eslint-disable-next-line @typescript-eslint/no-explicit-any
let hls: any = null;
let hlsGen = 0; // incremented in stopStream; each hls instance's error handler captures its own gen
let state: "idle" | "loading" | "playing" | "paused" | "error" = "idle";
let roaming = 0;
let roamGen = 0;
let announced = "";

// ---------------------------------------------------------------- the pet

let rigPet: RigPet | null = null;
/** The pet it was opened from listens along; the window wears its skin. */
let listener: string | null = null;
async function seat(petId: string | null) {
  listener = petId;
  const sk = applySkin(conn ? (await skinFor(conn, petId)).skin.id : new URLSearchParams(location.search).get("skin"));
  const old = $<HTMLCanvasElement>("rpet");
  if (old.dataset.skin === sk.id) return;
  const cv = old.cloneNode(false) as HTMLCanvasElement;
  cv.dataset.skin = sk.id;
  old.replaceWith(cv);
  rigPet = await mountPortrait(cv, sk.rig ?? undefined).catch(() => null);
  if (rigPet) rigPet.expr = state === "playing" ? "calm" : "neutral";
}

/** What it has to say about what is on. Only what it knows. */
function petLine(): string {
  if (state === "error") return line("radio.error");
  if (state === "loading") return line(roaming > 0 ? "radio.roam" : "radio.tuning");
  if (!cur) return line("radio.pick");
  if (state === "paused") return line("radio.paused");
  if (state === "idle") return line("radio.last");
  const h = heard[cur.stationuuid];
  if (!h || h.n <= 1) return line("radio.first");
  const days = h.prev ? (h.last - h.prev) / 86_400_000 : 0;
  if (h.n >= 5) return line("radio.often");
  if (days >= 5 && days < 12) return line("radio.lastweek");
  if (days < 1.5) return line("radio.again");
  return line("radio.before");
}

function showLine() {
  const text = petLine();
  if (lineEl.textContent === text) return;
  lineEl.classList.add("swap");
  setTimeout(() => {
    lineEl.textContent = text;
    lineEl.classList.remove("swap");
  }, 180);
}

function setState(s: typeof state, note?: string) {
  state = s;
  if (rigPet) rigPet.expr = s === "playing" ? "calm" : s === "error" ? "worried" : "neutral";
  for (const k of ["playing", "loading", "error"] as const) player.classList.toggle(k, s === k);
  playBtn.innerHTML = s === "playing" || s === "loading" ? PAUSE : PLAY;
  playBtn.disabled = !cur;
  statusEl.textContent =
    note ??
    {
      idle: tr("电台", "Radio"),
      loading: tr("连接中…", "Connecting…"),
      playing: tr("正在播", "Now playing"),
      paused: tr("已暂停", "Paused"),
      error: tr("连不上这个台，换一个试试", "Can't reach this station, try another"),
    }[s];
  share();
  showLine();
  renderList();
  renderRecent();
}

/** "刚才" / "3 小时前" / "昨天" / "5 天前" */
function ago(ts: number): string {
  const m = (Date.now() - ts) / 60_000;
  if (m < 60) return tr("刚才", "just now");
  if (m < 24 * 60) return tr(`${Math.floor(m / 60)} 小时前`, `${Math.floor(m / 60)}h ago`);
  const d = Math.floor(m / (24 * 60));
  return d === 1 ? tr("昨天", "yesterday") : tr(`${d} 天前`, `${d} days ago`);
}

/** Under the player: the last few stations that were on, one click to go back to one. */
function renderRecent() {
  const box = $("recent");
  const past = Object.values(heard)
    .filter((h) => h.s.stationuuid !== cur?.stationuuid)
    .sort((a, b) => b.last - a.last)
    .slice(0, 5);
  box.replaceChildren(Object.assign(document.createElement("h3"), { textContent: tr("最近听过", "Recently on") }));
  if (!past.length) {
    box.append(Object.assign(document.createElement("p"), { className: "none", textContent: tr("换过的台会排在这里，点一下就回去。", "Stations you switch away from line up here. One click takes you back.") }));
    return;
  }
  for (const h of past) {
    const b = document.createElement("button");
    b.type = "button";
    const tx = document.createElement("span");
    tx.className = "tx";
    tx.append(
      Object.assign(document.createElement("b"), { textContent: h.s.name }),
      Object.assign(document.createElement("small"), { textContent: [countryName(h.s.countrycode, h.s.country), tagLine(h.s)].filter(Boolean).join(" · ") }),
    );
    b.append(Object.assign(document.createElement("span"), { className: "dot" }), tx, Object.assign(document.createElement("span"), { className: "when", textContent: ago(h.last) }));
    b.onclick = () => void play(h.s);
    box.append(b);
  }
}

/** For the tray panel (same origin, reads it on open). */
function share() {
  save("radio.now", { name: cur?.name ?? "", playing: state === "playing" || state === "loading", ts: Date.now() });
}
/** Listening time goes on the daily report: 10 s per heartbeat while playing. */
let conn: Conn | null = null;
setInterval(() => {
  if (state !== "playing") return;
  share();
  if (conn) void postJson(conn, "/v1/stat", { kind: "radio", what: "station", n: 10 }).catch(() => {});
}, 10_000);

function showCurrent() {
  if (!cur) return;
  nameEl.textContent = cur.name;
  const bits = [countryName(cur.countrycode, cur.country), tagLine(cur), cur.bitrate ? `${cur.codec || ""} ${cur.bitrate}k`.trim() : ""].filter(Boolean);
  metaEl.textContent = bits.join(" · ");
  favBtn.classList.toggle("on", favs.some((f) => f.stationuuid === cur!.stationuuid));
  if ("mediaSession" in navigator) {
    navigator.mediaSession.metadata = new MediaMetadata({
      title: cur.name,
      artist: countryName(cur.countrycode, cur.country),
      album: "Rill Radio",
      artwork: cur.favicon && /^https:/.test(cur.favicon) ? [{ src: cur.favicon }] : [],
    });
  }
}

function stopStream() {
  if (hls) {
    hlsGen++; // invalidate any pending error callbacks from this hls instance
    try { hls.destroy(); } catch { /* ignore */ }
    hls = null;
  }
  audio.pause();
  audio.removeAttribute("src");
  audio.load();
}

async function play(s: Station, fromRoam = false) {
  if (!fromRoam) roaming = 0;
  stopStream();
  cur = slim(s);
  save("radio.last", cur);
  showCurrent();
  setState("loading");
  const url = s.url_resolved || s.url;
  try {
    if (s.hls === 1 || /\.m3u8(\?|$)/i.test(url)) {
      const { default: Hls } = await import("hls.js");
      if (Hls.isSupported()) {
        hls = new Hls({ enableWorker: false });
        const myGen = hlsGen; // captured after stopStream() incremented — only valid for this instance
        hls.on(Hls.Events.ERROR, (_e: unknown, d: { fatal: boolean }) => { if (d.fatal && hlsGen === myGen) failed(); });
        hls.loadSource(url);
        hls.attachMedia(audio);
      } else audio.src = url;
    } else audio.src = url;
    await audio.play();
  } catch (e) {
    if ((e as Error).name !== "AbortError") failed();
  }
  // Radio Browser counts plays to rank stations.
  api(`/json/url/${s.stationuuid}`).catch(() => {});
}

function failed() {
  if (!cur) return;
  stopStream();
  setState("error");
  if (roaming > 0 && roaming < 5) void roam();
}

audio.addEventListener("playing", () => {
  if (cur && announced !== cur.stationuuid) {
    announced = cur.stationuuid;
    noteHeard(cur);
    setState("playing");
    rigPet?.react("perk");
    void tellPet(cur.name, petLine());
  } else setState("playing");
});
audio.addEventListener("waiting", () => state === "playing" && setState("loading", tr("缓冲中…", "Buffering…")));
audio.addEventListener("pause", () => state !== "error" && state !== "loading" && setState("paused"));
audio.addEventListener("error", () => audio.getAttribute("src") && failed());

function toggle() {
  if (!cur) return;
  if (state === "playing" || state === "loading") {
    stopStream(); // live streams: resume means reconnect, not catch up
    setState("paused");
  } else void play(cur);
}

async function roam() {
  roaming++;
  const gen = ++roamGen;
  setState("loading", tr("漫游中…", "Roaming…"));
  try {
    // _=timestamp busts browser/CDN caching so the "random" order truly varies each call.
    const list = await api<Station[]>(`/json/stations/search?order=random&hidebroken=true&limit=20&_=${Date.now()}`);
    const pool = list.filter(x => x.url_resolved && x.stationuuid !== cur?.stationuuid);
    const s = pool.length ? pool[Math.floor(Math.random() * pool.length)] : list[0];
    if (s && gen === roamGen) await play(s, true);
  } catch {
    if (gen === roamGen) setState("error", tr("连不上电台目录，检查一下网络", "Can't reach the station directory — check your connection"));
  }
}

/** The pet on the desktop says the same thing it says here. */
async function tellPet(name: string, text: string) {
  if (!inTauri) return;
  // Post to petd so the station registers as a bond moment for letters.
  if (conn) void postJson(conn, "/v1/stat", { kind: "radio", what: name, n: 0 }).catch(() => {});
  try {
    const { getAllWebviewWindows } = await import("@tauri-apps/api/webviewWindow");
    const { emitTo } = await import("@tauri-apps/api/event");
    const pets = (await getAllWebviewWindows()).map((w) => w.label).filter((l) => l.startsWith("pet-")).sort();
    const target = pets.find((l) => l === "pet-claude-code") ?? pets[0];
    if (!target) return;
    await emitTo(target, "say", { text, action: null });
  } catch {
    /* no pets */
  }
}

// ---------------------------------------------------------------- list

let tab: Tab = asTab(load<string>("radio.tab", "often")) ?? "often";
/** Shown under your regulars (or instead of them, while there are none). */
let popular: Station[] = [];
let country: Country | null = null;
let query = "";
let rows: Station[] = [];
let loading = false;
let error = "";

const HEART = '<svg class="heart" viewBox="0 0 24 24" aria-hidden="true"><path d="M12 20s-7-4.3-7-9.5A4 4 0 0 1 12 8a4 4 0 0 1 7 2.5C19 15.7 12 20 12 20z"/></svg>';
const isOn = (s: Station) => cur?.stationuuid === s.stationuuid && state === "playing";

function stationRow(s: Station): HTMLButtonElement {
  const b = document.createElement("button");
  b.type = "button";
  b.className = "st" + (cur?.stationuuid === s.stationuuid ? " cur" : "") + (isOn(s) ? " on" : "");
  const eq = document.createElement("span");
  eq.className = "eq";
  eq.innerHTML = "<i></i><i></i><i></i>";
  const tx = document.createElement("span");
  tx.className = "tx";
  const nm = document.createElement("b");
  nm.textContent = s.name.trim();
  const sm = document.createElement("small");
  sm.textContent = [countryName(s.countrycode, s.country), tagLine(s)].filter(Boolean).join(" · ");
  tx.append(nm, sm);
  b.append(eq, tx);
  if (favs.some((f) => f.stationuuid === s.stationuuid)) b.insertAdjacentHTML("beforeend", HEART);
  b.onclick = () => (isOn(s) ? undefined : void play(s));
  return b;
}

function sub(text: string, cls = "", back?: () => void): HTMLElement {
  const d = document.createElement("div");
  d.className = `sub ${cls}`;
  d.append(Object.assign(document.createElement("span"), { textContent: text }));
  if (back) {
    const b = document.createElement("button");
    b.textContent = tr("‹ 返回", "‹ Back");
    b.onclick = back;
    d.appendChild(b);
  }
  return d;
}

function empty(text: string): HTMLElement {
  const d = document.createElement("div");
  d.className = "empty";
  d.textContent = text;
  return d;
}

function renderList() {
  const keep = list.scrollTop;
  list.replaceChildren();
  document.querySelectorAll<HTMLButtonElement>("#tabs button").forEach((b) => b.classList.toggle("on", !query && b.dataset.tab === tab));
  if (loading) return void list.appendChild(empty(tr("加载中…", "Loading…")));
  if (error) return void list.appendChild(empty(error));
  if (query) list.appendChild(sub(tr(`搜索“${query}”`, `Search “${query}”`), "", () => { q.value = ""; query = ""; void refresh(); }));
  else if (tab === "world" && !country) {
    const cs = (countries ?? []).filter((c) => c.stationcount > 0);
    list.appendChild(sub(tr(`${cs.length} 个国家和地区`, `${cs.length} countries and regions`), "y"));
    const grid = document.createElement("div");
    grid.className = "countries";
    for (const c of cs) {
      const b = document.createElement("button");
      b.className = "country";
      const nm = document.createElement("span");
      nm.className = "nm";
      nm.textContent = countryName(c.iso_3166_1, c.name);
      const n = document.createElement("small");
      n.textContent = String(c.stationcount);
      b.type = "button";
      b.append(nm, n);
      b.onclick = () => { country = c; void refresh(); };
      grid.appendChild(b);
    }
    list.appendChild(grid);
    list.scrollTop = keep;
    return;
  } else if (tab === "world" && country) list.appendChild(sub(countryName(country.iso_3166_1, country.name), "y", () => { country = null; void refresh(); }));
  else if (tab === "xs") list.appendChild(sub(tr("相声 · 评书 · 曲艺", "Chinese crosstalk (相声) and comedy")));
  else if (tab === "up") list.appendChild(sub(tr("快一点的：舞曲、放克、热歌", "Quicker: dance, funk, hits")));
  else if (tab === "calm") list.appendChild(sub(tr("慢一点的：氛围、古典、钢琴", "Slower: ambient, classical, piano")));
  if (tab === "often" && !query) {
    // Your own first; what everyone has on fills the rest of the shelf.
    if (rows.length) {
      list.appendChild(sub(tr("收藏的和常开的", "Favourites and regulars")));
      rows.forEach((s) => list.appendChild(stationRow(s)));
    } else list.appendChild(empty(tr("还没有常听的。播放时点 ♡ 收藏，听过几次的也会留在这里。", "No regulars yet. Tap ♡ while a station plays; ones you return to stay here too.")));
    if (popular.length) {
      list.appendChild(sub(tr("大家在听的", "What others have on")));
      popular.filter((s) => !rows.some((r) => r.stationuuid === s.stationuuid)).forEach((s) => list.appendChild(stationRow(s)));
    }
    list.scrollTop = keep;
    return;
  }
  if (!rows.length) {
    list.appendChild(empty(tr("没找到电台", "No stations found")));
    return;
  }
  rows.forEach((s) => list.appendChild(stationRow(s)));
  list.scrollTop = keep;
}

let seq = 0;
async function refresh() {
  const my = ++seq;
  error = "";
  if (tab === "often" && !query) {
    rows = regulars();
    loading = false;
    renderList();
    // The popular shelf arrives when the directory answers; your own are already up.
    try {
      const r = await hot();
      if (my !== seq) return;
      popular = r;
    } catch {
      if (my !== seq) return;
      if (!rows.length) error = tr("连不上电台目录，检查一下网络再试。", "Can't reach the station directory. Check your connection and try again.");
    }
    return renderList();
  }
  loading = true;
  renderList();
  try {
    let r: Station[] = [];
    if (query) r = dedupe(await stations(`search?name=${encodeURIComponent(query)}`, 80));
    else if (tab === "up") r = await upbeat();
    else if (tab === "calm") r = await quiet();
    else if (tab === "xs") r = await crosstalk();
    else if (tab === "world") {
      countries ??= (await api<Country[]>("/json/countries")).sort((a, b) => b.stationcount - a.stationcount);
      if (country) r = await cached(`c:${country.iso_3166_1}`, async () => dedupe(await stations(`bycountrycodeexact/${country!.iso_3166_1}`, 120)));
    }
    if (my !== seq) return;
    rows = r;
  } catch {
    if (my !== seq) return;
    error = tr("连不上电台目录，检查一下网络再试。", "Can't reach the station directory. Check your connection and try again.");
  }
  loading = false;
  list.scrollTop = 0;
  renderList();
}

// ---------------------------------------------------------------- wiring

async function main() {
  try {
    conn = await connect();
    await initLang(conn, { reload: false });
  } catch {
    /* standalone preview */
  }
  const wanted = asTab(new URLSearchParams(location.search).get("tab"));
  if (wanted) {
    tab = wanted;
    save("radio.tab", tab);
  }
  await seat(new URLSearchParams(location.search).get("pet"));
  document.title = tr("电台 · 睿洱", "Radio · Rill");
  if (cur) showCurrent();
  setState("idle", cur ? tr("上次听的", "Last played") : undefined);

  playBtn.onclick = toggle;
  $("roam").onclick = () => { roaming = 0; void roam(); };
  favBtn.onclick = () => {
    if (!cur) return;
    const i = favs.findIndex((f) => f.stationuuid === cur!.stationuuid);
    if (i >= 0) favs.splice(i, 1);
    else favs.unshift(slim(cur));
    save("radio.favs", favs);
    showCurrent();
    if (tab === "often") void refresh();
    else renderList();
  };
  const paintVol = () => vol.style.setProperty("--v", `${vol.value}%`);
  paintVol();
  vol.oninput = () => {
    audio.volume = Number(vol.value) / 100;
    save("radio.vol", Number(vol.value));
    paintVol();
  };
  document.querySelectorAll<HTMLButtonElement>("#tabs button").forEach((b) => {
    b.onclick = () => {
      tab = asTab(b.dataset.tab) ?? "often";
      save("radio.tab", tab);
      q.value = "";
      query = "";
      if (tab !== "world") country = null;
      void refresh();
    };
  });
  let deb = 0;
  q.oninput = () => {
    clearTimeout(deb);
    deb = window.setTimeout(() => {
      query = q.value.trim();
      void refresh();
    }, 400);
  };
  window.addEventListener("keydown", (e) => {
    if (e.key === " " && document.activeElement !== q) {
      e.preventDefault();
      toggle();
    }
  });
  if ("mediaSession" in navigator) {
    navigator.mediaSession.setActionHandler("play", () => state !== "playing" && toggle());
    navigator.mediaSession.setActionHandler("pause", () => state === "playing" && toggle());
    navigator.mediaSession.setActionHandler("stop", () => state === "playing" && toggle());
  }
  // The agent it listens with got another pet: that one takes the seat. The station keeps playing.
  if (conn) followSkin(conn, () => listener, () => void seat(listener));
  // The station plays for as long as its tab is there: the window around it only hides when closed.
  await onHost("radio-toggle", () => toggle());
  // Opened again from another pet: that one comes to listen. The station keeps playing.
  await onHost<string | null>("for-pet", (pet) => void seat(pet));
  await onHost<string>("radio-select", (shelf) => {
    tab = asTab(shelf) ?? tab;
    save("radio.tab", tab);
    q.value = "";
    query = "";
    country = null;
    void refresh();
  });
  // A pet asked "put something on?" and you said yes: that shelf, and something on it, now.
  await onHost<string>("radio-play", (shelf) => void playFrom(shelf));
  const asked = new URLSearchParams(location.search).get("play");
  if (asked) return void playFrom(asked);
  await refresh();
}

/** Open `shelf` and play from it: a station you have played before if one is on it, else the first. */
async function playFrom(shelf: string) {
  tab = asTab(shelf) ?? "often";
  save("radio.tab", tab);
  q.value = "";
  query = "";
  country = null;
  await refresh();
  const known = rows.find((s) => heard[s.stationuuid]) ?? (tab === "often" ? (rows[0] ?? popular[0]) : rows[0]);
  if (known && !isOn(known)) await play(known);
}

void main();
