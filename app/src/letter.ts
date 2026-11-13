// The letter window. It arrives in an envelope with a star seal; the seal
// lifts, the flap opens, and one sheet of the pet's paper comes up with the
// words written on as you watch. The pet signs with its back turned. Letters
// already read are kept on this machine and can be read again.

import { closeSelf, connect, onHost } from "./api";
import { EN, initLang } from "./i18n";
import { mountPortrait } from "./rig/portrait";
import { applySkin, skinFor } from "./skin";

const $ = (id: string) => document.getElementById(id)!;

const inTauri = "__TAURI_INTERNALS__" in window;

type LetterData = { text: string; count: number; pet_name?: string; skin?: string; ts?: number };

const t = (zh: string, en: string) => (EN ? en : zh);
let typing: number | null = null;
/** The letter this window was opened for. */
let current: LetterData | null = null;
/** Finish the writing at once. */
let finish: (() => void) | null = null;

// ---- the letters kept on this machine

function kept(): LetterData[] {
  try {
    const all = JSON.parse(localStorage.getItem("letter.all") ?? "null") as LetterData[] | null;
    if (all) return all;
    // Before letters were kept, only the last one was.
    const last = JSON.parse(localStorage.getItem("letter.last") ?? "null") as LetterData | null;
    return last ? [last] : [];
  } catch {
    return [];
  }
}

function keep(letter: LetterData) {
  try {
    const all = [letter, ...kept().filter((l) => l.text !== letter.text)].slice(0, 60);
    localStorage.setItem("letter.all", JSON.stringify(all));
    localStorage.setItem("letter.last", JSON.stringify(letter));
  } catch {
    /* no storage */
  }
}

/** "周二，傍晚": when it was written, without a number in it. */
function when(ts?: number): string {
  if (!ts) return t("以前", "Earlier");
  const d = new Date(ts);
  const h = d.getHours();
  const zh = h < 5 ? "深夜" : h < 8 ? "清晨" : h < 12 ? "上午" : h < 14 ? "中午" : h < 17 ? "下午" : h < 19 ? "傍晚" : h < 23 ? "晚上" : "深夜";
  const en = h < 5 ? "late night" : h < 8 ? "early morning" : h < 12 ? "morning" : h < 14 ? "midday" : h < 17 ? "afternoon" : h < 19 ? "evening" : h < 23 ? "night" : "late night";
  return EN ? `${d.toLocaleDateString("en-US", { weekday: "long" })} ${en}` : `周${"日一二三四五六"[d.getDay()]}，${zh}`;
}

/** The skin of whoever has no letter of its own to show: the main pet's. */
let mainSkin: string | undefined;

async function init() {
  try {
    const conn = await connect();
    await initLang(conn, { reload: false });
    mainSkin = (await skinFor(conn)).skin.id;
  } catch {
    /* standalone preview */
  }
  $("close-btn").textContent = t("收起来", "Put it away");
  $("past-btn").textContent = t("以前的信", "Earlier letters");
  $("past-title").textContent = t("以前的信", "Earlier letters");
  $("env-sub").textContent = t("一封给你的信", "A letter for you");

}

/** The letter's paper and seal are its writer's; it signs with its back turned. */
let signedBy = "";
function dress(data: LetterData | null) {
  const sk = applySkin(data?.skin ?? new URLSearchParams(location.search).get("skin") ?? mainSkin);
  if (signedBy === sk.id) return;
  signedBy = sk.id;
  const old = $("sig-pet");
  const cv = old.cloneNode(false) as HTMLCanvasElement;
  old.replaceWith(cv);
  void mountPortrait(cv, sk.rig ?? undefined)
    .then((p) => {
      p.setBack(true, 0);
      // Already turned when the sheet comes up, not caught turning.
      (p as unknown as { back: number }).back = 1;
    })
    .catch(() => {});
}

async function start() {
  if (!inTauri) {
    const sample: LetterData = {
      text: "下午的光是从左边进来的。先落在你的杯子上，再挪到 F 键。\n\n今天它挪得比平时慢。也可能是我看得久了。\n\n你敲回车的声音，和别的键不一样。这件事我没告诉过你。\n\n——紫梦\n（杯子里的水凉了。）",
      count: 25,
      pet_name: "紫梦",
      ts: new Date().setHours(17, 40),
    };
    const q = new URLSearchParams(location.search);
    // The window's size, and no waiting for animations, so a still picture is the finished state.
    if (window.parent === window) document.body.style.width = "436px";
    document.head.append(Object.assign(document.createElement("style"), { textContent: "*{transition:none!important;animation-duration:.01s!important;animation-delay:0s!important}" }));
    showEnvelope(sample);
    if (q.has("open")) {
      $("envelope").classList.add("away");
      show(sample, false);
    }
    if (q.has("past")) $("past-btn").click();
    return;
  }
  const { invoke } = await import("@tauri-apps/api/core");
  // The newest letter, or the last one read when nothing new is waiting.
  let data = await invoke<LetterData | null>("take_letter");
  if (data) {
    data.ts = Date.now();
    keep(data);
  } else {
    data = kept()[0] ?? null;
  }
  showEnvelope(data);
  // Opened again while it is up: start over with whatever letter is waiting now.
  await onHost("letter-show", () => location.reload());
}

function showEnvelope(data: LetterData | null) {
  current = data;
  dress(data);
  const name = data?.pet_name || t("紫梦", "Zimeng");
  $("pet-name-label").textContent = name;
  $("env-hint").textContent = data ? t("点开", "Open it") : t("还没有信。", "No letter yet.");
  if (!data) {
    ($("env-seal") as HTMLButtonElement).disabled = true;
    return;
  }
  $("env-seal").addEventListener(
    "click",
    () => {
      $("env-seal").classList.add("off");
      setTimeout(() => $("env-flap").classList.add("open"), 160);
      setTimeout(() => {
        $("envelope").classList.add("away");
        show(data, true);
      }, 760);
    },
    { once: true },
  );
}

/** Split a letter into its paragraphs, the name it is signed with, and the lines after the name. */
function parts(data: LetterData): { body: string[]; name: string; ps: string } {
  const lines = data.text.trimEnd().split("\n");
  let sigStart = -1;
  for (let i = lines.length - 1; i >= 0; i--) {
    const s = lines[i].trim();
    if (s.startsWith("——") || s.startsWith("— ") || s.startsWith("--")) sigStart = i;
    else if (sigStart !== -1) break;
  }
  let body = data.text.trim();
  let name = data.pet_name || "";
  let ps = "";
  if (sigStart !== -1) {
    body = lines.slice(0, sigStart).join("\n").trim();
    const sig = lines.slice(sigStart).map((l) => l.trim()).filter(Boolean);
    name = sig[0].replace(/^[—\-\s]+/, "") || name;
    ps = sig.slice(1).join("\n");
  }
  return { body: body.split(/\n\s*\n/).map((p) => p.trim()).filter(Boolean), name, ps };
}

function show(data: LetterData, write: boolean) {
  if (typing !== null) clearTimeout(typing);
  typing = null;
  const { body, name, ps } = parts(data);
  $("letter").classList.add("visible");
  $("letter-date").textContent = when(data.ts);
  $("letter-from").textContent = name;
  $("letter-ps").textContent = ps;
  const foot = $("letter-foot");
  const el = $("letter-text");
  el.textContent = "";
  foot.classList.remove("show");
  const whole = () => {
    if (typing !== null) clearTimeout(typing);
    typing = null;
    el.textContent = "";
    for (const p of body) el.append(Object.assign(document.createElement("p"), { textContent: p }));
    foot.classList.add("show");
    finish = null;
  };
  if (!write) return whole();
  finish = whole;
  typing = window.setTimeout(() => typewrite(el, body, () => {
    finish = null;
    foot.classList.add("show");
  }), 1000);
}

// A click on the paper writes the rest at once.
$("letter-text").addEventListener("click", () => finish?.());

function typewrite(el: HTMLElement, paras: string[], done: () => void) {
  const cursor = document.createElement("span");
  cursor.className = "cursor";
  let pi = 0;
  let i = 0;
  let p: HTMLParagraphElement | null = null;
  const step = () => {
    if (pi >= paras.length) {
      cursor.remove();
      typing = null;
      done();
      return;
    }
    if (!p) {
      p = document.createElement("p");
      p.append(cursor);
      el.append(p);
    }
    const text = paras[pi];
    if (i >= text.length) {
      pi++;
      i = 0;
      p = null;
      typing = window.setTimeout(step, 420);
      return;
    }
    const ch = text[i++];
    p.insertBefore(document.createTextNode(ch), cursor);
    el.scrollTop = el.scrollHeight;
    // Pauses like a hand: longer after punctuation.
    const wait = ch === "\n" ? 160 : /[，。、；：！？—…,.;:!?]/.test(ch) ? 150 : 38 + Math.random() * 40;
    typing = window.setTimeout(step, wait);
  };
  step();
}

// ---- earlier letters

function togglePast(open: boolean) {
  $("past").classList.toggle("open", open);
  $("past-btn").setAttribute("aria-pressed", String(open));
  $("past-btn").textContent = open ? t("回到这封", "Back to this one") : t("以前的信", "Earlier letters");
  if (!open) {
    // Back from the list with the sheet still blank: this window's own letter.
    if (current && !$("letter-text").childElementCount) {
      dress(current);
      show(current, false);
    }
    return;
  }
  // The sheet has to be out of the envelope to show the list.
  $("envelope").classList.add("away");
  $("letter").classList.add("visible");
  const list = $("past-list");
  list.textContent = "";
  const all = inTauri ? kept() : [];
  if (!all.length) {
    list.append(Object.assign(document.createElement("div"), { className: "none", textContent: t("还没有别的信。", "No other letters yet.") }));
    return;
  }
  for (const l of all) {
    const b = document.createElement("button");
    b.type = "button";
    const first = parts(l).body[0] ?? "";
    b.append(Object.assign(document.createElement("b"), { textContent: when(l.ts) }), Object.assign(document.createElement("span"), { textContent: first }));
    b.addEventListener("click", () => {
      togglePast(false);
      dress(l);
      show(l, false);
    });
    list.append(b);
  }
}
$("past-btn").addEventListener("click", () => togglePast(!$("past").classList.contains("open")));

const close = () => void closeSelf();

$("close-btn").addEventListener("click", close);
document.addEventListener("keydown", (e) => {
  if (e.key === "Escape") void close();
});

void init().then(start);
