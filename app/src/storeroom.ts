// 储物间: what the coding agents keep on this machine, and what every new
// session reads before you say a word. The pet counts (petd/storeroom.rs),
// says what it found, and packs old sessions into a box you can put back.

import { connect, getJson, postJson, subscribe, type Conn } from "./api";
import { initLang, t } from "./i18n";
import { mountPortrait } from "./rig/portrait";
import { applySkin, followSkin, skinFor } from "./skin";

interface Item { kind: "file" | "memory" | "hook" | "skill" | "mcp"; name: string; path?: string | null; tokens: number; unused: boolean; fix?: string | null }
interface Store { id: string; name: string; root: string; total: number; sessions: number; session_files: number; old: number; old_files: number; cleans_itself: boolean; project?: string | null; startup: Item[]; startup_tokens: number }
interface Boxed { file: string; agent: string; files: number; bytes: number; packed: number; at: number }
interface Report { agents: Store[]; boxes: Boxed[]; parked: string[]; at: number }
/** One file on the desktop: its kind, size, how long since it was changed, and (for a copy) what it copies. */
interface DeskFile { name: string; path: string; kind: string; bytes: number; days: number; same?: string | null }
/** A put-away: when, and how much went into 桌面收纳. */
interface Stowed { id: string; at: number; files: number; bytes: number; place?: string }
interface DeskReport { icons: number; files: number; old: DeskFile[]; installers: DeskFile[]; copies: DeskFile[]; big: DeskFile[]; stowed: Stowed[]; disk?: { drive: string; free: number; total: number } | null; at: number }

const $ = (id: string) => document.getElementById(id)!;
let conn: Conn;
let report: Report | null = null;
let shown = "claude";
let view: "night" | "agents" | "desktop" | "downloads" = "agents";
/** The last night's work, as petd kept it. */
interface Left { place: Place; n: number; bytes: number; old: number }
interface NightBox { file: string; agent: string; files: number; bytes: number; packed: number }
interface Night { at: number; stowed: Stowed[]; boxed: NightBox[]; left: Left[]; looked_only: boolean; days_left?: number | null }
interface NightReport { night: Night | null; disk: { drive: string; free: number; total: number } | null; days_left: number | null; kept: number; on: boolean }
let night: NightReport | null = null;
const desks: Record<"desktop" | "downloads", DeskReport | null> = { desktop: null, downloads: null };
const KINDS: Record<string, [string, string]> = {
  shot: ["截图", "Screenshots"],
  doc: ["文档", "Documents"],
  picture: ["图片", "Pictures"],
  media: ["音视频", "Audio & video"],
  archive: ["压缩包", "Archives"],
  installer: ["安装包", "Installers"],
  other: ["其他", "Other"],
};
const packing = new Map<string, { done: number; total: number }>();
const open = new Set<string>(["file", "memory", "hook"]);

const size = (b: number) => (b <= 0 ? "0" : b >= 1 << 30 ? `${(b / (1 << 30)).toFixed(1)} GB` : b >= 1 << 20 ? `${Math.round(b / (1 << 20))} MB` : `${Math.max(1, Math.round(b / 1024))} KB`);
const num = (n: number) => n.toLocaleString();
const KIND: Record<Item["kind"], [string, string]> = {
  file: ["说明文件", "Instruction files"],
  memory: ["记忆", "Memory"],
  hook: ["启动钩子", "Start-up hooks"],
  skill: ["技能", "Skills"],
  mcp: ["MCP 服务器", "MCP servers"],
};
const ORDER: Item["kind"][] = ["file", "memory", "hook", "skill", "mcp"];
const CHEV = '<svg viewBox="0 0 24 24" aria-hidden="true"><path d="M9 6l6 6-6 6"/></svg>';
const esc = (s: string) => s.replace(/[&<>"]/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;" })[c]!);

/** What the pet says it found: the biggest pile, and what every session carries for nothing. */
function said(r: Report): string {
  const heap = [...r.agents].sort((a, b) => b.old - a.old)[0];
  const parts: string[] = [];
  if (heap && heap.old >= 512 << 20) parts.push(t(`${heap.name} 的储物间堆了 ${size(heap.old)} 一个月以前的旧会话。`, `${heap.name}'s storeroom holds ${size(heap.old)} of sessions over a month old.`));
  const cc = r.agents.find((a) => a.id === "claude");
  const idle = cc ? cc.startup.filter((i) => i.unused).reduce((s, i) => s + i.tokens, 0) : 0;
  if (cc && idle >= 500) parts.push(t(`Claude 每开一个会话先读进去约 ${num(cc.startup_tokens)} 个 token，其中约 ${num(idle)} 个是一个月没用过的东西。`, `Every Claude session starts by reading about ${num(cc.startup_tokens)} tokens; about ${num(idle)} of them are things not used in a month.`));
  if (!parts.length) return t("看了一圈，还算干净。", "Had a look round. Fairly tidy.");
  return parts.join(" ");
}

function renderDisk(r: Report) {
  const box = $("disk");
  box.replaceChildren();
  for (const a of r.agents) {
    const p = packing.get(a.id);
    const el = document.createElement("div");
    el.className = "ag";
    const share = a.total ? Math.max(a.old ? 2 : 0, (a.old / a.total) * 100) : 0;
    el.innerHTML = `
      <div class="top"><b>${esc(a.name)}</b><span>${t(`整个文件夹 ${size(a.total)} · ${num(a.session_files)} 个会话`, `${size(a.total)} in all · ${num(a.session_files)} sessions`)}</span></div>
      <div class="bar" title="${t("一个月以前的旧会话占整个文件夹的比例", "Old sessions as a share of the whole folder")}"><i style="width:${share.toFixed(1)}%"></i></div>
      <div class="big">${size(a.old)}<small>${t(`是一个月以前的旧会话（${num(a.old_files)} 个）`, `of sessions over a month old (${num(a.old_files)})`)}</small></div>
      <p class="note">${a.cleans_itself ? t("Claude Code 会自己清掉一个月以前的，这里通常不多。", "Claude Code clears its month-old sessions itself, so there is rarely much here.") : t(`${a.name} 不会自己清，会一直攒着。`, `${a.name} never clears them; they just pile up.`)}</p>
      <div class="act"></div>`;
    const act = el.querySelector(".act")!;
    if (p) {
      act.innerHTML = `<div class="prog">${t(`装箱中…… ${p.done} / ${p.total}`, `Packing… ${p.done} / ${p.total}`)}<div class="bar"><i style="width:${p.total ? (p.done / p.total) * 100 : 0}%"></i></div></div>`;
    } else if (a.old >= 8 << 20) {
      const b = document.createElement("button");
      b.className = "primary";
      b.textContent = t(`把这 ${size(a.old)} 装箱`, `Pack these ${size(a.old)}`);
      b.title = t("压进储物间的一个箱子里，原文件检查无误后才移走；随时能放回去", "Into one box in the storeroom; the originals go only once the box checks out. You can always put them back.");
      b.onclick = () => void pack(a.id);
      act.append(b);
      act.insertAdjacentHTML("beforeend", `<span class="prog">${t("最近 30 天的不碰", "The last 30 days stay as they are")}</span>`);
    } else {
      act.innerHTML = `<span class="prog">${t("没什么可收的。", "Nothing worth packing.")}</span>`;
    }
    box.append(el);
  }
  if (r.boxes.length) {
    box.insertAdjacentHTML("beforeend", `<h3>${t("箱子", "Boxes")}</h3>`);
    for (const b of r.boxes) {
      const name = r.agents.find((a) => a.id === b.agent)?.name ?? b.agent;
      const d = new Date(b.at);
      const row = document.createElement("div");
      row.className = "box";
      row.innerHTML = `<span><b>${esc(name)}</b> · ${t(`${d.getMonth() + 1} 月 ${d.getDate()} 日`, d.toLocaleDateString("en-US", { month: "short", day: "numeric" }))} · ${t(`${num(b.files)} 个会话，原来 ${size(b.bytes)}，箱子 ${size(b.packed)}`, `${num(b.files)} sessions, ${size(b.bytes)} packed into ${size(b.packed)}`)}</span>`;
      const back = document.createElement("button");
      back.className = "ghost";
      back.textContent = t("放回去", "Put back");
      back.onclick = () => void unpack(b.file, back);
      row.append(back);
      box.append(row);
    }
  }
}

function renderStart(r: Report) {
  const who = $("who");
  who.replaceChildren();
  for (const a of r.agents) {
    const c = document.createElement("button");
    c.className = `chip${a.id === shown ? " on" : ""}`;
    c.textContent = a.name;
    c.onclick = () => {
      shown = a.id;
      renderStart(r);
    };
    who.append(c);
  }
  const a = r.agents.find((x) => x.id === shown) ?? r.agents[0];
  const box = $("start");
  box.replaceChildren();
  if (!a) return;
  const idle = a.startup.filter((i) => i.unused).reduce((s, i) => s + i.tokens, 0);
  box.insertAdjacentHTML(
    "beforeend",
    `<div class="sum"><span class="big">${t(`约 ${num(a.startup_tokens)} token`, `~${num(a.startup_tokens)} tokens`)}</span><small>${a.project ? t(`按最近的项目算：${esc(a.project)}`, `Counted for the latest project: ${esc(a.project)}`) : ""}</small></div>`,
  );
  if (idle > 0) box.insertAdjacentHTML("beforeend", `<div class="save">${t(`一个月没用过的占了约 ${num(idle)} 个，每个会话都能省下。`, `About ${num(idle)} of them are things unused for a month: every session could do without them.`)}</div>`);
  if (!a.startup.length) box.insertAdjacentHTML("beforeend", `<div class="waiting">${t(`${esc(a.name)} 开会话前几乎不额外读什么。`, `${esc(a.name)} reads next to nothing extra when a session starts.`)}</div>`);
  const max = Math.max(1, ...a.startup.map((i) => i.tokens));
  for (const kind of ORDER) {
    const its = a.startup.filter((i) => i.kind === kind);
    if (!its.length) continue;
    const g = document.createElement("div");
    g.className = `group${open.has(kind) ? " open" : ""}`;
    const sum = its.reduce((s, i) => s + i.tokens, 0);
    const unused = its.filter((i) => i.unused).length;
    const head = document.createElement("button");
    head.className = "gh";
    head.innerHTML = `${CHEV}<span class="k">${t(...KIND[kind])}</span><span class="n">${t(`${its.length} 个`, `${its.length}`)}${unused ? t(` · ${unused} 个一个月没用`, ` · ${unused} unused for a month`) : ""}</span><span class="t">${sum ? num(sum) : "—"}</span>`;
    head.onclick = () => {
      g.classList.toggle("open");
      if (g.classList.contains("open")) open.add(kind);
      else open.delete(kind);
    };
    const list = document.createElement("div");
    list.className = "items";
    // Skills: one word for all the unused ones, and a hand to set them aside.
    if (kind === "skill" && a.id === "claude" && unused) {
      const idle = its.filter((i) => i.unused);
      const fix = document.createElement("div");
      fix.className = "it";
      fix.innerHTML = `<div class="fix"><span>${t(`这 ${unused} 个一个月没用过，每个会话仍会先读一遍它们的说明（约 ${num(idle.reduce((s, i) => s + i.tokens, 0))} token）。可以先收进储物间，要用时一键放回。`, `These ${unused} haven't been used in a month, yet every session reads their descriptions first (~${num(idle.reduce((s, i) => s + i.tokens, 0))} tokens). Put them away in the storeroom; one click brings them back.`)}</span></div>`;
      const go = document.createElement("button");
      go.className = "primary";
      go.textContent = t(`收起这 ${unused} 个`, `Put these ${unused} away`);
      go.onclick = () => void park(idle.map((i) => i.name), false, go);
      fix.querySelector(".fix")!.append(go);
      list.append(fix);
    }
    for (const i of its) {
      const row = document.createElement("div");
      row.className = "it";
      row.innerHTML = `<div class="row"><span class="nm" title="${esc(i.path ?? i.name)}">${esc(i.name)}</span>${i.unused ? `<span class="tag">${t("一个月没用", "unused a month")}</span>` : ""}<span class="tk">${i.tokens ? `<span class="bar"><i style="width:${((i.tokens / max) * 100).toFixed(1)}%"></i></span>${num(i.tokens)}` : t("按需加载", "on demand")}</span></div>`;
      if (i.fix && !(kind === "skill" && a.id === "claude")) {
        const fix = document.createElement("div");
        fix.className = "fix";
        fix.innerHTML = `<span>${esc(i.fix)}</span>`;
        const cmd = i.fix.match(/claude mcp remove .+$/)?.[0];
        if (cmd) {
          const copy = document.createElement("button");
          copy.textContent = t("复制命令", "Copy command");
          copy.onclick = () => void navigator.clipboard.writeText(cmd).then(() => (copy.textContent = t("复制好了", "Copied")));
          fix.append(copy);
        }
        row.append(fix);
      }
      list.append(row);
    }
    g.append(head, list);
    box.append(g);
  }
  // Skills put away earlier: each can come back.
  if (a.id === "claude" && r.parked.length) {
    const g = document.createElement("div");
    g.className = "group open";
    g.innerHTML = `<div class="gh"><span class="k">${t("收在储物间的技能", "Skills put away")}</span><span class="n">${t(`${r.parked.length} 个，新会话不读`, `${r.parked.length}, not read by new sessions`)}</span></div>`;
    const list = document.createElement("div");
    list.className = "items";
    for (const name of r.parked) {
      const row = document.createElement("div");
      row.className = "it";
      row.innerHTML = `<div class="row"><span class="nm">${esc(name)}</span></div>`;
      const back = document.createElement("button");
      back.className = "ghost";
      back.textContent = t("放回去", "Put back");
      back.onclick = () => void park([name], true, back);
      row.querySelector(".row")!.append(back);
      list.append(row);
    }
    g.append(list);
    box.append(g);
  }
}

async function park(names: string[], back: boolean, btn: HTMLButtonElement) {
  btn.disabled = true;
  const r = await postJson<{ ok: boolean; moved?: number; message?: string }>(conn, "/v1/storeroom/park", { names, back }).catch((e) => ({ ok: false, message: String(e) }) as { ok: boolean; moved?: number; message?: string });
  await load(true);
  $("said").textContent = !r.ok
    ? t(`没挪成：${r.message ?? ""}`, `Couldn't move them: ${r.message ?? ""}`)
    : back
      ? t("放回去了。下一个会话又会读到它。", "Back in place. The next session will read it again.")
      : t(`收好了 ${r.moved ?? 0} 个。下一个会话开始就不用先读它们了，要用时在这里放回去。`, `Put ${r.moved ?? 0} away. From the next session on they aren't read first; bring any back from here.`);
}

type Place = "desktop" | "downloads";
type View = "night" | "agents" | Place;
const PLACES: Place[] = ["desktop", "downloads"];
const placeName = (p: Place) => (p === "desktop" ? t("桌面", "Desktop") : t("下载", "Downloads"));
const drawerName = (p: Place) => (p === "desktop" ? t("桌面收纳", "Desktop put-away") : t("下载收纳", "Downloads put-away"));
const spare = (d: DeskReport) => [...d.installers, ...d.copies].reduce((s, f) => s + f.bytes, 0);
const bigBytes = (d: DeskReport) => d.big.reduce((s, f) => s + f.bytes, 0);

/** A button that asks once more before it does something (the second click does it). */
function sure(b: HTMLButtonElement, ask: string, go: () => Promise<void>) {
  const label = b.textContent ?? "";
  let armed = 0;
  b.onclick = async () => {
    if (!armed) {
      b.textContent = ask;
      b.classList.add("sure");
      armed = window.setTimeout(() => {
        armed = 0;
        b.textContent = label;
        b.classList.remove("sure");
      }, 4000);
      return;
    }
    clearTimeout(armed);
    b.disabled = true;
    await go();
  };
}

/** The desktop or Downloads: what could be put away, and what could go. */
function renderDesk(d: DeskReport, place: Place) {
  $("stow-sub").textContent =
    place === "desktop"
      ? t("很久没改过的文件，收进桌面上的一个文件夹", "Files untouched for weeks go into one folder on the desktop")
      : t("很久没改过的文件，收进下载文件夹里的一个文件夹", "Files untouched for weeks go into one folder inside Downloads");
  const stow = $("stow");
  stow.replaceChildren();
  const oldBytes = d.old.reduce((s, f) => s + f.bytes, 0);
  const el = document.createElement("div");
  el.className = "ag";
  const share = d.files ? (d.old.length / d.files) * 100 : 0;
  const kinds = new Map<string, number>();
  for (const f of d.old) kinds.set(f.kind, (kinds.get(f.kind) ?? 0) + 1);
  el.innerHTML = `
    <div class="top"><b>${placeName(place)}</b><span>${place === "desktop" ? t(`${num(d.icons)} 个图标 · ${num(d.files)} 个文件`, `${num(d.icons)} icons · ${num(d.files)} files`) : t(`${num(d.files)} 个文件`, `${num(d.files)} files`)}</span></div>
    <div class="bar" title="${t("两周以上没改过的文件，占全部文件的比例", "Files unchanged for two weeks or more, as a share of all files")}"><i style="width:${share.toFixed(1)}%"></i></div>
    <div class="big">${t(`${num(d.old.length)} 个文件`, `${num(d.old.length)} files`)}<small>${t(`两周以上没改过，共 ${size(oldBytes)}`, `unchanged for two weeks or more, ${size(oldBytes)} in all`)}</small></div>
    <div class="kinds">${[...kinds].sort((a, b) => b[1] - a[1]).map(([k, n]) => `<span>${t(...(KINDS[k] ?? KINDS.other))}<b>${num(n)}</b></span>`).join("")}</div>
    <p class="note">${t("快捷方式、文件夹、程序、两周内改过的文件都不动。收进去的按类型分开放，截图再按月份分。", "Shortcuts, folders, programs and anything changed in the last two weeks stay. What goes in is sorted by type; screenshots also by month.")}</p>
    <div class="act"></div>`;
  const act = el.querySelector(".act")!;
  if (d.old.length) {
    const b = document.createElement("button");
    b.className = "primary";
    b.textContent = t(`收进「${drawerName(place)}」`, `Put into “${drawerName(place)}”`);
    b.onclick = () => void stowAll(place, b);
    act.append(b);
    act.insertAdjacentHTML("beforeend", `<span class="prog">${t("随时可以放回原处", "Can always be put back")}</span>`);
  } else {
    act.innerHTML = `<span class="prog">${t("没有要收的。", "Nothing to put away.")}</span>`;
  }
  stow.append(el);
  if (d.stowed.length) {
    stow.insertAdjacentHTML("beforeend", `<h3>${t("收过的", "Put away before")}</h3>`);
    for (const s of d.stowed) {
      const dt = new Date(s.at);
      const row = document.createElement("div");
      row.className = "box";
      row.innerHTML = `<span><b>${t(`${dt.getMonth() + 1} 月 ${dt.getDate()} 日`, dt.toLocaleDateString("en-US", { month: "short", day: "numeric" }))}</b> · ${t(`${num(s.files)} 个文件，${size(s.bytes)}`, `${num(s.files)} files, ${size(s.bytes)}`)}</span>`;
      const back = document.createElement("button");
      back.className = "ghost";
      back.textContent = t("放回原处", "Put back");
      back.onclick = () => void unstow(place, s.id, back);
      row.append(back);
      stow.append(row);
    }
  }

  const clean = $("clean");
  clean.replaceChildren();
  const group = (title: [string, string], files: DeskFile[], about: (f: DeskFile) => string, foot: string, bin: boolean) => {
    if (!files.length) return;
    const g = document.createElement("div");
    g.className = "group open";
    const bytes = files.reduce((s, f) => s + f.bytes, 0);
    g.innerHTML = `<button class="gh">${CHEV}<span class="k">${t(...title)}</span><span class="n">${t(`${num(files.length)} 个`, `${num(files.length)}`)}</span><span class="t">${size(bytes)}</span></button>`;
    g.querySelector(".gh")!.addEventListener("click", () => g.classList.toggle("open"));
    const list = document.createElement("div");
    list.className = "items";
    for (const f of files) {
      const row = document.createElement("div");
      row.className = "it";
      row.innerHTML = `<div class="row"><span class="nm" title="${esc(f.path)}">${esc(f.name)}</span><span class="age">${esc(about(f))}</span><span class="sz">${size(f.bytes)}</span></div>`;
      // Every file can be looked at where it is.
      const look = document.createElement("button");
      look.className = "ghost";
      look.textContent = t("显示", "Show");
      look.title = t("在文件夹中显示", "Show in folder");
      look.onclick = () => void postJson(conn, "/v1/drawer/reveal", { place, path: f.path }).catch(() => {});
      row.querySelector(".row")!.append(look);
      list.append(row);
    }
    const doit = document.createElement("div");
    doit.className = "do";
    doit.innerHTML = `<span>${foot}</span>`;
    if (bin) {
      const b = document.createElement("button");
      b.textContent = files.some((f) => f.same) ? t("把副本移到回收站", "Move the copies to the Recycle Bin") : t("移到回收站", "Move to Recycle Bin");
      sure(b, t(`确定，${num(files.length)} 个移到回收站`, `Yes, move ${num(files.length)} to the bin`), () => recycle(place, files));
      doit.append(b);
    }
    list.append(doit);
    g.append(list);
    clean.append(g);
  };
  const drive = (d.disk?.drive ?? "C:").replace(":", "");
  group(["装好以后用不上的安装包", "Installers already used"], d.installers, (f) => t(`放了 ${f.days} 天`, `${f.days} days here`), t("装完的安装包可以删掉，要用时再下载。", "Once installed, the installer can go; download it again if needed."), true);
  group(["重复的文件", "Duplicates"], d.copies, (f) => t(`和「${f.same ?? ""}」一样`, `same as “${f.same ?? ""}”`), t("内容和原件完全一样，只删副本，原件留着。", "Byte for byte the same as the original: only the copies go, the originals stay."), true);
  group(["很大的文件", "Large files"], d.big, (f) => t(`${f.days} 天没改过`, `unchanged ${f.days} days`), t(`${placeName(place)}在 ${drive} 盘上。${drive} 盘吃紧时，这些可以挪到别的盘。`, `${placeName(place)} lives on drive ${drive}. When it runs short, these could move to another drive.`), false);
  if (!clean.children.length) clean.innerHTML = `<div class="waiting">${t("没有看到可以清掉的。", "Nothing that could go.")}</div>`;
}

function deskSaid(d: DeskReport, place: Place): string {
  const where = placeName(place);
  if (d.old.length >= 20) return t(`${where}我看过了。很多东西，两个星期没人碰过。`, `I've looked over ${where}. A lot of it hasn't been touched in two weeks.`);
  if (d.installers.length || d.copies.length) return t(`${where}我看过了。有几样，可以不留。`, `I've looked over ${where}. A few things needn't stay.`);
  return t(`${where}我看过了。还算清爽。`, `I've looked over ${where}. Fairly clear.`);
}

/** The drive the folders are on: how full it is and, when it runs short, what could be freed where. */
function renderDrive() {
  const box = $("drive");
  const disk = PLACES.map((p) => desks[p]?.disk).find(Boolean);
  if (!disk) return void (box.hidden = true);
  const short = disk.free * 10 < disk.total || disk.free < 10 * (1 << 30);
  const drive = disk.drive.replace(":", "");
  const parts: [string, number, View][] = [];
  const agentsOld = report?.agents.reduce((s, a) => s + a.old, 0) ?? 0;
  if (agentsOld >= 64 << 20) parts.push([t("agent 旧会话", "old agent sessions"), agentsOld, "agents"]);
  for (const p of PLACES) {
    const d = desks[p];
    const n = d ? spare(d) + bigBytes(d) : 0;
    if (n >= 64 << 20) parts.push([placeName(p), n, p]);
  }
  box.className = `drive${short ? " short" : ""}`;
  box.innerHTML = `<span>${t(`${esc(drive)} 盘剩 <b class="free">${size(disk.free)}</b>，共 ${size(disk.total)}`, `Drive ${esc(drive)}: <b class="free">${size(disk.free)}</b> free of ${size(disk.total)}`)}</span>`;
  if (short && parts.length) {
    box.insertAdjacentHTML("beforeend", `<span>${t("能腾的：", "Could free:")}</span>`);
    for (const [name, bytes, v] of parts.sort((a, b) => b[1] - a[1])) {
      const b = document.createElement("button");
      b.className = "ghost";
      b.textContent = `${name} ${size(bytes)}`;
      b.onclick = () => setView(v);
      box.append(b);
    }
  }
  box.hidden = false;
}

function setView(v: View) {
  view = v;
  $("v-night").classList.toggle("on", v === "night");
  $("m-night").hidden = $("f-night").hidden = v !== "night";
  if (v === "night") {
    for (const id of ["m-agents", "f-agents", "m-desk", "f-desk"]) $(id).hidden = true;
    for (const id of ["v-agents", "v-desk", "v-dl"]) $(id).classList.remove("on");
    if (night) renderNight(night);
    else void loadNight();
    return;
  }
  $("v-agents").classList.toggle("on", v === "agents");
  $("v-desk").classList.toggle("on", v === "desktop");
  $("v-dl").classList.toggle("on", v === "downloads");
  const desky = v !== "agents";
  for (const [id, on] of [["m-agents", !desky], ["f-agents", !desky], ["m-desk", desky], ["f-desk", desky]] as const) ($(id) as HTMLElement).hidden = !on;
  if (v === "agents") {
    if (report) render();
    return;
  }
  const d = desks[v];
  if (d) {
    $("said").textContent = deskSaid(d, v);
    renderDesk(d, v);
    return;
  }
  $("said").textContent = t("我在看……", "Having a look…");
  $("stow").innerHTML = `<div class="waiting"><div class="dots"><i></i><i></i><i></i></div><span>${t("在清点……", "Counting…")}</span></div>`;
  $("clean").replaceChildren();
  void loadDesk(v);
}

/** How long the space lasts, in words. */
function runway(days: number): string {
  if (days >= 730) return t("照最近的用法，两年以上都够用。", "At the recent pace, it lasts more than two years.");
  if (days >= 60) return t(`照最近的用法，大约还能用 ${Math.round(days / 30)} 个月。`, `At the recent pace, about ${Math.round(days / 30)} months left.`);
  return t(`照最近的用法，大约 ${days} 天就满了。`, `At the recent pace, full in about ${days} days.`);
}

/** 昨晚: what it did while you were away (each can go back), and what waits for you. */
function renderNight(r: NightReport) {
  const n = r.night;
  const when = n ? new Date(n.at) : null;
  const did = $("did");
  did.replaceChildren();
  if (!n) {
    did.innerHTML = `<div class="waiting">${t("还没有过夜。你晚上离开电脑以后，它才开始。", "No night yet. It starts once you leave the computer in the evening.")}</div>`;
  } else if (!n.stowed.length && !n.boxed.length) {
    did.innerHTML = `<div class="waiting">${n.looked_only ? t("这次只看了，没动东西。", "This time it only looked; nothing was moved.") : t("昨晚没有要收的。", "Nothing needed putting away last night.")}</div>`;
  } else {
    did.insertAdjacentHTML("beforeend", `<h3>${t(`${when!.getMonth() + 1} 月 ${when!.getDate()} 日 ${String(when!.getHours()).padStart(2, "0")}:${String(when!.getMinutes()).padStart(2, "0")}`, when!.toLocaleString("en-US", { month: "short", day: "numeric", hour: "2-digit", minute: "2-digit" }))}</h3>`);
    for (const s of n.stowed) {
      const place = (s.place ?? "desktop") as Place;
      const row = document.createElement("div");
      row.className = "box";
      row.innerHTML = `<span>${t(`<b>${placeName(place)}</b>：${num(s.files)} 个很久没碰的文件收进了「${drawerName(place)}」`, `<b>${placeName(place)}</b>: ${num(s.files)} untouched files went into “${drawerName(place)}”`)}</span>`;
      const back = document.createElement("button");
      back.className = "ghost";
      back.textContent = t("放回原处", "Put back");
      back.onclick = async () => {
        back.disabled = true;
        const a = await postJson<Answer & { back?: number }>(conn, "/v1/drawer/unstow", { id: s.id }).catch(failed);
        n.stowed = n.stowed.filter((x) => x.id !== s.id);
        desks[place] = null;
        renderNight(r);
        $("said").textContent = a.ok
          ? t(`放回去了。${placeName(place)}我先不动了，只看着，有要收的再问你。`, `Put back. I'll leave ${placeName(place)} alone and only look; I'll ask before putting anything away there.`)
          : t(`没放回去：${a.message ?? ""}`, `Couldn't put back: ${a.message ?? ""}`);
      };
      row.append(back);
      did.append(row);
    }
    for (const b of n.boxed) {
      const row = document.createElement("div");
      row.className = "box";
      row.innerHTML = `<span>${t(`<b>${esc(b.agent === "claude" ? "Claude Code" : "Codex")}</b>：${num(b.files)} 个一个月以前的会话装了箱，腾出 ${size(b.bytes - b.packed)}`, `<b>${esc(b.agent === "claude" ? "Claude Code" : "Codex")}</b>: ${num(b.files)} month-old sessions boxed, ${size(b.bytes - b.packed)} freed`)}</span>`;
      const back = document.createElement("button");
      back.className = "ghost";
      back.textContent = t("放回去", "Put back");
      back.onclick = async () => {
        back.disabled = true;
        await unpack(b.file, back);
        n.boxed = n.boxed.filter((x) => x.file !== b.file);
        renderNight(r);
      };
      row.append(back);
      did.append(row);
    }
  }
  if (r.kept) did.insertAdjacentHTML("beforeend", `<p class="note" style="margin:0;font-size:12px;color:var(--muted)">${t(`你放回去的 ${num(r.kept)} 个文件，我记着，不会再收。`, `The ${num(r.kept)} files you put back are remembered and won't be put away again.`)}</p>`);

  const wait = $("wait");
  wait.replaceChildren();
  if (r.disk) {
    const d = r.disk;
    const drive = d.drive.replace(":", "");
    const box = document.createElement("div");
    box.className = "runway";
    box.innerHTML = `<div class="top"><b>${t(`${esc(drive)} 盘`, `Drive ${esc(drive)}`)}</b></div>
      <div class="bar"><i style="width:${(((d.total - d.free) / d.total) * 100).toFixed(1)}%"></i></div>
      <div class="big">${t(`剩 ${size(d.free)}`, `${size(d.free)} free`)}<small>${t(`共 ${size(d.total)}`, `of ${size(d.total)}`)}</small></div>
      <p>${r.days_left != null ? runway(r.days_left) : t("每天记一次剩多少，记满一周就能估出还能用多久。最近没见它变少。", "It notes the free space once a day; after a week it can tell how long it lasts. It hasn't been shrinking lately.")}</p>`;
    wait.append(box);
  }
  const lefts = n?.left.filter((l) => l.n || l.old) ?? [];
  for (const l of lefts) {
    const row = document.createElement("div");
    row.className = "left";
    const parts: string[] = [];
    if (l.n) parts.push(t(`${num(l.n)} 个用不上的安装包或副本，${size(l.bytes)}`, `${num(l.n)} spent installers or copies, ${size(l.bytes)}`));
    if (l.old) parts.push(t(`${num(l.old)} 个很久没碰的文件没收（我在等你说）`, `${num(l.old)} untouched files left as they are (waiting for your word)`));
    row.innerHTML = `<span><b>${placeName(l.place)}</b>：${parts.join(t("；", "; "))}</span>`;
    const go = document.createElement("button");
    go.textContent = t("去看看", "Have a look");
    go.onclick = () => setView(l.place);
    row.append(go);
    wait.append(row);
  }
  if (!lefts.length && !r.disk) wait.innerHTML = `<div class="waiting">${t("没有等你定的。", "Nothing waiting for you.")}</div>`;
  $("said").textContent = !n
    ? t("我晚上会来收拾。", "I'll tidy up in the evening.")
    : n.stowed.length || n.boxed.length
      ? t("昨晚你走后，我把能放回的先收了。删东西的事，等你定。", "After you left last night I put away what can go back. Deleting waits for you.")
      : lefts.length
        ? t("昨晚看了一圈。有几样，等你定。", "I looked around last night. A few things wait for you.")
        : t("昨晚看了一圈，都还好。", "I looked around last night. All fine.");
}

async function loadNight() {
  const r = await getJson<NightReport>(conn, "/v1/night").catch(() => null);
  if (!r) return;
  night = r;
  $("v-night").hidden = !r.night;
  if (view === "night") renderNight(r);
}

type Answer = { ok?: boolean; message?: string };
const failed = (e: unknown) => ({ ok: false, message: String(e) });
const loading = new Set<Place>();

async function loadDesk(place: Place) {
  if (loading.has(place)) return;
  loading.add(place);
  const r = await getJson<DeskReport & Answer>(conn, `/v1/drawer?place=${place}`).catch(failed);
  loading.delete(place);
  if (!("old" in r) || r.ok === false) {
    if (view === place) $("said").textContent = t(`${placeName(place)}打不开：${r.message ?? ""}`, `Couldn't open ${placeName(place)}: ${r.message ?? ""}`);
    return;
  }
  desks[place] = r;
  renderDrive();
  if (view === place) setView(place);
}

async function stowAll(place: Place, b: HTMLButtonElement) {
  b.disabled = true;
  b.textContent = t("在收……", "Putting away…");
  const r = await postJson<Answer & { done?: { moved: number; skipped: number } }>(conn, "/v1/drawer/stow", { place }).catch(failed);
  desks[place] = null;
  await loadDesk(place);
  const done = "done" in r ? r.done : undefined;
  $("said").textContent = !r.ok
    ? t(`没收成：${r.message ?? ""}`, `Couldn't put them away: ${r.message ?? ""}`)
    : t(`收好了 ${done?.moved ?? 0} 个，都在「${drawerName(place)}」里，按类分开放的。`, `Put ${done?.moved ?? 0} away in “${drawerName(place)}”, sorted by type.`) +
      (done?.skipped ? t(` 有 ${done.skipped} 个正被别的程序开着，没动。`, ` ${done.skipped} were open in another program and stayed.`) : "");
}

async function unstow(place: Place, id: string, b: HTMLButtonElement) {
  b.disabled = true;
  const r = await postJson<Answer & { back?: number; stuck?: number }>(conn, "/v1/drawer/unstow", { id }).catch(failed);
  desks[place] = null;
  await loadDesk(place);
  const back = "back" in r ? r.back : 0;
  const stuck = "stuck" in r ? r.stuck : 0;
  $("said").textContent = !r.ok
    ? t(`没放回去：${r.message ?? ""}`, `Couldn't put back: ${r.message ?? ""}`)
    : t(`放回去了，${back ?? 0} 个回到了原处。`, `Put back: ${back ?? 0} are where they were.`) +
      (stuck ? t(` 有 ${stuck} 个的原处已经有同名文件，还留在收纳里。`, ` ${stuck} have a file of the same name in their place and stay in the folder.`) : "");
}

async function recycle(place: Place, files: DeskFile[]) {
  const r = await postJson<Answer & { n?: number }>(conn, "/v1/drawer/recycle", { place, paths: files.map((f) => f.path) }).catch(failed);
  desks[place] = null;
  await loadDesk(place);
  const n = "n" in r ? r.n : 0;
  $("said").textContent = r.ok
    ? t(`${n ?? 0} 个移到了回收站。反悔的话，在回收站里还原就行。`, `${n ?? 0} went to the Recycle Bin. Restore them from there if you change your mind.`)
    : t(`没移成：${r.message ?? ""}`, `Couldn't move them: ${r.message ?? ""}`);
}

function render() {
  if (view !== "agents") return setView(view);
  if (!report) return;
  $("said").textContent = said(report);
  renderDisk(report);
  renderStart(report);
}

async function load(fresh = false) {
  if (fresh) $("said").textContent = t("我再看一遍……", "Having another look…");
  report = await getJson<Report>(conn, `/v1/storeroom${fresh ? "?fresh=1" : ""}`);
  if (!report.agents.some((a) => a.id === shown)) shown = report.agents[0]?.id ?? "claude";
  render();
  renderDrive();
}

async function pack(agent: string) {
  packing.set(agent, { done: 0, total: 0 });
  render();
  const r = await postJson<{ ok: boolean; message?: string; box?: Boxed }>(conn, "/v1/storeroom/pack", { agent }).catch((e) => ({ ok: false, message: String(e) }) as { ok: boolean; message?: string; box?: Boxed });
  packing.delete(agent);
  await load(true);
  $("said").textContent = r.ok && r.box
    ? t(`装好了。腾出 ${size(r.box.bytes - r.box.packed)}，箱子只有 ${size(r.box.packed)}，想要随时放回去。`, `Done. ${size(r.box.bytes - r.box.packed)} freed; the box is only ${size(r.box.packed)}, and it can all go back whenever you like.`)
    : t(`没装成：${r.message ?? ""}`, `Couldn't pack: ${r.message ?? ""}`);
}

async function unpack(file: string, btn: HTMLButtonElement) {
  btn.disabled = true;
  btn.textContent = t("放回去中……", "Putting back…");
  const r = await postJson<{ ok: boolean; message?: string; files?: number }>(conn, "/v1/storeroom/unpack", { file }).catch((e) => ({ ok: false, message: String(e) }) as { ok: boolean; message?: string; files?: number });
  await load(true);
  $("said").textContent = r.ok ? t(`放回去了，${r.files ?? 0} 个会话回到了原处。`, `Put back: ${r.files ?? 0} sessions are where they were.`) : t(`没放回去：${r.message ?? ""}`, `Couldn't put back: ${r.message ?? ""}`);
}

/** In a browser, `?demo=desk`: a desktop that has been used for a year, for looking at the page. */
function demoDesk(): DeskReport {
  const f = (name: string, kind: string, mb: number, days: number, same?: string): DeskFile => ({ name, path: `C:\\Users\\you\\Desktop\\${name}`, kind, bytes: mb * (1 << 20), days, same });
  const old: DeskFile[] = [];
  for (let i = 0; i < 31; i++) old.push(f(`截图 2026-0${1 + (i % 9)}-1${i % 10}.png`, "shot", 0.6, 20 + i * 7));
  for (let i = 0; i < 18; i++) old.push(f(`文档 ${i}.docx`, "doc", 0.3, 30 + i * 5));
  for (let i = 0; i < 9; i++) old.push(f(`安装包 ${i}.exe`, "installer", 120, 40 + i * 9));
  for (let i = 0; i < 7; i++) old.push(f(`资料 ${i}.zip`, "archive", 60, 50 + i * 3));
  for (let i = 0; i < 6; i++) old.push(f(`图片 ${i}.jpg`, "picture", 3, 60));
  for (let i = 0; i < 3; i++) old.push(f(`录音 ${i}.m4a`, "media", 40, 90));
  return {
    icons: 126,
    files: 112,
    old,
    installers: [f("安装包 v2.3.exe", "installer", 182, 41), f("setup_x64.msi", "installer", 96, 73), f("驱动.exe", "installer", 410, 120), f("输入法安装.exe", "installer", 88, 205)],
    copies: [f("报价单 (1).xlsx", "doc", 0.2, 33, "报价单.xlsx"), f("合同终版 (1).pdf", "doc", 4, 12, "合同终版.pdf"), f("海报 - 副本.psd", "picture", 186, 64, "海报.psd")],
    big: [f("演示录屏.mp4", "media", 2350, 47), f("旧项目备份.7z", "archive", 3900, 160), f("素材合集.zip", "archive", 980, 88)],
    stowed: [{ id: "a", at: Date.now() - 9 * 86_400_000, files: 46, bytes: 830 << 20 }],
    at: Date.now(),
  };
}

async function main() {
  $("v-night").onclick = () => setView("night");
  $("v-agents").onclick = () => setView("agents");
  $("v-desk").onclick = () => setView("desktop");
  $("v-dl").onclick = () => setView("downloads");
  if (new URLSearchParams(location.search).get("demo") === "desk") {
    applySkin("zimeng");
    void mountPortrait($("pet") as HTMLCanvasElement, "rig/zimeng/").catch(() => {});
    desks.desktop = demoDesk();
    return setView("desktop");
  }
  conn = await connect();
  await initLang(conn);
  const sk = applySkin((await skinFor(conn)).skin.id);
  followSkin(conn);
  void mountPortrait($("pet") as HTMLCanvasElement, sk.rig ?? undefined).catch(() => {});
  subscribe(conn, (type, data) => {
    if (type !== "storeroom") return;
    const d = data as { agent: string; done: number; total: number };
    packing.set(d.agent, { done: d.done, total: d.total });
    if (report) renderDisk(report);
  });
  $("again").onclick = () => {
    if (view === "agents") return void load(true);
    if (view === "night") return void loadNight();
    desks[view] = null;
    setView(view);
  };
  // Opened from a pet's offer: the desktop, Downloads, or "disk" (wherever the most could be freed).
  const go = async (tab: string | null) => {
    if (tab === "desktop" || tab === "downloads" || tab === "night") return setView(tab);
    if (tab !== "disk") return;
    await Promise.all(PLACES.map((p) => (desks[p] ? null : loadDesk(p))));
    const free = (p: Place) => {
      const d = desks[p];
      return d ? spare(d) + d.big.reduce((s, f) => s + f.bytes, 0) : 0;
    };
    const agentsOld = report?.agents.reduce((s, a) => s + a.old, 0) ?? 0;
    const options: [View, number][] = [["agents", agentsOld], ["desktop", free("desktop")], ["downloads", free("downloads")]];
    setView(options.sort((a, b) => b[1] - a[1])[0][0]);
  };
  window.addEventListener("message", (e) => {
    if (e.origin === location.origin && e.data?.type === "goto-tab") void go(String(e.data.payload ?? ""));
  });
  // Opened on a folder's view: that comes up at once; the agents are counted meanwhile.
  const first = new URLSearchParams(location.search).get("tab");
  if (first !== "disk") void go(first);
  for (const p of PLACES) if (!desks[p]) void loadDesk(p);
  void loadNight();
  await load();
  if (first === "disk") void go(first);
}

main().catch((e) => {
  $("said").textContent = t("储物间打不开：", "The storeroom didn't open: ") + String(e);
});
