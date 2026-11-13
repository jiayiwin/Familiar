// The one window everything opens in. Each page (the room, the gallery, the
// radio, a letter, the daily report, the first-run page) is a tab that can be
// closed; the frame around them is drawn here. A page behind another stays
// alive, so the radio keeps playing while you look at something else.
//
// The host says what to open (`open-tab`: { kind, pet?, tab? }); a page asks
// to be closed with a message ({ type: "close-tab" }). Pages are frames of
// this window and talk to the host through it (see public/embed.js).

import { connect, inTauri } from "./api";
import { initLang, t } from "./i18n";

type Kind = "room" | "gallery" | "radio" | "letter" | "report" | "storeroom" | "song" | "welcome";
interface Ask { kind: Kind; pet?: string | null; tab?: string | null }
interface Tab { kind: Kind; pet: string | null; frame: HTMLIFrameElement; button: HTMLButtonElement }

const KINDS: Record<Kind, { zh: string; en: string; icon: string; page: string }> = {
  room: { zh: "小屋", en: "Room", page: "room.html", icon: "M3.500 10.500 12 3.500l8.500 7V19a1.500 1.500 0 0 1-1.500 1.500h-4.500v-6h-5v6H5A1.500 1.500 0 0 1 3.500 19z" },
  gallery: { zh: "图鉴", en: "Gallery", page: "gallery.html", icon: "M4 4.500h6.500a1.500 1.500 0 0 1 1.500 1.500v14a1.500 1.500 0 0 0-1.500-1.500H4zM20 4.500h-6.500A1.500 1.500 0 0 0 12 6v14a1.500 1.500 0 0 1 1.500-1.500H20z" },
  radio: { zh: "电台", en: "Radio", page: "radio.html", icon: "M4 9.500h16V20H4zM7.500 9.500l9-5M8.500 13v3.500M13 13.500h4M13 16.500h4" },
  letter: { zh: "来信", en: "Letter", page: "letter.html", icon: "M3.500 6h17v12.500h-17zM3.500 7l8.500 6 8.500-6" },
  report: { zh: "日报", en: "Report", page: "report.html", icon: "M5 20v-8M12 20V4.500M19 20v-5.500" },
  storeroom: { zh: "储物间", en: "Storeroom", page: "storeroom.html", icon: "M4 8.500 12 4l8 4.500v9.500a1.500 1.500 0 0 1-1.500 1.500h-13A1.500 1.500 0 0 1 4 18zM4 8.500h16M9.500 12.500h5" },
  song: { zh: "收工曲", en: "Day's end", page: "song.html", icon: "M9 18.500V6l10-2v12.500M9 18.500a2.500 2.500 0 1 1-5 0 2.500 2.500 0 0 1 5 0zM19 16.500a2.500 2.500 0 1 1-5 0 2.500 2.500 0 0 1 5 0zM9 10l10-2" },
  welcome: { zh: "欢迎", en: "Welcome", page: "welcome.html", icon: "M12 3.500l2.300 5.400 5.700.600-4.300 3.900 1.300 5.800L12 16.200 6.900 19.200l1.300-5.800L4 9.500l5.700-.600z" },
};
const isKind = (k: unknown): k is Kind => typeof k === "string" && k in KINDS;

const bar = document.getElementById("tabs")!;
const views = document.getElementById("views")!;
const query = new URLSearchParams(location.search);
const tabs: Tab[] = [];
let front: Tab | null = null;

/** The page's address: in a browser (no host) the port and token ride along. */
function address(a: Ask): string {
  const q = new URLSearchParams();
  if (a.pet && (a.kind === "room" || a.kind === "radio")) q.set("pet", a.pet);
  if (a.tab && (a.kind === "gallery" || a.kind === "storeroom")) q.set("tab", a.tab);
  // A pet's offer accepted: the radio starts on that shelf.
  if (a.tab?.startsWith("play:") && a.kind === "radio") q.set("play", a.tab.slice(5));
  // 收工曲: which day (`play:<day>` when a pet's offer was accepted: it starts at once).
  if (a.kind === "song" && a.tab) {
    q.set("day", a.tab.replace(/^play:/, ""));
    if (a.tab.startsWith("play:")) q.set("play", "1");
    if (a.pet) q.set("pet", a.pet);
  }
  for (const k of ["port", "token"]) if (query.get(k)) q.set(k, query.get(k)!);
  const s = q.toString();
  return KINDS[a.kind].page + (s ? `?${s}` : "");
}

const tell = (tab: Tab, type: string, payload: unknown) => tab.frame.contentWindow?.postMessage({ type, payload }, location.origin);

/** The frame wears what the page in front wears. */
const WORN = ["--ground", "--paper", "--ink", "--ink-2", "--muted", "--line", "--rule", "--well", "--accent", "--accent-soft", "--shade"];
function dress() {
  const doc = front?.frame.contentDocument;
  if (!doc?.documentElement.dataset.skin) return;
  const from = getComputedStyle(doc.documentElement);
  for (const k of WORN) {
    const v = from.getPropertyValue(k).trim();
    if (v) document.documentElement.style.setProperty(k, v);
  }
}

function show(tab: Tab) {
  front = tab;
  for (const x of tabs) {
    x.frame.classList.toggle("back", x !== tab);
    x.button.classList.toggle("on", x === tab);
    x.button.setAttribute("aria-selected", String(x === tab));
  }
  document.title = `${t(KINDS[tab.kind].zh, KINDS[tab.kind].en)} · ${t("睿洱", "Rill")}`;
  dress();
}

async function closeWindow() {
  if (!inTauri) return;
  const { getCurrentWindow } = await import("@tauri-apps/api/window");
  await getCurrentWindow().hide();
}

function close(tab: Tab) {
  const i = tabs.indexOf(tab);
  if (i < 0) return;
  tabs.splice(i, 1);
  tab.frame.remove();
  tab.button.remove();
  if (front === tab) {
    front = null;
    const next = tabs[Math.min(i, tabs.length - 1)];
    if (next) show(next);
  }
  // Nothing left to show: the window goes too.
  if (!tabs.length) void closeWindow();
}

function open(a: Ask) {
  const had = tabs.find((x) => x.kind === a.kind);
  if (had) {
    const pet = a.pet ?? null;
    if (a.kind === "room" && pet !== had.pet) {
      // It becomes the room of the pet it was opened from.
      had.pet = pet;
      had.frame.src = address(a);
    } else if (a.kind === "radio") {
      // That pet comes to listen; the station keeps playing.
      had.pet = pet;
      tell(had, "for-pet", pet);
      if (a.tab?.startsWith("play:")) tell(had, "radio-play", a.tab.slice(5));
    } else if ((a.kind === "gallery" || a.kind === "storeroom") && a.tab) tell(had, "goto-tab", a.tab);
    else if (a.kind === "letter") had.frame.contentWindow?.location.reload(); // whatever letter is waiting now
    else if (a.kind === "song" && a.tab) had.frame.src = address(a); // another day
    return show(had);
  }
  const k = KINDS[a.kind];
  const frame = document.createElement("iframe");
  frame.title = t(k.zh, k.en);
  frame.allow = "autoplay";
  frame.src = address(a);
  const button = document.createElement("button");
  button.type = "button";
  button.className = "tab";
  button.setAttribute("role", "tab");
  button.innerHTML = `<svg viewBox="0 0 24 24" aria-hidden="true"><path d="${k.icon}"/></svg><span class="nm"></span><span class="x" role="button" title="${t("关闭", "Close")}"><svg viewBox="0 0 12 12" aria-hidden="true"><path d="M2.500 2.500l7 7M9.500 2.500l-7 7"/></svg></span>`;
  button.querySelector(".nm")!.textContent = t(k.zh, k.en);
  const tab: Tab = { kind: a.kind, pet: a.pet ?? null, frame, button };
  button.addEventListener("click", (e) => ((e.target as HTMLElement).closest(".x") ? close(tab) : show(tab)));
  // A middle click closes a tab, as everywhere else.
  button.addEventListener("auxclick", (e) => e.button === 1 && close(tab));
  frame.addEventListener("load", () => {
    const root = frame.contentDocument?.documentElement;
    if (!root) return;
    // Its skin arrives a moment after the page: dress the frame when it does, and when it changes.
    new MutationObserver(() => front === tab && dress()).observe(root, { attributes: true, attributeFilter: ["data-skin", "style"] });
    if (front === tab) dress();
  });
  tabs.push(tab);
  bar.append(button);
  views.append(frame);
  show(tab);
}

// A page asks to be closed (the letter's "收起来", a click on the report card),
// or for another page (the report's "听这一天的").
window.addEventListener("message", (e) => {
  if (e.origin !== location.origin) return;
  if (e.data?.type === "open-tab" && isKind(e.data.payload?.kind)) return open(e.data.payload as Ask);
  if (e.data?.type !== "close-tab") return;
  const tab = tabs.find((x) => x.frame.contentWindow === e.source);
  if (tab) close(tab);
});
document.addEventListener("keydown", (e) => {
  if (e.ctrlKey && e.key.toLowerCase() === "w" && front) {
    e.preventDefault();
    close(front);
  }
});

async function main() {
  // The tabs come up even if the language cannot be asked for yet.
  try {
    await initLang(await connect());
  } catch {
    // Chinese labels, until a page inside says otherwise.
  }
  for (const el of document.querySelectorAll<HTMLElement>("[data-en-title]")) el.title = t(el.title, el.dataset.enTitle!);
  document.getElementById("close")!.addEventListener("click", () => void closeWindow());
  // What it was opened for: `?open=room&pet=…`, or several at once in a browser (`?open=room,gallery`).
  for (const kind of (query.get("open") ?? "room").split(",")) if (isKind(kind)) open({ kind, pet: query.get("pet"), tab: query.get("tab") });
  const first = tabs.find((x) => x.kind === query.get("front")) ?? tabs[0];
  if (first) show(first);
  if (!inTauri) return;
  const { getCurrentWindow } = await import("@tauri-apps/api/window");
  const win = getCurrentWindow();
  document.getElementById("min")!.addEventListener("click", () => void win.minimize());
  // Bigger: pull the corner; a double-click on the bar fills the screen and back (the drag region does that itself).
  document.getElementById("grip")!.addEventListener("pointerdown", (e) => {
    if (e.button === 0) void win.startResizeDragging("SouthEast");
  });
  const fitFrame = async () => document.body.classList.toggle("max", await win.isMaximized());
  await win.onResized(() => void fitFrame());
  // Closing only hides it: the tabs, and the radio, are still there when it comes back.
  await win.onCloseRequested((e) => {
    e.preventDefault();
    void win.hide();
  });
  await win.listen<Ask>("open-tab", (e) => isKind(e.payload?.kind) && open(e.payload));
}

void main();
