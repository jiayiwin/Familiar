// Daily report card (design §7.2): agent work time, turns, fish, bugs and the
// tug result, drawn on a canvas so it can be copied as an image.

import { connect, fileUrl, getJson, inTauri, RARITY_COLORS, type CampDay, type Conn, type Report } from "./api";
import { toast } from "./preview";
import { initLang, t as tr } from "./i18n"; // `t` is a common local name here

const W = 400;
const canvas = document.getElementById("card") as HTMLCanvasElement;
const ctx = canvas.getContext("2d")!;
let conn: Conn;
let names: Record<string, string> = {};
let day = new Date();

const ymd = (d: Date) => `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, "0")}-${String(d.getDate()).padStart(2, "0")}`;

function eff(d?: CampDay): number {
  if (!d || d.turns < 3) return 0;
  return (d.clean - d.errors) / d.turns;
}

function rrect(x: number, y: number, w: number, h: number, r: number) {
  ctx.beginPath();
  ctx.roundRect(x, y, w, h, r);
}

function text(t: string, x: number, y: number, size: number, color: string, weight = "normal", align: CanvasTextAlign = "left", maxW?: number) {
  ctx.font = weight === "serif" ? `bold ${size}px Bahnschrift, "Segoe UI", "Microsoft YaHei UI", sans-serif` : `${weight} ${size}px "Segoe UI", "Microsoft YaHei UI", "PingFang SC", system-ui, sans-serif`;
  ctx.fillStyle = color;
  ctx.textAlign = align;
  ctx.textBaseline = "alphabetic";
  if (maxW) ctx.fillText(t, x, y, maxW);
  else ctx.fillText(t, x, y);
}

/**
 * A big number with small units ("1 小时 5 分" → big 1, small 小时, big 5…),
 * shrunk to fit its column so wide values never run into the next one.
 */
function stat(label: string, value: string, x: number, y: number, maxW = 112) {
  const parts = value.match(/[+\-<]?\d+(?:\.\d+)?|[^\d+\-<]+/g) ?? [value];
  const big = (p: string) => /\d/.test(p);
  const font = (p: string, k: number) => (big(p) ? `bold ${Math.round(26 * k)}px Bahnschrift, "Segoe UI", sans-serif` : `600 ${Math.round(12 * k)}px "Segoe UI", "Microsoft YaHei UI", sans-serif`);
  const width = (k: number) =>
    parts.reduce((w, p) => {
      ctx.font = font(p.trim() || p, k);
      return w + ctx.measureText(p.trim()).width + (big(p) ? 0 : 3 * k);
    }, 0);
  const k = Math.min(1, maxW / Math.max(1, width(1)));
  let cx = x;
  ctx.textAlign = "left";
  ctx.textBaseline = "alphabetic";
  for (const p of parts) {
    const t = p.trim();
    if (!t) continue;
    ctx.font = font(t, k);
    ctx.fillStyle = big(t) ? "#1f2245" : "#4a4e72";
    if (!big(t)) cx += 2 * k;
    ctx.fillText(t, cx, y);
    cx += ctx.measureText(t).width + (big(t) ? 0 : 1 * k);
  }
  text(label, x, y + 20, 11.5, "#8b8ca6", "600");
}

/** Work time for the report: "<1 分钟", "25 分钟", "1 小时 5 分". */
function workTime(ms: number): string {
  const m = Math.floor(ms / 60000);
  if (m < 1) return tr("<1 分钟", "<1 min");
  if (m < 60) return tr(`${m} 分钟`, `${m} min`);
  return tr(`${Math.floor(m / 60)} 小时 ${m % 60} 分`, `${Math.floor(m / 60)} h ${m % 60} min`);
}

function fishIcon(x: number, y: number, color: string) {
  ctx.fillStyle = color;
  ctx.beginPath();
  ctx.ellipse(x, y, 9, 5, 0, 0, Math.PI * 2);
  ctx.fill();
  ctx.beginPath();
  ctx.moveTo(x - 8, y);
  ctx.lineTo(x - 14, y - 5);
  ctx.lineTo(x - 14, y + 5);
  ctx.fill();
  ctx.fillStyle = "#fff";
  ctx.fillRect(x + 3, y - 2, 2, 2);
}

interface Games {
  affinity: number;
  packs: { "claude-code": string | null; codex: string | null };
}
let games: Games | null = null;
const photos: HTMLImageElement[] = [];

function draw(r: Report) {
  const fish = r.catches.filter((c) => c.outcome !== "boot" && c.outcome !== "escape");
  const rows = Math.ceil(Math.min(fish.length, 24) / 2);
  const photo = (games?.affinity ?? 0) >= 80 && photos.length === 2;
  const H = 396 + rows * 22 + (fish.length ? 10 : 0) + (photo ? 120 : 0);
  const dpr = devicePixelRatio || 1;
  canvas.width = W * dpr;
  canvas.height = H * dpr;
  canvas.style.width = `${W}px`;
  canvas.style.height = `${H}px`;
  ctx.setTransform(dpr, 0, 0, dpr, 0, 0);

  // Card: an indigo header with a few bold shapes, then tinted stat tiles.
  ctx.fillStyle = "#f6f1e7";
  ctx.fillRect(0, 0, W, H);
  const hg = ctx.createLinearGradient(0, 0, W, 0);
  hg.addColorStop(0, "#2b3275");
  hg.addColorStop(1, "#3b44a0");
  ctx.fillStyle = hg;
  ctx.fillRect(0, 0, W, 76);
  ctx.save();
  ctx.beginPath();
  ctx.rect(0, 0, W, 76);
  ctx.clip();
  const disc = (x: number, y: number, r: number, c: string) => {
    ctx.fillStyle = c;
    ctx.beginPath();
    ctx.arc(x, y, r, 0, Math.PI * 2);
    ctx.fill();
  };
  disc(W - 40, 80, 46, "#ee5b3c");
  disc(W - 112, 14, 18, "#f4b63f");
  disc(W + 4, 18, 26, "#6fc3a0");
  ctx.fillStyle = "#ad98e6";
  ctx.beginPath();
  ctx.arc(W - 150, 76, 22, Math.PI, 0);
  ctx.fill();
  ctx.restore();
  text(tr("今日报告", "Daily Report"), 20, 36, 21, "#ffffff", "serif");
  text(r.day, 20, 58, 11, "rgba(255,255,255,0.65)");
  const coin = tr(`金币 ${r.balance}`, `Coins ${r.balance}`);
  ctx.font = `bold 13px Bahnschrift, "Segoe UI", "Microsoft YaHei UI", sans-serif`;
  const cw = ctx.measureText(coin).width + 22;
  ctx.fillStyle = "#ffffff";
  rrect(W - 20 - cw - 96, 24, cw, 26, 13);
  ctx.fill();
  text(coin, W - 20 - 96 - cw / 2, 42, 13, "#2b3275", "serif", "center");
  const tints = ["#fde4d6", "#dcf1e6", "#e8e2fa", "#dde9fa", "#fdf0c8", "#fde4d6"];
  [[12, 88], [140, 88], [268, 88], [12, 154], [140, 154], [268, 154]].forEach(([x, y], i) => {
    ctx.fillStyle = "#ffffff";
    rrect(x, y, 120, 58, 12);
    ctx.fill();
    ctx.save();
    rrect(x, y, 120, 58, 12);
    ctx.clip();
    disc(x + 112, y + 6, 20, tints[i]);
    ctx.restore();
  });

  stat(tr("agent 工作时长", "Agent work time"), r.work_ms ? workTime(r.work_ms) : "0", 22, 118, 104);
  stat(tr("完成轮数", "Turns done"), String(r.turns), 150, 118, 104);
  stat(tr("一次通过", "First-try passes"), String(r.clean), 278, 118, 104);
  stat(tr("钓到的鱼", "Fish caught"), String(fish.length), 22, 184, 104);
  stat(tr("拍死的虫", "Bugs squashed"), `${r.bugs}${r.bosses ? ` + ${r.bosses} Boss` : ""}`, 150, 184, 104);
  stat(tr("今日金币", "Coins today"), `+${r.coins_earned}`, 278, 184, 104);

  // Tug result.
  const cc = r.by_camp["claude-code"];
  const cx = r.by_camp["codex"];
  let y = 244;
  ctx.fillStyle = "#e6ddcc";
  ctx.fillRect(20, y - 16, W - 40, 1);
  text(tr("今日拔河", "Today's tug-of-war"), 20, y + 4, 13, "#8b8ca6", "bold");
  if (cc || cx) {
    const a = eff(cc);
    const b = eff(cx);
    const verdict = !cc || !cx
        ? tr("只有一方上场", "Only one team played")
        : a > b
          ? tr("Claude Code 领先", "Claude Code leads")
          : b > a
            ? tr("Codex 领先", "Codex leads")
            : tr("平局", "Tie");
    text(verdict, W - 20, y + 4, 13, "#1f2245", "bold", "right");
    y += 26;
    for (const [name, d, color] of [["Claude Code", cc, "#e0794f"], ["Codex", cx, "#2f9a8a"]] as const) {
      if (!d) continue;
      text(name, 20, y, 12, color, "bold");
      text(
        tr(
          `${d.turns} 轮 · 一次通过 ${d.clean} · 出错 ${d.errors} · 效率分 ${eff(d).toFixed(2)}`,
          `${d.turns} turns · ${d.clean} first-try · ${d.errors} errors · score ${eff(d).toFixed(2)}`,
        ),
        110, y, 12, "#1f2245", "normal", "left", W - 130,
      );
      y += 20;
    }
  } else {
    y += 26;
    text(tr("今天还没有完成的回合", "No finished turns yet today"), 20, y, 12, "#8b8ca6");
    y += 20;
  }

  // Fish list.
  y += 14;
  ctx.fillStyle = "#e6ddcc";
  ctx.fillRect(20, y - 16, W - 40, 1);
  text(tr("鱼篓", "Fish basket"), 20, y + 4, 13, "#8b8ca6", "bold");
  y += 26;
  if (!fish.length) {
    text(tr("今天一条鱼都没钓到", "Not a single fish today"), 20, y, 12, "#8b8ca6");
    y += 20;
  }
  fish.slice(0, 24).forEach((f, i) => {
    const x = i % 2 === 0 ? 34 : W / 2 + 14;
    const yy = y + Math.floor(i / 2) * 22;
    fishIcon(x, yy - 4, RARITY_COLORS[f.rarity] ?? "#8a9bb0");
    text(`${names[f.fish_id] ?? f.fish_id}${f.coins ? `  +${f.coins}` : ""}`, x + 16, yy, 12, "#1f2245", "normal", "left", W / 2 - 34);
  });
  if (fish.length > 24) text(tr(`…还有 ${fish.length - 24} 条`, `…and ${fish.length - 24} more`), W - 20, y + rows * 22, 11, "#8b8ca6", "normal", "right");

  if (photo) {
    // Affinity 80+: the two pets in one picture.
    const py = H - 140;
    ctx.fillStyle = "#e6ddcc";
    ctx.fillRect(20, py - 6, W - 40, 1);
    text(tr(`合照 · 好感度 ${games!.affinity}`, `Photo · affinity ${games!.affinity}`), 20, py + 12, 13, "#8b8ca6", "bold");
    ctx.imageSmoothingEnabled = false;
    ctx.drawImage(photos[0], W / 2 - 100, py + 18, 96, 96);
    ctx.drawImage(photos[1], W / 2 + 4, py + 18, 96, 96);
  }
  text(
    tr("战绩仅供娱乐，任务难度不同，不代表模型能力", "Just for fun: tasks differ in difficulty, so this says nothing about model ability"),
    W / 2, H - 14, 10, "#a9a8bd", "normal", "center", W - 24,
  );
}

async function load() {
  const r = await getJson<Report>(conn, `/v1/report?day=${ymd(day)}`);
  draw(r);
  // Fit the window to the card (no empty space under it).
  if (inTauri) {
    const { getCurrentWindow, LogicalSize } = await import("@tauri-apps/api/window");
    const h = Math.min(screen.availHeight - 60, document.body.scrollHeight + 8);
    await getCurrentWindow().setSize(new LogicalSize(460, h)).catch(() => {});
  }
}

async function main() {
  conn = await connect();
  await initLang(conn);
  const stats = await getJson<{ fish: { id: string; name: string }[] }>(conn, "/v1/stats");
  names = Object.fromEntries(stats.fish.map((f) => [f.id, f.name]));
  games = await getJson<Games>(conn, "/v1/games").catch(() => null);
  if (games && games.affinity >= 80 && games.packs["claude-code"] && games.packs.codex) {
    for (const p of [games.packs["claude-code"], games.packs.codex]) {
      const img = new Image();
      img.crossOrigin = "anonymous";
      img.src = fileUrl(conn, p!, "preview.png");
      await new Promise((r) => ((img.onload = r), (img.onerror = r)));
      if (img.naturalWidth) photos.push(img);
    }
  }
  await load();
  canvas.onclick = async () => {
    if (!inTauri) return;
    const { getCurrentWindow } = await import("@tauri-apps/api/window");
    await getCurrentWindow().close();
  };
  document.getElementById("prev")!.onclick = () => {
    day = new Date(day.getTime() - 86_400_000);
    void load();
  };
  document.getElementById("next")!.onclick = () => {
    day = new Date(day.getTime() + 86_400_000);
    void load();
  };
  document.getElementById("copy")!.onclick = () => {
    canvas.toBlob(async (blob) => {
      if (!blob) return;
      try {
        await navigator.clipboard.write([new ClipboardItem({ "image/png": blob })]);
        toast(tr("已复制，可以直接粘贴到聊天里", "Copied — paste it straight into a chat"));
      } catch (e) {
        toast(tr(`复制失败：${String(e)}`, `Copy failed: ${String(e)}`));
      }
    }, "image/png");
  };
}

main().catch((e) => {
  document.body.innerHTML = `<div class="banner">${tr("报告加载失败：", "Failed to load the report: ")}${String(e)}</div>`;
});
