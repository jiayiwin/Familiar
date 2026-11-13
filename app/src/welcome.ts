// First start: 紫梦 acts an agent's states out (here and on the desktop, where
// the daemon scripts the real pet), then you hook the agents up.

import { closeSelf, connect, inTauri, postJson, type Conn } from "./api";
import { initLang, t } from "./i18n";
import { faceFor, mountPortrait, type Mood } from "./rig/portrait";
import type { RigPet } from "./rig/rig.js";
import { applySkin, followSkin, skinFor } from "./skin";

const $ = (id: string) => document.getElementById(id)!;
let conn: Conn;
let actor: RigPet | null = null;

interface Beat {
  at: number;
  ms: number;
  state: string;
  who: number;
  action: string;
  bubble: string;
}
let timers: number[] = [];

/** The face that goes with a state on stage. */
const MOODS: Record<string, Mood> = { thinking: "work", tool: "work", attention: "wait", done: "done", error: "worry" };

/** Play the show: the daemon scripts the desktop pet, this window follows the same beats. */
async function play() {
  timers.forEach(clearTimeout);
  timers = [];
  const legend = [...document.querySelectorAll<HTMLLIElement>("#legend li")];
  const say = $("say");
  legend.forEach((li) => li.classList.remove("on"));
  say.classList.remove("on");
  const replay = $("replay") as HTMLButtonElement;
  replay.disabled = true;
  const { beats } = await postJson<{ beats: Beat[] }>(conn, "/v1/demo", {}).catch(() => ({ beats: [] as Beat[] }));
  let end = 0;
  // One actor here: the beats of the states in the legend, in order.
  const mine = (beats ?? []).filter((b) => b.state in MOODS).sort((a, b) => a.at - b.at);
  for (const b of mine) {
    end = Math.max(end, b.at + b.ms);
    timers.push(
      window.setTimeout(() => {
        legend.forEach((li) => li.classList.toggle("on", li.dataset.state === b.state));
        if (actor) {
          actor.expr = faceFor(MOODS[b.state], b.state);
          if (b.state === "done") actor.react("cheer");
          else if (b.state === "attention") actor.react("perk");
        }
        say.textContent = b.bubble;
        say.classList.add("on");
      }, b.at),
    );
  }
  timers.push(
    window.setTimeout(() => {
      legend.forEach((li) => li.classList.remove("on"));
      say.classList.remove("on");
      if (actor) actor.expr = "neutral";
      replay.disabled = false;
    }, end + 400),
  );
}

const close = () => closeSelf();

/** Same confirm-and-install as the settings' 接入 entries. */
async function hookUp(which: "claude-code" | "codex") {
  if (!inTauri) return;
  const { invoke } = await import("@tauri-apps/api/core");
  await invoke("connect_agent", { which });
}

async function main() {
  conn = await connect();
  await initLang(conn);
  const sk = applySkin((await skinFor(conn)).skin.id);
  followSkin(conn);
  $("hello").textContent = t(`这是${sk.zh}`, `This is ${sk.en}`);
  actor = await mountPortrait($("actor") as HTMLCanvasElement, sk.rig ?? undefined).catch(() => null);
  $("done").onclick = () => void close();
  $("replay").onclick = () => void play();
  $("hook-cc").onclick = () => void hookUp("claude-code");
  $("hook-cx").onclick = () => void hookUp("codex");
  await play();
}

main().catch((e) => {
  document.body.textContent = t("加载失败：", "Failed to load: ") + String(e);
});
