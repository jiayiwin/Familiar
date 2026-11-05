// The tray panel: opens next to the tray icon. Actions close it and go
// through menu_pick; toggles go through tray_pick and keep it open.

import { invoke } from "@tauri-apps/api/core";
import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";

interface Opt { id: string; label: string }
interface State {
  version: string; en: boolean; coins: number; broken: number; mode: string; lang: string;
  focus: boolean; autostart: boolean; boss: string; focusKey: string; modes: Opt[]; langs: Opt[]; anchor?: string;
}

const win = getCurrentWebviewWindow();
const wrap = document.getElementById("wrap")!;
const card = document.getElementById("card")!;
let st: State | null = null;
let modesOpen = false;
let closing = false;

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

function row(id: string, icon: string, tint: string, text: string, hint = "", warn = false) {
  return `<button class="row" data-go="${id}"><span class="ic tn-${tint.slice(4)}">${icon}</span><span class="tx">${text}</span>${hint ? `<span class="hint${warn ? " warn" : ""}">${esc(hint)}</span>` : ""}</button>`;
}

function render() {
  if (!st) return;
  const s = st;
  document.documentElement.lang = s.en ? "en" : "zh-CN";
  const mode = (s.modes.find((m) => m.id === s.mode)?.label ?? s.mode).split(/[（(]/)[0].trim();
  card.innerHTML = `
    <div class="head"><span class="dots"></span><img src="/logo.png" alt="">
      <span class="name"><b>Familiar</b><small>v${esc(s.version)} · ${t("本地运行", "Local only")}</small></span>
      <button class="coins" data-go="report" title="${t("今日报告", "Today's report")}"><i></i>${s.coins}</button></div>
    <div class="body">
      <div class="quick">
        <button class="tile tn-mint" data-go="show"><span class="ic">👀</span><span class="lb">${t("显示宠物", "Show pets")}</span><span class="k">&nbsp;</span></button>
        <button class="tile tn-peach" data-go="hideall"><span class="ic">🙈</span><span class="lb">${t("全部隐藏", "Hide all")}</span><span class="k">${esc(s.boss)}</span></button>
        <button class="tile tn-lilac${s.focus ? " on" : ""}" data-tg="focus"><span class="ic">🎧</span><span class="lb">${t("专注模式", "Focus")}</span><span class="k">${s.focus ? t("已开启", "On") : esc(s.focusKey)}</span></button>
      </div>
      <div class="sec"><h4>${t("玩", "Play")}</h4>
        ${row("gallery", "📖", "--t-sky", t("宠物图鉴", "Gallery"), s.broken ? t(`${s.broken} 个包有错误`, `${s.broken} broken`) : "", s.broken > 0)}
        ${row("report", "📊", "--t-butter", t("今日报告", "Today's report"), t(`${s.coins} 金币`, `${s.coins} coins`))}
        ${row("shop", "🛍️", "--t-peach", t("商店", "Shop"))}
        ${row("spring", "🏮", "--t-peach", t("春节倒计时", "Lunar New Year"), t("抢红包", "Red envelopes"))}
        ${row("reminders", "⏰", "--t-mint", t("健康提醒", "Health reminders"), t("提肛 · 休息", "Kegels · breaks"))}
      </div>
      <div class="sec"><h4 class="y">${t("设置", "Settings")}</h4>
        <button class="row${modesOpen ? " open" : ""}" data-x="modes"><span class="ic tn-lilac">🧭</span><span class="tx">${t("路由模式", "Routing mode")}</span><span class="hint">${esc(mode)}</span><span class="chev">›</span></button>
        <div class="opts${modesOpen ? " open" : ""}">${s.modes.map((m) => `<button class="opt${m.id === s.mode ? " on" : ""}" data-tg="mode|${m.id}"><i></i>${esc(m.label)}</button>`).join("")}</div>
        <div class="row static"><span class="ic tn-sky">🌐</span><span class="tx">${t("语言", "Language")}</span>
          <span class="seg">${s.langs.map((l) => `<button class="${l.id === s.lang ? "on" : ""}" data-tg="lang|${l.id}">${esc(l.label)}</button>`).join("")}</span></div>
        <button class="row" data-tg="autostart"><span class="ic tn-butter">🚀</span><span class="tx">${t("开机自启", "Start with Windows")}</span><span class="sw${s.autostart ? " on" : ""}"></span></button>
        <div class="row static"><span class="ic tn-mint">🔌</span><span class="tx">${t("接入", "Hook up")}</span>
          <span class="hook"><button class="cc" data-go="connect|claude">Claude Code</button><button class="cx" data-go="connect|codex">Codex</button></span></div>
      </div>
    </div>
    <div class="foot">
      <button data-go="config">⚙ ${t("编辑配置", "Edit config")}</button>
      <button data-go="folder">📁 ${t("数据目录", "Data folder")}</button>
      <button class="quit" data-go="quit">${t("退出", "Quit")}</button>
    </div>`;
}

card.addEventListener("click", (e) => {
  e.stopPropagation();
  const b = (e.target as HTMLElement).closest<HTMLElement>("[data-go],[data-tg],[data-x]");
  if (!b) return;
  if (b.dataset.x === "modes") { modesOpen = !modesOpen; render(); return; }
  if (b.dataset.tg) {
    // Instant feedback; petd's answer replaces it.
    if (st && b.dataset.tg === "focus") st.focus = !st.focus;
    if (st && b.dataset.tg === "autostart") st.autostart = !st.autostart;
    if (st && b.dataset.tg.startsWith("mode|")) st.mode = b.dataset.tg.slice(5);
    if (st && b.dataset.tg.startsWith("lang|")) st.lang = b.dataset.tg.slice(5);
    render();
    toggle(b.dataset.tg);
    return;
  }
  if (b.dataset.go) pick(b.dataset.go);
});
wrap.addEventListener("click", close);
window.addEventListener("keydown", (e) => { if (e.key === "Escape") close(); });
void win.onFocusChanged(({ payload }) => { if (!payload) close(); });

void win.listen<State>("tray-open", (e) => {
  closing = false;
  modesOpen = false;
  st = e.payload;
  wrap.classList.toggle("top", e.payload.anchor === "top");
  wrap.classList.remove("open");
  render();
  requestAnimationFrame(() => requestAnimationFrame(() => wrap.classList.add("open")));
});
void win.listen<State>("tray-state", (e) => {
  const anchor = st?.anchor;
  st = { ...e.payload, anchor };
  render();
});
