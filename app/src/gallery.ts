// Gallery: browse packs, favorite them, pick one per camp or per project,
// switch routing mode, see broken packs with reasons.

import {
  connect, duration, getJson, inTauri, postJson, RARITY_COLORS, RARITY_NAMES, subscribe, targetName,
  type BrokenPack, type ConfigInfo, type Conn, type PackInfo, type Report,
} from "./api";
import { Previews, toast } from "./preview";
import { drawHat, drawWearBehind, drawWearFront, fitOf, ropeStyle, SLOT_NAMES, SLOTS } from "./decor";
import type { Sprites } from "./sprites";
import { EN, initLang, t as tr } from "./i18n"; // `t` is a common local name here
import { IMAGE_EXT, setupMaker } from "./maker-ui";

const MODES = (): [string, string, string][] => [
  ["per-camp", tr("按阵营", "Per team"), tr("每个阵营一只主宠物，每个会话一个小分身跟在旁边（默认）", "One main pet per team, with a mini for each session beside it (default)")],
  ["single", tr("单宠物", "Single"), tr("只有一只宠物，显示所有会话的聚合状态", "Just one pet, showing the combined state of all sessions")],
  ["per-rule", tr("按规则", "Per rule"), tr("config.yaml 里每条 pets 规则一只宠物，适合按项目分配", "One pet per pets rule in config.yaml, handy for assigning by project")],
  ["per-session", tr("按会话", "Per session"), tr("每个会话一只完整的宠物，最多 max_pets 只", "A full pet for every session, up to max_pets")],
];

const $ = (id: string) => document.getElementById(id)!;
let conn: Conn;
let previews: Previews;
let packs: PackInfo[] = [];
let broken: BrokenPack[] = [];
let rev = 0;
let cfg: ConfigInfo;
let tagFilter = "";

function esc(s: string): string {
  return s.replace(/[&<>"]/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;" })[c]!);
}

function packName(id: string): string {
  return packs.find((p) => p.id === id)?.name ?? id;
}

async function load() {
  const [p, c] = await Promise.all([
    getJson<{ packs: PackInfo[]; broken: BrokenPack[]; rev: number }>(conn, "/v1/packs"),
    getJson<ConfigInfo>(conn, "/v1/config"),
  ]);
  const packsChanged = p.rev !== rev || JSON.stringify(p.packs) !== JSON.stringify(packs);
  const cfgChanged = JSON.stringify(c) !== JSON.stringify(cfg);
  packs = p.packs;
  broken = p.broken;
  rev = p.rev;
  cfg = c;
  if (packsChanged || cfgChanged) await render(packsChanged);
}

async function act(path: string, body: unknown, ok?: string) {
  const r = await postJson(conn, path, body);
  if (!r.ok) toast(r.message || tr("操作失败", "Something went wrong"));
  else if (ok ?? r.message) toast(ok ?? r.message);
  await load();
}

async function render(rebuildGrid: boolean) {
  $("error").innerHTML = cfg.error ? `<div class="banner">${tr("config.yaml 有错误，暂时用上一次的配置：", "config.yaml has an error; using the last good config for now: ")}${esc(cfg.error)}</div>` : "";

  // Modes.
  $("modes").innerHTML = MODES().map(([id, name]) => `<button data-mode="${id}" class="${cfg.mode === id ? "on" : ""}">${name}</button>`).join("");
  $("modehint").textContent = MODES().find((m) => m[0] === cfg.mode)?.[2] ?? "";
  $("modes").querySelectorAll<HTMLButtonElement>("button").forEach((b) => {
    b.onclick = () => act("/v1/mode", { mode: b.dataset.mode });
  });

  // Current picks.
  const cur: string[] = [
    `<div><span>${tr("Claude Code 阵营", "Claude Code team")}</span><b>${esc(packName(cfg.in_use["claude-code"]))}</b></div>`,
    `<div><span>${tr("Codex 阵营", "Codex team")}</span><b>${esc(packName(cfg.in_use.codex))}</b></div>`,
  ];
  for (const [proj, pack] of Object.entries(cfg.selection.projects ?? {})) {
    cur.push(`<div><span>${tr("项目", "Project")} ${esc(proj)}</span><span><b>${esc(packName(pack))}</b>
      <button class="ghost" data-unproj="${esc(proj)}" title="${tr("恢复用阵营宠物", "Go back to the team pet")}">✕</button></span></div>`);
  }
  $("current").innerHTML = cur.join("");
  $("current").querySelectorAll<HTMLButtonElement>("[data-unproj]").forEach((b) => {
    b.onclick = () => act("/v1/unselect", { target: `project:${b.dataset.unproj}` });
  });

  // Tag filter.
  const tags = [...new Set(packs.flatMap((p) => p.tags))].sort();
  const chip = (id: string, label: string) => `<span class="chip ${tagFilter === id ? "on" : ""}" data-tag="${esc(id)}">${esc(label)}</span>`;
  $("tags").innerHTML = [chip("", tr("全部", "All")), chip("★", tr("★ 收藏", "★ Favorites")), ...tags.map((t) => chip(t, t))].join("");
  $("tags").querySelectorAll<HTMLElement>(".chip").forEach((c) => {
    c.onclick = () => {
      tagFilter = c.dataset.tag ?? "";
      void render(true);
    };
  });

  // Broken packs.
  $("broken-wrap").hidden = broken.length === 0;
  $("broken").innerHTML = broken
    .map((b) => `<li><b>${esc(b.id ?? b.dir)}</b>：${b.errors.map(esc).join("；")}<br><code>${esc(b.dir)}</code></li>`)
    .join("");

  if (rebuildGrid) await renderGrid();
  else updateBadges();
}

function visiblePacks(): PackInfo[] {
  const favs = cfg.selection.favorites ?? [];
  return packs.filter((p) => !tagFilter || (tagFilter === "★" ? favs.includes(p.id) : p.tags.includes(tagFilter)));
}

let gridGen = 0;
async function renderGrid() {
  // Renders can overlap (a reload while previews are loading): only the
  // newest one may touch the grid, or cards get added twice.
  const gen = ++gridGen;
  previews.clear();
  const grid = $("grid");
  grid.innerHTML = "";
  const list = visiblePacks();
  if (!list.length) grid.innerHTML = `<div class="meta">${tr("没有符合条件的宠物包。", "No packs match.")}</div>`;
  for (const p of list) {
    const card = document.createElement("div");
    card.className = "card";
    card.dataset.pack = p.id;
    const canvas = await previews.mount(p.id, rev);
    if (gen !== gridGen) return;
    card.appendChild(canvas);
    const body = document.createElement("div");
    body.innerHTML = `
      <div class="inuse"></div>
      ${p.builtin ? "" : `<button class="del ghost" title="${tr("删除这个导入的宠物包", "Delete this imported pack")}">${tr("删除", "Delete")}</button>`}
      <div class="title"><b>${esc(p.name || p.id)}</b><button class="star" title="${tr("收藏", "Favorite")}">★</button></div>
      <div class="meta">${esc(p.author || "")}${p.version ? ` · v${esc(p.version)}` : ""}${p.builtin ? tr(" · 内置", " · built-in") : ""}</div>
      <div>${p.tags.map((t) => `<span class="tag">${esc(t)}</span>`).join("")}</div>
      <div class="actions">
        <button data-t="claude-code">${tr("给 Claude Code", "For Claude Code")}</button>
        <button data-t="codex">${tr("给 Codex", "For Codex")}</button>
      </div>
      <div class="projpick">
        <input list="projects-seen" placeholder="${tr("项目名，可用 * 通配", "project (* ok)")}" size="14" />
        <button class="proj">${tr("给项目", "For project")}</button>
      </div>`;
    card.appendChild(body);
    grid.appendChild(card);

    const del = body.querySelector<HTMLButtonElement>(".del");
    if (del) {
      let armed = 0;
      del.onclick = async () => {
        // Two clicks within 3 s, so a stray click never deletes anything.
        if (!armed) {
          del.textContent = tr("再点一次确认", "Click again to confirm");
          armed = window.setTimeout(() => {
            armed = 0;
            del.textContent = tr("删除", "Delete");
          }, 3000);
          return;
        }
        clearTimeout(armed);
        const { invoke } = await import("@tauri-apps/api/core");
        try {
          toast(await invoke<string>("remove_pack", { id: p.id }));
        } catch (e) {
          toast(String(e));
        }
        await load();
      };
    }
    body.querySelector<HTMLButtonElement>(".star")!.onclick = () => {
      const on = !(cfg.selection.favorites ?? []).includes(p.id);
      void act("/v1/favorite", { pack: p.id, on }, on ? tr("已收藏", "Added to favorites") : tr("已取消收藏", "Removed from favorites"));
    };
    body.querySelectorAll<HTMLButtonElement>("[data-t]").forEach((b) => {
      b.onclick = () => pick(b.dataset.t!, p.id);
    });
    const input = body.querySelector("input")!;
    const go = () => {
      const proj = input.value.trim();
      if (!proj) return toast(tr("先填项目文件夹名", "Enter a project folder name first"));
      input.value = "";
      void pick(`project:${proj}`, p.id);
    };
    body.querySelector<HTMLButtonElement>("button.proj")!.onclick = go;
    input.onkeydown = (e) => {
      if (e.key === "Enter") go();
    };
  }
  let dl = document.getElementById("projects-seen");
  if (!dl) {
    dl = document.createElement("datalist");
    dl.id = "projects-seen";
    document.body.appendChild(dl);
  }
  dl.innerHTML = cfg.projects_seen.map((x) => `<option value="${esc(x)}"></option>`).join("");
  updateBadges();
}

async function pick(target: string, pack: string) {
  const other = target === "claude-code" ? "codex" : target === "codex" ? "claude-code" : null;
  const r = await postJson(conn, "/v1/select", { target, pack });
  if (!r.ok) return toast(r.message);
  if (other && cfg.in_use[other] === pack) {
    toast(
      tr(
        `${targetName(other)}正在使用 ${packName(pack)}，两边会显示 CC / CX 标记，Codex 那只换个颜色`,
        `${targetName(other)} is also using ${packName(pack)}: both get a CC / CX tag and the Codex one changes color`,
      ),
    );
  } else {
    toast(tr(`${targetName(target)}现在用 ${packName(pack)}`, `${targetName(target)} now uses ${packName(pack)}`));
  }
  await load();
}

function updateBadges() {
  const favs = cfg.selection.favorites ?? [];
  document.querySelectorAll<HTMLElement>(".card").forEach((card) => {
    const id = card.dataset.pack!;
    const tags: string[] = [];
    if (cfg.in_use["claude-code"] === id) tags.push(`<span class="cc">CC</span>`);
    if (cfg.in_use.codex === id) tags.push(`<span class="cx">CX</span>`);
    for (const [proj, pk] of Object.entries(cfg.selection.projects ?? {})) {
      if (pk === id) tags.push(`<span class="pj">${esc(proj)}</span>`);
    }
    card.querySelector(".inuse")!.innerHTML = tags.join("");
    card.classList.toggle("picked", tags.length > 0);
    card.querySelector(".star")!.classList.toggle("on", favs.includes(id));
    card.querySelectorAll<HTMLButtonElement>("[data-t]").forEach((b) => {
      const t = b.dataset.t as "claude-code" | "codex";
      const mine = cfg.in_use[t] === id;
      b.disabled = mine;
      const other = t === "claude-code" ? "codex" : "claude-code";
      b.title = !mine && cfg.in_use[other] === id ? tr(`对方阵营（${targetName(other)}）正在使用`, `In use by the other team (${targetName(other)})`) : "";
      b.textContent = (t === "claude-code" ? tr("给 Claude Code", "For Claude Code") : tr("给 Codex", "For Codex")) + (b.title ? " ⚑" : "");
    });
  });
}

interface FishInfo {
  id: string;
  name: string;
  rarity: string;
  note: string;
  count: number;
  first: number;
}

interface Stats {
  wallet: number;
  income_today: number;
  daily_cap: number;
  fish: FishInfo[];
  today: Report;
}

function fishCanvas(color: string, locked: boolean): HTMLCanvasElement {
  const c = document.createElement("canvas");
  const dpr = devicePixelRatio || 1;
  c.width = 44 * dpr;
  c.height = 30 * dpr;
  c.style.width = "44px";
  c.style.height = "30px";
  const g = c.getContext("2d")!;
  g.scale(dpr, dpr);
  g.fillStyle = locked ? "#c3c1d0" : color;
  g.beginPath();
  g.ellipse(24, 15, 14, 8, 0, 0, Math.PI * 2);
  g.fill();
  g.beginPath();
  g.moveTo(11, 15);
  g.lineTo(2, 7);
  g.lineTo(2, 23);
  g.fill();
  if (!locked) {
    g.fillStyle = "#fff";
    g.fillRect(31, 11, 3, 3);
    g.fillStyle = "#1f2245";
    g.fillRect(32, 12, 2, 2);
  }
  return c;
}

async function renderFish() {
  const st = await getJson<Stats>(conn, "/v1/stats");
  const got = st.fish.filter((f) => f.count > 0).length;
  $("fish-progress").textContent = tr(`已收集 ${got} / ${st.fish.length} 种`, `Collected ${got} / ${st.fish.length} kinds`);
  const grid = $("fishgrid");
  grid.innerHTML = "";
  for (const f of st.fish) {
    const locked = f.count === 0;
    const el = document.createElement("div");
    el.className = `fish${locked ? " locked" : ""}`;
    el.appendChild(fishCanvas(RARITY_COLORS[f.rarity] ?? "#8a9bb0", locked));
    const info = document.createElement("div");
    info.innerHTML = `<span class="rar" style="color:${RARITY_COLORS[f.rarity]}">${RARITY_NAMES[f.rarity] ?? f.rarity}</span>
      <b>${locked ? "？？？" : esc(f.name)}</b>
      <small>${locked ? tr("还没钓到过", "Not caught yet") : tr(`钓到 ${f.count} 条`, `Caught ${f.count}`) + ` · ${esc(f.note)}`}</small>`;
    el.appendChild(info);
    grid.appendChild(el);
  }
}

async function renderWallet() {
  void renderSeason();
  const st = await getJson<Stats>(conn, "/v1/stats");
  const t = st.today;
  const pct = Math.min(100, (st.income_today / Math.max(1, st.daily_cap)) * 100);
  $("kpis").innerHTML = `
    <div class="kpi"><span>${tr("金币余额", "Coin balance")}</span><div class="big">${st.wallet}</div></div>
    <div class="kpi"><span>${tr("今日收入 / 上限", "Earned today / cap")}</span><div class="big">${st.income_today} / ${st.daily_cap}</div><div class="meter"><i style="width:${pct}%"></i></div></div>
    <div class="kpi"><span>${tr("图鉴进度", "Collection")}</span><div class="big">${st.fish.filter((f) => f.count > 0).length} / ${st.fish.length}</div></div>`;
  const camp = (name: string, key: string) => {
    const d = t.by_camp[key];
    return d ? `<div class="kpi"><span>${name}</span><div class="big">${tr(`${d.turns} 轮`, `${d.turns} turns`)}</div><span>${tr(`一次通过 ${d.clean} · 出错 ${d.errors}`, `${d.clean} first-try · ${d.errors} errors`)}</span></div>` : "";
  };
  const g = await getJson<{ affinity: number; races: Record<string, number>; duels: Record<string, number>; bets: Record<string, number> }>(conn, "/v1/games").catch(() => null);
  const rec = (m: Record<string, number> | undefined) =>
    m ? `Claude Code ${m["claude-code"] ?? 0} : ${m["codex"] ?? 0} Codex${m["tie"] ? tr(`，平 ${m["tie"]}`, `, ${m["tie"]} ties`) : ""}` : "—";
  const games = g
    ? `<div class="kpi"><span>${tr("两只宠物的好感度", "Affinity between the two pets")}</span><div class="big">${g.affinity}</div><div class="meter"><i style="width:${g.affinity}%"></i></div><span>${tr("20 击掌 · 50 一起睡 · 80 合照", "20 high-five · 50 nap together · 80 photo")}</span></div>
       <div class="kpi"><span>${tr("赛跑战绩", "Race record")}</span><div class="big" style="font-size:16px">${rec(g.races)}</div></div>
       <div class="kpi"><span>${tr("擂台战绩", "Duel record")}</span><div class="big" style="font-size:16px">${rec(g.duels)}</div></div>
       <div class="kpi"><span>${tr("你的下注", "Your bets")}</span><div class="big" style="font-size:16px">${tr(`赢 ${g.bets.won ?? 0} · 输 ${g.bets.lost ?? 0}`, `Won ${g.bets.won ?? 0} · Lost ${g.bets.lost ?? 0}`)}</div></div>`
    : "";
  $("today").innerHTML = games + `
    <div class="kpi"><span>${tr("agent 工作时长", "Agent work time")}</span><div class="big">${t.work_ms ? duration(t.work_ms) : "0"}</div></div>
    <div class="kpi"><span>${tr("钓到的鱼", "Fish caught")}</span><div class="big">${t.catches.filter((c) => c.outcome !== "boot" && c.outcome !== "escape").length}</div></div>
    <div class="kpi"><span>${tr("拍死的虫", "Bugs squashed")}</span><div class="big">${t.bugs}${t.bosses ? ` + ${t.bosses} Boss` : ""}</div></div>
    <div class="kpi"><span>${tr("挨的鞭子", "Lashes taken")}</span><div class="big">${t.whips}</div></div>
    <div class="kpi"><span>${tr("玩的游戏", "Games played")}</span><div class="big">${t.games}</div></div>
    <div class="kpi"><span>${tr("听电台", "Radio time")}</span><div class="big">${t.radio_s ? duration(t.radio_s * 1000) : "0"}</div></div>
    <div class="kpi"><span>${tr("提肛", "Kegels")}</span><div class="big">${t.kegel_reps ? tr(`${t.kegel_reps} 组`, `${t.kegel_reps} reps`) : "0"}</div></div>
    <div class="kpi"><span>${tr("收到的信", "Letters")}</span><div class="big">${t.letters}</div></div>
    <div class="kpi"><span>${tr("最多同时在线 agent", "Peak agents online")}</span><div class="big">${t.peak_agents}</div></div>
    ${camp("Claude Code", "claude-code")}${camp("Codex", "codex")}`;
}

// ------------------------------------------------------------ import

type Outcome =
  | { status: "installed"; id: string; name: string; replaced: boolean }
  | { status: "conflict"; id: string; name: string; new_version: string; existing_version: string; existing_builtin: boolean }
  | { status: "invalid"; errors: string[] };

function showResult(html: string) {
  $("import-result").innerHTML = html;
}

async function importPath(path: string, overwrite = false): Promise<void> {
  if (!inTauri) return;
  const { invoke } = await import("@tauri-apps/api/core");
  const file = path.split(/[\\/]/).pop() ?? path;
  let r: Outcome;
  try {
    r = await invoke<Outcome>("import_pack", { path, overwrite });
  } catch (e) {
    showResult(`<div class="banner"><b>${tr("导入失败", "Import failed")}</b>${tr(`（${esc(file)}）：`, ` (${esc(file)}): `)}${esc(String(e))}</div>`);
    return;
  }
  if (r.status === "installed") {
    showResult(
      tr(
        `<div class="okbar">已${r.replaced ? "覆盖" : "导入"} <b>${esc(r.name || r.id)}</b>，可以在下面选给 Claude Code、Codex 或某个项目。</div>`,
        `<div class="okbar">${r.replaced ? "Overwrote" : "Imported"} <b>${esc(r.name || r.id)}</b>. Assign it below to Claude Code, Codex or a project.</div>`,
      ),
    );
    toast(tr(`已导入 ${r.name || r.id}`, `Imported ${r.name || r.id}`));
    await load();
  } else if (r.status === "invalid") {
    showResult(`<div class="banner"><b>${tr(`${esc(file)} 没通过校验，没有安装：`, `${esc(file)} failed validation and was not installed:`)}</b><ul>${r.errors.map((e) => `<li>${esc(e)}</li>`).join("")}</ul>
      ${tr(`宠物包怎么写见 README 的"自制宠物"一节。`, `See the "Make your own pet" section of the README for the pack format.`)}</div>`);
  } else {
    const old = r.existing_builtin
      ? tr(`内置的 v${esc(r.existing_version)}`, `built-in v${esc(r.existing_version)}`)
      : tr(`已安装的 v${esc(r.existing_version)}`, `installed v${esc(r.existing_version)}`);
    showResult(`<div class="askbar">${tr(
      `已经有 id 为 <b>${esc(r.id)}</b> 的宠物包（${old}），要用 v${esc(r.new_version || "?")} 覆盖吗？`,
      `A pack with id <b>${esc(r.id)}</b> already exists (${old}). Overwrite it with v${esc(r.new_version || "?")}?`,
    )}
      ${r.existing_builtin ? tr("（内置版本不会被删除，删掉导入的这份就会恢复）", "(The built-in version is kept; delete the imported copy to get it back.)") : ""}
      <button class="primary" id="ow-yes">${tr("覆盖", "Overwrite")}</button><button id="ow-no">${tr("取消", "Cancel")}</button></div>`);
    $("ow-yes").onclick = () => void importPath(path, true);
    $("ow-no").onclick = () => showResult("");
  }
}

async function setupImport() {
  if (!inTauri) {
    ($("import-dir") as HTMLButtonElement).disabled = true;
    ($("import-zip") as HTMLButtonElement).disabled = true;
    ($("make-pet") as HTMLButtonElement).disabled = true;
    return;
  }
  const maker = setupMaker({
    conn,
    installed: async (id, name, replaced) => {
      showResult(`<div class="okbar">${tr(`已${replaced ? "更新" : "做好"} <b>${esc(name)}</b>，让它给谁用？`, `<b>${esc(name)}</b> is ${replaced ? "updated" : "ready"}. Who gets it?`)}
        <button class="primary" id="mk-cc">${tr("给 Claude Code", "For Claude Code")}</button> <button id="mk-cx">${tr("给 Codex", "For Codex")}</button></div>`);
      $("mk-cc").onclick = () => { showResult(""); void pick("claude-code", id); };
      $("mk-cx").onclick = () => { showResult(""); void pick("codex", id); };
      toast(tr(`已安装 ${name}`, `Installed ${name}`));
      await load();
      document.querySelector(`.card[data-pack="${CSS.escape(id)}"]`)?.scrollIntoView({ block: "center", behavior: "smooth" });
    },
  });
  $("make-pet").onclick = () => maker.open();
  const { open } = await import("@tauri-apps/plugin-dialog");
  $("import-dir").onclick = async () => {
    const p = await open({ directory: true, title: tr("选择宠物包文件夹（里面有 pet.json）", "Choose a pet pack folder (with pet.json inside)") });
    if (typeof p === "string") await importPath(p);
  };
  $("import-zip").onclick = async () => {
    const p = await open({ title: tr("选择宠物包 zip", "Choose a pet pack zip"), filters: [{ name: tr("宠物包", "Pet pack"), extensions: ["zip"] }] });
    if (typeof p === "string") await importPath(p);
  };
  // Drop folders or zips anywhere on the window.
  const { getCurrentWebview } = await import("@tauri-apps/api/webview");
  await getCurrentWebview().onDragDropEvent(async (e) => {
    const t = e.payload.type;
    $("drop").hidden = !(t === "enter" || t === "over");
    if (t === "drop") {
      const img = e.payload.paths.find((p) => IMAGE_EXT.test(p));
      if (img) maker.open(img);
      for (const p of e.payload.paths) if (!IMAGE_EXT.test(p)) await importPath(p);
    }
  });
}

// ------------------------------------------------------------ progress

interface Ach { id: string; name: string; desc: string; coins: number; unlocked_at: number | null }
interface Season { week: string; cc: number; cx: number; winner: string }
interface Progress {
  achievements: Ach[];
  seasons: Season[];
  week: string;
  current: { "claude-code": number; codex: number } | null;
  champion: string | null;
}
interface ShopItem { id: string; name: string; kind: string; price: number; desc: string; owned: boolean }
interface Shop { wallet: number; items: ShopItem[]; equipped: { pack: string; slot: string; item: string }[] }

const campName = (c: string) => (c === "claude-code" ? "Claude Code" : c === "codex" ? "Codex" : tr("平局", "Tie"));

async function renderAchievements() {
  const p = await getJson<Progress>(conn, "/v1/progress");
  const got = p.achievements.filter((a) => a.unlocked_at).length;
  $("ach-progress").textContent = tr(`已解锁 ${got} / ${p.achievements.length}`, `Unlocked ${got} / ${p.achievements.length}`);
  $("achgrid").innerHTML = p.achievements
    .map(
      (a) => `<div class="ach${a.unlocked_at ? "" : " locked"}"><span class="coin">+${a.coins}</span>
        <b>${a.unlocked_at ? "🏆 " : "🔒 "}${esc(a.name)}</b><small>${esc(a.desc)}${
          a.unlocked_at ? ` · ${new Date(a.unlocked_at).toLocaleDateString(EN ? "en-US" : "zh-CN")}` : ""
        }</small></div>`,
    )
    .join("");
}

async function renderSeason() {
  const p = await getJson<Progress>(conn, "/v1/progress");
  const c = p.current;
  $("season").innerHTML = `
    <div class="kpi"><span>${esc(p.week)} · Claude Code</span><div class="big">${c ? c["claude-code"].toFixed(2) : "0"}</div></div>
    <div class="kpi"><span>${esc(p.week)} · Codex</span><div class="big">${c ? c.codex.toFixed(2) : "0"}</div></div>
    <div class="kpi"><span>${tr("上周冠军（这周戴皇冠）", "Last week's champ (wears the crown)")}</span><div class="big">${p.champion ? campName(p.champion) : "—"}</div></div>`;
  $("season-history").innerHTML = p.seasons.length
    ? tr("历届：", "Past seasons: ") +
      p.seasons
        .map((s) => `${esc(s.week)} ${campName(s.winner)}` + tr(`（${s.cc.toFixed(2)} : ${s.cx.toFixed(2)}）`, ` (${s.cc.toFixed(2)} : ${s.cx.toFixed(2)})`))
        .join(tr("；", "; "))
    : tr(
        "赛季每周一结算：上周每天拔河效率分加起来，高的一方这周戴皇冠。",
        "Seasons settle every Monday: last week's daily tug-of-war scores are added up, and the higher team wears the crown this week.",
      );
}

/** Shop sections in rack order: the wearable slots, then the rope and the lines. */
const SHOP_KINDS = [...SLOTS, "rope", "quips"];
let shopCat = "all";
let shop: Shop | null = null;
let fitSprites: Sprites | null = null;
let fitLoopOn = false;

/** What `pack` has on, from the equipped list: the hat and the other slots. */
function outfitOf(s: Shop, pack: string): { hat?: string; wear: Record<string, string> } {
  const wear: Record<string, string> = {};
  let hat: string | undefined;
  for (const e of s.equipped) {
    if (e.pack !== pack) continue;
    if (e.slot === "hat") hat = e.item;
    else if (e.slot !== "rope") wear[e.slot] = e.item;
  }
  return { hat, wear };
}

/**
 * The pet, dressed, with its bottom-centre near the bottom of a w×h box:
 * outfit pieces behind, the sprite, pieces in front, the hat.
 */
function drawDressed(g: CanvasRenderingContext2D, sp: Sprites, w: number, h: number, wear: Record<string, string>, hat: string | undefined, now: number, fill = 0.7): void {
  const a = sp.action("idle") ?? Object.values(sp.pack.actions)[0];
  if (!a) return;
  const fr = sp.frameAt(a, now);
  const f = sp.pack.frame;
  const raw = Math.min(w / f.w, h / f.h) * fill;
  const sc = f.w >= 96 || f.h >= 96 ? raw : Math.max(1, Math.floor(raw));
  const cx = w / 2;
  const bottom = h - Math.round(h * 0.13);
  const fit = fitOf(sp.body(a, fr.idx), f.w, f.h, cx, bottom, sc);
  drawWearBehind(g, wear, fit, now);
  sp.draw(g, a, fr.idx, cx, bottom, sc);
  drawWearFront(g, wear, fit, now);
  if (hat) {
    const hd = sp.head(a, fr.idx);
    drawHat(g, hat, cx + (hd.x - f.w / 2) * sc, bottom - (f.h - hd.top) * sc, hd.w * sc, now);
  }
}

/** A card picture: the chosen pet wearing just this item (static), or the rope / a speech bubble. */
function itemIcon(it: ShopItem): HTMLCanvasElement {
  const W = 148, H = 122;
  const cv = document.createElement("canvas");
  const dpr = devicePixelRatio || 1;
  cv.width = W * dpr;
  cv.height = H * dpr;
  cv.style.height = `${H}px`;
  const g = cv.getContext("2d")!;
  g.scale(dpr, dpr);
  if (it.kind === "rope") {
    g.strokeStyle = ropeStyle(g, it.id, 16, W - 16);
    g.lineWidth = 7;
    g.lineCap = "round";
    g.beginPath();
    g.moveTo(16, H * 0.42);
    g.quadraticCurveTo(W / 2, H * 0.72, W - 16, H * 0.42);
    g.stroke();
    // A little flag at the middle, like the real one.
    g.fillStyle = "#ee5b3c";
    g.fillRect(W / 2 - 1, H * 0.57 - 14, 2, 14);
    g.beginPath();
    g.moveTo(W / 2 + 1, H * 0.57 - 13);
    g.lineTo(W / 2 + 12, H * 0.57 - 9);
    g.lineTo(W / 2 + 1, H * 0.57 - 5);
    g.fill();
  } else if (it.kind === "quips") {
    g.fillStyle = "#ffffff";
    g.strokeStyle = "#1f2245";
    g.lineWidth = 1.5;
    g.beginPath();
    g.roundRect(W / 2 - 50, 26, 100, 40, 14);
    g.fill();
    g.stroke();
    g.beginPath();
    g.moveTo(W / 2 - 14, 65);
    g.lineTo(W / 2 - 8, 78);
    g.lineTo(W / 2, 65);
    g.fill();
    g.fillStyle = "#1f2245";
    g.font = `600 12px "Segoe UI", "Microsoft YaHei UI", sans-serif`;
    g.textAlign = "center";
    g.textBaseline = "middle";
    g.fillText(it.id === "quips_poet" ? tr("编译如风…", "like wind…") : tr("能跑就行", "ship it"), W / 2, 46);
  } else if (fitSprites) {
    const wear = it.kind === "hat" ? {} : { [it.kind]: it.id };
    drawDressed(g, fitSprites, W, H, wear, it.kind === "hat" ? it.id : undefined, 1234, 0.82);
  }
  return cv;
}

/** The fitting room: the chosen pet in everything it has on, animated. */
function fitLoop(): void {
  if (fitLoopOn) return;
  fitLoopOn = true;
  const cv = $("fitcanvas") as HTMLCanvasElement;
  const dpr = devicePixelRatio || 1;
  cv.width = 218 * dpr;
  cv.height = 218 * dpr;
  const g = cv.getContext("2d")!;
  const tick = (now: number) => {
    requestAnimationFrame(tick);
    if ($("tab-shop").hidden || !shop || !fitSprites) return;
    g.setTransform(dpr, 0, 0, dpr, 0, 0);
    g.clearRect(0, 0, 218, 218);
    const { hat, wear } = outfitOf(shop, ($("shop-pack") as HTMLSelectElement).value);
    drawDressed(g, fitSprites, 218, 218, wear, hat, now);
  };
  requestAnimationFrame(tick);
}

async function renderShop() {
  const s = await getJson<Shop>(conn, "/v1/shop");
  shop = s;
  $("shop-wallet").textContent = String(s.wallet);
  const sel = $("shop-pack") as HTMLSelectElement;
  const keep = sel.value || cfg.in_use["claude-code"];
  sel.innerHTML = packs.map((p) => `<option value="${esc(p.id)}">${esc(p.name || p.id)}</option>`).join("");
  sel.value = packs.some((p) => p.id === keep) ? keep : (packs[0]?.id ?? "");
  sel.onchange = () => void renderShop();
  const pack = sel.value;
  fitSprites = pack ? await previews.sprites(pack, rev).catch(() => null) : null;
  fitLoop();
  const byId = new Map(s.items.map((i) => [i.id, i]));
  const worn = (it: ShopItem) =>
    s.equipped.some((e) => e.item === it.id && (it.kind === "rope" ? e.pack === "*" : e.pack === pack));

  // What it has on, with a way to take each piece off.
  const on = s.equipped.filter((e) => (e.pack === pack && e.slot !== "rope") || (e.pack === "*" && e.slot === "rope"));
  $("fitworn").innerHTML = on.length
    ? on.map((e) => `<span class="chip" data-slot="${esc(e.slot)}">${esc(byId.get(e.item)?.name ?? e.item)}<button title="${tr("脱下", "Take off")}" data-off="${esc(e.slot)}">✕</button></span>`).join("")
    : `<span class="empty">${tr("什么都没穿。挑一件试试。", "Nothing on yet. Try something.")}</span>`;
  $("fitworn").querySelectorAll<HTMLButtonElement>("[data-off]").forEach((b) => {
    b.onclick = async () => {
      const r = await postJson(conn, "/v1/shop/equip", { pack, slot: b.dataset.off, item: null });
      if (!r.ok) toast(r.message);
      await renderShop();
    };
  });

  // Racks.
  const count = (k: string) => s.items.filter((i) => i.kind === k);
  const cats = [["all", tr("全部", "All"), s.items.length] as const, ...SHOP_KINDS.map((k) => [k, SLOT_NAMES[k]?.() ?? k, count(k).length] as const)];
  $("shopcats").innerHTML = cats
    .filter(([, , n]) => n > 0)
    .map(([k, name, n]) => `<span class="chip${shopCat === k ? " on" : ""}" data-cat="${k}">${esc(name)}<span class="n">${n}</span></span>`)
    .join("");
  $("shopcats").querySelectorAll<HTMLElement>("[data-cat]").forEach((el) => {
    el.onclick = () => {
      shopCat = el.dataset.cat!;
      void renderShop();
    };
  });

  const order = (k: string) => SHOP_KINDS.indexOf(k);
  const items = s.items.filter((i) => shopCat === "all" || i.kind === shopCat).sort((a, b) => order(a.kind) - order(b.kind) || a.price - b.price);
  const grid = $("shopgrid");
  grid.innerHTML = "";
  for (const it of items) {
    const isWorn = worn(it);
    const locked = !it.owned && s.wallet < it.price;
    const el = document.createElement("div");
    el.className = `item${isWorn ? " worn" : ""}${locked ? " locked" : ""}`;
    el.dataset.slot = it.kind;
    el.appendChild(itemIcon(it));
    const info = document.createElement("div");
    let action = "";
    if (!it.owned) {
      action = `<button class="primary" ${locked ? "disabled" : ""}>${locked ? tr(`还差 ${it.price - s.wallet} 金币`, `${it.price - s.wallet} coins short`) : tr("买下", "Buy")}</button>`;
    } else if (it.kind === "quips") {
      action = `<small>${tr("已拥有，干活时会用上", "Owned — used while working")}</small>`;
    } else if (isWorn) {
      action = `<button>${tr("脱下", "Take off")}</button>`;
    } else {
      action = `<button class="primary">${it.kind === "rope" ? tr("换上", "Use") : tr("穿上", "Wear")}</button>`;
    }
    const price = it.owned ? `<span class="price owned">${tr("已拥有", "Owned")}</span>` : `<span class="price">${it.price}</span>`;
    info.innerHTML = `<span class="slot">${esc(SLOT_NAMES[it.kind]?.() ?? it.kind)}</span>${isWorn ? `<span class="wornbadge">${tr("穿着", "On")}</span>` : ""}
      <div class="ttl"><b>${esc(it.name)}</b>${price}</div><small>${esc(it.desc)}</small><div class="act">${action}</div>`;
    el.appendChild(info);
    const btn = info.querySelector("button");
    if (btn) {
      btn.onclick = async () => {
        let r;
        if (!it.owned) r = await postJson(conn, "/v1/shop/buy", { item: it.id });
        else if (isWorn) r = await postJson(conn, "/v1/shop/equip", { pack, slot: it.kind, item: null });
        else r = await postJson(conn, "/v1/shop/equip", { pack, slot: it.kind, item: it.id });
        if (!r.ok) toast(r.message);
        else if (r.message) toast(r.message);
        await renderShop();
      };
    }
    grid.appendChild(el);
  }
}

// ---------------------------------------------------------------- reminders

interface Reminders { kegel: string[]; kegel_reps: number; water: boolean; breathe: boolean }

async function renderHealth() {
  const box = $("tab-health");
  let r = await getJson<Reminders>(conn, "/v1/reminders");
  const save = async (next: Reminders) => {
    const res = await postJson(conn, "/v1/reminders", next);
    if (res.ok) r = next;
    draw();
    $("rm-msg").textContent = res.message;
    $("rm-msg").style.color = res.ok ? "" : "var(--red)";
  };
  const draw = () => {
    const mins = Math.max(1, Math.round((3 + r.kegel_reps * 6) / 60));
    box.innerHTML = `
      <section class="hcard">
        <h3>🍑 ${tr("提肛提醒", "Kegel reminders")}</h3>
        <div class="note">${tr("每天到点，宠物会转过身、屁股对着你做一组：收紧时夹紧上提、尾巴竖起，放松时松开、尾巴摇摇；头顶有「收紧 ⬆」「放松 ⬇」和倒计时，还有提示音，跟着它做就行。点一下宠物可以跳过。",
          "At each time, your pet turns around and does a set with its bottom to you: squeezing and lifting with its tail up, then letting go with a wag. “Squeeze ⬆” / “Relax ⬇” and a countdown show above it, with a soft beep. Click the pet to skip.")}</div>
        <div class="hrow"><span>${tr("提醒时间", "Times")}</span>
          <span class="chips">${r.kegel.map((t) => `<span class="chip">${t}<button data-del="${t}" title="${tr("删除", "Remove")}">✕</button></span>`).join("") || `<span class="note" style="margin:0">${tr("还没设，提醒是关的", "None yet — reminders are off")}</span>`}</span>
          <input type="time" id="kg-t" value="${r.kegel.length ? "15:30" : "10:30"}" /><button id="kg-add">${tr("添加", "Add")}</button></div>
        <div class="hrow"><span>${tr("每次", "Each set")}</span>
          <select id="kg-reps">${[5, 10, 15, 20].map((n) => `<option value="${n}"${n === r.kegel_reps ? " selected" : ""}>${tr(`${n} 组`, `${n} reps`)}</option>`).join("")}</select>
          <span class="note" style="margin:0">${tr(`约 ${mins} 分钟`, `about ${mins} min`)}</span>
          <span class="sp"></span><button id="kg-try" class="primary">${tr("现在试一组", "Try a set now")}</button></div>
        <details class="howto" open><summary>${tr("动作要领", "How to do it")}</summary><ol>
          <li>${tr("<b>找对肌肉：</b>就是憋尿、忍住放屁时用力的那块肌肉（盆底肌）。", "<b>Find the muscle:</b> the one you use to hold in pee or gas (the pelvic floor).")}</li>
          <li>${tr("<b>收紧上提：</b>宠物喊「收紧 ⬆」时，把肛门和会阴往上、往里收，保持 3 秒。", "<b>Squeeze and lift:</b> on “Squeeze ⬆”, pull the area up and in, and hold for 3 s.")}</li>
          <li>${tr("<b>完全放松：</b>喊「放松 ⬇」时彻底松开 3 秒。放松和收紧一样重要。", "<b>Let go fully:</b> on “Relax ⬇”, release completely for 3 s. Relaxing matters as much as squeezing.")}</li>
          <li>${tr("<b>呼吸自然：</b>别憋气；肚子、屁股和大腿尽量不跟着用力。", "<b>Keep breathing:</b> don't hold your breath; keep your belly, buttocks and thighs loose.")}</li>
          <li>${tr("坐着、站着都能做，旁边的人看不出来。", "Sitting or standing both work, and nobody can tell.")}</li>
        </ol><div class="note">${tr("刚开始每次 5 组，习惯后 10–15 组，每天 2–3 次。如果有盆底或肛肠方面的不适，先问问医生。",
          "Start with 5 reps, build up to 10–15, two or three times a day. If you have pelvic or rectal problems, ask a doctor first.")}</div></details>
      </section>
      <section class="hcard">
        <h3>${tr("其他提醒", "Other reminders")}</h3>
        <label class="hchk"><input type="checkbox" id="rm-water"${r.water ? " checked" : ""} /> 💧 ${tr("喝水：空闲时每小时提醒一次", "Water: once an hour while idle")}</label>
        <label class="hchk"><input type="checkbox" id="rm-breathe"${r.breathe ? " checked" : ""} /> 🌬️ ${tr("深呼吸：10 分钟内等你超过 6 次时，一起做 30 秒深呼吸", "Breathing: 30 s of deep breaths when you're waited on 6+ times in 10 min")}</label>
      </section>
      <div id="rm-msg" class="note"></div>`;
    box.querySelectorAll<HTMLButtonElement>("[data-del]").forEach((b) => (b.onclick = () => void save({ ...r, kegel: r.kegel.filter((t) => t !== b.dataset.del) })));
    $("kg-add").onclick = () => {
      const t = ($("kg-t") as HTMLInputElement).value;
      if (t) void save({ ...r, kegel: [...r.kegel, t] });
    };
    ($("kg-reps") as HTMLSelectElement).onchange = (e) => void save({ ...r, kegel_reps: Number((e.target as HTMLSelectElement).value) });
    ($("rm-water") as HTMLInputElement).onchange = (e) => void save({ ...r, water: (e.target as HTMLInputElement).checked });
    ($("rm-breathe") as HTMLInputElement).onchange = (e) => void save({ ...r, breathe: (e.target as HTMLInputElement).checked });
    $("kg-try").onclick = async () => {
      const res = await fetch(conn.base + "/v1/cmd", {
        method: "POST",
        headers: { "X-Familiar-Token": conn.token, "Content-Type": "application/json" },
        body: JSON.stringify({ cmd: "kegel" }),
      });
      toast(await res.text());
    };
  };
  draw();
}

function setupTabs() {
  const tabs = document.querySelectorAll<HTMLButtonElement>("#tabs button");
  tabs.forEach((b) => {
    b.onclick = () => {
      tabs.forEach((x) => x.classList.toggle("on", x === b));
      for (const id of ["pets", "fish", "wallet", "ach", "shop", "health"]) $(`tab-${id}`).hidden = id !== b.dataset.tab;
      if (b.dataset.tab === "health") void renderHealth();
      if (b.dataset.tab === "fish") void renderFish();
      if (b.dataset.tab === "wallet") void renderWallet();
      if (b.dataset.tab === "ach") void renderAchievements();
      if (b.dataset.tab === "shop") void renderShop();
    };
  });
}

async function main() {
  conn = await connect();
  await initLang(conn);
  previews = new Previews(conn);
  setupTabs();
  await setupImport();
  await load();
  // gallery.html?tab=shop opens a tab directly.
  const tab = new URLSearchParams(location.search).get("tab");
  if (tab) document.querySelector<HTMLButtonElement>(`#tabs button[data-tab="${tab}"]`)?.click();
  subscribe(conn, (type) => {
    if (type === "packs.changed" || type === "pets.layout") void load();
  });
  // Pick up hand edits of config.yaml too.
  setInterval(() => void load(), 3000);

  const invoke = async (cmd: string) => {
    if (!inTauri) return;
    const { invoke } = await import("@tauri-apps/api/core");
    await invoke(cmd);
  };
  $("open-dir").onclick = () => invoke("open_pets_dir");
  $("open-cfg").onclick = () => invoke("open_config");
  $("open-report").onclick = () => invoke("open_report_window");
}

main().catch((e) => {
  document.body.innerHTML = `<div class="banner">${tr("图鉴加载失败：", "Failed to load the Gallery: ")}${String(e)}</div>`;
});
