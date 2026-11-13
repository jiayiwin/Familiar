// Gallery: the five pets and the one you picked, the sweets, what the pet
// remembers, and the settings.

import { connect, getJson, inTauri, onHost, postJson, RARITY_COLORS, RARITY_NAMES, type Conn } from "./api";
import { toast } from "./toast";
import { drawTreat, whenOf } from "./treats";
import { EN, initLang, t as tr } from "./i18n"; // `t` is a common local name here
import { mountPortrait } from "./rig/portrait";
import { applySkin, followSkin, ORDER, SKINS, type Skin, type SkinId } from "./skin";

const TABS = ["pets", "fish", "mem", "settings"];

const $ = (id: string) => document.getElementById(id)!;
let conn: Conn;

function esc(s: string): string {
  return s.replace(/[&<>"]/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;" })[c]!);
}

const ICONS: Record<string, string> = {
  paw: `<ellipse cx="12" cy="15.5" rx="4.6" ry="3.6"/><circle cx="6.3" cy="10.6" r="1.7"/><circle cx="10" cy="6.8" r="1.7"/><circle cx="14" cy="6.8" r="1.7"/><circle cx="17.7" cy="10.6" r="1.7"/>`,
  lift: `<circle cx="12" cy="12" r="9"/><path d="M12 16.5V8M8.5 11.5L12 8l3.5 3.5"/>`,
  globe: `<circle cx="12" cy="12" r="9"/><path d="M3 12h18M12 3c3 3.2 3 14.8 0 18M12 3c-3 3.2-3 14.8 0 18"/>`,
};
const ic = (k: string) =>
  `<svg class="ic" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round">${ICONS[k] ?? ""}</svg>`;

/** A progress ring, `p` in 0..1, with `label` in the middle. */
function ring(p: number, label: string): string {
  const c = 2 * Math.PI * 34;
  return `<div class="ring"><svg viewBox="0 0 80 80"><circle class="trk" cx="40" cy="40" r="34"/>
    <circle class="val" cx="40" cy="40" r="34" stroke-dasharray="${c.toFixed(1)}" stroke-dashoffset="${(c * (1 - Math.max(0, Math.min(1, p)))).toFixed(1)}" transform="rotate(-90 40 40)"/></svg><b>${label}</b></div>`;
}

// ------------------------------------------------------------ the five

/** Who is with which agent, and who has arrived (the daemon knows; the looks are in the skins). */
/** The agents by the short names written under their pets. */
const AGENT_TAGS: Record<string, string> = { "claude-code": "Claude", codex: "Codex", gemini: "Gemini", cursor: "Cursor", opencode: "OpenCode" };
interface Cast { main: string; companions: Record<string, string>; cast: { id: string; name: string; instrument: string; here: boolean }[] }
let cast: Cast | null = null;
let picked: SkinId = "zimeng";
let fiveBuilt = false;
/** Pet id -> the pet windows that wear it now (their agent's short name and window id). */
let onDesk: Record<string, { agent: string; id: string }[]> = {};
let arrivedAt = 0;

/** A pet that is here idles in the picture; one that is not is its still. */
function artOf(sk: Skin, here: boolean): HTMLElement {
  if (!here || !sk.rig) return Object.assign(document.createElement("img"), { className: "art", src: sk.art ?? "", alt: "" });
  const cv = Object.assign(document.createElement("canvas"), { className: "art" });
  void mountPortrait(cv, sk.rig).catch(() => {});
  return cv;
}

async function renderFive() {
  const [views, mem, settings] = await Promise.all([
    getJson<{ pets: { id: string; camp: string | null; marker?: string; skin?: string }[] }>(conn, "/v1/pets").catch(() => null),
    getJson<{ arrived: number }>(conn, "/v1/memories").catch(() => null),
    getJson<{ pets?: Cast }>(conn, "/v1/settings").catch(() => null),
  ]);
  cast = settings?.pets ?? cast;
  // The gallery belongs to no one pet: it wears the main pet's skin.
  applySkin(cast?.main);
  onDesk = {};
  for (const p of views?.pets ?? []) (onDesk[p.skin ?? "zimeng"] ??= []).push({ agent: p.marker ?? p.id, id: p.id });
  arrivedAt = mem?.arrived ?? 0;
  const here = (id: SkinId) => cast?.cast.find((c) => c.id === id)?.here ?? id === "zimeng";
  const withWhom = (id: SkinId) => (onDesk[id] ?? []).map((w) => w.agent);
  const status = (id: SkinId) => {
    if (!here(id)) return tr("还没来", "Not here yet");
    const w = withWhom(id);
    if (w.length) return tr(`陪着 ${w.join("、")}`, `With ${w.join(" and ")}`);
    // At home: it comes out when its agent starts.
    const agent = Object.entries(cast?.companions ?? {}).find(([, pet]) => pet === id)?.[0];
    const name = agent ? AGENT_TAGS[agent] ?? agent : null;
    return name ? tr(`在家 · 等 ${name}`, `At home · waiting for ${name}`) : tr("在家", "At home");
  };
  const box = $("five");
  if (!fiveBuilt) {
    fiveBuilt = true;
    for (const id of ORDER) {
      const sk = SKINS[id];
      const b = document.createElement("button");
      b.type = "button";
      b.dataset.pet = id;
      b.style.setProperty("--tint", sk.colors.room);
      b.append(artOf(sk, here(id)), Object.assign(document.createElement("b"), { textContent: tr(sk.zh, sk.en) }), document.createElement("small"));
      b.onclick = () => {
        picked = id;
        void renderFive();
      };
      box.append(b);
    }
    const all = document.createElement("div");
    all.className = "all";
    all.append(
      Object.assign(document.createElement("b"), { textContent: tr("五只到齐", "When all five are here") }),
      Object.assign(document.createElement("span"), { textContent: tr("可以合奏。钢琴、大提琴、长笛、木琴、竖琴，每只用自己的身体当乐器。", "They can play together: piano, cello, flute, xylophone, harp, each with its own body for an instrument.") }),
    );
    box.append(all);
  }
  for (const id of ORDER) {
    const b = box.querySelector<HTMLElement>(`[data-pet="${id}"]`)!;
    b.className = `pt${id === picked ? " on" : ""}${here(id) ? "" : " away"}`;
    b.querySelector("small")!.textContent = status(id);
  }

  // The one you picked.
  const sk = SKINS[picked];
  const one = $("one");
  if (one.dataset.pet !== sk.id) {
    one.dataset.pet = sk.id;
    one.textContent = "";
    const pic = document.createElement("div");
    pic.className = `pic${here(sk.id) ? "" : " away"}`;
    pic.style.setProperty("--tint", sk.colors.room);
    pic.append(artOf(sk, here(sk.id)));
    const who = document.createElement("div");
    who.className = "who";
    who.append(Object.assign(document.createElement("b"), { textContent: tr(sk.zh, sk.en) }), Object.assign(document.createElement("span"), { textContent: tr(...sk.about) }));
    const facts = document.createElement("div");
    facts.className = "facts";
    const grow = Object.assign(document.createElement("div"), { className: "grow" });
    const btns = document.createElement("div");
    btns.className = "btns";
    one.append(pic, who, facts, grow, btns);
  }
  const fact = (k: string, v: string) => `<div><span class="k">${esc(k)}</span><span class="v">${esc(v)}</span></div>`;
  const facts = one.querySelector<HTMLElement>(".facts")!;
  const btns = one.querySelector<HTMLElement>(".btns")!;
  facts.innerHTML = fact(tr("乐器", "Plays"), tr(...sk.plays));
  btns.innerHTML = "";
  if (!here(sk.id)) {
    facts.innerHTML += fact(tr("现在", "Now"), tr("还在路上。画好了，还没学会动。", "On its way. Drawn, but not moving yet."));
    return;
  }
  const days = arrivedAt ? Math.floor((Date.now() - arrivedAt) / 86_400_000) + 1 : 1;
  const w = withWhom(sk.id);
  facts.innerHTML += fact(tr("陪着", "With"), w.length ? w.join(tr("、", ", ")) : tr("现在没陪谁", "No one right now")) + (sk.id === "zimeng" ? fact(tr("来了", "Here"), tr(`第 ${days} 天`, `Day ${days}`)) : "");
  // Changing who a pet is with changes that agent's whole skin: its window, menu, room, letters.
  const pair = async (source: string) => {
    const r = await postJson(conn, "/v1/settings", { key: `companion:${source}`, value: sk.id });
    toast(r.message);
    await renderFive();
  };
  const menu = async (id: string) => {
    if (!inTauri) return;
    const { invoke } = await import("@tauri-apps/api/core");
    await invoke("menu_pick", { id });
  };
  const add = (cls: string, label: string, run: () => void) => {
    const b = Object.assign(document.createElement("button"), { type: "button", className: cls, textContent: label });
    b.onclick = run;
    btns.append(b);
  };
  const mine = (cast?.companions ?? {}) as Record<string, string>;
  if (mine["claude-code"] !== sk.id) add("go", tr("让它陪 Claude", "Put it with Claude"), () => void pair("claude-code"));
  if (mine.codex !== sk.id) add(mine["claude-code"] !== sk.id ? "alt" : "go", tr("让它陪 Codex", "Put it with Codex"), () => void pair("codex"));
  const win = onDesk[sk.id]?.[0]?.id;
  if (win && btns.childElementCount < 2) add("alt", tr("去它的小屋", "To its room"), () => void menu(`room|${win}`));
}

// ------------------------------------------------------------ what it remembers

async function renderMem() {
  const box = $("mem");
  const d = await getJson<{ items: { ts: number; text: string }[] }>(conn, "/v1/memories").catch(() => null);
  box.textContent = "";
  if (!d || !d.items.length) {
    box.append(Object.assign(document.createElement("div"), { className: "none", textContent: d ? tr("还没有。它注意到的事会记在这里。", "Nothing yet. What it notices is kept here.") : tr("这一页要新版的后台，重启睿洱后再看。", "This page needs the new daemon. Restart Rill and look again.") }));
    return;
  }
  const today = new Date().toDateString();
  const yesterday = new Date(Date.now() - 86_400_000).toDateString();
  let key = "";
  let day: HTMLElement | null = null;
  for (const it of d.items) {
    const at = new Date(it.ts);
    if (at.toDateString() !== key) {
      key = at.toDateString();
      const label =
        key === today ? tr("今天", "Today") : key === yesterday ? tr("昨天", "Yesterday") : EN ? at.toLocaleDateString("en-US", { weekday: "long", month: "long", day: "numeric" }) : `${at.getMonth() + 1} 月 ${at.getDate()} 日，周${"日一二三四五六"[at.getDay()]}`;
      box.append(Object.assign(document.createElement("h2"), { textContent: label }));
      day = Object.assign(document.createElement("div"), { className: "day" });
      box.append(day);
    }
    const row = Object.assign(document.createElement("div"), { className: "it" });
    row.append(
      Object.assign(document.createElement("time"), { textContent: `${String(at.getHours()).padStart(2, "0")}:${String(at.getMinutes()).padStart(2, "0")}` }),
      Object.assign(document.createElement("span"), { textContent: it.text }),
    );
    day!.append(row);
  }
}

// ---------------------------------------------------------------- fish

/** One kind of sweet or fruit (the daemon still calls the list `fish`, as it did when they were). */
interface FishInfo {
  id: string;
  name: string;
  rarity: string;
  note: string;
  count: number;
  first: number;
  shape?: string;
  colors?: string[];
  from?: string | null;
  hours?: [number, number] | null;
  months?: [number, number] | null;
}

interface Stats {
  fish: FishInfo[];
}

const RARITIES = ["common", "rare", "epic", "legendary"];
/** One kind, drawn; a kind not met yet is a pale shape. */
function fishCanvas(f: FishInfo, locked: boolean): HTMLCanvasElement {
  const W = 108, H = 78;
  const c = document.createElement("canvas");
  const dpr = devicePixelRatio || 1;
  c.width = Math.round(W * dpr);
  c.height = Math.round(H * dpr);
  c.style.width = `${W}px`;
  c.style.height = `${H}px`;
  const g = c.getContext("2d")!;
  g.scale(dpr, dpr);
  if (!locked && (f.rarity === "epic" || f.rarity === "legendary")) {
    g.shadowColor = f.colors?.[0] ?? "#f6d77a";
    g.shadowBlur = 14;
  }
  drawTreat(g, f, W / 2, H / 2 + 2, 1.55, locked);
  return c;
}

async function renderFish() {
  const st = await getJson<Stats>(conn, "/v1/stats");
  const got = st.fish.filter((f) => f.count > 0);
  const total = got.reduce((n, f) => n + f.count, 0);
  $("fishhead").innerHTML = `<div class="head fishhead">
    <div class="num"><span>${tr("已收集", "COLLECTED")}</span><b>${got.length}</b><i>/ ${st.fish.length}</i></div>
    <div class="rars">${RARITIES.map((r) => {
      const all = st.fish.filter((f) => f.rarity === r);
      const have = all.filter((f) => f.count > 0).length;
      return all.length
        ? `<div class="rarrow" style="--rc:${RARITY_COLORS[r]}"><em>${RARITY_NAMES[r]}</em><div class="bar"><i style="width:${(have / all.length) * 100}%"></i></div><span>${have}/${all.length}</span></div>`
        : "";
    }).join("")}</div>
    <div class="ringbox">${ring(got.length / Math.max(1, st.fish.length), String(total))}<span>${tr("一共得到的颗数", "earned in all")}</span></div>
  </div>`;
  const grid = $("fishgrid");
  grid.innerHTML = "";
  st.fish.forEach((f, i) => {
    const locked = f.count === 0;
    const shiny = !locked && (f.rarity === "epic" || f.rarity === "legendary");
    const el = document.createElement("div");
    el.className = `fc${locked ? " locked" : ""}`;
    el.style.setProperty("--rc", RARITY_COLORS[f.rarity] ?? "#7f8aa3");
    el.style.setProperty("--i", String(i));
    el.innerHTML = `<div class="tank">${shiny ? `<i class="sheen"></i>` : ""}</div>
      ${locked ? "" : `<span class="cnt">×${f.count}</span>`}
      <div class="info"><span class="rar">${RARITY_NAMES[f.rarity] ?? f.rarity}</span>
      <b>${locked ? "？？？" : esc(f.name)}</b>
      <small>${locked ? tr("还没遇到过", "Not met yet") + (whenOf(f, tr) ? ` · ${whenOf(f, tr)}` : "") : esc(f.note)}</small></div>`;
    el.querySelector(".tank")!.prepend(fishCanvas(f, locked));
    grid.appendChild(el);
  });
}

// ---------------------------------------------------------------- settings

interface Settings { language: string; size: string; chatter: string; kegel_reps?: number; config_error?: string | null; focus_mode: boolean; sounds: boolean; approve: boolean; night?: boolean; approve_wait_s: number; usage: { primary: UsageWindow | null; secondary: UsageWindow | null } | null }
interface UsageWindow { used_percent: number; window_minutes: number; resets_at: number }

async function renderSettings() {
  const box = $("tab-settings");
  let s = await getJson<Settings>(conn, "/v1/settings");
  const set = async (key: string, value: string | boolean | number) => {
    const res = await postJson(conn, "/v1/settings", { key, value });
    if (key === "language" && res.ok) {
      // Every label on the page is in the old language: come back on this tab in the new one.
      const q = new URLSearchParams(location.search);
      q.set("tab", "settings");
      setTimeout(() => (location.search = q.toString()), 250);
      return;
    }
    s = await getJson<Settings>(conn, "/v1/settings");
    draw();
    $("st-msg").textContent = res.message;
    $("st-msg").style.color = res.ok ? "" : "var(--red)";
  };
  const seg = (key: keyof Settings, opts: [string, string][]) =>
    `<span class="seg2">${opts.map(([id, label]) => `<button data-set="${key}" data-v="${id}" class="${s[key] === id ? "on" : ""}">${label}</button>`).join("")}</span>`;
  const sw = (key: keyof Settings, title: string, hint: string) =>
    `<label class="sw"><div class="txt"><b>${title}</b><small>${hint}</small></div>
      <input type="checkbox" data-flag="${key}"${s[key] ? " checked" : ""} /><span class="knob"></span></label>`;
  const draw = () => {
    const chatterHint: Record<string, string> = {
      quiet: tr("它不主动说话，也不主动做事。", "It says and does nothing on its own."),
      normal: tr("偶尔说一句；久坐、深夜、卡住时会跳舞、奏一段、带你提肛。", "A line now and then; a dance, a piece of music or a kegel set when you sit long, stay late or get stuck."),
      lively: tr("话多一些，也更早注意到你坐久了。", "More lines, and it notices a long sit sooner."),
    };
    const win = (w: UsageWindow | null | undefined, zh: string, en: string) => (w ? tr(`${zh} ${Math.round(w.used_percent)}%`, `${en} ${Math.round(w.used_percent)}%`) : "");
    const nums = [win(s.usage?.primary, "5 小时", "5-hour"), win(s.usage?.secondary, "每周", "weekly")].filter(Boolean).join(" · ");
    const usageNow = nums ? `<span class="chip" style="margin-left:8px">${nums}</span>` : "";
    const reps = s.kegel_reps ?? 10;
    const mins = Math.max(1, Math.round((3 + reps * 6) / 60));
    const steps: [string, string][] = [
      ["<b>找对肌肉：</b>就是憋尿、忍住放屁时用力的那块肌肉（盆底肌）。", "<b>Find the muscle:</b> the one you use to hold in pee or gas (the pelvic floor)."],
      ["<b>收紧上提：</b>它说「收紧」时，把肛门和会阴往上、往里收，保持 3 秒。", "<b>Squeeze and lift:</b> on “Squeeze”, pull the area up and in, and hold for 3 s."],
      ["<b>完全放松：</b>它说「放松」时彻底松开 3 秒。放松和收紧一样重要。", "<b>Let go fully:</b> on “Relax”, release completely for 3 s. Relaxing matters as much as squeezing."],
      ["<b>呼吸自然：</b>别憋气；肚子、屁股和大腿尽量不跟着用力。", "<b>Keep breathing:</b> don't hold your breath; keep your belly, buttocks and thighs loose."],
      ["坐着、站着都能做，旁边的人看不出来。", "Sitting or standing both work, and nobody can tell."],
    ];
    box.innerHTML = `
      ${s.config_error ? `<div class="banner">${tr("config.yaml 有错，先用着上一次的配置：", "config.yaml has an error; using the last good config for now: ")}${esc(s.config_error)}</div>` : ""}
      <section class="hcard">
        <div class="hhead"><div class="ib">${ic("paw")}</div><div><h3>${tr("它", "The pet")}</h3></div></div>
        <div class="srow"><div class="txt"><b>${tr("大小", "Size")}</b><small>${tr("小 0.75 倍，中 1 倍，大 1.35 倍。", "Small 0.75×, medium 1×, large 1.35×.")}</small></div>
          ${seg("size", [["s", tr("小", "S")], ["m", tr("中", "M")], ["l", tr("大", "L")]])}</div>
        <div class="srow"><div class="txt"><b>${tr("话多话少", "Chattiness")}</b><small>${chatterHint[s.chatter] ?? ""}</small></div>
          ${seg("chatter", [["quiet", tr("安静", "Quiet")], ["normal", tr("正常", "Normal")], ["lively", tr("活泼", "Lively")]])}</div>
        ${sw("focus_mode", tr("安静一会儿", "Quiet for a while"), tr("只留状态和“等你”的提醒，别的都不动、不出声。", "Only the status and the “waiting for you” alert; nothing else moves or sounds."))}
        ${sw("sounds", tr("声音", "Sound"), tr("琴声和提示音。", "Their instruments and the alert notes."))}
      </section>
      <section class="hcard">
        <div class="hhead"><div class="ib">${ic("lift")}</div><div><h3>${tr("提肛", "Kegels")}</h3></div></div>
        <div class="note">${tr("不定时。你坐久了，它会说一声，然后转过身、背对着你带你做一组：收紧时它夹紧、尾巴提起，放松时松开、尾巴摇一下，头顶有倒计时。点它一下可以跳过。",
          "No schedule. When you have sat a long while, it says so, turns its back and leads a set: it squeezes with its tail lifted, then lets go with a wag, with a countdown above it. Click it to skip.")}</div>
        <div class="hrow"><span>${tr("每次", "Each set")}</span>
          <select id="kg-reps">${[5, 10, 15, 20].map((n) => `<option value="${n}"${n === reps ? " selected" : ""}>${tr(`${n} 下`, `${n} reps`)}</option>`).join("")}</select>
          <span class="note" style="margin:0">${tr(`约 ${mins} 分钟`, `about ${mins} min`)}</span>
          <span class="sp"></span><button id="kg-try" class="primary">${tr("现在做一组", "Do a set now")}</button></div>
        <details class="howto"><summary>${tr("动作要领", "How to do it")}</summary>
          <div class="steps">${steps.map(([zh, en]) => `<div>${tr(zh, en)}</div>`).join("")}</div>
          <div class="note">${tr("刚开始每次 5 下，习惯后 10–15 下，每天 2–3 次。如果有盆底或肛肠方面的不适，先问问医生。",
          "Start with 5 reps, build up to 10–15, two or three times a day. If you have pelvic or rectal problems, ask a doctor first.")}</div></details>
      </section>
      <section class="hcard">
        <div class="hhead"><div class="ib">${ic("globe")}</div><div><h3>${tr("语言与接入", "Language and agents")}</h3></div></div>
        <div class="srow"><div class="txt"><b>${tr("界面语言", "Language")}</b><small>${tr("「自动」跟随系统。", "“Auto” follows the system.")}</small></div>
          ${seg("language", [["auto", tr("自动", "Auto")], ["zh", "中文"], ["en", "English"]])}</div>
        ${sw("approve", tr("在宠物上批准权限", "Approve permissions on the pet"),
          tr(`Claude Code 要权限时，宠物上方弹出一张卡片，写着它要运行的命令或要改的文件，点「允许」或「拒绝」就行，不用切回终端。打开以后，终端里自己的确认框要等这张卡片（最多 ${s.approve_wait_s} 秒，或者你在卡片上点「回终端决定」）。要显示的命令内容只交给本机的睿洱。全屏、共享屏幕或按了老板键时不弹。`,
             `When Claude Code asks for permission, a card opens above the pet with the command it wants to run or the file it wants to change: click Allow or Deny without going back to the terminal. While this is on, the terminal's own prompt waits for the card (up to ${s.approve_wait_s} s, or until you click “Decide in terminal”). The command text goes only to Rill on this machine. Nothing pops up in fullscreen, while sharing the screen, or under the boss key.`))}
        ${s.approve ? `<div class="srow"><div class="txt"><b>${tr("卡片等多久", "How long the card waits")}</b><small>${tr("时间到了还没点，终端里照常问你。", "With no answer by then, the terminal asks as usual.")}</small></div>
          <span class="seg2">${[15, 30, 60].map((n) => `<button data-wait="${n}" class="${s.approve_wait_s === n ? "on" : ""}">${tr(`${n} 秒`, `${n} s`)}</button>`).join("")}</span></div>` : ""}
        <div class="srow"><div class="txt"><b>${tr("接入 agent", "Hook up agents")}</b><small>${tr("给 Claude Code 装插件，或在 Codex 的 hooks.json 里追加 hook。动手前都会先说明要改什么。", "Installs the plugin into Claude Code, or appends hooks to Codex's hooks.json. Both say what they change first.")}</small></div>
          <button data-hook="claude-code"${inTauri ? "" : " disabled"}>Claude Code…</button><button data-hook="codex"${inTauri ? "" : " disabled"}>Codex…</button></div>
        <div class="srow"><div class="txt"><b>${tr("其他 agent", "Other agents")}</b><small>${tr("Gemini CLI、Cursor、OpenCode 也能各有一只：看状态、得糖、拍虫。Cursor 没有「等你批准」的事件，它那只不会叫你。", "Gemini CLI, Cursor and OpenCode can each have one too: status, fishing, bugs. Cursor has no “waiting for approval” event, so its pet won't call you.")}</small></div>
          <button data-hook="gemini"${inTauri ? "" : " disabled"}>Gemini CLI…</button><button data-hook="cursor"${inTauri ? "" : " disabled"}>Cursor…</button><button data-hook="opencode"${inTauri ? "" : " disabled"}>OpenCode…</button></div>
        <div class="srow"><div class="txt"><b>${tr("Claude Code 用量", "Claude Code usage")}${usageNow}</b><small>${tr("接上以后，右键 Claude Code 的宠物能看到 5 小时和每周额度用了多少，用到 90% 时它会说一声。数字来自 Claude Code 自己的状态栏，只有订阅账号才有。", "Once hooked up, the Claude Code pet's menu shows how much of the 5-hour and weekly limits is used, and it says so at 90%. The numbers come from Claude Code's own status line, on subscription accounts only.")}</small></div>
          <button data-hook="usage"${inTauri ? "" : " disabled"}>${tr("接上…", "Hook up…")}</button></div>
        <div class="srow"><div class="txt"><b>${tr("agent 的储物间", "The agents' storeroom")}</b><small>${tr("它们攒在这台机器上的旧会话占了多少磁盘，每开一个会话先读进去多少 token、哪些一个月没用过。可以装箱、收起，都能放回。堆多了宠物会提一句。", "How much disk their old sessions take on this machine, and how many tokens every new session reads first, with what went unused for a month. Pack or put things away, and bring them back. The pet mentions it when it piles up.")}</small></div>
          <button data-pick="storeroom"${inTauri ? "" : " disabled"}>${tr("看看…", "Have a look…")}</button></div>
        ${sw("night", tr("夜里整理", "Tidy at night"),
          tr("晚上你离开电脑半小时后，它把桌面和下载里很久没碰的文件收进一个文件夹，把 agent 一个月以前的会话装箱；安装包、副本只列出来，不删。第二天告诉你做了什么，每样都能放回。你放回去的，它记住，不再动。关掉以后只看不动。",
             "Half an hour after you leave in the evening, it puts files untouched for weeks on the desktop and in Downloads into one folder, and boxes the agents' month-old sessions; installers and copies are only listed, never deleted. The next day it tells you what it did, and each thing can go back. What you put back, it remembers and leaves alone. Off: it only looks."))}
        <div class="srow"><div class="txt"><b>${tr("快捷键和其他", "Hotkeys and the rest")}</b><small>${tr("老板键、专注键写在 config.yaml 里，保存后自动生效。", "The boss key and the focus key live in config.yaml; changes apply on save.")}</small></div>
          <button data-open="open_config"${inTauri ? "" : " disabled"}>${tr("编辑配置…", "Edit config…")}</button></div>
      </section>
      <section class="hcard">
        <div class="hhead"><div class="ib">${ic("globe")}</div><div><h3>${tr("本机", "This computer")}</h3></div></div>
        <div class="srow"><div class="txt"><b>${tr("开机自启", "Start with Windows")}</b><small>${tr("开机时一起启动。", "Starts with the computer.")}</small></div>
          <button data-auto${inTauri ? "" : " disabled"} id="st-auto">…</button></div>
        <div class="srow"><div class="txt"><b>${tr("宠物的语言模型", "The pets' language model")}</b><small>${tr("下载到本机的小模型，用来写信，不联网。", "A small model kept on this computer, for writing letters. Works offline.")}</small></div>
          <button data-model${inTauri ? "" : " disabled"} id="st-model">…</button></div>
        <div class="srow"><div class="txt"><b>${tr("数据文件夹", "The data folder")}</b><small>${tr("配置、糖罐、信和日记。", "Settings, the jar, letters and the diary.")}</small></div>
          <button data-pick="folder"${inTauri ? "" : " disabled"}>${tr("打开…", "Open…")}</button></div>
      </section>
      <div id="st-msg" class="note"></div>`;
    box.querySelectorAll<HTMLButtonElement>("[data-set]").forEach((b) => (b.onclick = () => void set(b.dataset.set!, b.dataset.v!)));
    box.querySelectorAll<HTMLInputElement>("[data-flag]").forEach((c) => (c.onchange = () => void set(c.dataset.flag!, c.checked)));
    box.querySelectorAll<HTMLButtonElement>("[data-wait]").forEach((b) => (b.onclick = () => void set("approve_wait_s", Number(b.dataset.wait))));
    ($("kg-reps") as HTMLSelectElement).onchange = (e) => void set("kegel_reps", Number((e.target as HTMLSelectElement).value));
    $("kg-try").onclick = async () => {
      const res = await fetch(conn.base + "/v1/cmd", {
        method: "POST",
        headers: { "X-RillBuddy-Token": conn.token, "Content-Type": "application/json" },
        body: JSON.stringify({ cmd: "kegel" }),
      });
      toast(await res.text());
    };
    box.querySelectorAll<HTMLButtonElement>("[data-hook]").forEach(
      (b) =>
        (b.onclick = async () => {
          const { invoke } = await import("@tauri-apps/api/core");
          await invoke("connect_agent", { which: b.dataset.hook });
        }),
    );
    // Auto-start and the model belong to the host: ask it how they stand, and change them through it.
    const auto = $("st-auto") as HTMLButtonElement, model = $("st-model") as HTMLButtonElement;
    const host = async () => {
      if (!inTauri) return;
      const { invoke } = await import("@tauri-apps/api/core");
      const st = await invoke<{ autostart: boolean }>("tray_info").catch(() => null);
      auto.textContent = st?.autostart ? tr("已开启 · 关闭", "On · turn off") : tr("已关闭 · 开启", "Off · turn on");
      const m = await invoke<{ ready: boolean; done: number; total: number }>("model_state").catch(() => null);
      model.disabled = !!m?.ready || (!!m && m.total > 0 && m.done < m.total);
      model.textContent = m?.ready ? tr("已下载", "Downloaded") : m && m.total > 0 ? tr(`下载中 ${Math.round((m.done / m.total) * 100)}%`, `Downloading ${Math.round((m.done / m.total) * 100)}%`) : tr("下载…", "Download…");
      if (m && !m.ready && m.total > 0) window.setTimeout(() => void host(), 800);
    };
    void host();
    auto.onclick = async () => {
      const { invoke } = await import("@tauri-apps/api/core");
      await invoke("tray_pick", { id: "autostart" });
      void host();
    };
    model.onclick = async () => {
      const { invoke } = await import("@tauri-apps/api/core");
      await invoke("start_model_download");
      window.setTimeout(() => void host(), 400);
    };
    box.querySelectorAll<HTMLButtonElement>("[data-pick]").forEach(
      (b) =>
        (b.onclick = async () => {
          const { invoke } = await import("@tauri-apps/api/core");
          await invoke("menu_pick", { id: b.dataset.pick });
        }),
    );
    box.querySelectorAll<HTMLButtonElement>("[data-open]").forEach(
      (b) =>
        (b.onclick = async () => {
          const { invoke } = await import("@tauri-apps/api/core");
          await invoke(b.dataset.open!);
        }),
    );
  };
  draw();
}

/** Open a tab. */
function go(tab: string): void {
  // Links from before the reminders moved into the settings still arrive.
  if (tab === "health") tab = "settings";
  if (!TABS.includes(tab)) return;
  document.querySelectorAll<HTMLButtonElement>("#tabs button").forEach((x) => x.classList.toggle("on", x.dataset.tab === tab));
  for (const id of TABS) $(`tab-${id}`).hidden = id !== tab;
  if (tab === "pets") void renderFive();
  if (tab === "settings") void renderSettings();
  if (tab === "fish") void renderFish();
  if (tab === "mem") void renderMem();
}

async function main() {
  conn = await connect();
  await initLang(conn);
  document.querySelectorAll<HTMLButtonElement>("#tabs button").forEach((b) => {
    b.onclick = () => go(b.dataset.tab!);
  });
  await renderFive();
  // The pairing was changed somewhere else (the config file): the page follows.
  followSkin(conn, () => null, () => void renderFive());
  // gallery.html?tab=settings opens a tab directly.
  const tab = new URLSearchParams(location.search).get("tab");
  if (tab) go(tab);
  // The tray and the pet menu open a tab in a gallery that is already up.
  void onHost<string>("goto-tab", (tab) => go(tab));
}

main().catch((e) => {
  document.body.innerHTML = `<div class="banner">${tr("图鉴加载失败：", "Failed to load the Gallery: ")}${esc(String(e))}</div>`;
});
