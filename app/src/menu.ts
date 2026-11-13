// The right-click menu: a short card beside the pet. A group opens a second
// card next to its row; groups inside that one slide within it.

interface Item {
  id?: string;
  label: string;
  icon?: string;
  enabled: boolean;
  checked?: boolean;
  hint?: string;
  glyph?: string;
  dot?: boolean;
  children?: Item[];
}

interface Open {
  model: { pet: string; title: string; skin?: string; sub?: string; camp?: string | null; items: Item[] };
  /** 记一件事: the line to write in instead of the menu (`text`: what was selected, to start from). */
  note?: { text?: string | null };
  /** Top of the pet's window. */
  top?: number;
  cx: number;
  cy: number;
  petH: number;
  w: number;
  h: number;
}

import { applySkin } from "./skin";
import { t as tr } from "./i18n";

const inTauri = "__TAURI_INTERNALS__" in window;
const CAMPS: Record<string, [string, string]> = { "claude-code": ["Claude", "#e0794f"], codex: ["Codex", "#2f9a8a"] };
const stage = document.getElementById("stage")!;
let closing = false;

async function call(cmd: string, args?: Record<string, unknown>) {
  if (!inTauri) return console.log(cmd, args ?? "");
  const { invoke } = await import("@tauri-apps/api/core");
  await invoke(cmd, args);
}

function el<K extends keyof HTMLElementTagNameMap>(tag: K, cls?: string, text?: string): HTMLElementTagNameMap[K] {
  const e = document.createElement(tag);
  if (cls) e.className = cls;
  if (text != null) e.textContent = text;
  return e;
}

/** Line icons, drawn on a 24 grid. */
const GLYPHS: Record<string, string> = {
  music: '<path d="M9 18V6l10-2v12"/><circle cx="6.5" cy="18" r="2.5"/><circle cx="16.5" cy="16" r="2.5"/>',
  // a sweet in its wrapper
  sweet:'<ellipse cx="12" cy="12" rx="5" ry="4"/><path d="M7.2 10.6 3 8.5l1.4 3.5L3 15.500l4.200-2.100M16.800 10.600 21 8.500l-1.400 3.500L21 15.500l-4.200-2.100"/>',
  mail: '<rect x="3" y="5.5" width="18" height="13" rx="3"/><path d="M4 8l8 5.5L20 8"/>',
  home: '<path d="M4 11.5 12 5l8 6.5V19a1.5 1.5 0 0 1-1.5 1.5h-13A1.5 1.5 0 0 1 4 19z"/><path d="M10 20.5v-5h4v5"/>',
  moon: '<path d="M20 14.5A8 8 0 0 1 9.5 4a8 8 0 1 0 10.5 10.5z"/>',
  more: '<circle cx="5.5" cy="12" r="1.3"/><circle cx="12" cy="12" r="1.3"/><circle cx="18.5" cy="12" r="1.3"/>',
  chev: '<path d="M9 6l6 6-6 6"/>',
  back: '<path d="M15 6l-6 6 6 6"/>',
  check: '<path d="M5 12.5l4.5 4.5L19 7.5"/>',
};

function glyph(name: string, size = 20, width = 1.9, cls = ""): SVGSVGElement {
  const t = document.createElement("template");
  t.innerHTML = `<svg aria-hidden="true" class="${cls}" width="${size}" height="${size}" viewBox="0 0 24 24" fill="none" stroke-width="${width}" stroke-linecap="round" stroke-linejoin="round">${GLYPHS[name] ?? ""}</svg>`;
  return t.content.firstElementChild as SVGSVGElement;
}

function close() {
  if (closing) return;
  closing = true;
  stage.querySelectorAll(".card").forEach((c) => c.classList.remove("in"));
  void call("menu_close");
}

let last: Open | null = null;

function pick(id: string) {
  // 记一件事 stays here: the menu turns into the line you write in.
  if (id.startsWith("note|") && last) return renderNote({ ...last, note: {} });
  closing = true;
  void call("menu_pick", { id });
}

const clamp = (v: number, lo: number, hi: number) => Math.max(lo, Math.min(hi, v));

/** One row of either card. `onGroup` opens what is behind a group. */
function row(node: Item, onGroup: (n: Item, b: HTMLButtonElement) => void): HTMLButtonElement {
  const b = el("button", `row${node.checked ? " checked" : ""}`);
  b.type = "button";
  const has = !!node.children?.length;
  b.disabled = !node.enabled || (!node.id && !has);
  if (node.glyph) b.append(glyph(node.glyph));
  b.append(el("span", "tx", node.label));
  if (node.hint) b.append(el("span", "hint", node.hint));
  if (node.dot) b.append(el("span", "dot"));
  if (node.checked) b.append(glyph("check", 16, 2.4, "ok"));
  if (has) b.append(glyph("chev", 16, 2.2));
  if (node.checked != null) {
    b.setAttribute("role", "menuitemcheckbox");
    b.setAttribute("aria-checked", String(node.checked));
  }
  b.title = node.hint ? `${node.label} · ${node.hint}` : node.label;
  b.addEventListener("click", (e) => {
    e.stopPropagation();
    if (has) onGroup(node, b);
    else if (node.id) pick(node.id);
  });
  return b;
}

/** The bubble over its head with one line to write in. Enter writes it down; Esc, or a click elsewhere, lets it go. */
function renderNote(o: Open) {
  closing = false;
  last = o;
  applySkin(o.model.skin);
  stage.innerHTML = "";
  const card = el("div", "card note");
  const input = el("input");
  input.maxLength = 120;
  input.placeholder = tr("记一件事", "Write something down");
  input.value = o.note?.text?.trim().slice(0, 120) ?? "";
  const hint = el("div", "nh", tr("回车记下 · Esc 算了 · 「等 Claude 做完…」到时提醒", "Enter keeps it · Esc lets go · “when Claude is done…” waits for it"));
  card.append(input, hint);
  card.addEventListener("click", (e) => e.stopPropagation());
  stage.append(card);
  // Over the pet's head, where its bubble goes.
  const W = card.offsetWidth, H = card.offsetHeight;
  const head = (o.top ?? o.cy - o.petH * 0.58) + 52;
  const left = clamp(o.cx - W / 2, 8, o.w - W - 8);
  Object.assign(card.style, { left: `${left}px`, top: `${clamp(head - H - 10, 8, o.h - H - 8)}px` });
  card.style.setProperty("--tail", `${clamp(o.cx - left, 20, W - 20)}px`);
  stage.onclick = () => close();
  let sending = false;
  input.addEventListener("keydown", async (e) => {
    if (e.key !== "Enter" || e.isComposing || sending) return;
    e.preventDefault();
    const text = input.value.trim();
    if (!text) return close();
    sending = true;
    const r = await writeDown(o.model.pet, text);
    if (r.ok) return close();
    sending = false;
    hint.textContent = r.message ?? tr("没记上。", "Couldn't keep it.");
    hint.classList.add("bad");
  });
  requestAnimationFrame(() => {
    card.classList.add("in");
    input.focus();
    input.setSelectionRange(input.value.length, input.value.length);
  });
}

async function writeDown(pet: string, text: string): Promise<{ ok: boolean; message?: string }> {
  if (!inTauri) return { ok: true };
  const { connect, postJson } = await import("./api");
  const conn = await connect();
  return postJson<{ ok: boolean; message?: string }>(conn, "/v1/todos", { text, pet }).catch(() => ({ ok: false }));
}

function render(o: Open) {
  if (o.note) return renderNote(o);
  last = o;
  closing = false;
  // The menu of the pet that was right-clicked wears that pet's skin.
  applySkin(o.model.skin);
  stage.innerHTML = "";

  const main = el("div", "card main");
  main.setAttribute("role", "menu");
  const who = el("div", "who");
  const nm = el("div", "nm");
  nm.append(el("span", "", o.model.title));
  const camp = CAMPS[o.model.camp ?? ""];
  if (camp) {
    const tag = el("span", "camp");
    const dot = el("i");
    dot.style.background = camp[1];
    tag.append(dot, document.createTextNode(camp[0]));
    nm.append(tag);
  }
  who.append(nm);
  if (o.model.sub) who.append(el("div", "sb", o.model.sub));
  main.append(who);

  let sub: HTMLDivElement | null = null;
  let from: HTMLButtonElement | null = null;
  let timer = 0;
  const drop = () => {
    sub?.remove();
    sub = null;
    from?.classList.remove("open");
    from = null;
  };
  const openSub = (node: Item, b: HTMLButtonElement) => {
    if (from === b) return;
    drop();
    from = b;
    b.classList.add("open");
    sub = subCard(node, b, main, o);
    sub.addEventListener("pointerenter", () => clearTimeout(timer));
  };

  for (const node of o.model.items) {
    const has = !!node.children?.length;
    const b = row(node, (n, btn) => (from === btn ? drop() : openSub(n, btn)));
    // Resting on a group opens it; moving to another row closes it, with a
    // moment's grace so the pointer can cross rows on its way to the card.
    b.addEventListener("pointerenter", () => {
      clearTimeout(timer);
      if (has) timer = window.setTimeout(() => openSub(node, b), sub ? 220 : 90);
      else if (sub) timer = window.setTimeout(drop, 260);
    });
    b.addEventListener("pointerleave", () => {
      if (has && from !== b) clearTimeout(timer);
    });
    main.append(b);
  }
  stage.append(main);

  // Beside the pet, on the side with more room, its foot near the pet's feet.
  const W = main.offsetWidth;
  const H = main.offsetHeight;
  const gap = clamp(o.petH * 0.5, 80, 120);
  const right = o.cx + gap + W + 150 <= o.w || o.cx - gap - W < 8;
  const left = clamp(right ? o.cx + gap : o.cx - gap - W, 8, o.w - W - 8);
  const top = clamp(o.cy + o.petH * 0.3 - H, 8, o.h - H - 8);
  Object.assign(main.style, { left: `${left}px`, top: `${top}px` });
  main.style.setProperty("--ox", right ? "0" : "100%");

  main.addEventListener("click", (e) => e.stopPropagation());
  stage.onclick = (e) => {
    if (sub && sub.contains(e.target as globalThis.Node)) return;
    close();
  };
  requestAnimationFrame(() => requestAnimationFrame(() => main.classList.add("in")));
}

/** The second card, next to the row that opened it. */
function subCard(root: Item, from: HTMLButtonElement, main: HTMLElement, o: Open): HTMLDivElement {
  const card = el("div", "card sub");
  card.setAttribute("role", "menu");
  card.addEventListener("click", (e) => e.stopPropagation());
  stage.append(card);

  const stack: Item[] = [root];
  const place = () => {
    const m = main.getBoundingClientRect();
    const r = from.getBoundingClientRect();
    const w = card.offsetWidth;
    const h = card.offsetHeight;
    // Overlap the first card a little, like a drawer pulled out of it.
    let left = m.right - 14;
    if (left + w > o.w - 8) left = m.left - w + 14;
    card.style.left = `${clamp(left, 8, o.w - w - 8)}px`;
    card.style.top = `${clamp(r.top - 9, 8, o.h - h - 8)}px`;
  };
  const show = () => {
    const node = stack[stack.length - 1];
    card.innerHTML = "";
    if (stack.length > 1) {
      const head = el("div", "head");
      const back = el("button", "back");
      back.type = "button";
      back.setAttribute("aria-label", "返回");
      back.append(glyph("back", 16, 2.2));
      back.addEventListener("click", () => {
        stack.pop();
        show();
      });
      head.append(back, el("span", "", node.label));
      card.append(head);
    }
    for (const c of node.children ?? []) {
      card.append(
        row(c, (n) => {
          stack.push(n);
          show();
        }),
      );
    }
    place();
  };
  show();
  requestAnimationFrame(() => requestAnimationFrame(() => card.classList.add("in")));
  return card;
}

window.addEventListener("keydown", (e) => {
  if (e.key === "Escape") close();
});

if (inTauri) {
  // The menu's words come from the host; the line to write in speaks the same language.
  void import("./api").then(async ({ connect }) => (await import("./i18n")).initLang(await connect(), { reload: false })).catch(() => {});
  void import("@tauri-apps/api/webviewWindow").then(({ getCurrentWebviewWindow }) => {
    const win = getCurrentWebviewWindow();
    void win.listen<Open>("menu-open", (e) => render(e.payload));
    // Clicking anywhere else (another app, the taskbar) closes the menu.
    void win.onFocusChanged(({ payload }) => {
      if (!payload) close();
    });
  });
} else {
  // Outside the app (a browser on the dev server): a sample to look at.
  const q = new URLSearchParams(location.search);
  const kids = (...names: string[]): Item[] => names.map((label, i) => ({ id: `x${i}`, label, enabled: true }));
  const sample: Open = {
    cx: Number(q.get("cx") ?? 260),
    cy: Number(q.get("cy") ?? innerHeight - 110),
    petH: 220,
    w: innerWidth,
    h: innerHeight,
    model: {
      pet: "p",
      title: q.get("skin") === "mashu" ? "麻薯" : "紫梦",
      skin: q.get("skin") ?? "zimeng",
      sub: "在看 api-server · 今天 2 小时 14 分",
      camp: q.get("camp") ?? "claude-code",
      items: [
        { label: "陪我一下", glyph: "music", enabled: true, children: kids("跳支舞", "弹首曲子", "放电台", "做一组提肛") },
        { id: "feed", label: "喂它", glyph: "sweet", hint: "罐里 3 颗", enabled: true },
        { id: "letter", label: "有一封信", glyph: "mail", dot: true, enabled: true },
        { id: "room", label: "小屋", glyph: "home", hint: "今天、图鉴、设置", enabled: true },
        { id: "focus", label: "安静一会儿", glyph: "moon", checked: q.has("quiet"), enabled: true },
        {
          label: "更多",
          glyph: "more",
          enabled: true,
          children: [
            { label: "多喂几颗", enabled: true, children: kids("×5", "×10") },
            { label: "用量", enabled: true, children: [{ label: "5 小时额度 已用 42% · 2 小时后重置", enabled: false }] },
            { label: "设置", enabled: true, children: kids("大小", "话多话少", "语言", "全部设置…") },
            ...kids("图鉴", "报告", "隐藏"),
          ],
        },
      ],
    },
  };
  // `?note=` : the line to write in, as the hotkey opens it with something selected.
  render(q.has("note") ? { ...sample, top: sample.cy - 160, note: { text: q.get("note") } } : sample);
  const open = q.get("open");
  if (open) setTimeout(() => stage.querySelectorAll<HTMLButtonElement>(".main .row")[Number(open)]?.click(), 50);
}
