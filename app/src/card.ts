// Hover card: lists the sessions behind a pet (or one mini's session).
// Pet windows emit `card:show` / `card:hide` to this window.

import { connect, duration, getJson, SOURCE_NAMES, STATE_NAMES, targetName, type PetView, type SessionCard } from "./api";
import { initLang, t } from "./i18n";
import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";
import { availableMonitors, LogicalPosition, LogicalSize } from "@tauri-apps/api/window";

const W = 288;
const el = document.getElementById("card")!;
const win = getCurrentWebviewWindow();

interface ShowReq {
  pet: string;
  session: string | null;
  ax: number;
  ay: number;
  aw: number;
}

function esc(s: string): string {
  return s.replace(/[&<>"]/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;" })[c]!);
}

function row(s: SessionCard, now: number): string {
  const what = [s.tool, s.file].filter(Boolean).join(" · ");
  return `<div class="row">
    <span class="dot ${s.state}"></span>
    <span class="name">${esc(s.project || t("(未知项目)", "(unknown project)"))}</span>
    <span class="state ${s.state}">${STATE_NAMES[s.state]}</span>
    <span class="meta">${SOURCE_NAMES[s.source] ?? esc(s.source)} · ${t("已运行", "up")} ${duration(now - s.started_at)}${
      what ? ` · ${t("最近", "last")} ${esc(what)}` : ""
    }</span>
  </div>`;
}

function render(v: PetView | undefined, session: string | null): void {
  const now = Date.now();
  if (!v) {
    el.innerHTML = `<div class="empty">${t("这只宠物已经不在了。", "This pet is gone.")}</div>`;
    return;
  }
  const list = session ? v.sessions.filter((s) => s.key === session) : v.sessions;
  const title = session ? t("会话", "Session") : targetName(v.target);
  const sub = session ? "" : t(`${v.online} 个在线`, `${v.online} online`) + (v.waiting ? t(` · ${v.waiting} 个等你`, ` · ${v.waiting} waiting`) : "");
  el.innerHTML = `<h1><span>${esc(title)}</span><small>${esc(sub)}</small></h1>
    ${list.length ? list.map((s) => row(s, now)).join("") : `<div class="empty">${t("没有在线会话，宠物在睡觉。", "No live sessions — the pet is napping.")}</div>`}
    ${session ? "" : `<div class="hint">${t("右键宠物可以换一只、改路由模式", "Right-click the pet to switch pets or routing mode")}</div>`}`;
}

async function place(req: ShowReq): Promise<void> {
  const h = Math.ceil(el.getBoundingClientRect().height) + 8;
  let x = req.ax + req.aw / 2 - W / 2;
  let y = req.ay - h - 4;
  // Keep inside the monitor that holds the pet.
  for (const m of await availableMonitors()) {
    const s = m.scaleFactor;
    const left = m.workArea.position.x / s;
    const top = m.workArea.position.y / s;
    const right = left + m.workArea.size.width / s;
    const bottom = top + m.workArea.size.height / s;
    if (req.ax >= left && req.ax < right && req.ay >= top && req.ay < bottom) {
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
    const { pets } = await getJson<{ pets: PetView[] }>(conn, "/v1/pets");
    if (current !== req) return;
    render(pets.find((p) => p.id === req.pet), req.session);
    await place(req);
  };

  await win.listen<ShowReq>("card:show", async (e) => {
    current = e.payload;
    await refresh();
    if (current === e.payload) await win.show();
    clearInterval(timer);
    timer = window.setInterval(() => void refresh(), 1000);
  });
  await win.listen("card:hide", async () => {
    current = null;
    clearInterval(timer);
    await win.hide();
  });
}

main().catch((e) => console.error("card", e));
