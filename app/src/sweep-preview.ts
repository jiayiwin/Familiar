// The sweep on a pretend desktop: the same choreography and broom as the app,
// with the pet window and the icons played on one canvas. For looking at it,
// and for recording it; never shipped.

import { FOOT_H, PetRenderer } from "./pet";
import { loadRig } from "./rig/rig.js";
import { createPerformer } from "./rig/perform.js";
import { applySkin } from "./skin";
import { line } from "./voice";
import { t as tr } from "./i18n";
import { BRISTLE, drawBroom, newBroom, sweep, sweepLean, type DeskItem, type DeskPlan, type Pose, type Stage } from "./sweep";

const q = new URLSearchParams(location.search);
const W = 1280, H = 720, BAR = 44;
const canvas = document.getElementById("desk") as HTMLCanvasElement;
canvas.width = W;
canvas.height = H;
const ctx = canvas.getContext("2d")!;
const RIG_PX = 0.288;
const CW = 80, CH = 96, OX = 8, OY = 6;

type Kind = "system" | "app" | "folder" | "doc" | "picture" | "media" | "archive" | "other";
interface Ic { key: string; kind: Kind; x: number; y: number; tint?: string; old?: boolean; born?: number; fade?: number }
/** `?stow`: a desktop covered edge to edge, and the files nobody touched in weeks go into 桌面收纳. */
const STOW = q.has("stow");
const DRAWER = "桌面收纳";

// ---- the icons: a few in the corner where they belong, the rest wherever they were dropped
const cell = (c: number, r: number) => [OX + c * CW, OY + r * CH] as const;
const icons: Ic[] = [
  { key: "回收站", kind: "system", x: cell(0, 0)[0], y: cell(0, 0)[1] },
  { key: "此电脑", kind: "system", x: cell(0, 1)[0], y: cell(0, 1)[1] },
  { key: "浏览器", kind: "app", x: cell(0, 2)[0], y: cell(0, 2)[1], tint: "#7fb0e6" },
  { key: "聊天", kind: "app", x: cell(0, 3)[0], y: cell(0, 3)[1], tint: "#8cc79a" },
  { key: "音乐", kind: "app", x: 412, y: 108, tint: "#e79aa8" },
  { key: "项目资料", kind: "folder", x: 1046, y: 64 },
  { key: "设计稿", kind: "folder", x: 598, y: 420 },
  { key: "周报.docx", kind: "doc", x: 330, y: 372 },
  { key: "合同扫描.pdf", kind: "doc", x: 862, y: 236 },
  { key: "需求说明.md", kind: "doc", x: 1130, y: 410 },
  { key: "预算.xlsx", kind: "doc", x: 744, y: 108 },
  { key: "截图 11-10.png", kind: "picture", x: 950, y: 486 },
  { key: "截图 11-11.png", kind: "picture", x: 1010, y: 520 },
  { key: "海报草稿.psd", kind: "picture", x: 470, y: 268 },
  { key: "头像.jpg", kind: "picture", x: 216, y: 508 },
  { key: "会议录音.m4a", kind: "media", x: 690, y: 548 },
  { key: "素材.zip", kind: "archive", x: 1150, y: 196 },
  { key: "安装包.exe", kind: "archive", x: 540, y: 32 },
];
if (STOW) icons.splice(0, icons.length, ...crowded());
const home = new Map(icons.map((i) => [i.key, { x: i.x, y: i.y }]));

/** A desktop someone has used for a year: every cell taken, most of it old. */
function crowded(): Ic[] {
  let seed = 7;
  const rnd = () => {
    seed = (seed + 0x6d2b79f5) | 0;
    let t = Math.imul(seed ^ (seed >>> 15), 1 | seed);
    t = (t + Math.imul(t ^ (t >>> 7), 61 | t)) ^ t;
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
  const out: Ic[] = [
    { key: "回收站", kind: "system", x: 0, y: 0 },
    { key: "此电脑", kind: "system", x: 0, y: 0 },
    { key: "浏览器", kind: "app", x: 0, y: 0, tint: "#7fb0e6" },
    { key: "聊天", kind: "app", x: 0, y: 0, tint: "#8cc79a" },
    { key: "邮件", kind: "app", x: 0, y: 0, tint: "#e7b47d" },
    { key: "网盘", kind: "app", x: 0, y: 0, tint: "#9db7e6" },
  ];
  out.forEach((i, k) => Object.assign(i, { x: OX, y: OY + k * CH }));
  const made: [Kind, string][] = [];
  for (let k = 0; k < 30; k++) made.push(["picture", `截图 2026-${String(1 + Math.floor(rnd() * 10)).padStart(2, "0")}-${String(1 + Math.floor(rnd() * 28)).padStart(2, "0")}.png`]);
  for (const n of ["周报-第 31 周.docx", "周报-第 32 周.docx", "周报-第 41 周.docx", "会议纪要 0612.docx", "会议纪要 0903.docx", "报价单.xlsx", "报价单 (1).xlsx", "合同扫描.pdf", "发票 0418.pdf", "发票 0522.pdf", "需求说明.md", "新建文本文档.txt", "新建文本文档 (2).txt", "简历.pdf", "季度汇总.pptx", "排期.xlsx", "访谈记录.docx", "周报-第 44 周.docx", "周报-第 45 周.docx", "合同终版.pdf", "合同终版 (1).pdf", "报销单.xlsx", "课程笔记.docx", "读书笔记.md", "演讲稿.docx", "年会方案.pptx", "名单.xlsx"]) made.push(["doc", n]);
  for (const n of ["IMG_2041.jpg", "IMG_2042.jpg", "海报草稿.psd", "头像.jpg", "封面.png", "参考图 3.jpg"]) made.push(["picture", n]);
  for (const n of ["素材.zip", "资料包.rar", "安装包 v2.3.exe", "安装包 v2.4.exe", "setup_x64.msi", "驱动.exe", "字体合集.zip", "旧项目备份.7z"]) made.push(["archive", n]);
  for (const n of ["会议录音.m4a", "演示录屏.mp4", "采访 0730.m4a"]) made.push(["media", n]);
  for (const n of ["项目资料", "设计稿", "新建文件夹", "新建文件夹 (2)", "归档"]) made.push(["folder", n]);
  // Every cell from the second column on, in no order at all.
  const rows = Math.floor((H - BAR - OY) / CH);
  const cells: [number, number][] = [];
  for (let c = 1; c < Math.floor(W / CW); c++) for (let r = 0; r < rows; r++) cells.push([c, r]);
  for (let i = cells.length - 1; i > 0; i--) {
    const j = Math.floor(rnd() * (i + 1));
    [cells[i], cells[j]] = [cells[j], cells[i]];
  }
  made.forEach(([kind, key], k) => {
    const [c, r] = cells[k];
    // Files: two in three untouched for weeks. Folders stay where they are.
    out.push({ key, kind, x: OX + c * CW, y: OY + r * CH, old: kind !== "folder" && rnd() < 0.7 });
  });
  return out;
}

const ORDER: Kind[] = ["system", "app", "folder", "doc", "picture", "media", "archive", "other"];
/** The same packing as petd's tidy::layout, for one screen. */
function layout(list: Ic[]): Map<string, [number, number]> {
  // System icons and shortcuts keep their order (column by column); files go by name.
  const read = (i: Ic) => Math.floor((i.x + CW / 2) / CW) * 10000 + i.y;
  const sorted = [...list].sort((a, b) => ORDER.indexOf(a.kind) - ORDER.indexOf(b.kind) || (a.kind === "system" || a.kind === "app" ? read(a) - read(b) : a.key.localeCompare(b.key, "zh")));
  const rows = Math.max(1, Math.floor((H - BAR - OY) / CH));
  const out = new Map<string, [number, number]>();
  let col = 0, row = 0;
  let last: Kind | null = null;
  for (const i of sorted) {
    if (last && last !== i.kind && row > 0) {
      row += 1;
      if (row >= rows - 1) {
        col += 1;
        row = 0;
      }
    }
    last = i.kind;
    out.set(i.key, [OX + col * CW, OY + row * CH]);
    row += 1;
    if (row >= rows) {
      col += 1;
      row = 0;
    }
  }
  return out;
}

// ---- the pet's "window" on this canvas
const pet = { x: 0, y: 0, w: 200, h: 200 };
const broom = newBroom();
let band = "#cfc3e6";
let homeAt = { x: 0, y: 0 };
const bristleOff = () => ({ x: pet.w / 2 + BRISTLE[0], y: pet.h - FOOT_H + BRISTLE[1] });

function glide(to: { x: number; y: number }, ms: number, even = false): Promise<void> {
  const from = { x: pet.x, y: pet.y };
  const t0 = performance.now();
  return new Promise((done) => {
    const step = () => {
      const k = Math.min(1, (performance.now() - t0) / ms);
      const e = even ? (k < 0.12 ? (k * k) / 0.24 : k > 0.88 ? 1 - ((1 - k) * (1 - k)) / 0.24 : (k - 0.06) / 0.88) : k * k * (3 - 2 * k);
      pet.x = from.x + (to.x - from.x) * e;
      pet.y = from.y + (to.y - from.y) * e;
      if (k < 1) requestAnimationFrame(step);
      else done();
    };
    step();
  });
}

const stage: Stage = {
  async plan(): Promise<DeskPlan> {
    if (STOW) {
      // The folder appears first; what stays is packed, what goes flies into it.
      const stay = icons.filter((i) => !i.old);
      const drawer: Ic = { key: DRAWER, kind: "folder", x: 0, y: 0, born: performance.now() };
      const to = layout([...stay, drawer]);
      [drawer.x, drawer.y] = to.get(DRAWER)!;
      icons.push(drawer);
      const items: DeskItem[] = icons
        .filter((i) => i.key !== DRAWER)
        .map((i) => (i.old ? { key: i.key, kind: i.kind, x: i.x, y: i.y, tx: drawer.x, ty: drawer.y, stow: true } : { key: i.key, kind: i.kind, x: i.x, y: i.y, tx: to.get(i.key)![0], ty: to.get(i.key)![1] }));
      return { items, cw: CW, ch: CH, strays: 0 };
    }
    const to = layout(icons);
    const items: DeskItem[] = icons.map((i) => ({ key: i.key, kind: i.kind, x: i.x, y: i.y, tx: to.get(i.key)![0], ty: to.get(i.key)![1] }));
    return { items, cw: CW, ch: CH, strays: 0 };
  },
  arrived(key) {
    const i = icons.find((c) => c.key === key);
    if (i) i.fade = performance.now();
  },
  async stow(keys) {
    for (const k of keys) {
      const at = icons.findIndex((c) => c.key === k);
      if (at >= 0) icons.splice(at, 1);
    }
    return true;
  },
  begin: async () => true,
  move(m) {
    for (const [key, x, y] of m) {
      const i = icons.find((c) => c.key === key);
      if (i) Object.assign(i, { x, y });
    }
  },
  end: async () => {},
  async flyTo(x, y, ms) {
    const o = bristleOff();
    await glide({ x: x - o.x, y: y - o.y }, ms);
  },
  where: async () => ({ x: pet.x + bristleOff().x, y: pet.y + bristleOff().y }),
  async home() {
    const d = Math.hypot(homeAt.x - pet.x, homeAt.y - pet.y);
    await glide(homeAt, Math.max(120, Math.min(2600, d / 0.33)), true);
  },
};

// ---- drawing the desktop
function wallpaper() {
  const g = ctx.createLinearGradient(0, 0, 0, H);
  g.addColorStop(0, "#c9d6ef");
  g.addColorStop(0.55, "#e8def0");
  g.addColorStop(1, "#f6e3d6");
  ctx.fillStyle = g;
  ctx.fillRect(0, 0, W, H);
  // Soft hills, far and near.
  for (const [y, amp, col] of [[470, 40, "rgba(176,196,170,.55)"], [540, 30, "rgba(150,178,146,.6)"]] as const) {
    ctx.fillStyle = col;
    ctx.beginPath();
    ctx.moveTo(0, H);
    for (let x = 0; x <= W; x += 20) ctx.lineTo(x, y + Math.sin(x / 210 + y) * amp + Math.sin(x / 90) * amp * 0.25);
    ctx.lineTo(W, H);
    ctx.fill();
  }
  // A pale sun.
  ctx.fillStyle = "rgba(255,248,232,.7)";
  ctx.beginPath();
  ctx.arc(1010, 150, 60, 0, Math.PI * 2);
  ctx.fill();
}

function taskbar() {
  ctx.fillStyle = "rgba(246,244,250,.92)";
  ctx.fillRect(0, H - BAR, W, BAR);
  ctx.fillStyle = "rgba(120,110,140,.18)";
  ctx.fillRect(0, H - BAR, W, 1);
  const cols = ["#9db7e6", "#8cc79a", "#e7b47d", "#c8a6e6", "#e79aa8", "#7fc6c2"];
  cols.forEach((c, k) => {
    ctx.fillStyle = c;
    ctx.beginPath();
    ctx.roundRect(W / 2 - 3 * 40 + k * 40 + 6, H - BAR + 8, 28, 28, 7);
    ctx.fill();
  });
  ctx.fillStyle = "#5b4a57";
  ctx.font = "12px system-ui, 'Microsoft YaHei', sans-serif";
  ctx.textAlign = "right";
  ctx.fillText("16:20", W - 16, H - BAR + 19);
  ctx.fillText("2026/11/12", W - 16, H - BAR + 35);
}

function glyph(i: Ic, cx: number, top: number) {
  const s = 40;
  const x = cx - s / 2, y = top;
  ctx.save();
  ctx.lineJoin = ctx.lineCap = "round";
  ctx.shadowColor = "rgba(40,30,50,.25)";
  ctx.shadowBlur = 4;
  ctx.shadowOffsetY = 1.5;
  const page = (fill: string, ink?: () => void) => {
    ctx.fillStyle = fill;
    ctx.beginPath();
    ctx.moveTo(x + 7, y);
    ctx.lineTo(x + 26, y);
    ctx.lineTo(x + 34, y + 8);
    ctx.lineTo(x + 34, y + s);
    ctx.lineTo(x + 7, y + s);
    ctx.closePath();
    ctx.fill();
    ctx.shadowColor = "transparent";
    ctx.fillStyle = "rgba(0,0,0,.08)";
    ctx.beginPath();
    ctx.moveTo(x + 26, y);
    ctx.lineTo(x + 26, y + 8);
    ctx.lineTo(x + 34, y + 8);
    ctx.fill();
    ink?.();
  };
  const ext = i.key.split(".").pop() ?? "";
  if (i.kind === "system" && i.key === "回收站") {
    ctx.fillStyle = "#eef1f6";
    ctx.beginPath();
    ctx.moveTo(x + 8, y + 8);
    ctx.lineTo(x + 32, y + 8);
    ctx.lineTo(x + 29, y + s);
    ctx.lineTo(x + 11, y + s);
    ctx.closePath();
    ctx.fill();
    ctx.shadowColor = "transparent";
    ctx.strokeStyle = "#9aa6b8";
    ctx.lineWidth = 1.5;
    for (const dx of [15, 20, 25]) {
      ctx.beginPath();
      ctx.moveTo(x + dx, y + 13);
      ctx.lineTo(x + dx, y + s - 4);
      ctx.stroke();
    }
    ctx.fillStyle = "#c7cfdb";
    ctx.fillRect(x + 6, y + 4, 28, 4);
  } else if (i.kind === "system") {
    ctx.fillStyle = "#6f8fc4";
    ctx.beginPath();
    ctx.roundRect(x + 2, y + 4, 36, 25, 3);
    ctx.fill();
    ctx.shadowColor = "transparent";
    ctx.fillStyle = "#cfe0f6";
    ctx.fillRect(x + 5, y + 7, 30, 19);
    ctx.fillStyle = "#8a96a8";
    ctx.fillRect(x + 16, y + 29, 8, 6);
    ctx.fillRect(x + 10, y + 35, 20, 3);
  } else if (i.kind === "app") {
    ctx.fillStyle = i.tint ?? "#9db7e6";
    ctx.beginPath();
    ctx.roundRect(x + 3, y + 2, 34, 34, 9);
    ctx.fill();
    ctx.shadowColor = "transparent";
    ctx.fillStyle = "rgba(255,255,255,.85)";
    ctx.beginPath();
    ctx.arc(x + 20, y + 19, 8, 0, Math.PI * 2);
    ctx.fill();
    // the shortcut arrow
    ctx.fillStyle = "#fff";
    ctx.fillRect(x + 1, y + 26, 11, 11);
    ctx.strokeStyle = "#3d6db8";
    ctx.lineWidth = 1.6;
    ctx.beginPath();
    ctx.moveTo(x + 4, y + 34);
    ctx.lineTo(x + 9, y + 29);
    ctx.moveTo(x + 6, y + 29);
    ctx.lineTo(x + 9, y + 29);
    ctx.lineTo(x + 9, y + 32);
    ctx.stroke();
  } else if (i.kind === "folder") {
    ctx.fillStyle = "#e8b84f";
    ctx.beginPath();
    ctx.roundRect(x + 1, y + 6, 16, 8, 2);
    ctx.fill();
    ctx.fillStyle = "#f6cf6a";
    ctx.beginPath();
    ctx.roundRect(x + 1, y + 10, 38, 27, 3);
    ctx.fill();
  } else if (i.kind === "doc") {
    const tag = ext === "pdf" ? "#d9655b" : ext === "xlsx" ? "#4f9a68" : ext === "md" ? "#7a7f8c" : "#4c7fd0";
    page("#fbfbfd", () => {
      ctx.strokeStyle = "#c6ccd8";
      ctx.lineWidth = 1.4;
      for (let k = 0; k < 4; k++) {
        ctx.beginPath();
        ctx.moveTo(x + 11, y + 15 + k * 5);
        ctx.lineTo(x + 30, y + 15 + k * 5);
        ctx.stroke();
      }
      ctx.fillStyle = tag;
      ctx.beginPath();
      ctx.roundRect(x + 3, y + 26, 20, 11, 2);
      ctx.fill();
      ctx.fillStyle = "#fff";
      ctx.font = "bold 7px system-ui, sans-serif";
      ctx.textAlign = "center";
      ctx.fillText(ext.toUpperCase().slice(0, 4), x + 13, y + 34);
    });
  } else if (i.kind === "picture") {
    page("#fbfbfd", () => {
      ctx.fillStyle = "#cfe3f6";
      ctx.fillRect(x + 10, y + 12, 21, 20);
      ctx.fillStyle = "#86b878";
      ctx.beginPath();
      ctx.moveTo(x + 10, y + 32);
      ctx.lineTo(x + 18, y + 21);
      ctx.lineTo(x + 24, y + 27);
      ctx.lineTo(x + 27, y + 24);
      ctx.lineTo(x + 31, y + 32);
      ctx.fill();
      ctx.fillStyle = "#f3c75a";
      ctx.beginPath();
      ctx.arc(x + 26, y + 16, 2.6, 0, Math.PI * 2);
      ctx.fill();
    });
  } else if (i.kind === "media") {
    page("#fbfbfd", () => {
      ctx.fillStyle = "#a585d6";
      ctx.beginPath();
      ctx.arc(x + 20, y + 23, 9, 0, Math.PI * 2);
      ctx.fill();
      ctx.fillStyle = "#fff";
      ctx.beginPath();
      ctx.moveTo(x + 17, y + 18);
      ctx.lineTo(x + 25, y + 23);
      ctx.lineTo(x + 17, y + 28);
      ctx.fill();
    });
  } else {
    const exe = ext === "exe";
    ctx.fillStyle = exe ? "#7aa3dc" : "#d8b07a";
    ctx.beginPath();
    ctx.roundRect(x + 4, y + 6, 32, 31, 4);
    ctx.fill();
    ctx.shadowColor = "transparent";
    ctx.fillStyle = exe ? "#fff" : "#8a6a3e";
    if (exe) {
      ctx.beginPath();
      ctx.moveTo(x + 20, y + 30);
      ctx.lineTo(x + 13, y + 22);
      ctx.lineTo(x + 17, y + 22);
      ctx.lineTo(x + 17, y + 13);
      ctx.lineTo(x + 23, y + 13);
      ctx.lineTo(x + 23, y + 22);
      ctx.lineTo(x + 27, y + 22);
      ctx.fill();
    } else {
      for (let k = 0; k < 5; k++) ctx.fillRect(x + 18 + (k % 2) * 3, y + 7 + k * 4, 3, 3);
    }
  }
  ctx.restore();
}

function drawIcon(i: Ic) {
  const cx = i.x + CW / 2;
  const now = performance.now();
  // Into the folder: it shrinks into it. A new folder: it opens out.
  const k = i.fade ? Math.max(0, 1 - (now - i.fade) / 260) : i.born ? Math.min(1, 0.3 + (now - i.born) / 380) : 1;
  if (k <= 0) return;
  ctx.save();
  ctx.globalAlpha = i.fade ? k : 1;
  ctx.translate(cx, i.y + 26);
  ctx.scale(k, k);
  ctx.translate(-cx, -(i.y + 26));
  glyph(i, cx, i.y + 6);
  ctx.restore();
  if (i.fade) return;
  ctx.save();
  ctx.font = "12px system-ui, 'Microsoft YaHei', sans-serif";
  ctx.textAlign = "center";
  ctx.fillStyle = "#fff";
  ctx.shadowColor = "rgba(20,15,30,.75)";
  ctx.shadowBlur = 3;
  ctx.shadowOffsetY = 1;
  let label = i.key;
  while (ctx.measureText(label).width > CW - 4 && label.length > 2) label = label.slice(0, -1);
  if (label !== i.key) label = label.slice(0, -1) + "…";
  ctx.fillText(label, cx, i.y + 62);
  ctx.restore();
}

// ---- go
async function main() {
  const me = applySkin(q.get("skin") ?? "zimeng");
  band = me.colors.accentOff;
  const rig = await loadRig(me.rig ?? "rig/zimeng/");
  const perf = createPerformer(null, rig);
  const scale = RIG_PX;
  const [x0, y0, x1, y1] = rig.rig.bbox;
  // The bubble and tag layer, like the pet window's top canvas.
  const top = document.createElement("canvas");
  const topCtx = top.getContext("2d")!;
  const r = new PetRenderer(topCtx);
  r.rigBox = { w: (x1 - x0) * scale, h: (y1 - y0) * scale };
  r.onResize = (w, h) => {
    top.width = w;
    top.height = h;
    // Like the pet window: it grows upwards, its feet stay where they are.
    pet.x += (pet.w - w) / 2;
    pet.y += pet.h - h;
    pet.w = w;
    pet.h = h;
  };
  r.fit();
  homeAt = { x: 860, y: H - BAR - pet.h };
  Object.assign(pet, homeAt);

  const lean = (p: Pose, dir: 1 | -1) => {
    if (p === "ride") Object.assign(rig.play, { rot: dir * 7, yaw: dir * 0.5, look: [dir * 4, 0], face: "happy" });
    else if (p === "sweep") Object.assign(rig.play, { rot: -5, yaw: -0.4, look: [-5, 3], face: null });
    else Object.assign(rig.play, { rot: 0, yaw: null, look: null, face: null });
  };

  let last = performance.now();
  const frame = (now: number) => {
    const dt = Math.min(0.1, (now - last) / 1000);
    last = now;
    if (broom.pose === "sweep") rig.play.rot = sweepLean(broom, now);
    perf.tick(dt);
    r.update(now);
    wallpaper();
    for (const i of icons) drawIcon(i);
    taskbar();
    ctx.save();
    ctx.translate(pet.x + pet.w / 2 - rig.foot[0] * scale, pet.y + pet.h - FOOT_H - rig.foot[1] * scale);
    ctx.restore();
    drawBroom(ctx, broom, pet.x + pet.w / 2, pet.y + pet.h - FOOT_H, 1, now, band, "back");
    ctx.save();
    ctx.translate(pet.x + pet.w / 2 - rig.foot[0] * scale, pet.y + pet.h - FOOT_H - rig.foot[1] * scale);
    perf.draw(ctx, scale);
    ctx.restore();
    drawBroom(ctx, broom, pet.x + pet.w / 2, pet.y + pet.h - FOOT_H, 1, now, band, "front");
    ctx.drawImage(top, pet.x, pet.y);
    requestAnimationFrame(frame);
  };
  requestAnimationFrame(frame);

  (window as unknown as { go: () => Promise<number | null> }).go = async () => {
    if (STOW) {
      // It asks first; the recording answers for you.
      r.ask(tr("桌面快铺满了。很久没碰的那些，我收起来？", "The desktop's nearly full. Put the old ones away?"), tr("好", "Yes"), tr("不用", "Not now"), 30_000, () => {});
      await new Promise((d) => window.setTimeout(d, 2600));
      r.answer(false);
    }
    const n = await sweep(stage, broom, { pose: lean });
    if (n) r.ask(STOW ? tr("收好了。都在「桌面收纳」里，按类分开放的。", "Put away, by type, in the 桌面收纳 folder.") : line("tidy.done"), tr("放回原处", "Put them back"), tr("留着", "Keep"), 30_000, () => {});
    return n;
  };
  (window as unknown as { back: () => void }).back = () => {
    r.answer(true);
    for (const i of icons) Object.assign(i, home.get(i.key));
    r.say(line("tidy.undo"), 3000);
  };
  // For checking how a question sits in the bubble.
  (window as unknown as { __ask: (text: string) => void }).__ask = (text) => r.ask(text, tr("好", "Yes"), tr("不用", "Not now"), 60_000, () => {});
  (window as unknown as { ready: boolean; __pet: typeof pet }).ready = true;
  (window as unknown as { __pet: typeof pet }).__pet = pet;
  if (q.has("auto")) window.setTimeout(() => void (window as unknown as { go: () => void }).go(), 900);
}

void main();
