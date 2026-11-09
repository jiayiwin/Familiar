// The radio: stations from all over the world via the Radio Browser
// directory (radio-browser.info), played straight from each station's
// stream. Nothing goes online until this window is opened. Closing the window
// only hides it, so the station keeps playing.

import { connect, inTauri, postJson, type Conn } from "./api";
import { EN, initLang, t as tr } from "./i18n";

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
type Tab = "hot" | "xs" | "world" | "fav";

const SERVERS = ["de1", "fi1", "de2", "nl1", "at1"].map((s) => `https://${s}.api.radio-browser.info`);
const $ = <T extends HTMLElement>(id: string) => document.getElementById(id) as T;
const list = $("list"), player = $("player"), playBtn = $<HTMLButtonElement>("play"), favBtn = $<HTMLButtonElement>("fav");
const statusEl = $("status"), nameEl = $("name"), metaEl = $("meta"), logoEl = $("logo");
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
const hash = (s: string) => {
  let h = 0;
  for (const c of s) h = (h * 31 + c.charCodeAt(0)) >>> 0;
  return h;
};
const tint = (s: string) => `tn-${hash(s) % 5}`;
/** A hue for a station's art, from its name: the same station always gets the same colours. */
const hue = (s: string) => hash(s) % 360;
const initial = (s: string) => [...s.replace(/^[\s\p{P}\p{S}\d]+/u, "")][0]?.toUpperCase() ?? "♪";
const TAG_ZH: Record<string, string> = {
  crosstalk: "相声", storytelling: "评书", comedy: "喜剧", news: "新闻", music: "音乐", entertainment: "娱乐", culture: "文化", opera: "戏曲",
  pop: "流行", talk: "谈话", sports: "体育", classical: "古典", jazz: "爵士", rock: "摇滚", traffic: "交通", information: "资讯",
  "easy listening": "轻音乐", oldies: "老歌", literature: "文学", tv: "电视伴音", country: "乡村", electronic: "电子", dance: "舞曲",
  hits: "热歌", chillout: "放松", lounge: "休闲", ambient: "氛围", folk: "民谣", "hip hop": "嘻哈", kids: "儿童", religion: "宗教",
};
const tagLine = (s: Station) =>
  [...new Set(s.tags.split(",").map((t) => t.trim()).filter((t) => t && !/^\d/.test(t)).map((t) => (EN ? t : TAG_ZH[t.toLowerCase()] ?? t)))].slice(0, 3).join(" · ");

function icon(s: Station, el: HTMLElement) {
  el.textContent = initial(s.name);
  el.classList.add(tint(s.name));
  if (s.favicon && /^https?:/.test(s.favicon)) {
    const img = new Image();
    img.referrerPolicy = "no-referrer";
    img.onload = () => {
      if (img.naturalWidth < 8) return;
      el.textContent = "";
      el.classList.remove(...["tn-0", "tn-1", "tn-2", "tn-3", "tn-4"]);
      el.appendChild(img);
    };
    img.src = s.favicon;
  }
}

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

function setState(s: typeof state, note?: string) {
  state = s;
  for (const k of ["playing", "loading", "error"] as const) player.classList.toggle(k, s === k);
  playBtn.innerHTML = s === "playing" || s === "loading" ? PAUSE : PLAY;
  playBtn.disabled = !cur;
  statusEl.textContent =
    note ??
    {
      idle: tr("Familiar 电台", "Familiar Radio"),
      loading: tr("连接中…", "Connecting…"),
      playing: tr("正在播放", "Now playing"),
      paused: tr("已暂停", "Paused"),
      error: tr("连不上这个台，换一个试试", "Can't reach this station, try another"),
    }[s];
  share();
  renderList();
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
  player.style.setProperty("--h", String(hue(cur.name)));
  nameEl.textContent = cur.name;
  const bits = [countryName(cur.countrycode, cur.country), tagLine(cur), cur.bitrate ? `${cur.codec || ""} ${cur.bitrate}k`.trim() : ""].filter(Boolean);
  metaEl.textContent = bits.join(" · ");
  logoEl.className = "logo";
  logoEl.innerHTML = "";
  icon(cur, logoEl);
  logoEl.classList.remove(...["tn-0", "tn-1", "tn-2", "tn-3", "tn-4"]);
  favBtn.classList.toggle("on", favs.some((f) => f.stationuuid === cur!.stationuuid));
  favBtn.textContent = favBtn.classList.contains("on") ? "♥" : "♡";
  if ("mediaSession" in navigator) {
    navigator.mediaSession.metadata = new MediaMetadata({
      title: cur.name,
      artist: countryName(cur.countrycode, cur.country),
      album: "Familiar Radio",
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
  setState("playing");
  if (cur && announced !== cur.stationuuid) {
    announced = cur.stationuuid;
    void tellPet(cur.name);
  }
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

/** One pet mentions the new station in a bubble. */
async function tellPet(name: string) {
  if (!inTauri) return;
  try {
    const { getAllWebviewWindows } = await import("@tauri-apps/api/webviewWindow");
    const { emitTo } = await import("@tauri-apps/api/event");
    const pets = (await getAllWebviewWindows()).map((w) => w.label).filter((l) => l.startsWith("pet-")).sort();
    const target = pets.find((l) => l === "pet-claude-code") ?? pets[0];
    if (target) await emitTo(target, "say", { text: tr(`📻 正在听：${name}`, `📻 Listening to ${name}`), action: null });
  } catch {
    /* no pets */
  }
}

// ---------------------------------------------------------------- list

let tab: Tab = load<Tab>("radio.tab", "hot");
let country: Country | null = null;
let query = "";
let rows: Station[] = [];
let loading = false;
let error = "";

const EQ = "<i></i><i></i><i></i><i></i>";
const isOn = (s: Station) => cur?.stationuuid === s.stationuuid && state === "playing";

function stationRow(s: Station, i = 0): HTMLButtonElement {
  const b = document.createElement("button");
  b.className = "st" + (cur?.stationuuid === s.stationuuid ? " cur" : "");
  b.style.animationDelay = `${Math.min(i, 12) * 28}ms`;
  const ic = document.createElement("span");
  ic.className = "ic";
  icon(s, ic);
  const tx = document.createElement("span");
  tx.className = "tx";
  const nm = document.createElement("b");
  if (isOn(s)) {
    const eq = document.createElement("span");
    eq.className = "eq";
    eq.innerHTML = EQ;
    nm.appendChild(eq);
  }
  const name = document.createElement("span");
  name.className = "nm";
  name.textContent = s.name.trim();
  nm.appendChild(name);
  const sm = document.createElement("small");
  const cc = document.createElement("span");
  cc.className = "cc";
  cc.textContent = s.countrycode || "··";
  sm.append(cc, [countryName(s.countrycode, s.country), tagLine(s)].filter(Boolean).join(" · "));
  tx.append(nm, sm);
  b.append(ic, tx);
  if (favs.some((f) => f.stationuuid === s.stationuuid)) {
    const h = document.createElement("span");
    h.className = "heart";
    h.textContent = "♥";
    b.appendChild(h);
  }
  const go = document.createElement("span");
  go.className = "go";
  go.innerHTML = '<svg viewBox="0 0 24 24"><path d="M8 5.5v13a1 1 0 0 0 1.5.86l10.5-6.5a1 1 0 0 0 0-1.72L9.5 4.64A1 1 0 0 0 8 5.5z"/></svg>';
  b.appendChild(go);
  b.onclick = () => (isOn(s) ? undefined : void play(s));
  return b;
}

/** A big tile for the top of the popular rack. */
function featured(s: Station, rank: number): HTMLButtonElement {
  const b = document.createElement("button");
  b.className = "ft" + (cur?.stationuuid === s.stationuuid ? " cur" : "");
  b.style.setProperty("--h", String(hue(s.name)));
  b.style.animationDelay = `${rank * 60}ms`;
  const ic = document.createElement("span");
  ic.className = "ic";
  icon(s, ic);
  const rk = document.createElement("span");
  rk.className = "rank";
  rk.textContent = String(rank + 1).padStart(2, "0");
  const nm = document.createElement("b");
  nm.textContent = s.name.trim();
  const sm = document.createElement("small");
  sm.textContent = [countryName(s.countrycode, s.country), tagLine(s)].filter(Boolean).join(" · ");
  b.append(ic, rk, nm, sm);
  b.onclick = () => (isOn(s) ? undefined : void play(s));
  return b;
}

/** Slide the pill under the active tab. */
function moveInd() {
  const on = document.querySelector<HTMLButtonElement>("#tabs button.on");
  const ind = document.querySelector<HTMLElement>("#tabs .ind");
  if (!ind) return;
  if (!on) {
    ind.style.width = "0";
    return;
  }
  ind.style.left = `${on.offsetLeft}px`;
  ind.style.width = `${on.offsetWidth}px`;
}

function sub(text: string, cls = "", back?: () => void): HTMLElement {
  const d = document.createElement("div");
  d.className = `sub ${cls}`;
  d.textContent = text;
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
  moveInd();
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
      const cc = document.createElement("span");
      cc.className = "cc";
      cc.textContent = c.iso_3166_1;
      const nm = document.createElement("span");
      nm.className = "nm";
      nm.textContent = countryName(c.iso_3166_1, c.name);
      const n = document.createElement("small");
      n.textContent = String(c.stationcount);
      b.append(cc, nm, n);
      b.onclick = () => { country = c; void refresh(); };
      grid.appendChild(b);
    }
    list.appendChild(grid);
    list.scrollTop = keep;
    return;
  } else if (tab === "world" && country) list.appendChild(sub(countryName(country.iso_3166_1, country.name), "y", () => { country = null; void refresh(); }));
  else if (tab === "xs") list.appendChild(sub(tr("相声 · 评书 · 曲艺", "Chinese crosstalk (相声) and comedy")));
  else if (tab === "hot") list.appendChild(sub(tr("今日推荐", "Featured today")));
  else if (tab === "fav") list.appendChild(sub(tr("我的收藏", "My favorites")));
  if (!rows.length) {
    list.appendChild(empty(tab === "fav" && !query ? tr("还没有收藏。播放时点 ♡ 收藏电台。", "No favorites yet. Tap ♡ while a station plays.") : tr("没找到电台", "No stations found")));
    return;
  }
  let rest = rows;
  if (tab === "hot" && !query && rows.length >= 6) {
    // The top few as big tiles, the rest as rows.
    const feat = document.createElement("div");
    feat.className = "feat";
    rows.slice(0, 5).forEach((s, i) => feat.appendChild(featured(s, i)));
    list.appendChild(feat);
    list.appendChild(sub(tr("更多热门", "More popular")));
    rest = rows.slice(5);
  }
  rest.forEach((s, i) => list.appendChild(stationRow(s, i)));
  list.scrollTop = keep;
}

// ---------------------------------------------------------------- spectrum

/** Bars behind the station name that move to the music (shaped, not measured:
 *  most streams forbid reading their audio, so this is a lively impression). */
function vizLoop() {
  const cv = document.getElementById("viz") as HTMLCanvasElement | null;
  if (!cv) return;
  const dpr = devicePixelRatio || 1;
  const W = cv.clientWidth, H = cv.clientHeight;
  if (W && cv.width !== Math.round(W * dpr)) {
    cv.width = Math.round(W * dpr);
    cv.height = Math.round(H * dpr);
  }
  const g = cv.getContext("2d")!;
  g.setTransform(dpr, 0, 0, dpr, 0, 0);
  g.clearRect(0, 0, W, H);
  const t = performance.now() / 1000;
  const n = 36, gap = 3, bw = (W - gap * (n + 1)) / n;
  const live = state === "playing" ? 1 : state === "loading" ? 0.35 : 0.12;
  for (let i = 0; i < n; i++) {
    const k = i / (n - 1);
    const env = 0.35 + 0.65 * Math.exp(-((k - 0.42) ** 2) / 0.09); // louder in the middle
    const wave = 0.5 + 0.5 * Math.sin(t * 2.1 + i * 0.55) * Math.sin(t * 0.7 + i * 0.21) + 0.25 * Math.sin(t * 5.3 + i * 1.7);
    const h = Math.max(3, (6 + wave * env * (H - 16) * live));
    const x = gap + i * (bw + gap);
    const grad = g.createLinearGradient(0, H - h, 0, H);
    grad.addColorStop(0, "rgba(255,255,255,0.95)");
    grad.addColorStop(1, "rgba(255,255,255,0.05)");
    g.fillStyle = grad;
    g.beginPath();
    g.roundRect(x, H - h, bw, h, bw / 2);
    g.fill();
  }
  requestAnimationFrame(vizLoop);
}

let seq = 0;
async function refresh() {
  const my = ++seq;
  error = "";
  if (tab === "fav" && !query) {
    rows = favs;
    loading = false;
    return renderList();
  }
  loading = true;
  renderList();
  try {
    let r: Station[] = [];
    if (query) r = dedupe(await stations(`search?name=${encodeURIComponent(query)}`, 80));
    else if (tab === "hot") r = await hot();
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
  const wanted = new URLSearchParams(location.search).get("tab");
  if (wanted === "hot" || wanted === "xs" || wanted === "world" || wanted === "fav") {
    tab = wanted;
    save("radio.tab", tab);
  }
  document.title = tr("电台 · Familiar", "Radio · Familiar");
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
    if (tab === "fav") void refresh();
    else renderList();
  };
  const paintVol = () => vol.style.setProperty("--v", `${vol.value}%`);
  paintVol();
  vol.oninput = () => {
    audio.volume = Number(vol.value) / 100;
    save("radio.vol", Number(vol.value));
    paintVol();
  };
  requestAnimationFrame(vizLoop);
  window.addEventListener("resize", moveInd);
  document.querySelectorAll<HTMLButtonElement>("#tabs button").forEach((b) => {
    b.onclick = () => {
      tab = b.dataset.tab as Tab;
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
  if (inTauri) {
    const { getCurrentWebviewWindow } = await import("@tauri-apps/api/webviewWindow");
    const w = getCurrentWebviewWindow();
    // Closing hides the window; the station keeps playing.
    await w.onCloseRequested(async (e) => {
      e.preventDefault();
      await w.hide();
    });
    await w.listen("radio-toggle", () => toggle());
    await w.listen<Tab>("radio-select", (e) => {
      tab = e.payload;
      save("radio.tab", tab);
      q.value = "";
      query = "";
      country = null;
      void refresh();
    });
  }
  await refresh();
}

void main();
