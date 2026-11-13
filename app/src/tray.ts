// The tray panel: opens next to the tray icon. Who is here, the three switches
// people reach for, and the room; the other windows and the plumbing are
// folded under 更多.
// Actions close it and go through menu_pick; toggles go through tray_pick and
// keep it open.

import { mountPortrait } from "./rig/portrait";
import { applySkin, skin } from "./skin";

interface Opt { id: string; label: string }
interface Pet { id: string; camp?: string | null; agent: string; state: string; waiting: boolean; skin?: string; name?: string }
interface State {
  version: string; en: boolean; lang: string; skin?: string;
  focus: boolean; autostart: boolean; boss: string; focusKey: string; langs: Opt[]; anchor?: string;
  size?: string; sounds?: boolean; initiative?: boolean; hidden?: boolean; pets?: Pet[];
  modelReady?: boolean; modelDlDone?: number; modelDlTotal?: number;
}

const inTauri = "__TAURI_INTERNALS__" in window;
const wrap = document.getElementById("wrap")!;
const card = document.getElementById("card")!;
const petsEl = document.getElementById("pets")!;
const rest = document.getElementById("rest")!;
const acts = document.getElementById("acts")!;
const quit = document.getElementById("quit")!;
let st: State | null = null;
let closing = false;

async function invoke<T = void>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  if (!inTauri) {
    console.log(cmd, args ?? "");
    return undefined as T;
  }
  const core = await import("@tauri-apps/api/core");
  return core.invoke<T>(cmd, args);
}

const t = (zh: string, en: string) => (st?.en ? en : zh);
const esc = (s: string) => s.replace(/[&<>"]/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;" })[c]!);

function close() {
  if (closing) return;
  closing = true;
  wrap.classList.remove("open");
  void invoke("menu_close");
}
const pick = (id: string) => { closing = true; void invoke("menu_pick", { id }); };
const toggle = (id: string) => void invoke("tray_pick", { id });

function row(id: string, text: string, hint = "") {
  return `<button class="row" data-go="${id}"><span class="tx">${text}</span>${hint ? `<span class="hint">${esc(hint)}</span>` : ""}</button>`;
}

function sw(id: string, on: boolean, title: string, desc: string) {
  return `<button class="sw-row" role="switch" aria-checked="${on}" data-tg="${id}"><span class="tx"><b>${title}</b><small>${desc}</small></span><span class="sw${on ? " on" : ""}"></span></button>`;
}

// ---- who is here: the tiles stay, so each pet keeps idling between renders
const tiles = new Map<string, HTMLElement>();
function renderPets(pets: Pet[]) {
  petsEl.querySelector(".nobody")?.remove();
  const shown = pets.slice(0, 2);
  for (const [id, el] of tiles) {
    if (!shown.some((p) => p.id === id)) {
      el.remove();
      tiles.delete(id);
    }
  }
  for (const p of shown) {
    // Each tile is that pet's own corner: its body, its room's colour, its name.
    const sk = skin(p.skin);
    let el = tiles.get(p.id);
    if (el && el.dataset.skin !== sk.id) {
      el.remove();
      el = undefined;
    }
    if (!el) {
      el = document.createElement("div");
      el.dataset.skin = sk.id;
      el.style.background = sk.colors.room;
      el.innerHTML = `<canvas></canvas><b></b><span class="st"><i></i><span></span></span>`;
      tiles.set(p.id, el);
      petsEl.append(el);
      void mountPortrait(el.querySelector("canvas")!, sk.rig ?? undefined).catch(() => {});
    }
    el.className = `pet ${p.camp ?? ""}${p.waiting ? " waiting" : ""}`;
    el.querySelector("b")!.textContent = p.name || t(sk.zh, sk.en);
    el.querySelector(".st span")!.textContent = `${p.agent} · ${p.state}`;
  }
  if (!shown.length) {
    const none = document.createElement("div");
    none.className = "nobody";
    none.textContent = st?.hidden ? t("都藏起来了。", "All hidden.") : t("现在没有宠物在场。", "No pets here right now.");
    petsEl.append(none);
  }
}

function render() {
  if (!st) return;
  const s = st;
  // The panel belongs to no one pet: it wears the main pet's skin.
  applySkin(s.skin);
  document.documentElement.lang = s.en ? "en" : "zh-CN";
  renderPets(s.pets ?? []);
  const size = (s.size ?? "m").toLowerCase();
  const keep = rest.scrollTop;
  rest.innerHTML = `
    <div class="box">
      ${sw("focus", s.focus, t("安静一会儿", "Quiet for a while"), `${t("只提醒“等你”，别的不动", "Only “waiting for you”; nothing else")} · ${esc(s.focusKey)}`)}
      ${sw(s.initiative === false ? "chatter|normal" : "chatter|quiet", s.initiative !== false, t("主动陪伴", "Company on its own"), t("久坐、深夜、等得久时，它跳一段、奏一段，或问你一句", "Long sits, late nights, long waits: a dance, a piece, or a question"))}
      ${sw("sounds", s.sounds !== false, t("声音", "Sound"), t("琴声和提示音", "Their instruments and the alert sounds"))}
    </div>
    <div class="line"><span>${t("大小", "Size")}</span>
      <div class="seg">${[["s", t("小", "S")], ["m", t("中", "M")], ["l", t("大", "L")]].map(([id, n]) => `<button class="${size === id ? "on" : ""}" data-tg="size|${id}">${n}</button>`).join("")}</div>
    </div>
    <div class="box">${row("settings", t("设置…", "Settings…"), t("话量、提肛、接入、储物间", "Chattiness, kegels, agents, storeroom"))}</div>
    <div class="ver">RillBuddy v${esc(s.version)}</div>`;
  rest.scrollTop = keep;
  acts.innerHTML = `<button class="go" data-go="room">${t("打开小屋", "Open the room")}</button>${
    s.hidden
      ? `<button class="alt" data-go="show">${t("都叫出来", "Bring them back")}</button>`
      : `<button class="alt" data-go="hideall" title="${esc(s.boss)}">${t("都藏起来", "Hide them all")}</button>`
  }`;
  quit.textContent = t("退出睿洱", "Quit Rill");
}

card.addEventListener("click", (e) => {
  e.stopPropagation();
  const b = (e.target as HTMLElement).closest<HTMLElement>("[data-go],[data-tg]");
  if (!b) return;
  if (b.dataset.tg) {
    const id = b.dataset.tg;
    if (st) {
      if (id === "focus") st.focus = !st.focus;
      if (id === "sounds") st.sounds = st.sounds === false;
      if (id.startsWith("chatter|")) st.initiative = id !== "chatter|quiet";
      if (id.startsWith("size|")) st.size = id.slice(5);
      if (id === "autostart") st.autostart = !st.autostart;
      if (id.startsWith("lang|")) st.lang = id.slice(5);
    }
    render();
    toggle(id);
    return;
  }
  if (b.dataset.go) pick(b.dataset.go);
});
wrap.addEventListener("click", close);
window.addEventListener("keydown", (e) => { if (e.key === "Escape") close(); });

function open(payload: State) {
  closing = false;
  st = payload;
  wrap.classList.toggle("top", payload.anchor === "top");
  wrap.classList.remove("open");
  render();
  rest.scrollTop = 0;
  requestAnimationFrame(() => requestAnimationFrame(() => wrap.classList.add("open")));
}

if (inTauri) {
  void import("@tauri-apps/api/webviewWindow").then(({ getCurrentWebviewWindow }) => {
    const win = getCurrentWebviewWindow();
    void win.onFocusChanged(({ payload }) => { if (!payload) close(); });
    void win.listen<State>("tray-open", (e) => {
      open(e.payload);
    });
    void win.listen<State>("tray-state", (e) => {
      const anchor = st?.anchor;
      st = { ...e.payload, anchor };
      render();
    });
  });
} else {
  // Outside the app (a browser on the dev server): a sample to look at.
  const q = new URLSearchParams(location.search);
  wrap.style.width = "380px"; // the panel window's width
  open({
    version: "0.1.26", en: q.has("en"), lang: "auto", focus: false, autostart: true,
    skin: q.get("skin") ?? "zimeng", boss: "Ctrl+Alt+H", focusKey: "Ctrl+Alt+F", size: "m", sounds: true, initiative: true, hidden: false,
    langs: [{ id: "auto", label: "跟随系统" }, { id: "zh", label: "中文" }, { id: "en", label: "English" }],
    pets: [
      { id: "a", camp: "claude-code", agent: "Claude", state: "干活中", waiting: false, skin: q.get("skin") ?? "zimeng" },
      { id: "b", camp: "codex", agent: "Codex", state: "等你 6 分钟", waiting: true, skin: q.get("skin") === "mashu" ? "zimeng" : "mashu" },
    ],
    modelReady: true,
  });
}
