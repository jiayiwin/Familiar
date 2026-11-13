// 「手上的事」 on a pretend desktop: how a thing gets written down, where the pet
// keeps it, how it is crossed off, and when it is brought up again. For
// looking at before it is built; never shipped.

import { FOOT_H, PetRenderer } from "./pet";
import { loadRig } from "./rig/rig.js";
import { createPerformer } from "./rig/perform.js";
import { applySkin } from "./skin";

const q = new URLSearchParams(location.search);
const W = 1280, H = 720, BAR = 44;
const canvas = document.getElementById("desk") as HTMLCanvasElement;
canvas.width = W;
canvas.height = H;
const ctx = canvas.getContext("2d")!;
const RIG_PX = 0.288;
const FONT = "'Segoe UI Variable Text', 'Segoe UI', 'Microsoft YaHei UI', 'PingFang SC', system-ui, sans-serif";
const card = document.getElementById("card")!;
const cap = document.getElementById("cap")!;

type Scene = "capture" | "holding" | "card" | "done" | "remind" | "morning";
const SCENES: Scene[] = ["capture", "holding", "card", "done", "remind", "morning"];
const CAPTION: Record<Scene, [string, string]> = {
  capture: ["① 记一件事", "右键 → 陪我一下 → 记一件事；在气泡里打一行，回车。也可以选中一段字按 Ctrl+Alt+N。"],
  holding: ["② 它替你拿着", "有事在手上，它身上就别一张小纸条。不显示数字。"],
  card: ["③ 悬停卡片里的「手上的事」", "agent 收工时说的“下一步”，也会自己落成一张纸条。"],
  done: ["④ 勾掉一件", "点圆圈勾掉，它把那张纸揉成一团吃掉。"],
  remind: ["⑤ 挂在 agent 身上的提醒", "“等 Claude 做完提醒我看 PR”：不定时间，Claude 这轮停下来、你正好在，它才开口。"],
  morning: ["⑥ 第二天早上", "昨天没做完的，跟夜里整理的那句话一起说，不另外弹。"],
};

const pet = { x: 0, y: 0, w: 0, h: 0 };
let notes = 3;
let typed = "";
let showInput = false;
let ball: { t0: number; from: [number, number]; to: [number, number] } | null = null;
let flying: { t0: number; from: [number, number] } | null = null;

// ---- the desk
function wallpaper() {
  const g = ctx.createLinearGradient(0, 0, 0, H);
  g.addColorStop(0, "#c9d6ef");
  g.addColorStop(0.55, "#e8def0");
  g.addColorStop(1, "#f6e3d6");
  ctx.fillStyle = g;
  ctx.fillRect(0, 0, W, H);
  for (const [y, amp, col] of [[470, 40, "rgba(176,196,170,.55)"], [540, 30, "rgba(150,178,146,.6)"]] as const) {
    ctx.fillStyle = col;
    ctx.beginPath();
    ctx.moveTo(0, H);
    for (let x = 0; x <= W; x += 20) ctx.lineTo(x, y + Math.sin(x / 210 + y) * amp + Math.sin(x / 90) * amp * 0.25);
    ctx.lineTo(W, H);
    ctx.fill();
  }
  ctx.fillStyle = "rgba(255,248,232,.7)";
  ctx.beginPath();
  ctx.arc(1010, 150, 60, 0, Math.PI * 2);
  ctx.fill();
}

function taskbar(clock: string) {
  ctx.fillStyle = "rgba(246,244,250,.92)";
  ctx.fillRect(0, H - BAR, W, BAR);
  ctx.fillStyle = "rgba(120,110,140,.18)";
  ctx.fillRect(0, H - BAR, W, 1);
  ["#9db7e6", "#8cc79a", "#e7b47d", "#c8a6e6", "#e79aa8", "#7fc6c2"].forEach((c, k) => {
    ctx.fillStyle = c;
    ctx.beginPath();
    ctx.roundRect(W / 2 - 3 * 40 + k * 40 + 6, H - BAR + 8, 28, 28, 7);
    ctx.fill();
  });
  ctx.fillStyle = "#5b4a57";
  ctx.font = "12px system-ui, 'Microsoft YaHei', sans-serif";
  ctx.textAlign = "right";
  const [time, date] = clock.split(" ");
  ctx.fillText(time, W - 16, H - BAR + 19);
  ctx.fillText(date, W - 16, H - BAR + 35);
}

/** A slip of paper: folded corner, two lines of writing, a pin in the pet's colour. */
function slip(x: number, y: number, rot: number, s = 1, crumple = 0) {
  ctx.save();
  ctx.translate(x, y);
  ctx.rotate(rot);
  ctx.scale(s, s);
  if (crumple > 0) {
    // Balled up: a lumpy round of the same paper.
    const r = 9 - crumple * 2;
    ctx.fillStyle = "#fbf3df";
    ctx.strokeStyle = "#cdbda9";
    ctx.lineWidth = 1;
    ctx.beginPath();
    for (let k = 0; k <= 10; k++) {
      const a = (k / 10) * Math.PI * 2;
      const rr = r * (0.82 + 0.18 * Math.sin(k * 2.7));
      k ? ctx.lineTo(Math.cos(a) * rr, Math.sin(a) * rr) : ctx.moveTo(Math.cos(a) * rr, Math.sin(a) * rr);
    }
    ctx.closePath();
    ctx.fill();
    ctx.stroke();
    ctx.beginPath();
    ctx.moveTo(-4, -2);
    ctx.lineTo(1, 1);
    ctx.lineTo(3, -3);
    ctx.stroke();
    ctx.restore();
    return;
  }
  ctx.shadowColor = "rgba(110,88,140,.25)";
  ctx.shadowBlur = 4;
  ctx.shadowOffsetY = 1.5;
  ctx.fillStyle = "#fbf3df";
  ctx.beginPath();
  ctx.moveTo(-11, -13);
  ctx.lineTo(6, -13);
  ctx.lineTo(11, -8);
  ctx.lineTo(11, 13);
  ctx.lineTo(-11, 13);
  ctx.closePath();
  ctx.fill();
  ctx.shadowColor = "transparent";
  ctx.strokeStyle = "#d8c9b3";
  ctx.lineWidth = 1;
  ctx.stroke();
  ctx.fillStyle = "#efe2c6";
  ctx.beginPath();
  ctx.moveTo(6, -13);
  ctx.lineTo(6, -8);
  ctx.lineTo(11, -8);
  ctx.closePath();
  ctx.fill();
  ctx.strokeStyle = "#9b8a9a";
  ctx.lineWidth = 1.2;
  ctx.lineCap = "round";
  for (const [y, w] of [[-4, 14], [1, 11], [6, 13]] as const) {
    ctx.beginPath();
    ctx.moveTo(-7, y);
    for (let x = -7; x <= -7 + w; x += 2) ctx.lineTo(x, y + Math.sin(x * 1.3 + y) * 0.7);
    ctx.stroke();
  }
  ctx.fillStyle = band;
  ctx.beginPath();
  ctx.arc(-4, -10, 2.4, 0, Math.PI * 2);
  ctx.fill();
  ctx.restore();
}

/** The line you write into: the pet's paper bubble with a caret, and a quiet hint under it. */
function inputBubble(now: number) {
  const cx = pet.x + pet.w / 2;
  const top = pet.y + pet.h - FOOT_H - rigH - 6;
  const tw = 270, bh = 62;
  const bx = cx - tw / 2, by = top - bh - 8;
  ctx.save();
  ctx.shadowColor = "rgba(110,88,140,.28)";
  ctx.shadowBlur = 10;
  ctx.shadowOffsetY = 3;
  ctx.fillStyle = "#fffaf2";
  ctx.beginPath();
  ctx.roundRect(bx, by, tw, bh, 14);
  ctx.moveTo(cx - 6, by + bh - 1);
  ctx.lineTo(cx, by + bh + 6);
  ctx.lineTo(cx + 6, by + bh - 1);
  ctx.fill();
  ctx.restore();
  ctx.strokeStyle = "#d9cbc8";
  ctx.lineWidth = 1;
  ctx.beginPath();
  ctx.roundRect(bx + 0.5, by + 0.5, tw - 1, bh - 1, 13.5);
  ctx.stroke();
  // The field.
  ctx.fillStyle = "#ffffff";
  ctx.strokeStyle = band;
  ctx.beginPath();
  ctx.roundRect(bx + 10, by + 9, tw - 20, 26, 9);
  ctx.fill();
  ctx.stroke();
  ctx.textBaseline = "middle";
  ctx.textAlign = "left";
  ctx.font = `600 12.5px ${FONT}`;
  ctx.fillStyle = typed ? "#5b4a57" : "#a597a2";
  ctx.fillText(typed || "记一件事", bx + 20, by + 22.5);
  if (Math.floor(now / 530) % 2 === 0) {
    const x = bx + 20 + (typed ? ctx.measureText(typed).width + 1 : 0);
    ctx.fillStyle = "#5b4a57";
    ctx.fillRect(x, by + 15, 1, 15);
  }
  ctx.font = `500 10.5px ${FONT}`;
  ctx.fillStyle = "#8b7c89";
  ctx.textAlign = "center";
  ctx.fillText("回车记下 · Esc 算了 · “等 Claude 做完” 也行", cx, by + 48);
}

let band = "#cfc3e6";
let rigW = 0, rigH = 0;
const wait = (ms: number) => new Promise<void>((d) => window.setTimeout(d, ms));

// ---- the card
interface Todo { what: string; from: string; wait?: boolean; agent?: boolean; gone?: boolean; hover?: boolean }
function renderCard(todos: Todo[], opts: { typing?: string } = {}) {
  // Beside the pet, on its left, standing on the taskbar like the pet does.
  card.style.left = `${Math.round(pet.x + pet.w / 2 - rigW / 2 - 300 - 18)}px`;
  card.innerHTML = `<h1><span>紫梦 · Claude Code</span><small>今天 2 小时 14 分</small></h1>
    <div class="row"><span class="dot tool"></span><span class="name">api-server</span><span class="state">在改代码</span><span class="meta">修登录超时 · 12 分钟</span></div>
    <div class="row"><span class="dot done"></span><span class="name">notes-site</span><span class="state">做完了</span><span class="meta">把首页换成新配色</span></div>
    <h2><span>手上的事</span><small>点圆圈勾掉</small></h2>
    ${todos
      .map(
        (t) => `<div class="todo${t.gone ? " gone" : ""}${t.hover ? " hover" : ""}"><span class="tick"></span><span class="what">${t.what}</span>
      <span class="from">${t.agent ? "<i></i>" : ""}${t.wait ? `<span class="wait">${t.from}</span>` : t.from}</span></div>`,
      )
      .join("")}
    <div class="add${opts.typing != null ? " typing" : ""}">${opts.typing ?? "记一件事…"}</div>
    <div class="hint">点一行会话，到它的窗口去</div>`;
  card.classList.add("on");
  card.style.top = `${Math.round(H - BAR - 14 - card.offsetHeight)}px`;
}

const TODOS: Todo[] = [
  { what: "周五前把发票寄出去", from: "今天 16:20 记的" },
  { what: "看 api-server 的 PR", from: "等 Claude 这轮做完", wait: true, agent: true },
  { what: "给登录补上超时的测试", from: "Claude 收工时留的", agent: true },
];

// ---- go
async function main() {
  const me = applySkin(q.get("skin") ?? "zimeng");
  band = me.colors.accentOff;
  const rig = await loadRig(me.rig ?? "rig/zimeng/");
  const perf = createPerformer(null, rig);
  const scale = RIG_PX;
  const [x0, y0, x1, y1] = rig.rig.bbox;
  rigW = (x1 - x0) * scale;
  rigH = (y1 - y0) * scale;
  const top = document.createElement("canvas");
  const topCtx = top.getContext("2d")!;
  const r = new PetRenderer(topCtx);
  r.rigBox = { w: rigW, h: rigH };
  r.onResize = (w, h) => {
    top.width = w;
    top.height = h;
    pet.x += (pet.w - w) / 2;
    pet.y += pet.h - h;
    pet.w = w;
    pet.h = h;
  };
  r.fit();
  Object.assign(pet, { x: 900, y: H - BAR - pet.h });

  let clock = "16:20 2026/11/12";
  let last = performance.now();
  const frame = (now: number) => {
    const dt = Math.min(0.1, (now - last) / 1000);
    last = now;
    perf.tick(dt);
    r.update(now);
    wallpaper();
    taskbar(clock);
    const feetX = pet.x + pet.w / 2, feetY = pet.y + pet.h - FOOT_H;
    ctx.save();
    ctx.translate(feetX - rig.foot[0] * scale, feetY - rig.foot[1] * scale);
    perf.draw(ctx, scale);
    ctx.restore();
    // The slip it carries, pinned at its side and swaying a little with it.
    const sx = feetX + rigW * 0.36, sy = feetY - rigH * 0.3;
    if (notes > 0) {
      const sway = Math.sin(now / 900) * 0.06;
      if (notes > 1) slip(sx + 4, sy + 3, 0.28 + sway, 0.92);
      slip(sx, sy, 0.12 + sway);
    }
    if (flying) {
      // A new slip comes down from the bubble into its hand.
      const k = Math.min(1, (now - flying.t0) / 650);
      const e = 1 - (1 - k) ** 3;
      const x = flying.from[0] + (sx - flying.from[0]) * e;
      const y = flying.from[1] + (sy - flying.from[1]) * e - Math.sin(k * Math.PI) * 40;
      slip(x, y, 0.12 + (1 - e) * 1.2, 0.7 + 0.3 * e);
      if (k >= 1) {
        flying = null;
        notes = Math.max(1, notes);
        rig.react("perk");
      }
    }
    if (ball) {
      // A crossed-off slip, balled up, goes into its mouth.
      const k = Math.min(1, (now - ball.t0) / 800);
      const e = k < 0.5 ? 2 * k * k : 1 - (-2 * k + 2) ** 2 / 2;
      const x = ball.from[0] + (ball.to[0] - ball.from[0]) * e;
      const y = ball.from[1] + (ball.to[1] - ball.from[1]) * e - Math.sin(k * Math.PI) * 60;
      if (k < 1) slip(x, y, k * 6, 1, Math.min(1, k * 3));
      else {
        ball = null;
        rig.react("cheer");
        Object.assign(rig.play, { face: "happy" });
        window.setTimeout(() => Object.assign(rig.play, { face: null }), 1600);
      }
    }
    if (showInput) inputBubble(now);
    ctx.drawImage(top, pet.x, pet.y);
    requestAnimationFrame(frame);
  };
  requestAnimationFrame(frame);

  const caption = (s: Scene) => (cap.innerHTML = `${CAPTION[s][0]}<small>${CAPTION[s][1]}</small>`);
  const reset = () => {
    card.classList.remove("on");
    showInput = false;
    typed = "";
    r.answer(true);
    r.say("", 1);
  };
  const scenes: Record<Scene, () => Promise<void>> = {
    capture: async () => {
      notes = 0;
      showInput = true;
      const text = "周五前把发票寄出去";
      for (const ch of text) {
        typed += ch;
        await wait(140);
      }
      await wait(q.has("still") ? 0 : 900);
      if (q.has("still")) return;
      showInput = false;
      flying = { t0: performance.now(), from: [pet.x + pet.w / 2, pet.y + pet.h - FOOT_H - rigH - 40] };
      await wait(700);
      r.say("记下了。我替你拿着。", 3200);
    },
    holding: async () => {
      notes = 3;
      r.say("纸条有点沉。是周五的那件。", 6000);
    },
    card: async () => {
      notes = 3;
      renderCard(TODOS.map((t, i) => ({ ...t, hover: i === 0 })));
    },
    done: async () => {
      notes = 3;
      renderCard(TODOS.map((t, i) => ({ ...t, hover: i === 0 })));
      await wait(q.has("still") ? 0 : 700);
      renderCard(TODOS.map((t, i) => ({ ...t, gone: i === 0 })));
      const feetX = pet.x + pet.w / 2, feetY = pet.y + pet.h - FOOT_H;
      notes = 2;
      ball = { t0: performance.now(), from: [feetX + rigW * 0.36, feetY - rigH * 0.3], to: [feetX + 2, feetY - rigH * 0.42] };
      await wait(900);
      r.say("这张纸有点脆。", 3600);
    },
    remind: async () => {
      notes = 2;
      clock = "17:05 2026/11/12";
      r.ask("Claude 停下来了。你说过，它做完要看 PR。", "好", "晚点", 60_000, () => {}, true);
    },
    morning: async () => {
      notes = 2;
      clock = "09:12 2026/11/13";
      r.ask("昨晚你走后，我收拾了一下。昨天的纸条还在我这儿。", "看看", "不用", 60_000, () => {}, true);
    },
  };
  const play = async (s: Scene) => {
    reset();
    caption(s);
    await wait(60);
    await scenes[s]();
  };
  const w = window as unknown as { play: typeof play; ready: boolean; SCENES: Scene[] };
  w.play = play;
  w.SCENES = SCENES;
  const first = (q.get("scene") as Scene) ?? "capture";
  await play(SCENES.includes(first) ? first : "capture");
  w.ready = true;
  if (q.has("all")) {
    for (const s of SCENES.slice(SCENES.indexOf(first) + 1)) {
      await wait(5200);
      await play(s);
    }
  }
}

void main();
