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

function text(t: string, x: number, y: number, size: number, color: string, weight = "normal", align: CanvasTextAlign = "left", maxW?: number) {
  ctx.font = `${weight} ${size}px 'Microsoft YaHei', 'PingFang SC', system-ui, sans-serif`;
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
  const font = (p: string, k: number) => `bold ${Math.round((big(p) ? 26 : 13) * k)}px 'Microsoft YaHei', 'PingFang SC', system-ui, sans-serif`;
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
    ctx.fillStyle = big(t) ? "#2b1d16" : "#5a4a40";
    if (!big(t)) cx += 2 * k;
    ctx.fillText(t, cx, y);
    cx += ctx.measureText(t).width + (big(t) ? 0 : 1 * k);
  }
  text(label, x, y + 20, 12, "#7a6a5f");
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
  const H = 380 + rows * 22 + (fish.length ? 10 : 0) + (photo ? 120 : 0);
  const dpr = devicePixelRatio || 1;
  canvas.width = W * dpr;
  canvas.height = H * dpr;
  canvas.style.width = `${W}px`;
  canvas.style.height = `${H}px`;
  ctx.setTransform(dpr, 0, 0, dpr, 0, 0);

  // Card.
  ctx.fillStyle = "#fffaf3";
  ctx.fillRect(0, 0, W, H);
  ctx.fillStyle = "#d9772b";
  ctx.fillRect(0, 0, W, 64);
  text(tr("Familiar 今日报告", "Familiar daily report"), 20, 30, 18, "#fff", "bold");
  text(r.day, 20, 50, 12, "rgba(255,255,255,0.85)");
  text(tr(`金币 ${r.balance}`, `Coins ${r.balance}`), W - 20, 38, 14, "#fff", "bold", "right");

  stat(tr("agent 工作时长", "Agent work time"), r.work_ms ? workTime(r.work_ms) : "0", 20, 104, 122);
  stat(tr("完成轮数", "Turns done"), String(r.turns), 154, 104);
  stat(tr("一次通过", "First-try passes"), String(r.clean), 274, 104);
  stat(tr("钓到的鱼", "Fish caught"), String(fish.length), 20, 170, 122);
  stat(tr("拍死的虫", "Bugs squashed"), `${r.bugs}${r.bosses ? ` + ${r.bosses} Boss` : ""}`, 154, 170);
  stat(tr("今日金币", "Coins today"), `+${r.coins_earned}`, 274, 170);

  // Tug result.
  const cc = r.by_camp["claude-code"];
  const cx = r.by_camp["codex"];
  let y = 228;
  ctx.fillStyle = "#eadfd2";
  ctx.fillRect(20, y - 16, W - 40, 1);
  text(tr("今日拔河", "Today's tug-of-war"), 20, y + 4, 13, "#7a6a5f", "bold");
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
    text(verdict, W - 20, y + 4, 13, "#2b1d16", "bold", "right");
    y += 26;
    for (const [name, d, color] of [["Claude Code", cc, "#d9772b"], ["Codex", cx, "#1f9d8f"]] as const) {
      if (!d) continue;
      text(name, 20, y, 12, color, "bold");
      text(
        tr(
          `${d.turns} 轮 · 一次通过 ${d.clean} · 出错 ${d.errors} · 效率分 ${eff(d).toFixed(2)}`,
          `${d.turns} turns · ${d.clean} first-try · ${d.errors} errors · score ${eff(d).toFixed(2)}`,
        ),
        110, y, 12, "#2b1d16", "normal", "left", W - 130,
      );
      y += 20;
    }
  } else {
    y += 26;
    text(tr("今天还没有完成的回合", "No finished turns yet today"), 20, y, 12, "#7a6a5f");
    y += 20;
  }

  // Fish list.
  y += 14;
  ctx.fillStyle = "#eadfd2";
  ctx.fillRect(20, y - 16, W - 40, 1);
  text(tr("鱼篓", "Fish basket"), 20, y + 4, 13, "#7a6a5f", "bold");
  y += 26;
  if (!fish.length) {
    text(tr("今天一条鱼都没钓到", "Not a single fish today"), 20, y, 12, "#7a6a5f");
    y += 20;
  }
  fish.slice(0, 24).forEach((f, i) => {
    const x = i % 2 === 0 ? 34 : W / 2 + 14;
    const yy = y + Math.floor(i / 2) * 22;
    fishIcon(x, yy - 4, RARITY_COLORS[f.rarity] ?? "#8a9bb0");
    text(`${names[f.fish_id] ?? f.fish_id}${f.coins ? `  +${f.coins}` : ""}`, x + 16, yy, 12, "#2b1d16", "normal", "left", W / 2 - 34);
  });
  if (fish.length > 24) text(tr(`…还有 ${fish.length - 24} 条`, `…and ${fish.length - 24} more`), W - 20, y + rows * 22, 11, "#7a6a5f", "normal", "right");

  if (photo) {
    // Affinity 80+: the two pets in one picture.
    const py = H - 140;
    ctx.fillStyle = "#eadfd2";
    ctx.fillRect(20, py - 6, W - 40, 1);
    text(tr(`合照 · 好感度 ${games!.affinity}`, `Photo · affinity ${games!.affinity}`), 20, py + 12, 13, "#7a6a5f", "bold");
    ctx.imageSmoothingEnabled = false;
    ctx.drawImage(photos[0], W / 2 - 100, py + 18, 96, 96);
    ctx.drawImage(photos[1], W / 2 + 4, py + 18, 96, 96);
  }
  text(
    tr("战绩仅供娱乐，任务难度不同，不代表模型能力", "Just for fun: tasks differ in difficulty, so this says nothing about model ability"),
    W / 2, H - 14, 10, "#a99a8e", "normal", "center", W - 24,
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
