// The right-click menu: a ring of buttons around the pet. Groups open a card
// beside their button; nested groups slide inside the card.

import { invoke } from "@tauri-apps/api/core";
import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";

interface Item {
  id?: string;
  label: string;
  icon?: string;
  enabled: boolean;
  checked?: boolean;
  hint?: string;
  children?: Item[];
}

interface Open {
  model: { pet: string; title: string; camp?: string | null; items: Item[] };
  cx: number;
  cy: number;
  petH: number;
  w: number;
  h: number;
}

const ACCENTS: Record<string, string> = { "claude-code": "#b5704f", codex: "#4f7f6f" };
const stage = document.getElementById("stage")!;
const win = getCurrentWebviewWindow();
let closing = false;

function el<K extends keyof HTMLElementTagNameMap>(tag: K, cls?: string, text?: string): HTMLElementTagNameMap[K] {
  const e = document.createElement(tag);
  if (cls) e.className = cls;
  if (text != null) e.textContent = text;
  return e;
}

function close() {
  if (closing) return;
  closing = true;
  stage.classList.remove("open");
  void invoke("menu_close");
}

function pick(id: string) {
  closing = true;
  void invoke("menu_pick", { id });
}

/**
 * Angles (degrees, screen coordinates: 0 = right, 90 = down) for `n`
 * buttons at radius `r` that keep every button on screen. Pets usually stand
 * on the taskbar, so the upper arc is the common case.
 */
function layout(n: number, o: Open): { r: number; angles: number[] } {
  const m = 44; // half a button plus its label
  const gapPx = 76; // button spacing along the arc
  const fits = (r: number, d: number) => {
    const x = o.cx + r * Math.cos((d * Math.PI) / 180);
    const y = o.cy + r * Math.sin((d * Math.PI) / 180);
    return x > m && x < o.w - m && y > m - 6 && y < o.h - m;
  };
  const base = Math.max(100, Math.min(140, o.petH * 0.62));
  for (let r = base; r <= 340; r += 8) {
    const ok = Array.from({ length: 360 }, (_, d) => fits(r, d));
    if (ok.every(Boolean) && (2 * Math.PI * r) / n >= gapPx) {
      return { r, angles: Array.from({ length: n }, (_, i) => -90 + (360 * i) / n) };
    }
    // Longest run of free angles (wrapping around).
    let best = { start: 0, len: 0 };
    for (let s = 0; s < 360; s++) {
      if (!ok[s] || ok[(s + 359) % 360]) continue;
      let len = 0;
      while (len < 360 && ok[(s + len) % 360]) len++;
      if (len > best.len) best = { start: s, len };
    }
    const step = ((gapPx / r) * 180) / Math.PI;
    if (best.len - 4 >= step * (n - 1)) {
      const spread = Math.min((best.len - 4) / Math.max(1, n - 1), step * 1.25);
      const mid = best.start + best.len / 2;
      const from = mid - (spread * (n - 1)) / 2;
      return { r, angles: Array.from({ length: n }, (_, i) => from + spread * i) };
    }
  }
  return { r: base, angles: Array.from({ length: n }, (_, i) => 180 + (180 * i) / Math.max(1, n - 1)) };
}

function render(o: Open) {
  closing = false;
  stage.innerHTML = "";
  stage.classList.remove("open");
  const accent = ACCENTS[o.model.camp ?? ""] ?? "#a88454";
  document.documentElement.style.setProperty("--accent", accent);
  document.documentElement.style.setProperty("--accent-soft", `${accent}1f`);

  const { r, angles } = layout(o.model.items.length, o);
  for (const [cls, size] of [["disc", r * 2 + 90], ["ring", r * 2]] as const) {
    const h = el("div", `halo ${cls}`);
    Object.assign(h.style, { left: `${o.cx}px`, top: `${o.cy}px`, width: `${size}px`, height: `${size}px` });
    stage.append(h);
  }

  // Title above the topmost button.
  const ys = angles.map((d) => o.cy + r * Math.sin((d * Math.PI) / 180));
  const title = el("div", "title");
  title.append(el("i"), document.createTextNode(o.model.title));
  const ty = Math.max(30, Math.min(...ys) - 34);
  Object.assign(title.style, { left: `${Math.max(90, Math.min(o.w - 90, o.cx))}px`, top: `${ty}px` });
  stage.append(title);

  let active: HTMLButtonElement | null = null;
  let card: HTMLDivElement | null = null;
  const dropCard = () => {
    card?.remove();
    card = null;
    active?.classList.remove("active");
    active = null;
  };

  o.model.items.forEach((node, i) => {
    const rad = (angles[i] * Math.PI) / 180;
    const x = o.cx + r * Math.cos(rad);
    const y = o.cy + r * Math.sin(rad);
    const b = el("button", "btn");
    b.style.left = `${x}px`;
    b.style.top = `${y}px`;
    b.style.setProperty("--fx", `${o.cx - x}px`);
    b.style.setProperty("--fy", `${o.cy - y}px`);
    b.style.setProperty("--d", `${i * 28}ms`);
    b.append(el("span", "ic", node.icon ?? "•"), el("span", "lb", node.label));
    if (node.children?.length) b.append(el("span", "more", "+"));
    b.disabled = !node.enabled;
    b.addEventListener("click", (e) => {
      e.stopPropagation();
      if (node.children?.length) {
        if (active === b) return dropCard();
        dropCard();
        active = b;
        b.classList.add("active");
        card = openCard(node, x, y, o);
      } else if (node.id) pick(node.id);
    });
    stage.append(b);
  });

  stage.onclick = (e) => {
    if (card && card.contains(e.target as globalThis.Node)) return;
    close();
  };
  requestAnimationFrame(() => requestAnimationFrame(() => stage.classList.add("open")));
}

/** A card beside the button at (bx, by), pushed outward from the pet. */
function openCard(root: Item, bx: number, by: number, o: Open): HTMLDivElement {
  const card = el("div", "card");
  const W = 228;
  const right = bx >= o.cx;
  let left = right ? bx + 40 : bx - 40 - W;
  if (left + W > o.w - 8) left = bx - 40 - W;
  if (left < 8) left = bx + 40;
  left = Math.max(8, Math.min(o.w - W - 8, left));
  card.style.left = `${left}px`;
  card.addEventListener("click", (e) => e.stopPropagation());
  stage.append(card);

  const stack: Item[] = [root];
  const place = () => {
    const h = card.offsetHeight;
    const top = Math.max(8, Math.min(o.h - h - 8, by - Math.min(h / 2, 60)));
    card.style.top = `${top}px`;
    card.style.setProperty("--ox", `${bx - left}px`);
    card.style.setProperty("--oy", `${by - top}px`);
  };
  const show = () => {
    const node = stack[stack.length - 1];
    card.innerHTML = "";
    const head = el("div", "head");
    if (stack.length > 1) {
      const back = el("button", "back", "‹");
      back.addEventListener("click", () => {
        stack.pop();
        show();
      });
      head.append(back);
    }
    head.append(el("span", "ic", node.icon ?? ""), el("span", "", node.label));
    card.append(head);
    (node.children ?? []).forEach((c, i) => {
      const row = el("button", `row${c.checked ? " checked" : ""}`);
      row.style.setProperty("--d", `${i * 18}ms`);
      row.disabled = !c.enabled || (!c.id && !c.children?.length);
      row.append(el("span", "ic", c.icon ?? (c.checked ? "" : "")), el("span", "tx", c.label));
      if (c.hint) row.append(el("span", "hint", c.hint));
      if (c.checked) row.append(el("span", "ok", "✓"));
      if (c.children?.length) row.append(el("span", "chev", "›"));
      row.title = c.label;
      row.addEventListener("click", () => {
        if (c.children?.length) {
          stack.push(c);
          show();
        } else if (c.id) pick(c.id);
      });
      card.append(row);
    });
    place();
  };
  show();
  return card;
}

void win.listen<Open>("menu-open", (e) => render(e.payload));
window.addEventListener("keydown", (e) => {
  if (e.key === "Escape") close();
});
// Clicking anywhere else (another app, the taskbar) closes the menu.
void win.onFocusChanged(({ payload }) => {
  if (!payload) close();
});
