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

/** Who shows up in each mode, as dots: big ones are pets, small ones their minis. */
const MODE_GLYPHS: Record<string, string> = {
  "per-camp": `<svg viewBox="0 0 46 22" fill="currentColor"><circle cx="7" cy="12" r="6"/><circle cx="17" cy="17" r="2.6"/><circle cx="30" cy="12" r="6"/><circle cx="40" cy="17" r="2.6"/></svg>`,
  single: `<svg viewBox="0 0 46 22" fill="currentColor"><circle cx="8" cy="11" r="7.5"/></svg>`,
  "per-rule": `<svg viewBox="0 0 46 22" fill="currentColor"><circle cx="6" cy="9" r="5"/><circle cx="21" cy="9" r="5"/><circle cx="36" cy="9" r="5"/><rect x="2" y="18" width="8" height="2" rx="1"/><rect x="17" y="18" width="8" height="2" rx="1"/><rect x="32" y="18" width="8" height="2" rx="1"/></svg>`,
  "per-session": `<svg viewBox="0 0 46 22" fill="currentColor"><circle cx="5" cy="6" r="3.6"/><circle cx="16" cy="6" r="3.6"/><circle cx="27" cy="6" r="3.6"/><circle cx="5" cy="16.5" r="3.6"/><circle cx="16" cy="16.5" r="3.6"/><circle cx="27" cy="16.5" r="3.6" opacity="0.45"/></svg>`,
};

const CAMPS = [["claude-code", "CLAUDE CODE"], ["codex", "CODEX"]] as const;
/** The hero's colour for each tab. */
const TAB_HUES: Record<string, number> = { pets: 236, fish: 204, wallet: 256, ach: 312, shop: 282, health: 172 };
const TAB_ICONS: Record<string, string> = { pets: "paw", fish: "fish", wallet: "wallet", ach: "medal", shop: "bag", health: "bell" };

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

function hash(s: string): number {
  let h = 0;
  for (let i = 0; i < s.length; i++) h = (h * 31 + s.charCodeAt(i)) >>> 0;
  return h;
}

/** Line icons, drawn on a 24 grid. */
const ICONS: Record<string, string> = {
  paw: `<ellipse cx="12" cy="15.5" rx="4.6" ry="3.6"/><circle cx="6.3" cy="10.6" r="1.7"/><circle cx="10" cy="6.8" r="1.7"/><circle cx="14" cy="6.8" r="1.7"/><circle cx="17.7" cy="10.6" r="1.7"/>`,
  fish: `<path d="M21 12c-3-4.6-9-5.6-13-1.8L4 7v10l4-3.2c4 3.8 10 2.8 13-1.8z"/><circle cx="16.5" cy="11" r="0.6"/>`,
  wallet: `<rect x="3" y="6" width="18" height="13" rx="3"/><path d="M15.5 12.5H21"/><path d="M6 6l9-3 1.5 3"/>`,
  medal: `<circle cx="12" cy="14.5" r="5.5"/><path d="M9 10L6.5 3h4L12 6.5 13.5 3h4L15 10"/>`,
  bag: `<path d="M5 8h14l-1 12H6z"/><path d="M9 8V7a3 3 0 016 0v1"/>`,
  bell: `<path d="M6 17v-6a6 6 0 0112 0v6l1.5 2h-15z"/><path d="M10 21.5h4"/>`,
  clock: `<circle cx="12" cy="12" r="9"/><path d="M12 7v5l3 2"/>`,
  bug: `<ellipse cx="12" cy="13.5" rx="4.5" ry="5.5"/><path d="M12 8V5M9.5 5.5L12 8l2.5-2.5M7.5 11L4 9M16.5 11L20 9M7.5 15.5L4 17.5M16.5 15.5l3.5 2"/>`,
  bolt: `<path d="M13 2L5 14h6l-1 8 8-12h-6z"/>`,
  game: `<rect x="2.5" y="7" width="19" height="11" rx="5.5"/><path d="M8 10.5v4M6 12.5h4"/><circle cx="16.5" cy="12.5" r="1"/>`,
  radio: `<rect x="3" y="8" width="18" height="12" rx="3"/><path d="M7 8l9-5"/><circle cx="8.5" cy="14" r="2.5"/><path d="M14.5 12.5h3.5M14.5 15.5h3.5"/>`,
  lift: `<circle cx="12" cy="12" r="9"/><path d="M12 16.5V8M8.5 11.5L12 8l3.5 3.5"/>`,
  letter: `<rect x="3" y="6" width="18" height="13" rx="2.5"/><path d="M3.5 8l8.5 6 8.5-6"/>`,
  agents: `<circle cx="8.5" cy="9" r="3"/><circle cx="17" cy="10.5" r="2.3"/><path d="M2.5 19c.6-3.2 2.9-5 6-5s5.4 1.8 6 5M16 15c2.6-.3 4.6 1.2 5.2 3.8"/>`,
  heart: `<path d="M12 20s-7.5-4.6-7.5-10.2A4.2 4.2 0 0112 7.2a4.2 4.2 0 017.5 2.6C19.5 15.4 12 20 12 20z"/>`,
  flag: `<path d="M5 21V4M5 4h12l-2.5 4 2.5 4H5"/>`,
  swords: `<path d="M5 4l10 10M19 4L9 14M4 20l4-4M20 20l-4-4M6.5 13.5l4 4M17.5 13.5l-4 4"/>`,
  dice: `<rect x="4" y="4" width="16" height="16" rx="4"/><circle cx="9" cy="9" r="0.8"/><circle cx="15" cy="15" r="0.8"/><circle cx="12" cy="12" r="0.8"/>`,
  crown: `<path d="M4 18L3 8l5 4 4-7 4 7 5-4-1 10z"/>`,
  lock: `<rect x="5" y="11" width="14" height="9" rx="2.5"/><path d="M8 11V8a4 4 0 018 0v3"/>`,
  star: `<path d="M12 3.5l2.6 5.3 5.9.9-4.3 4.1 1 5.8L12 16.9l-5.2 2.7 1-5.8-4.3-4.1 5.9-.9z"/>`,
  drop: `<path d="M12 3c4 5 6 8.2 6 11a6 6 0 01-12 0c0-2.8 2-6 6-11z"/>`,
  wind: `<path d="M3 9h10a3 3 0 10-3-3M3 14h15a3 3 0 11-3 3M3 19h6"/>`,
  folder: `<path d="M3 7a2 2 0 012-2h4l2 2.5h8a2 2 0 012 2V17a2 2 0 01-2 2H5a2 2 0 01-2-2z"/>`,
  turns: `<path d="M4 12a8 8 0 0113.7-5.6L20 8.5M20 4v4.5h-4.5M20 12a8 8 0 01-13.7 5.6L4 15.5M4 20v-4.5h4.5"/>`,
};
const ic = (k: string) =>
  `<svg class="ic" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round">${ICONS[k] ?? ""}</svg>`;

/** A progress ring, `p` in 0..1, with `label` in the middle. */
function ring(p: number, label: string, color = "#ffd166"): string {
  const c = 2 * Math.PI * 34;
  return `<div class="ring" style="--rc:${color}"><svg viewBox="0 0 80 80"><circle class="trk" cx="40" cy="40" r="34"/>
    <circle class="val" cx="40" cy="40" r="34" stroke-dasharray="${c.toFixed(1)}" stroke-dashoffset="${(c * (1 - Math.max(0, Math.min(1, p)))).toFixed(1)}" transform="rotate(-90 40 40)"/></svg><b>${label}</b></div>`;
}

/** Count a number up (or down) to `n`. */
function countTo(el: HTMLElement, n: number): void {
  const from = Number(el.dataset.n ?? 0);
  el.dataset.n = String(n);
  if (from === n) {
    el.textContent = String(n);
    return;
  }
  const t0 = performance.now();
  const step = (now: number) => {
    if (el.dataset.n !== String(n)) return;
    const k = Math.min(1, (now - t0) / 700);
    el.textContent = String(Math.round(from + (n - from) * (1 - Math.pow(1 - k, 3))));
    if (k < 1) requestAnimationFrame(step);
  };
  requestAnimationFrame(step);
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

  // Modes: a card each, with a little picture of who shows up.
  $("modes").innerHTML = MODES()
    .map(([id, name, hint]) => `<button data-mode="${id}" class="mode${cfg.mode === id ? " on" : ""}">${MODE_GLYPHS[id] ?? ""}<b>${name}</b><small>${hint}</small></button>`)
    .join("");
  $("modes").querySelectorAll<HTMLButtonElement>("button").forEach((b) => {
    b.onclick = () => act("/v1/mode", { mode: b.dataset.mode });
  });

  // The two team pets stand in the hero; projects with a pet of their own are listed here.
  void renderStage();
  const projs = Object.entries(cfg.selection.projects ?? {});
  $("current-wrap").hidden = projs.length === 0;
  $("current").innerHTML = projs
    .map(([proj, pack]) => `<span class="pj">${ic("folder")}<i>${esc(proj)}</i><b>${esc(packName(pack))}</b>
      <button data-unproj="${esc(proj)}" title="${tr("恢复用阵营宠物", "Go back to the team pet")}">✕</button></span>`)
    .join("");
  $("current").querySelectorAll<HTMLButtonElement>("[data-unproj]").forEach((b) => {
    b.onclick = () => act("/v1/unselect", { target: `project:${b.dataset.unproj}` });
  });
  countTo($("ct-pets"), packs.length);

  // Tag filter.
  const tags = [...new Set(packs.flatMap((p) => p.tags))].sort();
  const chip = (id: string, label: string) => `<span class="chip ${tagFilter === id ? "on" : ""}" data-tag="${esc(id)}">${esc(label)}</span>`;
  $("tags").innerHTML = [chip("", tr("全部", "All")), chip("★", tr("★ 收藏", "★ Favorites")), ...tags.map((t) => chip(t, t))].join("");
  $("petcount").textContent = tr(`${visiblePacks().length} 只`, `${visiblePacks().length} pets`);
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

// ------------------------------------------------------------ hero

let stageShop: Shop | null = null;
const stageSprites: Record<string, Sprites | null> = {};
const stageCanvases: Record<string, HTMLCanvasElement> = {};
let stageOn = false;

/** The two team pets in the hero, in whatever they are wearing. A click takes that pet to the fitting room. */
async function renderStage() {
  const box = $("stage");
  if (!box.childElementCount) {
    box.innerHTML = CAMPS.map(([camp, label]) => `<button class="duty" data-camp="${camp}" title="${tr("去试衣间", "To the fitting room")}"><canvas></canvas><span class="who">${label}</span><b></b></button>`).join("");
    for (const [camp] of CAMPS) {
      const el = box.querySelector<HTMLButtonElement>(`[data-camp="${camp}"]`)!;
      stageCanvases[camp] = el.querySelector("canvas")!;
      el.onclick = () => {
        shopWant = cfg.in_use[camp];
        go("shop");
      };
    }
  }
  for (const [camp] of CAMPS) {
    const pack = cfg.in_use[camp];
    box.querySelector(`[data-camp="${camp}"] b`)!.textContent = packName(pack);
    stageSprites[camp] = pack ? await previews.sprites(pack, rev).catch(() => null) : null;
  }
  stageShop = await getJson<Shop>(conn, "/v1/shop").catch(() => stageShop);
  if (!stageOn) {
    stageOn = true;
    requestAnimationFrame(stageTick);
  }
}

function stageTick(now: number): void {
  requestAnimationFrame(stageTick);
  const dpr = devicePixelRatio || 1;
  for (const [camp] of CAMPS) {
    const cv = stageCanvases[camp];
    if (!cv) continue;
    if (cv.width !== Math.round(132 * dpr)) {
      cv.width = Math.round(132 * dpr);
      cv.height = Math.round(132 * dpr);
    }
    const g = cv.getContext("2d")!;
    g.setTransform(dpr, 0, 0, dpr, 0, 0);
    g.clearRect(0, 0, 132, 132);
    const sp = stageSprites[camp];
    if (!sp) continue;
    const fit = stageShop ? outfitOf(stageShop, cfg.in_use[camp]) : { hat: undefined, wear: {} };
    drawDressed(g, sp, 132, 132, fit.wear, fit.hat, now, 0.74);
  }
}

/** The numbers in the hero: fish kinds, coins, achievements. */
async function refreshCounters() {
  const [st, pr] = await Promise.all([
    getJson<Stats>(conn, "/v1/stats").catch(() => null),
    getJson<Progress>(conn, "/v1/progress").catch(() => null),
  ]);
  if (st) {
    countTo($("ct-fish"), st.fish.filter((f) => f.count > 0).length);
    $("ct-fish-of").textContent = `/${st.fish.length}`;
    countTo($("ct-coins"), st.wallet);
  }
  if (pr) {
    countTo($("ct-ach"), pr.achievements.filter((a) => a.unlocked_at).length);
    $("ct-ach-of").textContent = `/${pr.achievements.length}`;
  }
}

/** Cards lean toward the pointer, and a glare follows it. */
function setupTilt(root: HTMLElement, sel: string): void {
  root.addEventListener("mousemove", (e) => {
    const c = (e.target as HTMLElement).closest<HTMLElement>(sel);
    if (!c) return;
    const b = c.getBoundingClientRect();
    const dx = (e.clientX - b.left) / b.width - 0.5, dy = (e.clientY - b.top) / b.height - 0.5;
    c.style.setProperty("--rx", `${(-dy * 7).toFixed(2)}deg`);
    c.style.setProperty("--ry", `${(dx * 9).toFixed(2)}deg`);
    c.style.setProperty("--gx", `${((dx + 0.5) * 100).toFixed(1)}%`);
    c.style.setProperty("--gy", `${((dy + 0.5) * 100).toFixed(1)}%`);
  });
  root.addEventListener("mouseout", (e) => {
    const c = (e.target as HTMLElement).closest<HTMLElement>(sel);
    if (!c || c.contains(e.relatedTarget as Node | null)) return;
    c.style.removeProperty("--rx");
    c.style.removeProperty("--ry");
  });
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
  for (const [i, p] of list.entries()) {
    const card = document.createElement("div");
    card.className = "pc";
    card.dataset.pack = p.id;
    card.style.setProperty("--h", String(hash(p.id) % 360));
    const canvas = await previews.mount(p.id, rev, 120);
    if (gen !== gridGen) return;
    const no = String(packs.indexOf(p) + 1).padStart(2, "0");
    const meta = [p.author, p.version ? `v${p.version}` : "", p.builtin ? tr("内置", "built-in") : ""].filter(Boolean).map(esc).join(" · ");
    const body = document.createElement("div");
    body.className = "pc-in";
    body.innerHTML = `
      <div class="pc-stage">
        <div class="inuse"></div>
        <button class="star" title="${tr("收藏", "Favorite")}">★</button>
        <span class="pc-no">No.${no}</span>
        ${p.builtin ? "" : `<button class="del" title="${tr("删除这个导入的宠物包", "Delete this imported pack")}">${tr("删除", "Delete")}</button>`}
      </div>
      <div class="pc-body">
        <div class="title"><b>${esc(p.name || p.id)}</b><span class="meta">${meta}</span></div>
        <div class="tags">${p.tags.map((t) => `<span class="tag">${esc(t)}</span>`).join("")}</div>
        <div class="assign">
          <button data-t="claude-code"><i></i>Claude Code</button>
          <button data-t="codex"><i></i>Codex</button>
        </div>
        <button class="projlink">${tr("＋ 指定给某个项目…", "+ Assign to a project…")}</button>
        <div class="projpick" hidden>
          <input list="projects-seen" placeholder="${tr("项目名，可用 * 通配", "project (* ok)")}" size="10" />
          <button class="proj">${tr("确定", "Assign")}</button>
        </div>
      </div>`;
    body.querySelector(".pc-stage")!.prepend(canvas);
    card.appendChild(body);
    // Only the first screenful rises in one by one; later cards just appear.
    card.style.animationDelay = `${Math.min(i, 12) * 35}ms, 0s`;
    grid.appendChild(card);

    const link = body.querySelector<HTMLButtonElement>(".projlink")!;
    link.onclick = () => {
      const pick = body.querySelector<HTMLElement>(".projpick")!;
      pick.hidden = !pick.hidden;
      if (!pick.hidden) pick.querySelector("input")!.focus();
    };

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
  document.querySelectorAll<HTMLElement>(".pc").forEach((card) => {
    const id = card.dataset.pack!;
    const tags: string[] = [];
    if (cfg.in_use["claude-code"] === id) tags.push(`<span class="cc">Claude Code</span>`);
    if (cfg.in_use.codex === id) tags.push(`<span class="cx">Codex</span>`);
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
      b.title = mine
        ? tr(`${targetName(t)}正在用它`, `${targetName(t)} is using it`)
        : cfg.in_use[other] === id
          ? tr(`给 ${targetName(t)}（对方阵营 ${targetName(other)} 也在用）`, `Give to ${targetName(t)} (${targetName(other)} is using it too)`)
          : tr(`给 ${targetName(t)}`, `Give to ${targetName(t)}`);
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

const RARITIES = ["common", "rare", "epic", "legendary"];
/** Hue and saturation each rarity's fish are painted around. */
const RARITY_PAINT: Record<string, [number, number]> = { common: [212, 24], rare: [214, 80], epic: [268, 74], legendary: [38, 94] };

/**
 * A fish of its own: the species id decides its build (long or stout, forked
 * or fan tail, stripes, spots or a plain flank) and nudges its colour.
 */
function fishCanvas(f: FishInfo, locked: boolean): HTMLCanvasElement {
  // Drawn on a 108×66 grid, shown a little larger.
  const W = 108, H = 66, Z = 1.18;
  const c = document.createElement("canvas");
  const dpr = devicePixelRatio || 1;
  c.width = Math.round(W * Z * dpr);
  c.height = Math.round(H * Z * dpr);
  c.style.width = `${Math.round(W * Z)}px`;
  c.style.height = `${Math.round(H * Z)}px`;
  const g = c.getContext("2d")!;
  g.scale(dpr * Z, dpr * Z);
  const k = hash(f.id);
  const [h0, sat] = RARITY_PAINT[f.rarity] ?? RARITY_PAINT.common;
  const hue = h0 + ((k % 37) - 18);
  const len = 25 + (k % 9), tall = 11 + ((k >> 4) % 7);
  const pattern = (k >> 8) % 4, forked = ((k >> 11) & 1) === 1;
  const cx = 58, cy = 33;
  const col = (dh: number, l: number, a = 1) => (locked ? `rgba(160, 160, 184, ${a})` : `hsl(${hue + dh} ${sat}% ${l}% / ${a})`);
  const body = () => {
    g.beginPath();
    g.moveTo(cx + len, cy);
    g.bezierCurveTo(cx + len * 0.5, cy - tall * 1.36, cx - len * 0.62, cy - tall * 1.08, cx - len, cy);
    g.bezierCurveTo(cx - len * 0.62, cy + tall * 1.08, cx + len * 0.5, cy + tall * 1.36, cx + len, cy);
    g.closePath();
  };
  if (!locked && (f.rarity === "epic" || f.rarity === "legendary")) {
    g.shadowColor = col(0, 62, 0.9);
    g.shadowBlur = 12;
  }
  // Tail and fins first, the body over them.
  g.fillStyle = col(-14, 44);
  const tx = cx - len + 4;
  g.beginPath();
  g.moveTo(tx, cy);
  if (forked) {
    g.lineTo(tx - 17, cy - 14);
    g.quadraticCurveTo(tx - 9, cy, tx - 17, cy + 14);
  } else {
    g.quadraticCurveTo(tx - 10, cy - 16, tx - 18, cy - 9);
    g.quadraticCurveTo(tx - 13, cy, tx - 18, cy + 9);
    g.quadraticCurveTo(tx - 10, cy + 16, tx, cy);
  }
  g.closePath();
  g.fill();
  g.beginPath();
  g.moveTo(cx + len * 0.1, cy - tall * 0.95);
  g.quadraticCurveTo(cx - len * 0.3, cy - tall - 11, cx - len * 0.62, cy - tall * 0.62);
  g.closePath();
  g.fill();
  g.beginPath();
  g.moveTo(cx + len * 0.05, cy + tall * 0.9);
  g.quadraticCurveTo(cx - len * 0.12, cy + tall + 8, cx - len * 0.36, cy + tall * 0.78);
  g.closePath();
  g.fill();
  g.shadowBlur = 0;
  const flank = g.createLinearGradient(0, cy - tall, 0, cy + tall);
  flank.addColorStop(0, col(-6, 46));
  flank.addColorStop(0.55, col(0, 62));
  flank.addColorStop(1, col(22, 86));
  g.fillStyle = flank;
  body();
  g.fill();
  if (locked) return c;
  g.save();
  body();
  g.clip();
  if (pattern === 1) {
    g.strokeStyle = col(-10, 34, 0.5);
    g.lineWidth = 4;
    for (let i = 0; i < 3; i++) {
      const x = cx - len * 0.45 + i * len * 0.42;
      g.beginPath();
      g.moveTo(x, cy - tall * 1.3);
      g.quadraticCurveTo(x - 5, cy, x, cy + tall * 1.3);
      g.stroke();
    }
  } else if (pattern === 2) {
    g.fillStyle = col(-10, 36, 0.5);
    for (let i = 0; i < 6; i++) {
      g.beginPath();
      g.arc(cx - len * 0.6 + ((k >> i) % 5) * len * 0.22, cy - tall * 0.5 + ((k >> (i + 3)) % 4) * tall * 0.32, 1.9, 0, Math.PI * 2);
      g.fill();
    }
  } else if (pattern === 3) {
    g.strokeStyle = col(30, 92, 0.75);
    g.lineWidth = 2;
    g.beginPath();
    g.moveTo(cx - len, cy + 1);
    g.quadraticCurveTo(cx, cy - 4, cx + len * 0.4, cy + 1);
    g.stroke();
  }
  // A highlight along the back.
  g.fillStyle = "rgba(255, 255, 255, 0.3)";
  g.beginPath();
  g.ellipse(cx + len * 0.05, cy - tall * 0.56, len * 0.56, tall * 0.2, -0.08, 0, Math.PI * 2);
  g.fill();
  g.restore();
  g.strokeStyle = col(-10, 36, 0.55);
  g.lineWidth = 1.4;
  g.beginPath();
  g.arc(cx + len * 0.22, cy, tall * 0.74, -1, 1);
  g.stroke();
  const ex = cx + len * 0.62, ey = cy - tall * 0.18;
  g.fillStyle = "#fff";
  g.beginPath();
  g.arc(ex, ey, 3.3, 0, Math.PI * 2);
  g.fill();
  g.fillStyle = "#1f2245";
  g.beginPath();
  g.arc(ex + 0.6, ey, 1.8, 0, Math.PI * 2);
  g.fill();
  g.fillStyle = "#fff";
  g.beginPath();
  g.arc(ex + 1.2, ey - 0.8, 0.6, 0, Math.PI * 2);
  g.fill();
  if (f.rarity === "legendary") {
    g.fillStyle = "#fff6c2";
    for (const [sx, sy, r] of [[cx - len - 4, cy - tall - 4, 3], [cx + len - 4, cy + tall + 3, 2.2], [cx + 4, cy - tall - 12, 1.8]]) {
      g.beginPath();
      g.moveTo(sx, sy - r);
      g.quadraticCurveTo(sx, sy, sx + r, sy);
      g.quadraticCurveTo(sx, sy, sx, sy + r);
      g.quadraticCurveTo(sx, sy, sx - r, sy);
      g.quadraticCurveTo(sx, sy, sx, sy - r);
      g.fill();
    }
  }
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
    <div class="ringbox">${ring(got.length / Math.max(1, st.fish.length), String(total), "#7fe0e0")}<span>${tr("一共钓到的条数", "fish caught in all")}</span></div>
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
    el.innerHTML = `<div class="tank">${locked ? "" : "<u></u><u></u>"}${shiny ? `<i class="sheen"></i>` : ""}</div>
      ${locked ? "" : `<span class="cnt">×${f.count}</span>`}
      <div class="info"><span class="rar">${RARITY_NAMES[f.rarity] ?? f.rarity}</span>
      <b>${locked ? "？？？" : esc(f.name)}</b>
      <small>${locked ? tr("还没钓到过", "Not caught yet") : esc(f.note)}</small></div>`;
    el.querySelector(".tank")!.prepend(fishCanvas(f, locked));
    grid.appendChild(el);
  });
  countTo($("ct-fish"), got.length);
}

async function renderWallet() {
  void renderSeason();
  const st = await getJson<Stats>(conn, "/v1/stats");
  const t = st.today;
  const have = st.fish.filter((f) => f.count > 0).length;
  $("bank").innerHTML = `<div class="bank"><div class="head"><i class="sheen"></i>
    <div class="bal"><span>${tr("金币余额", "COIN BALANCE")}</span><b>${st.wallet}</b><br />
      <button id="open-report">${tr("打开今日报告卡片", "Open today's report card")}</button></div>
    <div class="ringbox">${ring(st.income_today / Math.max(1, st.daily_cap), `${st.income_today}`)}<span>${tr(`今日收入 · 上限 ${st.daily_cap}`, `earned today · cap ${st.daily_cap}`)}</span></div>
    <div class="ringbox">${ring(have / Math.max(1, st.fish.length), `${have}/${st.fish.length}`, "#7fe0e0")}<span>${tr("鱼类图鉴", "fish collection")}</span></div>
  </div></div>`;
  $("open-report").onclick = async () => {
    if (!inTauri) return;
    const { invoke } = await import("@tauri-apps/api/core");
    await invoke("open_report_window");
  };
  countTo($("ct-coins"), st.wallet);

  const g = await getJson<{ affinity: number; races: Record<string, number>; duels: Record<string, number>; bets: Record<string, number> }>(conn, "/v1/games").catch(() => null);
  if (g) {
    const vs = (icon: string, name: string, m: Record<string, number> | undefined) => {
      const a = m?.["claude-code"] ?? 0, b = m?.["codex"] ?? 0;
      const bar = a + b ? `<div class="vsbar"><i style="width:${(a / (a + b)) * 100}%"></i></div>` : `<div class="vsbar none"></div>`;
      return `<div class="rec" title="Claude Code ${a} : ${b} Codex${m?.["tie"] ? tr(`，平 ${m["tie"]}`, `, ${m["tie"]} ties`) : ""}">${ic(icon)}<span>${name}</span><b>${a}</b>${bar}<b>${b}</b></div>`;
    };
    const marks: [number, string][] = [[20, tr("击掌", "high-five")], [50, tr("一起睡", "nap together")], [80, tr("合照", "photo")]];
    $("records").innerHTML = `
      <div class="box aff"><div class="lbl">${ic("heart")}${tr("两只宠物的好感度", "Affinity between the two")}</div><b>${g.affinity}</b>
        <div class="affbar"><i style="width:${Math.min(100, g.affinity)}%"></i>${marks.map(([n, l]) => `<u class="${g.affinity >= n ? "got" : ""}" style="left:${n}%" data-l="${n} ${l}"></u>`).join("")}</div></div>
      <div class="box">${vs("flag", tr("赛跑", "Races"), g.races)}${vs("swords", tr("擂台", "Duels"), g.duels)}
        <div class="rec">${ic("dice")}<span>${tr("你的下注", "Your bets")}</span><span class="plain">${tr(`赢 ${g.bets.won ?? 0} · 输 ${g.bets.lost ?? 0}`, `Won ${g.bets.won ?? 0} · Lost ${g.bets.lost ?? 0}`)}</span></div></div>`;
  } else {
    $("records").innerHTML = "";
  }

  let n = 0;
  const tile = (icon: string, label: string, value: string | number, cls = "") =>
    `<div class="tile ${cls}" style="--i:${n++}"><div class="ib">${ic(icon)}</div><div><b>${value}</b><span>${label}</span></div></div>`;
  const camp = (name: string, key: string, cls: string) => {
    const d = t.by_camp[key];
    return d ? tile("turns", `${name} · ${tr(`一次通过 ${d.clean} · 出错 ${d.errors}`, `${d.clean} first-try · ${d.errors} errors`)}`, tr(`${d.turns} 轮`, `${d.turns} turns`), cls) : "";
  };
  $("today").innerHTML =
    camp("Claude Code", "claude-code", "cc") + camp("Codex", "codex", "cx") +
    tile("clock", tr("agent 工作时长", "Agent work time"), t.work_ms ? duration(t.work_ms) : "0") +
    tile("agents", tr("最多同时在线 agent", "Peak agents online"), t.peak_agents) +
    tile("fish", tr("钓到的鱼", "Fish caught"), t.catches.filter((c) => c.outcome !== "boot" && c.outcome !== "escape").length) +
    tile("bug", tr("拍死的虫", "Bugs squashed"), `${t.bugs}${t.bosses ? ` + ${t.bosses} Boss` : ""}`) +
    tile("bolt", tr("挨的鞭子", "Lashes taken"), t.whips) +
    tile("game", tr("玩的游戏", "Games played"), t.games) +
    tile("radio", tr("听电台", "Radio time"), t.radio_s ? duration(t.radio_s * 1000) : "0") +
    tile("lift", tr("提肛", "Kegels"), t.kegel_reps ? tr(`${t.kegel_reps} 组`, `${t.kegel_reps} reps`) : "0") +
    tile("letter", tr("收到的信", "Letters"), t.letters);
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
      document.querySelector(`.pc[data-pack="${CSS.escape(id)}"]`)?.scrollIntoView({ block: "center", behavior: "smooth" });
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
  const got = p.achievements.filter((a) => a.unlocked_at);
  const earned = got.reduce((n, a) => n + a.coins, 0);
  const left = p.achievements.reduce((n, a) => n + a.coins, 0) - earned;
  $("achhead").innerHTML = `<div class="head achhead">
    <div class="num"><span>${tr("已解锁", "UNLOCKED")}</span><b>${got.length}</b><i>/ ${p.achievements.length}</i></div>
    <div class="sub">${tr(`成就已经带来 <b>${earned}</b> 金币`, `Achievements have paid <b>${earned}</b> coins`)}<br />${
      left ? tr(`剩下的还藏着 <b>${left}</b>`, `<b>${left}</b> more are still locked away`) : tr("全部拿到了", "Every one of them is yours")
    }</div>
    <div class="ringbox">${ring(got.length / Math.max(1, p.achievements.length), `${Math.round((got.length / Math.max(1, p.achievements.length)) * 100)}%`)}</div>
  </div>`;
  // Newest first, then what is still locked.
  const list = [...got.sort((a, b) => (b.unlocked_at ?? 0) - (a.unlocked_at ?? 0)), ...p.achievements.filter((a) => !a.unlocked_at)];
  $("achgrid").innerHTML = list
    .map(
      (a, i) => `<div class="ach${a.unlocked_at ? "" : " locked"}" style="--i:${i}"><div class="disc">${ic(a.unlocked_at ? "star" : "lock")}</div>
        <div class="txt"><b>${esc(a.name)}</b><small>${esc(a.desc)}</small>${
          a.unlocked_at ? `<span class="when">${new Date(a.unlocked_at).toLocaleDateString(EN ? "en-US" : "zh-CN")}</span>` : ""
        }</div><span class="coin">+${a.coins}</span></div>`,
    )
    .join("");
  countTo($("ct-ach"), got.length);
}

async function renderSeason() {
  const p = await getJson<Progress>(conn, "/v1/progress");
  const cc = p.current?.["claude-code"] ?? 0, cx = p.current?.codex ?? 0;
  const bar = cc + cx > 0 ? `<div class="vsbar"><i style="width:${(cc / (cc + cx)) * 100}%"></i></div>` : `<div class="vsbar none"></div>`;
  $("season").innerHTML = `<div class="versus">
    <div class="vs-side cc"><span>Claude Code</span><b>${cc.toFixed(2)}</b></div>
    <div class="vs-mid"><span class="wk">${esc(p.week)}</span>${bar}
      <div class="champ">${ic("crown")}${tr("上周冠军，这周戴皇冠：", "Last week's champ, in the crown: ")}<b>${p.champion ? campName(p.champion) : "—"}</b></div></div>
    <div class="vs-side cx"><span>Codex</span><b>${cx.toFixed(2)}</b></div>
  </div>`;
  $("season-history").innerHTML = p.seasons.length
    ? p.seasons.map((s) => `<span>${esc(s.week)}<b>${campName(s.winner)}</b>${s.cc.toFixed(2)} : ${s.cx.toFixed(2)}</span>`).join("")
    : `<div class="note" style="margin:0">${tr(
        "赛季每周一结算：上周每天拔河效率分加起来，高的一方这周戴皇冠。",
        "Seasons settle every Monday: last week's daily tug-of-war scores are added up, and the higher team wears the crown this week.",
      )}</div>`;
}

/** Shop sections in rack order: the wearable slots, then the rope and the lines. */
const SHOP_KINDS = [...SLOTS, "rope", "quips"];
let shopCat = "all";
/** The pet the fitting room should open on next (set by a click on the hero's stage). */
let shopWant: string | null = null;
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
  stageShop = s;
  $("shop-wallet").textContent = String(s.wallet);
  countTo($("ct-coins"), s.wallet);
  const sel = $("shop-pack") as HTMLSelectElement;
  const keep = shopWant ?? (sel.value || cfg.in_use["claude-code"]);
  shopWant = null;
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
    // The day from 6:00 to midnight, with a pin at each reminder and a line at now.
    const at = (hm: string) => {
      const [h, m] = hm.split(":").map(Number);
      return Math.max(0, Math.min(100, ((h * 60 + (m || 0) - 360) / 1080) * 100));
    };
    const d = new Date();
    const nowAt = at(`${d.getHours()}:${d.getMinutes()}`);
    const track = `<div class="daytrack">${[6, 9, 12, 15, 18, 21, 24].map((h) => `<u style="left:${at(`${h}:0`)}%">${h}:00</u>`).join("")}
      ${d.getHours() >= 6 ? `<span class="now" style="left:${nowAt}%"></span>` : ""}${r.kegel.map((t) => `<span class="pin" style="left:${at(t)}%" data-t="${t}"></span>`).join("")}</div>`;
    const steps: [string, string][] = [
      ["<b>找对肌肉：</b>就是憋尿、忍住放屁时用力的那块肌肉（盆底肌）。", "<b>Find the muscle:</b> the one you use to hold in pee or gas (the pelvic floor)."],
      ["<b>收紧上提：</b>宠物喊「收紧 ⬆」时，把肛门和会阴往上、往里收，保持 3 秒。", "<b>Squeeze and lift:</b> on “Squeeze ⬆”, pull the area up and in, and hold for 3 s."],
      ["<b>完全放松：</b>喊「放松 ⬇」时彻底松开 3 秒。放松和收紧一样重要。", "<b>Let go fully:</b> on “Relax ⬇”, release completely for 3 s. Relaxing matters as much as squeezing."],
      ["<b>呼吸自然：</b>别憋气；肚子、屁股和大腿尽量不跟着用力。", "<b>Keep breathing:</b> don't hold your breath; keep your belly, buttocks and thighs loose."],
      ["坐着、站着都能做，旁边的人看不出来。", "Sitting or standing both work, and nobody can tell."],
    ];
    box.innerHTML = `
      <section class="hcard">
        <div class="hhead"><div class="ib">${ic("lift")}</div><div><h3>${tr("提肛提醒", "Kegel reminders")}</h3></div></div>
        <div class="note">${tr("每天到点，宠物会转过身、屁股对着你做一组：收紧时夹紧上提、尾巴竖起，放松时松开、尾巴摇摇；头顶有「收紧 ⬆」「放松 ⬇」和倒计时，还有提示音，跟着它做就行。点一下宠物可以跳过。",
          "At each time, your pet turns around and does a set with its bottom to you: squeezing and lifting with its tail up, then letting go with a wag. “Squeeze ⬆” / “Relax ⬇” and a countdown show above it, with a soft beep. Click the pet to skip.")}</div>
        ${track}
        <div class="hrow"><span>${tr("提醒时间", "Times")}</span>
          <span class="chips">${r.kegel.map((t) => `<span class="chip">${t}<button data-del="${t}" title="${tr("删除", "Remove")}">✕</button></span>`).join("") || `<span class="note" style="margin:0">${tr("还没设，提醒是关的", "None yet — reminders are off")}</span>`}</span>
          <input type="time" id="kg-t" value="${r.kegel.length ? "15:30" : "10:30"}" /><button id="kg-add">${tr("添加", "Add")}</button></div>
        <div class="hrow"><span>${tr("每次", "Each set")}</span>
          <select id="kg-reps">${[5, 10, 15, 20].map((n) => `<option value="${n}"${n === r.kegel_reps ? " selected" : ""}>${tr(`${n} 组`, `${n} reps`)}</option>`).join("")}</select>
          <span class="note" style="margin:0">${tr(`约 ${mins} 分钟`, `about ${mins} min`)}</span>
          <span class="sp"></span><button id="kg-try" class="primary">${tr("现在试一组", "Try a set now")}</button></div>
        <details class="howto" open><summary>${tr("动作要领", "How to do it")}</summary>
          <div class="steps">${steps.map(([zh, en]) => `<div>${tr(zh, en)}</div>`).join("")}</div>
          <div class="note">${tr("刚开始每次 5 组，习惯后 10–15 组，每天 2–3 次。如果有盆底或肛肠方面的不适，先问问医生。",
          "Start with 5 reps, build up to 10–15, two or three times a day. If you have pelvic or rectal problems, ask a doctor first.")}</div></details>
      </section>
      <section class="hcard">
        <h3>${tr("其他提醒", "Other reminders")}</h3>
        <label class="sw"><div class="ib">${ic("drop")}</div><div class="txt"><b>${tr("喝水", "Water")}</b><small>${tr("空闲时每小时提醒一次", "Once an hour while idle")}</small></div>
          <input type="checkbox" id="rm-water"${r.water ? " checked" : ""} /><span class="knob"></span></label>
        <label class="sw"><div class="ib">${ic("wind")}</div><div class="txt"><b>${tr("深呼吸", "Breathing")}</b><small>${tr("10 分钟内等你超过 6 次时，一起做 30 秒深呼吸", "30 s of deep breaths when you're waited on 6+ times in 10 min")}</small></div>
          <input type="checkbox" id="rm-breathe"${r.breathe ? " checked" : ""} /><span class="knob"></span></label>
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

/** Slide the pill under the active tab. */
function moveInd(): void {
  const on = document.querySelector<HTMLElement>("#tabs button.on");
  const ind = document.querySelector<HTMLElement>("#tabs .ind");
  if (!on || !ind) return;
  ind.style.left = `${on.offsetLeft}px`;
  ind.style.width = `${on.offsetWidth}px`;
}

/** Open a tab: the pill slides over, the hero takes the tab's colour. */
function go(tab: string): void {
  if (!(tab in TAB_HUES)) return;
  document.querySelectorAll<HTMLButtonElement>("#tabs button").forEach((x) => x.classList.toggle("on", x.dataset.tab === tab));
  moveInd();
  $("hero").style.setProperty("--h", String(TAB_HUES[tab]));
  for (const id of Object.keys(TAB_HUES)) $(`tab-${id}`).hidden = id !== tab;
  if (tab === "health") void renderHealth();
  if (tab === "fish") void renderFish();
  if (tab === "wallet") void renderWallet();
  if (tab === "ach") void renderAchievements();
  if (tab === "shop") void renderShop();
}

function setupTabs() {
  document.querySelectorAll<HTMLButtonElement>("#tabs button").forEach((b) => {
    b.insertAdjacentHTML("afterbegin", ic(TAB_ICONS[b.dataset.tab!] ?? ""));
    b.onclick = () => go(b.dataset.tab!);
  });
  document.querySelectorAll<HTMLButtonElement>(".ct[data-go]").forEach((b) => (b.onclick = () => go(b.dataset.go!)));
  moveInd();
  window.addEventListener("resize", moveInd);
}

async function main() {
  conn = await connect();
  await initLang(conn);
  previews = new Previews(conn);
  setupTabs();
  setupTilt($("grid"), ".pc");
  await setupImport();
  await load();
  void refreshCounters();
  // gallery.html?tab=shop opens a tab directly.
  const tab = new URLSearchParams(location.search).get("tab");
  if (tab) go(tab);
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
}

main().catch((e) => {
  document.body.innerHTML = `<div class="banner">${tr("图鉴加载失败：", "Failed to load the Gallery: ")}${String(e)}</div>`;
});
