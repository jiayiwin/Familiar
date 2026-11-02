// First start: pick a pet for Claude Code, then one for Codex (design §5.1).
// Skipping keeps the recommended defaults already in config.yaml.

import { connect, getJson, inTauri, postJson, type Conn, type PackInfo } from "./api";
import { Previews, toast } from "./preview";
import { initLang, t } from "./i18n";

const STEPS = [
  { target: "claude-code", title: () => t("给 Claude Code 选一只宠物", "Pick a pet for Claude Code"), rec: "pixel-cat" },
  { target: "codex", title: () => t("给 Codex 选一只宠物", "Pick a pet for Codex"), rec: "pixel-bot" },
] as const;

const $ = (id: string) => document.getElementById(id)!;
let conn: Conn;
let previews: Previews;
let packs: PackInfo[] = [];
let rev = 0;
let step = 0;
const picks: string[] = [];

function esc(s: string): string {
  return s.replace(/[&<>"]/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;" })[c]!);
}

async function render() {
  const s = STEPS[step];
  const rec = packs.some((p) => p.id === s.rec) ? s.rec : packs[0]?.id;
  picks[step] ??= rec;
  $("step").textContent = t(`第 ${step + 1} 步，共 ${STEPS.length} 步`, `Step ${step + 1} of ${STEPS.length}`);
  $("title").textContent = s.title();
  $("lead").textContent =
    step === 0
      ? t(
          "宠物会跟着 agent 的真实状态动：思考、敲代码、等你批准、完成。之后随时可以在托盘的「宠物图鉴」里换。",
          "Your pet acts out what the agent is really doing: thinking, coding, waiting for your approval, done. You can switch anytime from the tray's Gallery.",
        )
      : picks[0]
        ? t(
            `两边选同一只也可以，Codex 那只会换个颜色，脚下标 CC / CX 区分。`,
            "Picking the same pet for both is fine — the Codex one gets a different color and a CC / CX tag under its feet.",
          )
        : "";
  ($("back") as HTMLButtonElement).hidden = step === 0;
  $("next").textContent = step === STEPS.length - 1 ? t("完成", "Done") : t("下一步", "Next");

  previews.clear();
  const grid = $("grid");
  grid.innerHTML = "";
  for (const p of packs) {
    const card = document.createElement("div");
    card.className = `card${picks[step] === p.id ? " picked" : ""}`;
    const sameAsFirst = step === 1 && picks[0] === p.id;
    card.appendChild(await previews.mount(p.id, rev, 112, sameAsFirst ? 180 : 0));
    card.insertAdjacentHTML(
      "beforeend",
      `${p.id === s.rec ? `<span class="rec">${t("推荐", "Recommended")}</span>` : ""}<b>${esc(p.name || p.id)}</b><div class="meta">${p.tags.map(esc).join(" · ")}</div>`,
    );
    card.onclick = () => {
      picks[step] = p.id;
      void render();
    };
    grid.appendChild(card);
  }
}

async function close() {
  if (!inTauri) return;
  const { getCurrentWindow } = await import("@tauri-apps/api/window");
  await getCurrentWindow().close();
}

async function finish() {
  for (let i = 0; i < STEPS.length; i++) {
    const pack = picks[i];
    if (!pack) continue;
    const r = await postJson(conn, "/v1/select", { target: STEPS[i].target, pack });
    if (!r.ok) return toast(r.message);
  }
  await close();
}

async function main() {
  conn = await connect();
  await initLang(conn);
  previews = new Previews(conn);
  const res = await getJson<{ packs: PackInfo[]; rev: number }>(conn, "/v1/packs");
  packs = res.packs;
  rev = res.rev;
  $("skip").onclick = () => void close();
  $("back").onclick = () => {
    step = Math.max(0, step - 1);
    void render();
  };
  $("next").onclick = () => {
    if (step < STEPS.length - 1) {
      step++;
      void render();
    } else {
      void finish();
    }
  };
  await render();
}

main().catch((e) => {
  document.body.innerHTML = `<div class="banner">${t("加载失败：", "Failed to load: ")}${String(e)}</div>`;
});
