// Hover card: lists the sessions behind a pet, and under them 手上的事 (the
// things it holds for you: tick one to cross it off, the last row writes one).
// Pet windows emit `card:show` / `card:hide` to this window.

import { connect, duration, getJson, postJson, SOURCE_NAMES, STATE_NAMES, type Conn, type PetView, type SessionCard, type Todo } from "./api";
import { initLang, t } from "./i18n";
import { applySkin } from "./skin";
import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";
import { availableMonitors, LogicalPosition, LogicalSize } from "@tauri-apps/api/window";

const W = 288;
const el = document.getElementById("card")!;
const win = getCurrentWebviewWindow();

/** The box the whole pet takes up, in logical screen px: the card never covers it. */
interface ShowReq {
  pet: string;
  ax: number;
  ay: number;
  aw: number;
  ah?: number;
  /** Opened by the pet itself (yesterday's notes): it stays a while without the pointer on it. */
  pin?: boolean;
}

const AGENTS: Record<string, string> = { "claude-code": "Claude", codex: "Codex", gemini: "Gemini", cursor: "Cursor", opencode: "OpenCode" };

function hm(ms: number): string {
  const d = new Date(ms);
  return `${String(d.getHours()).padStart(2, "0")}:${String(d.getMinutes()).padStart(2, "0")}`;
}

/** Where a thing came from, or what it waits on. */
function whence(x: Todo, now: number): string {
  if (x.wait && !x.done_at) {
    const a = AGENTS[x.wait] ?? x.wait;
    return x.ready
      ? `<span class="wait"><i class="ag"></i>${esc(t("${a} 停下来了", "${a} has stopped").replace("${a}", a))}</span>`
      : `<span class="wait"><i class="ag"></i>${esc(t("等 ${a} 这轮做完", "When ${a} is done").replace("${a}", a))}</span>`;
  }
  if (x.from !== "you") {
    const a = AGENTS[x.from] ?? x.from;
    const p = x.project ? ` · ${esc(x.project)}` : "";
    return `<i class="ag"></i>${esc(t("${a} 收工时留的", "Left by ${a} at the end").replace("${a}", a))}${p}`;
  }
  const day = new Date(now).toDateString() === new Date(x.at).toDateString();
  const yday = new Date(now - 86_400_000).toDateString() === new Date(x.at).toDateString();
  const when = day ? t(`今天 ${hm(x.at)}`, `Today ${hm(x.at)}`) : yday ? t("昨天", "Yesterday") : t(`${new Date(x.at).getMonth() + 1} 月 ${new Date(x.at).getDate()} 日`, new Date(x.at).toLocaleDateString("en-US", { month: "short", day: "numeric" }));
  return esc(t(`${when} 记的`, `Written ${when.toLowerCase()}`));
}

/** 手上的事: what is open (up to six), then what was crossed off today; the last row writes one. */
function todoBlock(list: Todo[], now: number): string {
  const open = list.filter((x) => !x.done_at);
  const gone = list.filter((x) => x.done_at && new Date(x.done_at).toDateString() === new Date(now).toDateString()).slice(0, 2);
  const shown = open.slice(0, 6);
  const rows = [...shown, ...gone]
    .map(
      (x) => `<div class="todo${x.done_at ? " gone" : ""}" data-id="${x.id}" title="${esc(x.done_at ? t("点圆圈放回手上", "Tick again to put it back") : t("点圆圈勾掉", "Tick to cross it off"))}">
      <span class="tick"></span><span class="what">${esc(x.text)}</span><span class="from">${whence(x, now)}</span></div>`,
    )
    .join("");
  const more = open.length > shown.length ? `<div class="more">${esc(t(`还有 ${open.length - shown.length} 件`, `${open.length - shown.length} more`))}</div>` : "";
  return `<h2><span>${t("手上的事", "In hand")}</span>${open.length ? `<small>${t("点圆圈勾掉", "Tick to cross off")}</small>` : ""}</h2>${rows}${more}
    <div class="add" data-add="1">${t("记一件事…", "Write something down…")}</div>`;
}

function esc(s: string): string {
  return s.replace(/[&<>"]/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;" })[c]!);
}

function row(s: SessionCard, now: number): string {
  const what = [s.tool, s.file].filter(Boolean).join(" · ");
  return `<div class="row${s.pid ? " go" : ""}" data-pid="${s.pid || 0}" data-name="${esc(s.project || "")}">
    <span class="dot ${s.state}"></span>
    <span class="name">${esc(s.project || t("(未知项目)", "(unknown project)"))}</span>
    <span class="state ${s.state}">${STATE_NAMES[s.state]}</span>
    <span class="meta">${SOURCE_NAMES[s.source] ?? esc(s.source)} · ${t("已运行", "up")} ${duration(now - s.started_at)}${
      what ? ` · ${t("最近", "last")} ${esc(what)}` : ""
    }</span>
    ${s.pid ? `<span class="go-to">${t("过去 ›", "Go ›")}</span>` : ""}
  </div>`;
}

/** A line under the rows for a moment (a window that could not be found). */
let note: { text: string; until: number } | null = null;

let todos: Todo[] = [];

function render(v: PetView | undefined): void {
  const now = Date.now();
  applySkin(v?.skin);
  let html: string;
  if (!v) {
    html = `<div class="empty">${t("这只宠物已经不在了。", "This pet is gone.")}</div>`;
  } else {
    const list = v.sessions;
    const title = v.marker ?? SOURCE_NAMES[v.camp ?? ""] ?? v.id;
    const sub = t(`${v.online} 个在线`, `${v.online} online`) + (v.waiting ? t(` · ${v.waiting} 个等你`, ` · ${v.waiting} waiting`) : "");
    html = `<h1><span>${esc(title)}</span><small>${esc(sub)}</small></h1>
    ${list.length ? list.map((s) => row(s, now)).join("") : `<div class="empty">${t("现在没有在跑的。", "Nothing is running right now.")}</div>`}
    ${note && now < note.until ? `<div class="note">${esc(note.text)}</div>` : ""}
    ${list.length ? `<div class="hint">${t("点一行，到它的窗口去", "Click a row to go to its window")}</div>` : ""}
    ${todoBlock(todos, now)}`;
  }
  // Rebuilt only when something changed, so a click never lands on a row that is being replaced.
  if (html !== shown) {
    shown = html;
    el.innerHTML = html;
  }
}
let shown = "";

async function place(req: ShowReq): Promise<void> {
  const h = Math.ceil(el.getBoundingClientRect().height) + 8;
  const ph = req.ah ?? 0;
  // Above the pet, centred on it.
  let x = req.ax + req.aw / 2 - W / 2;
  let y = req.ay - h - 6;
  // Inside the monitor that holds the pet. Where there is no room above it, the card goes beside it
  // (on the side with more room), never over it.
  for (const m of await availableMonitors()) {
    const s = m.scaleFactor;
    const left = m.workArea.position.x / s;
    const top = m.workArea.position.y / s;
    const right = left + m.workArea.size.width / s;
    const bottom = top + m.workArea.size.height / s;
    if (req.ax + req.aw / 2 >= left && req.ax + req.aw / 2 < right && req.ay + ph / 2 >= top && req.ay + ph / 2 < bottom + 40) {
      if (y < top + 4) {
        const roomRight = right - (req.ax + req.aw);
        x = roomRight >= W + 10 || roomRight > req.ax - left ? req.ax + req.aw + 6 : req.ax - W - 6;
        y = Math.min(Math.max(req.ay, top + 4), bottom - h - 4);
      }
      x = Math.min(Math.max(x, left + 4), right - W - 4);
      y = Math.max(y, top + 4);
      break;
    }
  }
  await win.setSize(new LogicalSize(W, h));
  await win.setPosition(new LogicalPosition(x, y));
}

async function main() {
  const conn = await connect();
  await initLang(conn);
  let current: ShowReq | null = null;
  let timer = 0;

  const refresh = async () => {
    if (!current) return;
    const req = current;
    const [{ pets }, held] = await Promise.all([
      getJson<{ pets: PetView[] }>(conn, "/v1/pets"),
      getJson<{ todos: Todo[] }>(conn, "/v1/todos").catch(() => ({ todos })),
    ]);
    todos = held.todos;
    if (current !== req) return;
    render(pets.find((p) => p.id === req.pet));
    await place(req);
  };

  // The pointer may cross from the pet onto the card (to click a row): while it is over the card, the card stays.
  let over = false;
  let leaving = 0;
  const hide = async () => {
    clearTimeout(leaving);
    current = null;
    over = false;
    clearInterval(timer);
    await win.hide();
  };
  /** `soft`: the pointer only left the pet; give it a moment to reach the card. */
  const hideSoon = (ms: number) => {
    clearTimeout(leaving);
    leaving = window.setTimeout(() => {
      if (!over) void hide();
    }, ms);
  };
  document.documentElement.addEventListener("mouseenter", () => {
    over = true;
    clearTimeout(leaving);
  });
  document.documentElement.addEventListener("mouseleave", () => {
    over = false;
    if (current) hideSoon(260);
  });

  // 手上的事: a tick crosses one off (this pet eats the slip) or puts it back; the last row writes one.
  el.addEventListener("mousedown", async (e) => {
    if (e.button !== 0 || !current) return;
    const target = e.target as HTMLElement;
    if (target.closest("[data-add]")) {
      const { invoke } = await import("@tauri-apps/api/core");
      const pet = current.pet;
      await hide();
      await invoke("note_open", { pet, text: null }).catch(() => {});
      return;
    }
    const row = target.closest<HTMLElement>(".todo");
    if (!row) return;
    const id = Number(row.dataset.id);
    const back = row.classList.contains("gone");
    row.classList.toggle("gone", !back);
    await done(conn, id, current.pet, back);
    void refresh();
  });

  // A row: that session's terminal or IDE comes to the front.
  el.addEventListener("mousedown", async (e) => {
    const r = (e.target as HTMLElement).closest<HTMLElement>(".row.go");
    if (!r || e.button !== 0) return;
    const pid = Number(r.dataset.pid);
    const { invoke } = await import("@tauri-apps/api/core");
    if (await invoke<boolean>("focus_session", { pid, project: r.dataset.name || null })) return void hide();
    const name = r.dataset.name || t("它", "it");
    note = { text: t(`找不到 ${name} 的窗口。`, `Can't find the window for ${name}.`), until: Date.now() + 3000 };
    void refresh();
  });

  await win.listen<ShowReq>("card:show", async (e) => {
    clearTimeout(leaving);
    current = e.payload;
    // Opened by the pet: up for a while, unless the pointer comes and goes.
    if (e.payload.pin) hideSoon(12_000);
    await refresh();
    if (current === e.payload) await win.show();
    clearInterval(timer);
    timer = window.setInterval(() => void refresh(), 1000);
  });
  await win.listen<{ soft?: boolean }>("card:hide", async (e) => {
    if (e.payload?.soft) hideSoon(420);
    else await hide();
  });
}

async function done(conn: Conn, id: number, pet: string, back: boolean) {
  await postJson(conn, "/v1/todos/done", { id, pet, back }).catch(() => {});
}

main().catch((e) => console.error("card", e));
