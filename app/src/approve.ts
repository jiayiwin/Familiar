// Approval card: when `features.approve` is on, a permission request from
// Claude Code waits here (above the pet) for Allow / Deny before it falls back
// to the terminal's own prompt. The daemon holds the hook; this window only
// shows what is asked and posts the answer.

import { connect, getJson, inTauri, postJson, subscribe, type Conn } from "./api";
import { initLang, t } from "./i18n";
import { applySkin, skinFor } from "./skin";
import { invoke } from "@tauri-apps/api/core";
import { getCurrentWebviewWindow, WebviewWindow } from "@tauri-apps/api/webviewWindow";
import { availableMonitors, LogicalPosition, LogicalSize } from "@tauri-apps/api/window";

interface Ask {
  id: number;
  pet: string | null;
  source: string;
  project: string;
  tool: string;
  detail: string;
  note: string;
  /** Epoch ms the daemon stops waiting at. */
  until: number;
  /** How long it waits in all, for the countdown bar. */
  wait: number;
}

const W = 340;
const el = document.getElementById("card")!;
// Outside the app (a browser preview) the card just sits in the page.
const win = inTauri ? getCurrentWebviewWindow() : null;
let conn: Conn;
let asks: Ask[] = [];
let shown = 0;

function esc(s: string): string {
  return s.replace(/[&<>"]/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;" })[c]!);
}

async function answer(id: number, behavior: "allow" | "deny" | "terminal") {
  asks = asks.filter((a) => a.id !== id);
  void render();
  await postJson(conn, "/v1/approve/answer", { id, behavior });
}

async function place(a: Ask): Promise<void> {
  if (!win) return;
  const h = Math.ceil(el.getBoundingClientRect().height) + 16;
  await win.setSize(new LogicalSize(W, h));
  const pet = a.pet ? await WebviewWindow.getByLabel(`pet-${a.pet}`) : null;
  const monitors = await availableMonitors();
  if (pet) {
    const sc = await pet.scaleFactor();
    const p = (await pet.outerPosition()).toLogical(sc);
    const s = (await pet.outerSize()).toLogical(sc);
    let x = p.x + s.width / 2 - W / 2;
    let y = p.y - h + 18;
    for (const m of monitors) {
      const f = m.scaleFactor;
      const left = m.workArea.position.x / f, top = m.workArea.position.y / f;
      const right = left + m.workArea.size.width / f, bottom = top + m.workArea.size.height / f;
      if (p.x + s.width / 2 >= left && p.x + s.width / 2 < right && p.y + s.height / 2 >= top && p.y + s.height / 2 < bottom) {
        x = Math.min(Math.max(x, left + 4), right - W - 4);
        // No room above the pet: sit beside its feet instead of over it.
        y = y < top + 4 ? Math.min(p.y + s.height - h, bottom - h - 4) : y;
        if (y < top + 4) y = top + 4;
        break;
      }
    }
    await win.setPosition(new LogicalPosition(x, y));
    return;
  }
  // The pet is not on screen: bottom-right of the first monitor.
  const m = monitors[0];
  if (m) {
    const f = m.scaleFactor;
    await win.setPosition(new LogicalPosition((m.workArea.position.x + m.workArea.size.width) / f - W - 16, (m.workArea.position.y + m.workArea.size.height) / f - h - 16));
  }
}

async function render(): Promise<void> {
  const a = asks[0];
  if (!a) {
    shown = 0;
    el.hidden = true;
    await win?.hide();
    return;
  }
  const left = Math.max(0, Math.min(100, ((a.until - Date.now()) / a.wait) * 100));
  if (shown === a.id) {
    // Same request: only the countdown and the queue length move.
    const bar = el.querySelector<HTMLElement>(".left i");
    if (bar) bar.style.width = `${left}%`;
    const more = el.querySelector<HTMLElement>("h1 em");
    if (more) more.hidden = asks.length < 2;
    if (more) more.textContent = `+${asks.length - 1}`;
    return;
  }
  shown = a.id;
  el.innerHTML = `<h1><span>${t("要你点头", "Needs your OK")}</span><small>${esc(a.project || "")}</small><em${asks.length < 2 ? " hidden" : ""}>+${asks.length - 1}</em></h1>
    <div class="left"><i style="width:${left}%"></i></div>
    <div class="body">
      <span class="tool">${esc(a.tool || "?")}</span>
      ${a.note ? `<div class="note">${esc(a.note)}</div>` : ""}
      ${a.detail ? `<pre>${esc(a.detail)}</pre>` : ""}
      <div class="btns"><button class="yes" id="yes">${t("允许", "Allow")}</button><button class="no" id="no">${t("拒绝", "Deny")}</button><button class="term" id="term">${t("回终端决定", "Decide in terminal")}</button></div>
      <div class="hint">${t("只管这一次。不点的话，时间到了终端里照常问你。", "This request only. Leave it, and the terminal asks as usual when the time is up.")}</div>
    </div>`;
  document.getElementById("yes")!.onclick = () => void answer(a.id, "allow");
  document.getElementById("no")!.onclick = () => void answer(a.id, "deny");
  document.getElementById("term")!.onclick = () => void answer(a.id, "terminal");
  el.hidden = false;
  await place(a);
  await win?.show();
}

async function add(a: Ask) {
  // Boss key, fullscreen, screen sharing: nothing pops up; the terminal asks.
  if (inTauri && (await invoke<boolean>("quiet").catch(() => false))) {
    await postJson(conn, "/v1/approve/answer", { id: a.id, behavior: "terminal" });
    return;
  }
  if (!asks.some((x) => x.id === a.id)) asks.push(a);
  void render();
}

async function main() {
  conn = await connect();
  await initLang(conn);
  subscribe(conn, (type, data) => {
    // The card is the asking agent's pet speaking: it wears that pet's skin.
    if (type === "approve.ask") void skinFor(conn, (data as Ask).source).then((s) => applySkin(s.skin.id)).then(() => add(data as Ask));
    else if (type === "approve.done") {
      const id = (data as { id: number }).id;
      asks = asks.filter((a) => a.id !== id);
      void render();
    }
  });
  applySkin((await skinFor(conn)).skin.id);
  const { asks: pending } = await getJson<{ asks: Ask[] }>(conn, "/v1/approve/pending");
  for (const a of pending) await add(a);
  setInterval(() => {
    const now = Date.now();
    asks = asks.filter((a) => a.until > now - 1500);
    void render();
  }, 500);
}

main().catch((e) => console.error("approve", e));
