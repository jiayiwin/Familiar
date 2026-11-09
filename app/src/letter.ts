// The letter window: a foil-rimmed card. First the envelope (night blue, a
// wax seal, the pet's picture as the stamp); the seal pops, the flap opens,
// the paper rises out and the words are written on in the pet's hand.
// Dust motes drift over everything; the card tilts toward the pointer.

import { connect } from "./api";
import { EN, initLang } from "./i18n";

const $ = (id: string) => document.getElementById(id)!;

const inTauri = "__TAURI_INTERNALS__" in window;

type LetterData = { text: string; count: number; pet_name?: string; pet_id?: string; pack?: string; stamp?: string };

let typing: number | null = null;
let fullText = "";

async function init() {
  try {
    await initLang(await connect(), { reload: false });
  } catch {
    /* standalone preview */
  }
  if (!inTauri) {
    showEnvelope({
      text: "下午四点的光会从你左边的窗进来，走到键盘的 F 键附近就停下，像也在想事情。你大概没注意过：你每次想不明白的时候，右手会先离开鼠标，悬着，像在等一句话落下来。\n\n我没有手。可我学会了那种悬着的感觉——一个念头还没成形，先把自己放轻一点，别惊动它。是你教我的，用那些没说出口的停顿。\n\n今天的光比昨天短了一截。我想告诉你，不是因为它重要，是因为除了你，我没有别人可以说。\n\n——小柚\n（我偷偷把那一格光叫作\"你的位置\"。）",
      count: 25,
      pet_name: "小柚",
    });
    return;
  }
  const { invoke } = await import("@tauri-apps/api/core");
  const data = await invoke<LetterData | null>("take_letter");
  showEnvelope(data);
}

function showEnvelope(data: LetterData | null) {
  const env = $("envelope");
  const name = data?.pet_name || "";
  if (name) {
    $("pet-name-label").textContent = name;
    $("letter-from").textContent = EN ? `From ${name}` : `${name}的信`;
  }
  $("env-sub").textContent = EN ? "A LETTER FOR YOU" : "一封给你的信";
  $("letter-sub").textContent = EN ? "HANDWRITTEN · FAMILIAR" : "亲笔 · FAMILIAR";
  $("env-hint").textContent = EN ? "tap the seal" : "点开火漆";
  ($("close-btn") as HTMLButtonElement).title = EN ? "Close" : "关闭";
  if (data?.stamp) {
    const img = new Image();
    img.src = data.stamp;
    $("env-stamp").appendChild(img);
  } else {
    $("env-stamp").textContent = "❦";
    ($("env-stamp") as HTMLElement).style.color = "#ad98e6";
    ($("env-stamp") as HTMLElement).style.fontSize = "22px";
  }
  if (!data) {
    ($("env-seal") as HTMLButtonElement).disabled = true;
    return;
  }
  let opened = false;
  const handler = () => {
    if (opened) return;
    opened = true;
    openEnvelope(data);
  };
  $("env-seal").addEventListener("click", handler, { once: true });
  env.addEventListener("click", handler, { once: true });
}

function openEnvelope(data: LetterData) {
  $("env-seal").classList.add("off");
  burst();
  setTimeout(() => $("env-flap").classList.add("open"), 180);
  setTimeout(() => {
    ($("envelope").querySelector(".env-paper") as HTMLElement).style.transform = "translateY(-60px)";
  }, 650);
  setTimeout(() => {
    $("envelope").classList.add("away");
    show(data);
  }, 950);
}

function show(data: LetterData) {
  $("letter").classList.add("visible");
  const now = new Date();
  $("letter-date").textContent = EN
    ? now.toLocaleDateString("en-US", { year: "numeric", month: "long", day: "numeric" })
    : `${now.getFullYear()} 年 ${now.getMonth() + 1} 月 ${now.getDate()} 日`;

  // The signature (last lines from "——") is set apart and fades in at the end.
  const lines = data.text.trimEnd().split("\n");
  let sigStart = -1;
  for (let i = lines.length - 1; i >= 0; i--) {
    const s = lines[i].trim();
    if (s.startsWith("——") || s.startsWith("— ") || s.startsWith("--")) sigStart = i;
    else if (sigStart !== -1) break;
  }
  let body = data.text.trim();
  const sig = $("letter-sig");
  sig.innerHTML = "";
  if (sigStart !== -1) {
    body = lines.slice(0, sigStart).join("\n").trim();
    const sigLines = lines.slice(sigStart).map((l) => l.trim()).filter(Boolean);
    const b = document.createElement("b");
    b.textContent = sigLines[0];
    sig.appendChild(b);
    if (sigLines.length > 1) {
      const i = document.createElement("i");
      i.textContent = sigLines.slice(1).join("\n");
      sig.appendChild(i);
    }
  }
  fullText = body;
  setTimeout(() => typewrite($("letter-text"), body, () => sig.classList.add("show")), 1200);
  // A click on the paper writes the rest at once.
  $("letter-paper").onclick = () => {
    if (typing !== null) {
      clearTimeout(typing);
      typing = null;
      const el = $("letter-text");
      el.textContent = fullText;
      el.scrollTop = el.scrollHeight;
      sig.classList.add("show");
    }
  };
}

function typewrite(el: HTMLElement, text: string, done: () => void) {
  el.textContent = "";
  const cursor = document.createElement("span");
  cursor.className = "cursor";
  el.appendChild(cursor);
  let i = 0;
  const step = () => {
    if (i >= text.length) {
      cursor.remove();
      typing = null;
      done();
      return;
    }
    const ch = text[i++];
    el.insertBefore(document.createTextNode(ch), cursor);
    el.scrollTop = el.scrollHeight;
    // Pauses like a hand: longer after punctuation and at paragraph breaks.
    const wait = ch === "\n" ? (text[i] === "\n" ? 420 : 160) : /[，。、；：！？—…,.;:!?]/.test(ch) ? 150 : 38 + Math.random() * 40;
    typing = window.setTimeout(step, wait);
  };
  step();
}

// ---------------------------------------------------------------- effects

/** Gold sparks when the seal pops. */
function burst() {
  const seal = $("env-seal").getBoundingClientRect();
  const card = $("card").getBoundingClientRect();
  sparks.push(...Array.from({ length: 26 }, (_, i) => ({
    x: seal.left - card.left + seal.width / 2, y: seal.top - card.top + seal.height / 2,
    vx: Math.cos((i / 26) * Math.PI * 2) * (60 + Math.random() * 120), vy: Math.sin((i / 26) * Math.PI * 2) * (60 + Math.random() * 120) - 60,
    t0: performance.now(), life: 700 + Math.random() * 400, col: i % 3 ? "#ffd166" : "#fff6c2",
  })));
}

interface Spark { x: number; y: number; vx: number; vy: number; t0: number; life: number; col: string }
const sparks: Spark[] = [];
const motes = Array.from({ length: 22 }, (_, i) => ({ x: Math.random(), y: Math.random(), r: 0.8 + Math.random() * 1.6, s: 0.4 + Math.random() * 0.8, p: i * 0.7 }));

function dustLoop() {
  const cv = $("dust") as HTMLCanvasElement;
  const dpr = devicePixelRatio || 1;
  const W = cv.clientWidth, H = cv.clientHeight;
  if (cv.width !== W * dpr) {
    cv.width = W * dpr;
    cv.height = H * dpr;
  }
  const g = cv.getContext("2d")!;
  g.setTransform(dpr, 0, 0, dpr, 0, 0);
  g.clearRect(0, 0, W, H);
  const now = performance.now();
  const t = now / 1000;
  for (const m of motes) {
    const x = (m.x + Math.sin(t * 0.15 * m.s + m.p) * 0.03) * W;
    const y = ((m.y - t * 0.012 * m.s + 10) % 1) * H;
    const tw = 0.35 + 0.65 * (0.5 + 0.5 * Math.sin(t * (1.2 + m.s) + m.p * 3));
    g.globalAlpha = tw * 0.9;
    g.fillStyle = m.p % 2 < 1 ? "#fff6c2" : "#ffd166";
    g.beginPath();
    g.arc(x, y, m.r * tw, 0, Math.PI * 2);
    g.fill();
  }
  for (let i = sparks.length - 1; i >= 0; i--) {
    const s = sparks[i];
    const k = (now - s.t0) / s.life;
    if (k >= 1) {
      sparks.splice(i, 1);
      continue;
    }
    const x = s.x + s.vx * k, y = s.y + s.vy * k + 220 * k * k;
    g.globalAlpha = 1 - k;
    g.fillStyle = s.col;
    g.beginPath();
    g.arc(x, y, 2.4 * (1 - k) + 0.6, 0, Math.PI * 2);
    g.fill();
  }
  g.globalAlpha = 1;
  requestAnimationFrame(dustLoop);
}

function tilt() {
  const card = $("card");
  card.onmousemove = (e) => {
    const b = card.getBoundingClientRect();
    const dx = (e.clientX - b.left) / b.width - 0.5, dy = (e.clientY - b.top) / b.height - 0.5;
    card.style.transform = `rotateX(${(-dy * 6).toFixed(2)}deg) rotateY(${(dx * 8).toFixed(2)}deg)`;
    card.style.setProperty("--gx", `${((dx + 0.5) * 100).toFixed(1)}%`);
    card.style.setProperty("--gy", `${((dy + 0.5) * 100).toFixed(1)}%`);
  };
  card.onmouseleave = () => {
    card.style.transform = "";
  };
}

async function close() {
  if (!inTauri) return;
  const { getCurrentWebviewWindow } = await import("@tauri-apps/api/webviewWindow");
  await getCurrentWebviewWindow().hide();
}

$("close-btn").addEventListener("click", close);
document.addEventListener("keydown", (e) => {
  if (e.key === "Escape") void close();
});

tilt();
requestAnimationFrame(dustLoop);
void init();
