// The tray panel: opens next to the tray icon. Actions close it and go
// through menu_pick; toggles go through tray_pick and keep it open.

import { invoke } from "@tauri-apps/api/core";
import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";

interface Opt { id: string; label: string }
interface HabitState { streak: number; best: number; today: string; active: boolean; }
interface State {
  version: string; en: boolean; coins: number; broken: number; mode: string; lang: string;
  focus: boolean; autostart: boolean; boss: string; focusKey: string; modes: Opt[]; langs: Opt[]; anchor?: string;
  modelReady?: boolean; modelDlDone?: number; modelDlTotal?: number;
  habits?: { smoking: HabitState; sugar: HabitState };
}

const win = getCurrentWebviewWindow();
const wrap = document.getElementById("wrap")!;
const card = document.getElementById("card")!;
let st: State | null = null;
let modesOpen = false;
let closing = false;
let dlPollTimer: ReturnType<typeof setInterval> | null = null;
let habitPickerOpen: string | null = null;

async function pollModelState() {
  const r = await invoke<{ ready: boolean; done: number; total: number }>("model_state");
  if (st) {
    st.modelReady = r.ready;
    st.modelDlDone = r.done;
    st.modelDlTotal = r.total;
    render();
  }
  if (r.ready || r.total === 0) {
    clearInterval(dlPollTimer!); dlPollTimer = null;
  }
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

function row(id: string, icon: string, tint: string, text: string, hint = "", warn = false) {
  return `<button class="row" data-go="${id}"><span class="ic tn-${tint.slice(4)}">${icon}</span><span class="tx">${text}</span>${hint ? `<span class="hint${warn ? " warn" : ""}">${esc(hint)}</span>` : ""}</button>`;
}

/** What the radio window is playing (it shares localStorage with this window). */
function radioHint(): string {
  try {
    const n = JSON.parse(localStorage.getItem("radio.now") ?? "null") as { name: string; playing: boolean; ts: number } | null;
    if (n?.playing && n.name && Date.now() - n.ts < 30_000) return `▶ ${n.name}`;
  } catch {
    /* nothing stored */
  }
  return t("全世界的电台", "Stations worldwide");
}

function habitHintText(h: HabitState | undefined, en: boolean): string {
  if (!h?.active) return en ? "not started" : "还没开始";
  if (!h.today) return en ? `Day ${h.streak}` : `第 ${h.streak} 天`;
  if (h.today === "relapsed") return en ? "Day 1" : "第 1 天";
  return en ? `Day ${h.streak} · noted` : `第 ${h.streak} 天 · 已记`;
}

function habitRowHTML(type: string, icon: string, label: string, h: HabitState | undefined, en: boolean): string {
  const hint = habitHintText(h, en);
  const isOpen = habitPickerOpen === type;
  const pickLabels = type === "smoking"
    ? [["clean", en ? "Didn't smoke" : "没抽"], ["resisted", en ? "Wanted to, didn't" : "想抽，没抽"], ["relapsed", en ? "Smoked" : "抽了"], ["craving", en ? "Craving right now" : "现在就想抽"]]
    : [["clean", en ? "No sugar" : "没碰糖"], ["resisted", en ? "Wanted some, didn't" : "想吃，没吃"], ["relapsed", en ? "Had some" : "吃了"], ["craving", en ? "Craving right now" : "现在就想吃"]];
  const picker = isOpen
    ? `<div class="habit-picker open">${pickLabels.map(([s, l]) => `<button class="habit-pick${s === "craving" ? " craving" : ""}" data-pick="${type}|${s}">${esc(l)}</button>`).join("")}</div>`
    : `<div class="habit-picker"></div>`;
  return `<button class="row habit-row${isOpen ? " open" : ""}" data-habit="${type}"><span class="ic tn-mint">${icon}</span><span class="tx">${esc(label)}</span><span class="hint">${esc(hint)}</span><span class="chev">›</span></button>${picker}`;
}

function modelHint(s: State): string {
  if (s.modelReady) return t("已就绪", "Ready");
  const done = s.modelDlDone ?? 0;
  const total = s.modelDlTotal ?? 0;
  if (total > 0 && done > 0) return `${Math.round(done / total * 100)}%`;
  return t("本地 AI 写信 (~390 MB)", "Local AI letters (~390 MB)");
}

function render() {
  if (!st) return;
  const s = st;
  document.documentElement.lang = s.en ? "en" : "zh-CN";
  const mode = (s.modes.find((m) => m.id === s.mode)?.label ?? s.mode).split(/[（(]/)[0].trim();
  card.innerHTML = `
    <div class="head"><span class="dots"></span><img src="/logo.png" alt="">
      <span class="name"><b>Familiar</b><small>v${esc(s.version)}</small></span>
      <button class="coins" data-go="report" title="${t("今日报告", "Today's report")}"><i></i>${s.coins}</button></div>
    <div class="body">
      <div class="quick">
        <button class="tile tn-mint" data-go="show"><span class="ic">👀</span><span class="lb">${t("显示宠物", "Show pets")}</span><span class="k">&nbsp;</span></button>
        <button class="tile tn-peach" data-go="hideall"><span class="ic">🙈</span><span class="lb">${t("隐藏", "Hide all")}</span><span class="k">${esc(s.boss)}</span></button>
        <button class="tile tn-lilac${s.focus ? " on" : ""}" data-tg="focus"><span class="ic">🎧</span><span class="lb">${t("专注", "Focus")}</span><span class="k">${s.focus ? t("开", "On") : esc(s.focusKey)}</span></button>
      </div>
      <div class="sec"><h4>${t("功能", "Play")}</h4>
        ${row("gallery", "📖", "--t-sky", t("宠物图鉴", "Gallery"), s.broken ? t(`${s.broken} 个包损坏`, `${s.broken} broken`) : "", s.broken > 0)}
        ${row("shop", "🛍️", "--t-peach", t("商店", "Shop"))}
        ${row("spring", "🏮", "--t-peach", t("春节", "Lunar New Year"), t("抢红包", "Red envelopes"))}
        ${row("reminders", "⏰", "--t-mint", t("健康提醒", "Reminders"), t("提肛 · 休息", "Kegels · breaks"))}
        ${row("radio", "📻", "--t-sky", t("电台", "Radio"), radioHint())}
        ${row("xs", "🎙️", "--t-lilac", t("说相声", "Comedy duo"), t("两只宠物说一段", "Two pets, one bit"))}
      </div>
      <div class="sec"><h4 class="g">${t("健康", "Health")}</h4>
        ${habitRowHTML("smoking", "🚬", t("戒烟", "No smoking"), s.habits?.smoking, s.en)}
        ${habitRowHTML("sugar", "🍬", t("戒糖", "No sugar"), s.habits?.sugar, s.en)}
      </div>
      <div class="sec"><h4 class="y">${t("设置", "Settings")}</h4>
        <button class="row${modesOpen ? " open" : ""}" data-x="modes"><span class="ic tn-lilac">🧭</span><span class="tx">${t("路由模式", "Mode")}</span><span class="hint">${esc(mode)}</span><span class="chev">›</span></button>
        <div class="opts${modesOpen ? " open" : ""}">${s.modes.map((m) => `<button class="opt${m.id === s.mode ? " on" : ""}" data-tg="mode|${m.id}"><i></i>${esc(m.label)}</button>`).join("")}</div>
        <div class="row static"><span class="ic tn-sky">🌐</span><span class="tx">${t("语言", "Language")}</span>
          <span class="seg">${s.langs.map((l) => `<button class="${l.id === s.lang ? "on" : ""}" data-tg="lang|${l.id}">${esc(l.label)}</button>`).join("")}</span></div>
        <button class="row" data-tg="autostart"><span class="ic tn-butter">🚀</span><span class="tx">${t("开机自启", "Auto-start")}</span><span class="sw${s.autostart ? " on" : ""}"></span></button>
        <div class="row static"><span class="ic tn-mint">🔌</span><span class="tx">${t("接入", "Hook up")}</span>
          <span class="hook"><button class="cc" data-go="connect|claude">Claude</button><button class="cx" data-go="connect|codex">Codex</button></span></div>
        <button class="row${s.modelReady ? " model-ready" : ""}" data-dl="model"><span class="ic tn-sky">🧠</span><span class="tx">${t("灵宠语言模型", "Pet language model")}</span><span class="hint${s.modelReady ? " ok" : ""}">${esc(modelHint(s))}</span></button>
      </div>
    </div>
    <div class="foot">
      <button data-go="report">📊 ${t("报告", "Report")}</button>
      <button data-go="config">⚙ ${t("配置", "Config")}</button>
      <button data-go="folder">📁</button>
      <button class="quit" data-go="quit">${t("退出", "Quit")}</button>
    </div>`;
}

card.addEventListener("click", (e) => {
  e.stopPropagation();
  const b = (e.target as HTMLElement).closest<HTMLElement>("[data-go],[data-tg],[data-x],[data-dl],[data-habit],[data-pick]");
  if (!b) return;
  if (b.dataset.x === "modes") { modesOpen = !modesOpen; render(); return; }
  if (b.dataset.dl === "model") {
    if (st?.modelReady) return; // already ready, nothing to do
    void invoke("start_model_download");
    if (!dlPollTimer) dlPollTimer = setInterval(() => void pollModelState(), 600);
    return;
  }
  if (b.dataset.habit) {
    const type = b.dataset.habit;
    habitPickerOpen = habitPickerOpen === type ? null : type;
    render();
    return;
  }
  if (b.dataset.pick) {
    const [type, status] = b.dataset.pick.split("|");
    if (status === "craving") {
      void invoke("habit_craving", { habitType: type });
      close();
      return;
    }
    void invoke<{ streak: number; best: number; today: string; msg: string }>("habit_checkin", { habitType: type, status }).then((r) => {
      if (st?.habits) (st.habits as Record<string, HabitState>)[type] = { streak: r.streak, best: r.best, today: r.today, active: true };
      habitPickerOpen = null;
      render();
    });
    return;
  }
  if (b.dataset.tg) {
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
  habitPickerOpen = null;
  st = e.payload;
  wrap.classList.toggle("top", e.payload.anchor === "top");
  wrap.classList.remove("open");
  render();
  requestAnimationFrame(() => requestAnimationFrame(() => wrap.classList.add("open")));
  // Sync model state once on open
  void pollModelState();
});
void win.listen<State>("tray-state", (e) => {
  const anchor = st?.anchor;
  st = { ...e.payload, anchor };
  render();
});
