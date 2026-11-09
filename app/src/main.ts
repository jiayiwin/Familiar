import { connect, fileUrl, getJson, inTauri, postJson, subscribe, type Conn, type Pack, type PetView, type WorldState } from "./api";
import { PetRenderer, type HitRect } from "./pet";
import { Sprites } from "./sprites";
import { initLang, t as tr } from "./i18n"; // `t` is a common local name here

const petId = new URLSearchParams(location.search).get("pet") ?? "main";
const canvas = document.getElementById("pet") as HTMLCanvasElement;
const ctx = canvas.getContext("2d")!;
const HOVER_MS = 1000;
/** 5 s, more than 12 clicks: annoyed (design §7.1). */
const ANNOY_WINDOW = 5000;
const ANNOY_CLICKS = 12;
const SULK_MS = 60_000;
/** Back and forth over the head 3 times within 2 s: petted. */
const PET_TURNS = 6;
const PET_WINDOW = 2000;

async function tauriWindow() {
  return (await import("@tauri-apps/api/window")).getCurrentWindow();
}

async function invoke<T = void>(cmd: string, args?: Record<string, unknown>): Promise<T | undefined> {
  if (!inTauri) return undefined;
  const { invoke } = await import("@tauri-apps/api/core");
  return invoke<T>(cmd, args).catch((e) => {
    console.error(cmd, e);
    return undefined;
  });
}

interface Script {
  pet: string;
  rule: string;
  action?: string | null;
  bubble?: string | null;
  ms: number;
  delay?: number;
  tug?: { lead: boolean | null; offset: number };
  /** Particles with the script (Spring Festival greetings, 抢红包 payouts). */
  burst?: "hearts" | "confetti" | "coins";
  burst_text?: string;
}

/** A two-pet game announced over WS, waiting for the walk to finish. */
let duoPlan: (Record<string, unknown> & { recv: number }) | null = null;

/** Lead flag of the tug scene we are about to act in (set by the script, used on arrival). */
let tugLead: boolean | null = null;
let tugMs = 0;

async function main() {
  const conn = await connect();
  await initLang(conn);
  const pet = new PetRenderer(ctx);
  let packKey = "";
  let loading: Promise<void> = Promise.resolve();
  let scriptTimers: number[] = [];

  pet.onResize = (w, h) => void resize(w, h);
  pet.onHits = (rects) => void reportHits(rects);

  async function usePack(id: string, rev: number, stage = 0) {
    const key = `${id}@${rev}#${stage}`;
    if (key === packKey) return;
    const animate = packKey !== "" && !packKey.startsWith(`${id}@`);
    packKey = key;
    const pack = await getJson<Pack>(conn, `/v1/packs/${encodeURIComponent(id)}`);
    // Evolved: the pack may ship a sprite folder per stage; missing files fall back.
    const dir = stage > 0 ? (pack.evolutions?.[stage - 1]?.dir ?? "") : "";
    const base = (p: string) => fileUrl(conn, id, p, rev);
    const evolved = dir ? (p: string) => fileUrl(conn, id, `${dir.replace(/\/$/, "")}/${p.split("/").pop()}`, rev) : undefined;
    const sprites = await Sprites.load(pack, evolved ?? base, evolved ? base : undefined);
    pet.switchTo(sprites, animate);
  }

  async function onView(v: PetView) {
    if (v.id !== petId) return;
    if (`${v.pack}@${v.pack_rev}#${v.stage ?? 0}` !== packKey) {
      loading = usePack(v.pack, v.pack_rev, v.stage ?? 0).catch((e) => console.error("pack", e));
    }
    // Attention must not wait for sprite downloads of a pack switch.
    if (v.state !== "attention" || !pet.sprites) await loading;
    pet.setView(v);
  }

  function onScript(s: Script) {
    if (s.pet !== petId) return;
    if (s.tug) {
      tugLead = s.tug.lead;
      tugMs = s.ms;
    }
    const run = () => {
      if (s.action) pet.override(s.action, s.ms, "script", s.bubble);
      else if (s.bubble) pet.say(s.bubble, s.ms);
      if (s.burst) pet.fx.burst(s.burst, s.burst_text || undefined, 200);
    };
    // Walk over first when the script moves the pet. One scene can queue
    // several beats for the same pet (lose line, then the whip), so each
    // delayed beat keeps its own timer; pet.cancel clears them all.
    if (s.delay) scriptTimers.push(window.setTimeout(run, s.delay));
    else run();
  }

  // 相声: this pet's actions on the show's timeline (the overlay draws the bubbles).
  let xsTimers: number[] = [];
  function playXs(d: Record<string, unknown>) {
    xsTimers.forEach(clearTimeout);
    xsTimers = [];
    const me = d.a === petId ? "a" : "b";
    const walk = Number(d.walk) || 1300;
    for (const l of (d.lines as { who: string; at: number; ms: number; act?: string | null }[]) ?? []) {
      if (!l.act || (l.who !== me && l.who !== "ab")) continue;
      const act = l.act;
      xsTimers.push(window.setTimeout(() => pet.override(act, Math.min(l.ms + 400, 3000), "script", null), walk + l.at));
    }
  }

  let lastFocus: boolean | null = null;
  const { pets } = await getJson<{ pets: PetView[] }>(conn, "/v1/pets");
  for (const v of pets) await onView(v);
  subscribe(conn, (type, data) => {
    const d = data as Record<string, unknown>;
    switch (type) {
      case "pet.state":
        void onView(data as PetView);
        break;
      case "pet.script":
        onScript(data as unknown as Script);
        break;
      case "duo":
        if (d.anchor === petId || d.mover === petId) duoPlan = { ...d, recv: performance.now() };
        break;
      case "codex.limits":
        if (d.pet === petId) {
          const used = Math.round(Number(d.used) || 0);
          if (d.kind === "card") playFx(pet, { kind: "card", from: d.from, used: d.used });
          else if (d.kind === "refresh") pet.override("cheer", 2500, "script", tr("额度回来了。继续。", "Quota's back. On we go."));
          else if (d.kind === "low") pet.override("sulk", 3500, "script", tr(`Codex 额度快见底了（已用 ${used}%），右键 → 重置卡`, `Codex quota almost gone (${used}% used). Right-click → reset card`));
        }
        break;
      case "xs":
        if (d.a === petId || d.b === petId) playXs(d);
        break;
      case "pet.cancel":
        if (d.pet === petId) {
          scriptTimers.forEach(clearTimeout);
          scriptTimers = [];
          xsTimers.forEach(clearTimeout);
          xsTimers = [];
          pet.clearOverride("script");
          pet.stopTug();
        }
        break;
      case "fx":
        if (d.pet === petId) playFx(pet, d.payload as Record<string, unknown>);
        break;
      case "whip":
        // The overlay draws the lash; the one hit jolts in its own window.
        if (d.to === petId) scriptTimers.push(window.setTimeout(() => pet.hit(), (Number(d.delay) || 0) + (Number(d.hit) || 0)));
        break;
      case "duel": {
        const pets = (d.pets as string[] | undefined) ?? [];
        const fmt = (ms: number) => (ms >= 60_000 ? tr(`${Math.floor(ms / 60_000)}分${Math.round((ms % 60_000) / 1000)}秒`, `${Math.floor(ms / 60_000)}m${Math.round((ms % 60_000) / 1000)}s`) : tr(`${Math.round(ms / 1000)}秒`, `${Math.round(ms / 1000)}s`));
        if (d.state === "start" && pets.includes(petId)) {
          pet.fx.chip("duel", tr("⚔ 擂台 · 等下一轮", "⚔ Duel · next turn"), "#b5332e");
          if (pets[0] === petId) pet.override("taunt", 2500, "script", tr("下一轮见真章。不出错，先做完的赢。", "Next turn settles it. Clean and first."));
          else setTimeout(() => pet.override("cheer", 2000, "script", tr("来啊，比比谁快！", "Bring it — let's see who's faster!")), 1800);
        } else if (d.state === "step" && d.pet === petId) {
          if (d.phase === "started") pet.fx.chip("duel", tr("⚔ 擂台 · 比赛中", "⚔ Duel · racing"), "#b5332e");
          else pet.fx.chip("duel", d.clean ? tr(`⚔ 做完了 ${fmt(Number(d.ms))}`, `⚔ Done in ${fmt(Number(d.ms))}`) : tr("⚔ 出错了…", "⚔ Error…"), d.clean ? "#2e7d32" : "#777");
        } else if (d.state === "done" && pets.includes(petId)) {
          if (d.winner === "tie") pet.fx.endChip("duel", "shatter", tr("平局", "Tie"));
          else if (d.winner_pet === petId) {
            pet.fx.endChip("duel", "win", tr("擂台胜 +15", "Duel won +15"));
            pet.fx.burst("confetti");
          } else pet.fx.endChip("duel", "lose", tr("擂台输了", "Duel lost"));
        } else if (d.state === "cancel" && pets.includes(petId)) {
          pet.fx.endChip("duel", "shatter", tr("取消", "Cancelled"));
        }
        break;
      }
      case "pet.fx":
        if (d.pet === petId && d.kind === "catch") {
          pet.reel(d.items as never[], Number(d.coins) || 0);
        }
        break;
      case "world.state": {
        const focus = (data as WorldState).focus;
        // Say so when focus mode flips, so nobody wonders where the games went.
        if (lastFocus !== null && focus !== lastFocus) {
          pet.say(focus ? tr("专注模式。我安静。", "Focus mode. I'll be quiet.") : tr("专注模式已关闭", "Focus mode off"), 3500);
        }
        lastFocus = focus;
        pet.setFocus(focus);
        pet.setRope((data as WorldState & { rope?: string | null }).rope);
        break;
      }
      case "race": {
        const pets = (d.pets as string[] | undefined) ?? [];
        if (d.state === "open" && pets.includes(petId)) {
          pet.fx.chip("race-flag", tr("🏁 赛跑中", "🏁 Racing"), "#e63946");
          pet.fx.progress("race", 0.04);
        } else if (d.state === "step" && d.pet === petId) {
          const n = Number(d.steps) || 0;
          pet.fx.progress("race", n / (n + 4));
        } else if (d.state === "done") {
          const net = Number(d.net) || 0;
          if (d.winner_pet === petId) {
            pet.fx.progress("race", 1);
            pet.fx.endChip("race-flag", "win", tr("🏆 赢了", "🏆 Won"));
            pet.fx.burst("confetti");
            if (pet.fx.hasChip("bet-race")) pet.fx.endChips("bet-race", "win", net > 0 ? `+${net}` : tr("押中", "Hit"));
            if (net > 0) pet.fx.burst("coins", `+${net}`, 300);
          } else if (d.loser_pet === petId) {
            pet.fx.endChip("race-flag", "lose", tr("跑输了", "Lost the race"));
            pet.fx.endChips("bet-race", "shatter");
          } else {
            pet.fx.endChips("bet-race", "lose");
          }
          setTimeout(() => pet.fx.dropProgress("race"), 1800);
        }
        break;
      }
      case "book":
        if (d.state === "done" && d.pet === petId) {
          const net = Number(d.net) || 0;
          pet.fx.endChips("bet-book", net > 0 ? "win" : "lose", net > 0 ? `✓ +${net}` : net < 0 ? `✗ ${net}` : tr("退注", "Refunded"));
          if (net > 0) pet.fx.burst("coins", `+${net}`, 200);
        }
        break;
      case "pet.fx":
        if (d.pet === petId && d.kind === "rps_round") pet.fx.hand(Number(d.sign), "pet", Number(d.delay) || 0, 900);
        break;
      case "achievement":
        if (d.pet === petId) pet.say(tr(`🏆 解锁成就「${String(d.name)}」`, `🏆 Achievement unlocked: ${String(d.name)}`) + (Number(d.coins) ? ` +${d.coins}` : ""), 5000);
        break;
      case "season": {
        const s = d.season as { week: string; winner: string };
        if (d.pet === petId) pet.say(tr(`上周（${s.week}）拔河冠军！这周戴皇冠`, `Tug-of-war champ of last week (${s.week})! Crown on all week`), 5000);
        break;
      }
    }
  });

  const frame = (t: number) => {
    pet.update(t);
    requestAnimationFrame(frame);
  };
  requestAnimationFrame(frame);

  setupMouse(pet, conn);
  if (inTauri) {
    await listenHost(pet, conn);
    await listenHostRest(pet);
  }
}

/** Events from the host: physics and the boss key. */
async function listenHost(pet: PetRenderer, conn: Conn) {
  const { getCurrentWebviewWindow } = await import("@tauri-apps/api/webviewWindow");
  const win = getCurrentWebviewWindow();
  let dizzy = false;
  await win.listen<{ kind: string }>("phys", (e) => {
    switch (e.payload.kind) {
      case "fly":
        dizzy = false;
        pet.override("fall", 0x7fffffff, "phys");
        break;
      case "dizzy":
        dizzy = true;
        void postJson(conn, `/v1/pets/${encodeURIComponent(petId)}/input`, { kind: "dizzy" });
        break;
      case "land":
        if (dizzy) pet.override("dizzy", 2000, "phys");
        else pet.clearOverride("phys");
        dizzy = false;
        break;
      case "walk":
        pet.override("walk", 0x7fffffff, "phys");
        break;
      case "stop":
        pet.clearOverride("phys");
        break;
      case "peek":
        pet.override("peek", 0, "phys");
        break;
      case "tug":
        pet.startTug((e.payload as { side?: string }).side === "right" ? "right" : "left", tugLead, tugMs || 4500);
        break;
      case "duo":
        if (duoPlan) {
          const p = duoPlan;
          pet.startDuo({
            game: String(p.game),
            side: (e.payload as { side?: string }).side === "right" ? "right" : "left",
            start: p.recv + Number(p.walk),
            ms: Number(p.ms),
            each: Number(p.each),
            hits: Number(p.hits),
            serve: p.anchor === petId,
            winner: p.winner === petId,
            seed: Number(p.seed),
          });
        }
        break;
    }
  });
  await win.listen<boolean>("boss", (e) => {
    pet.setMuted(e.payload);
    bossMuted = e.payload;
  });
  // Drop a pet pack (folder or .zip) on the pet to install it.
  await win.onDragDropEvent(async (e) => {
    if (e.payload.type !== "drop") return;
    const { invoke } = await import("@tauri-apps/api/core");
    for (const path of e.payload.paths) {
      try {
        const r = await invoke<{ status: string; name?: string; id?: string; errors?: string[] }>("import_pack", { path, overwrite: false });
        if (r.status === "installed") pet.override("cheer", 2000, "script", tr(`装好了：${r.name || r.id}，右键 → 换一只`, `Installed ${r.name || r.id}! Right-click → switch pet`));
        else if (r.status === "conflict") pet.say(tr(`已经有 ${r.id} 了，去图鉴里覆盖`, `${r.id} is already installed — overwrite it in the Gallery`), 4000);
        else pet.say(tr(`没装上：${(r.errors ?? [])[0] ?? "格式不对"}`, `Install failed: ${(r.errors ?? [])[0] ?? "not a valid pack"}`), 5000);
      } catch (err) {
        pet.say(String(err), 4000);
      }
    }
  });
  // Right-click games with real animation (also replayable over WS: /pet demo).
  await win.listen<Record<string, unknown>>("fx", (e) => playFx(pet, e.payload));
}

/** Boss key mutes everything, the pack sounds and these. */
let bossMuted = false;
function sfx(name: string) {
  if (bossMuted) return;
  const a = new Audio(`sfx/${name}.wav`);
  a.volume = 0.5;
  a.play().catch(() => {});
}

function playFx(pet: PetRenderer, p: Record<string, unknown>) {
  {
    switch (p.kind) {
      case "rps": {
        const coins = Number(p.coins) || 0;
        const [result, color] =
          p.outcome === "win" ? [tr("……你赢了。", "…you win."), "#c0392b"]
          : p.outcome === "lose" ? [tr("我赢啦～", "I win~"), "#2e7d32"]
          : [tr("平局，再来一把？", "Tie! Again?"), "#6b5040"];
        pet.fx.versus(Number(p.user), Number(p.pet), result, color);
        setTimeout(() => {
          // The jingle is yours: a win for you, a loss for you.
          if (p.outcome === "win") sfx("win");
          else if (p.outcome === "lose") sfx("lose");
          if (p.outcome === "win") {
            pet.override("sulk", 2200, "script", null);
            pet.fx.burst("confetti");
            if (coins > 0) pet.fx.burst("coins", `+${coins}`, 200);
          } else if (p.outcome === "lose") {
            pet.override("cheer", 2200, "script", null);
          }
        }, 1250);
        break;
      }
      case "card":
        pet.fx.card(Number(p.from) || 95, Number(p.used) || 0);
        setTimeout(() => {
          pet.override("cheer", 2600, "script", tr("刷新了。像没用过一样。", "Reset. Like it was never used."));
          pet.fx.burst("confetti");
        }, 2300);
        break;
      case "feed":
        if (p.ok) {
          // The fish flies in; when it lands: a bite, a shimmer, and whatever the pet has to say.
          const bond = String(p.bond ?? "");
          pet.fx.fish(() => {
            const baseText = String(p.text ?? tr("……嗯。", "…mm."));
            const hint = bond === "letter" ? tr("\n（有封信给你。在右键菜单里。）", "\n(There's a letter for you. In the right-click menu.)") : "";
            pet.override("eat", 0, "script", baseText + hint);
            pet.fx.burst("bite");
            pet.fx.burst("hearts", undefined, 350);
            if (bond === "letter" || bond === "secret") pet.fx.burst("glow", undefined, 500);
          });
        } else {
          pet.say(String(p.text), 3000);
        }
        break;
      case "bet":
        if (p.ok) {
          const label = p.game === "race" ? tr(`押 ${p.stake} ×${Number(p.odds).toFixed(1)}`, `Bet ${p.stake} ×${Number(p.odds).toFixed(1)}`) : `🎫 ${p.label} ${p.stake}`;
          pet.fx.chip(`bet-${p.game}-${Date.now()}`, label, "#c8901a");
        }
        pet.say(String(p.text), 3000);
        break;
    }
  }
}

async function listenHostRest(pet: PetRenderer) {
  const { getCurrentWebviewWindow } = await import("@tauri-apps/api/webviewWindow");
  const win = getCurrentWebviewWindow();
  // Results of right-click games (bets, rock-paper-scissors, feeding…).
  await win.listen<{ text: string; action?: string | null }>("say", (e) => {
    if (e.payload.action && e.payload.action !== "idle") pet.override(e.payload.action, 2200, "script", e.payload.text);
    else pet.say(e.payload.text, 3500);
  });
  // "来一局拔河" refused (e.g. Codex offline): only the camp pets say so.
  await win.listen<string>("tug-refused", (e) => {
    const v = pet.currentView();
    if (v?.camp === "claude-code" || v?.camp === "codex") pet.say(e.payload, 3500);
  });
  await win.listen("letter-notify", () => {
    pet.say(tr("我写了封信。在右键菜单里。", "I wrote you a letter. It's in the right-click menu."), 0);
  });
  if (await invoke<boolean>("quiet")) pet.setMuted(true);
}

/** Fit canvas and window to the pet, keeping the bottom-centre where it was. */
/** Latest size asked for, and whether a window resize is already running. */
let wanted: { w: number; h: number } | null = null;
let resizing = false;

async function resize(w: number, h: number) {
  const dpr = window.devicePixelRatio || 1;
  canvas.width = Math.round(w * dpr);
  canvas.height = Math.round(h * dpr);
  canvas.style.width = `${w}px`;
  canvas.style.height = `${h}px`;
  ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
  if (!inTauri) return;
  // Resizes are async; two in flight could finish out of order and leave the
  // window at an old size. Run one at a time, always toward the latest size.
  wanted = { w, h };
  if (resizing) return;
  resizing = true;
  try {
    const { LogicalSize, LogicalPosition } = await import("@tauri-apps/api/window");
    const win = await tauriWindow();
    while (wanted) {
      const t = wanted;
      wanted = null;
      const scale = await win.scaleFactor();
      const pos = (await win.outerPosition()).toLogical(scale);
      const size = (await win.innerSize()).toLogical(scale);
      if (Math.round(size.width) === t.w && Math.round(size.height) === t.h) continue;
      await win.setSize(new LogicalSize(t.w, t.h));
      await win.setPosition(new LogicalPosition(pos.x + size.width / 2 - t.w / 2, pos.y + size.height - t.h));
    }
  } finally {
    resizing = false;
  }
}

/** Only the pet bodies take clicks; the host makes the rest click-through. */
async function reportHits(rects: HitRect[]) {
  await invoke("set_hit_rects", { rects: rects.map((r) => ({ x: r.x, y: r.y, w: r.w, h: r.h })) });
}

/**
 * Click = squish (12 clicks in 5 s: annoyed, then 60 s of sulking);
 * press and move = drag and throw; stroking the head = petted;
 * right-click = menu; hover 1 s = session card.
 */
function setupMouse(pet: PetRenderer, conn: Conn) {
  let down: { x: number; y: number } | null = null;
  let hoverKey: string | null = null;
  let hoverTimer = 0;
  let cardShown = false;
  let clicks: number[] = [];
  let sulkUntil = 0;
  let stroke: { lastX: number; dir: number; turns: number[] } = { lastX: 0, dir: 0, turns: [] };
  let pettedUntil = 0;

  const hideCard = async () => {
    clearTimeout(hoverTimer);
    hoverKey = null;
    if (!cardShown || !inTauri) return;
    cardShown = false;
    const { emitTo } = await import("@tauri-apps/api/event");
    await emitTo("card", "card:hide", {});
  };

  const showCard = async (hit: HitRect) => {
    if (!inTauri) return;
    const win = await tauriWindow();
    const scale = await win.scaleFactor();
    const pos = (await win.innerPosition()).toLogical(scale);
    const { emitTo } = await import("@tauri-apps/api/event");
    cardShown = true;
    await emitTo("card", "card:show", { pet: petId, session: hit.key ?? null, ax: pos.x + hit.x, ay: pos.y + hit.y, aw: hit.w });
  };

  const onStroke = (e: MouseEvent, hit: HitRect | null) => {
    // Only over the top half of the main pet (its head).
    if (!hit || hit.kind !== "main" || e.offsetY > hit.y + hit.h * 0.55) {
      stroke = { lastX: e.offsetX, dir: 0, turns: [] };
      return;
    }
    const dx = e.offsetX - stroke.lastX;
    if (Math.abs(dx) < 3) return;
    const dir = Math.sign(dx);
    const now = performance.now();
    if (stroke.dir && dir !== stroke.dir) stroke.turns.push(now);
    stroke.dir = dir;
    stroke.lastX = e.offsetX;
    stroke.turns = stroke.turns.filter((t) => now - t < PET_WINDOW);
    if (stroke.turns.length >= PET_TURNS && now > pettedUntil) {
      stroke.turns = [];
      pettedUntil = now + 2500;
      pet.override("petted", 2200, "petted");
      void postJson(conn, `/v1/pets/${encodeURIComponent(petId)}/input`, { kind: "petted" });
    }
  };

  const onClick = () => {
    const now = performance.now();
    if (now < sulkUntil) {
      pet.override("sulk", 1500, "poke");
      return;
    }
    clicks = clicks.filter((t) => now - t < ANNOY_WINDOW);
    clicks.push(now);
    if (clicks.length > ANNOY_CLICKS) {
      clicks = [];
      sulkUntil = now + SULK_MS;
      pet.override("annoyed", 0, "poke", tr("够了！", "Enough!"));
      const side = Math.random() < 0.5 ? -1 : 1;
      void invoke("pet_hop", { vx: side * 420, vy: -520 });
      return;
    }
    pet.poke();
  };

  canvas.addEventListener("mousedown", (e) => {
    if (e.button === 0 && pet.hitTest(e.offsetX, e.offsetY)) down = { x: e.screenX, y: e.screenY };
    void hideCard();
  });
  canvas.addEventListener("mousemove", async (e) => {
    if (down) {
      if (Math.abs(e.screenX - down.x) + Math.abs(e.screenY - down.y) > 3) {
        down = null;
        // The host follows the cursor and throws the pet on release.
        await invoke("drag_start");
      }
      return;
    }
    const hit = pet.hitTest(e.offsetX, e.offsetY);
    onStroke(e, hit);
    const key = hit ? (hit.key ?? "main") : null;
    if (key === hoverKey) return;
    await hideCard();
    hoverKey = key;
    if (hit) hoverTimer = window.setTimeout(() => void showCard(hit), HOVER_MS);
  });
  canvas.addEventListener("mouseleave", () => void hideCard());
  canvas.addEventListener("mouseup", (e) => {
    if (down && pet.hitTest(e.offsetX, e.offsetY)?.kind === "main") onClick();
    down = null;
  });
  // Double-click: jump to the session's terminal or IDE (the one waiting on
  // you first; on a mini, that mini's session).
  canvas.addEventListener("dblclick", async (e) => {
    const hit = pet.hitTest(e.offsetX, e.offsetY);
    const v = pet.currentView();
    if (!hit || !v) return;
    const card = hit.key ? v.sessions.find((s) => s.key === hit.key) : v.sessions[0];
    if (!card) return pet.say(tr("现在没有在线的会话", "No live sessions right now"), 2500);
    const ok = await invoke<boolean>("focus_session", { pid: card.pid });
    if (!ok) pet.say(tr(`找不到 ${card.project || "这个会话"} 的窗口`, `Can't find the window for ${card.project || "this session"}`), 3000);
  });
  canvas.addEventListener("contextmenu", async (e) => {
    e.preventDefault();
    await hideCard();
    await invoke("pet_menu", { pet: petId });
  });

  if (inTauri) {
    // The host turns on click-through when the cursor leaves the pet bodies,
    // which also means no mouseleave reaches us.
    void import("@tauri-apps/api/webviewWindow").then(({ getCurrentWebviewWindow }) =>
      getCurrentWebviewWindow().listen("cursor-left", () => void hideCard()),
    );
  }
}

main().catch((e) => console.error("Familiar window failed", e));
