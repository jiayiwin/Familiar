// Draws one pet window: the main pet, its mini clones (per-camp mode), the
// bubble, badge, camp marker and label. Also owns the switch animation and
// reports which rectangles are clickable (everything else lets clicks through).

import type { MiniView, PetView, State } from "./api";
import type { Sprites } from "./sprites";
import { drawHat, ropeStyle } from "./decor";
import { Fx, type Anchor } from "./fx";
import { t as tr } from "./i18n"; // `t` is a common local name here

export const BUBBLE_H = 48;
const MIN_W = 200;
const FOOT_H = 18;
const GAP = 8;
const MINI_SCALE = 0.5;
const KEGEL_READY = 3000;
const KEGEL_REP = 6000;

interface KegelState { phase: "ready" | "squeeze" | "relax" | "done"; rep: number; left: number; k: number; repT: number }

/** Blend a CSS colour with another (`t` = share of the second). */
function mix(a: string, b: string, t: number): string {
  const rgb = (x: string): number[] => {
    const m = x.match(/\d+/g);
    if (x.startsWith("#")) return [1, 3, 5].map((i) => parseInt(x.slice(i, i + 2), 16));
    return m ? m.slice(0, 3).map(Number) : [240, 163, 94];
  };
  const [p, q] = [rgb(a), rgb(b)];
  return `rgb(${p.map((v, i) => Math.round(v + (q[i] - v) * t)).join(",")})`;
}

/** Where a kegel set is at: phase, rep (1-based), seconds left, clench 0..1. */
function kegelAt(kg: { start: number; reps: number }, now: number): KegelState {
  const t = now - kg.start;
  if (t < KEGEL_READY) return { phase: "ready", rep: 0, left: Math.ceil((KEGEL_READY - t) / 1000), k: 0, repT: 0 };
  const u = t - KEGEL_READY, rep = Math.floor(u / KEGEL_REP) + 1;
  if (rep > kg.reps) return { phase: "done", rep: kg.reps, left: 0, k: 0, repT: 1 };
  const w = (u % KEGEL_REP) / 1000, repT = w / (KEGEL_REP / 1000);
  const ease = (x: number) => { const y = Math.min(1, Math.max(0, x)); return y * y * (3 - 2 * y); };
  if (w < 3) return { phase: "squeeze", rep, left: Math.ceil(3 - w), k: ease(w / 0.5), repT };
  return { phase: "relax", rep, left: Math.ceil(6 - w), k: 1 - ease((w - 3) / 0.6), repT };
}

/** Session state -> pack action. */
const ACTION_FOR: Record<State, string> = {
  idle: "idle",
  thinking: "thinking",
  tool: "tool",
  reading: "reading",
  attention: "attention",
  done: "done",
  error: "error",
  stone: "stone",
  aborted: "aborted",
  sleeping: "sleep",
  offline: "sleep",
};

/** Minis only use four actions (design §6). */
function miniAction(s: State): string {
  if (s === "attention") return "attention";
  if (s === "done") return "done";
  if (s === "tool" || s === "thinking" || s === "reading") return "tool";
  return "idle";
}

const DEFAULT_BUBBLES: Record<string, string[]> = {
  get attention() { return [tr("轮到你啦！", "Your turn!")]; },
  get done() { return [tr("搞定！", "Done!")]; },
  get error() { return [tr("出错了…", "Oops, error…")]; },
  get stone() { return [tr("石化了…", "Turned to stone…")]; },
};

/** How long a bubble stays up; 0 = until the state changes. */
const BUBBLE_MS: Record<string, number> = { attention: 0, stone: 0, done: 4000, error: 4000 };
const WAKE_MS = 800;
const WAVE_MS = 600;
const SMOKE_MS = 400;
const MINI_LEAVE_WAVE = 600;
const MINI_LEAVE_MOVE = 400;
const SOUNDS_PER_MIN = 4;

export interface HitRect {
  x: number;
  y: number;
  w: number;
  h: number;
  kind: "main" | "mini";
  key?: string;
}

interface Mini {
  view: MiniView;
  action: string;
  start: number;
  x: number;
  alpha: number;
  /** performance.now() when it started leaving, 0 while present. */
  leaving: number;
}

/** A two-pet game as one pet sees it. `start`: performance.now() of the first shot. */
export interface Duo {
  game: string;
  side: "left" | "right";
  start: number;
  ms: number;
  each: number;
  hits: number;
  /** I play the first shot. */
  serve: boolean;
  winner: boolean;
  seed: number;
}

export class PetRenderer {
  sprites: Sprites | null = null;
  private view: PetView | null = null;
  private action = "idle";
  private actionStart = 0;
  private wakeUntil = 0;
  private bubbleText = "";
  private bubbleUntil = 0;
  private soundTimes: number[] = [];
  /** Temporary action on top of the state's own (scripts, physics, petting). */
  private over: { action: string; until: number; kind: string } | null = null;
  private muted = false;
  private rope: string | null = null;
  /** 4-7-8 breathing: until when. */
  private breathe: { start: number; until: number } | null = null;
  /** Guided kegel set: 3 s to get ready, then squeeze 3 s / relax 3 s per rep. */
  private kegel: { start: number; reps: number; last: string; fur: string } | null = null;
  /** Effects around the pet (hands, fish, chips, confetti…). */
  readonly fx = new Fx();
  /** Tug of war: which side of the rope we stand on, whether we lead, until when. */
  private tug: { side: "left" | "right"; lead: boolean | null; start: number; until: number } | null = null;
  private duo: Duo | null = null;
  private focus = false;
  private lastKey = "";
  private minis = new Map<string, Mini>();
  private switching: { next: Sprites; phase: "wave" | "smoke"; t0: number } | null = null;
  private lastFrame = 0;
  private lastHits = "";
  private lastSize = "";

  /** Called when the logical window size changes. */
  onResize: (w: number, h: number) => void = () => {};
  /** Called when the clickable rectangles change (logical px). */
  onHits: (rects: HitRect[]) => void = () => {};

  constructor(private ctx: CanvasRenderingContext2D) {}

  /** Pack scale times the user's size setting. */
  private sc(sp: Sprites): number {
    return sp.pack.frame.scale * (this.view?.size ?? 1);
  }

  /** Logical window size for the current pack and mini count. */
  size(): { w: number; h: number } {
    const sp = this.switching?.phase === "wave" ? this.sprites : (this.switching?.next ?? this.sprites);
    if (!sp) return { w: MIN_W, h: 176 };
    const f = sp.pack.frame;
    const sc = this.sc(sp);
    const mainW = f.w * sc;
    const miniW = f.w * sc * MINI_SCALE;
    const n = this.minis.size + (this.view?.minis_more ? 1 : 0);
    const side = Math.ceil(n / 2);
    return {
      w: Math.ceil(Math.max(MIN_W, mainW + 2 * side * (miniW + GAP) + 16)),
      h: Math.ceil(BUBBLE_H + f.h * sc + FOOT_H),
    };
  }

  /** Swap to another pack. Animated unless the pet is waiting on the user. */
  switchTo(next: Sprites, animate: boolean): void {
    if (!this.sprites || !animate || this.view?.state === "attention") {
      this.sprites = next;
      this.switching = null;
      this.lastKey = "";
      this.play(this.target(), false);
      this.checkSize();
      return;
    }
    this.switching = { next, phase: "wave", t0: performance.now() };
    this.play(this.sprites.action("wave") ? "wave" : "idle", false);
    // rAF pauses when the window is occluded or the screen is locked; make
    // sure the switch still completes.
    const sw = this.switching;
    setTimeout(() => {
      if (this.switching === sw) this.finishSwitch();
    }, WAVE_MS + SMOKE_MS + 200);
  }

  setView(v: PetView): void {
    const prev = this.view;
    this.view = v;
    this.syncMinis(v);
    this.checkSize();
    if (!this.sprites) return;
    // Attention never waits for an animation, and ends any gameplay action.
    if (v.state === "attention" && this.switching) this.finishSwitch();
    if (v.state === "attention" && this.over && this.over.kind !== "phys") this.over = null;
    if (prev && prev.size !== v.size) this.lastSize = "";
    this.checkSize();
    if (prev && prev.state === v.state && prev.since === v.since) {
      // Casting or packing up the fishing rod.
      if (prev.activity !== v.activity && !this.over && !this.wakeUntil) this.play(this.target(), false);
      return;
    }

    const now = performance.now();
    const waking = prev?.state === "sleeping" && v.state !== "sleeping" && v.state !== "offline";
    if (this.switching) {
      // The new state starts when the switch animation ends.
    } else if (waking && v.state !== "attention") {
      this.wakeUntil = now + WAKE_MS;
      this.play("wake", false);
    } else {
      this.wakeUntil = 0;
      this.play(this.target(), true);
    }

    const key = v.bubble?.key;
    if (key) {
      const lines = this.sprites.pack.bubbles[key] ?? DEFAULT_BUBBLES[key] ?? [];
      const text = lines[Math.floor(Math.random() * lines.length)] ?? "";
      this.bubbleText = (v.bubble?.project ? `[${v.bubble.project}] ` : "") + text;
      const ms = BUBBLE_MS[key] ?? 4000;
      this.bubbleUntil = ms === 0 ? Infinity : now + ms;
    } else {
      this.bubbleText = "";
    }
  }

  /** User poked the pet: one-shot squish (random sound), then back to what it was doing. */
  poke(): void {
    this.breathe = null; // a click skips the breathing exercise
    if (this.kegel) {
      this.kegel = null;
      this.say(tr("好，下次再练～", "OK, next time~"), 2500);
      return;
    }
    if (this.switching || !this.sprites?.action("squish") || this.view?.state === "attention") return;
    this.override("squish", 0, "poke");
    const set = this.sprites.pack.sounds?.squish ?? [];
    const pick = set[Math.floor(Math.random() * set.length)];
    if (pick) this.sound(pick);
  }

  /**
   * Play a temporary action. `ms` 0 means "one run" for one-shot actions.
   * Attention always wins over gameplay; only physics may override it.
   */
  override(action: string, ms: number, kind: string, bubble?: string | null): void {
    if (!this.sprites || this.switching) return;
    if (action === "kegel") {
      // Not a sprite: the body clenches and lets go to a coach overlay.
      this.breathe = null;
      this.kegel = { start: performance.now(), reps: Math.max(1, Math.round(((ms || 63_000) - KEGEL_READY) / KEGEL_REP)), last: "", fur: this.furColor() };
      this.bubbleText = "";
      return;
    }
    if (action === "breathe") {
      // Not a sprite: the body swells and settles on a 4-7-8 rhythm.
      const now = performance.now();
      this.breathe = { start: now, until: now + (ms || 30_000) };
      if (bubble) this.say(bubble, 6000);
      return;
    }
    if (this.view?.state === "attention" && kind !== "phys") return;
    const a = this.sprites.action(action);
    const now = performance.now();
    let dur = ms;
    if (a && !a.loop && !ms) dur = (a.frames / a.fps) * 1000;
    this.over = { action, until: dur ? now + dur : Infinity, kind };
    this.wakeUntil = 0;
    this.play(this.target(), kind === "script" || kind === "petted");
    if (bubble) this.say(bubble, Math.max(2500, ms || 3000));
  }

  clearOverride(kind?: string): void {
    if (!this.over || (kind && this.over.kind !== kind)) return;
    this.over = null;
    this.play(this.target(), false);
  }

  overrideKind(): string | null {
    return this.over && performance.now() < this.over.until ? this.over.kind : null;
  }

  say(text: string, ms: number): void {
    if (this.view?.state === "attention") return;
    this.bubbleText = text;
    this.bubbleUntil = performance.now() + ms;
    this.lastKey = "";
  }

  setMuted(on: boolean): void {
    this.muted = on;
  }

  currentView(): PetView | null {
    return this.view;
  }

  /** Lean back on a rope that runs to the window edge facing the other pet. */
  startTug(side: "left" | "right", lead: boolean | null, ms: number): void {
    if (this.view?.state === "attention") return;
    const now = performance.now();
    this.tug = { side, lead, start: now, until: now + ms };
    this.lastKey = "";
  }

  stopTug(): void {
    this.tug = null;
    this.duo = null;
    this.lastKey = "";
  }

  /** A two-pet game (badminton, ping-pong, arm wrestling) on a shared timeline. */
  startDuo(d: Duo): void {
    this.duo = d;
    this.lastKey = "";
  }

  setRope(rope: string | null | undefined): void {
    this.rope = rope ?? null;
  }

  setFocus(on: boolean): void {
    this.focus = on;
  }

  /** A catch reeled in: fish, boot or an escape. */
  reel(items: { outcome: string; name: string; rarity: string; coins: number }[], coins: number): void {
    if (!items.length) return;
    const fish = items.filter((i) => i.outcome === "fish");
    let text: string;
    if (items.length > 1) {
      text = tr(`收获 ${fish.length} 条鱼${coins ? `，+${coins} 金币` : ""}`, `Caught ${fish.length} fish${coins ? `, +${coins} coins` : ""}`);
    } else if (items[0].outcome === "fish") {
      const tag = { rare: tr("稀有！", "Rare! "), epic: tr("史诗！！", "Epic!! "), legendary: tr("传说！！！", "Legendary!!! ") }[items[0].rarity] ?? "";
      text = tr(`${tag}钓到 ${items[0].name}${coins ? ` +${coins}` : ""}`, `${tag}Got ${items[0].name}${coins ? ` +${coins}` : ""}`);
    } else if (items[0].outcome === "boot") {
      text = tr("钓到一只破靴子…", "Reeled in an old boot…");
    } else {
      text = tr("鱼跑了…", "It got away…");
    }
    if (fish.length && this.sprites?.action("fish_reel")) this.override("fish_reel", 0, "script", text);
    else this.say(text, 3500);
  }

  private syncMinis(v: PetView): void {
    const now = performance.now();
    const keep = new Set(v.minis.map((m) => m.key));
    const centre = this.size().w / 2;
    for (const m of v.minis) {
      const cur = this.minis.get(m.key);
      if (!cur || cur.leaving) {
        // Peeks out from behind the main pet.
        this.minis.set(m.key, { view: m, action: miniAction(m.state), start: now, x: cur?.x ?? centre, alpha: cur?.alpha ?? 0, leaving: 0 });
      } else {
        if (cur.view.state !== m.state || cur.view.since !== m.since) {
          cur.action = miniAction(m.state);
          cur.start = now;
        }
        cur.view = m;
      }
    }
    for (const [k, m] of this.minis) {
      if (!keep.has(k) && !m.leaving) {
        m.leaving = now;
        m.action = this.sprites?.action("wave") ? "wave" : "idle";
        m.start = now;
      }
    }
  }

  private finishSwitch(): void {
    if (!this.switching) return;
    this.sprites = this.switching.next;
    this.switching = null;
    this.lastKey = "";
    this.wakeUntil = 0;
    this.play(this.target(), true);
    this.checkSize();
  }

  private checkSize(): void {
    const { w, h } = this.size();
    const k = `${w}x${h}`;
    if (k !== this.lastSize) {
      this.lastSize = k;
      this.lastKey = "";
      this.onResize(w, h);
    }
  }

  private target(): string {
    const v = this.view;
    if (v?.state === "attention" && this.over?.kind !== "phys") return "attention";
    if (this.over && performance.now() < this.over.until && this.sprites?.action(this.over.action)) return this.over.action;
    const working = v?.state === "thinking" || v?.state === "tool" || v?.state === "reading";
    if (working && v?.activity === "fishing" && this.sprites?.action("fish")) return "fish";
    const calm = v?.state === "idle" || v?.state === "done";
    if (calm && v?.activity === "pack_bag" && this.sprites?.action("pack_bag")) return "pack_bag";
    if (calm && v?.activity === "dance" && this.sprites?.action("cheer")) return "cheer";
    return ACTION_FOR[v?.state ?? "sleeping"];
  }

  private play(name: string, withSound: boolean): void {
    const a = this.sprites?.action(name);
    if (!a) return;
    this.action = name;
    this.actionStart = performance.now();
    if (withSound && a.sound) this.sound(a.sound, name === "attention");
  }

  /** Global limit: 4 sounds a minute. Boss key mutes all; focus mode keeps only the attention chime. */
  private sound(path: string, important = false): void {
    if (this.muted || (this.focus && !important)) return;
    const now = Date.now();
    this.soundTimes = this.soundTimes.filter((t) => now - t < 60_000);
    if (this.soundTimes.length >= SOUNDS_PER_MIN || !this.sprites) return;
    this.soundTimes.push(now);
    const audio = new Audio(this.sprites.url(path));
    audio.volume = 0.6;
    audio.play().catch(() => {});
  }

  /** Advance animation and redraw if anything visible changed. */
  update(now: number): void {
    const dt = Math.min(0.1, (now - (this.lastFrame || now)) / 1000);
    this.lastFrame = now;
    const sp = this.sprites;
    if (!sp) return;

    // Switch timeline: wave on the old pet, a puff of smoke, then the new pet.
    if (this.switching) {
      const t = now - this.switching.t0;
      if (this.switching.phase === "wave" && t >= WAVE_MS) {
        this.switching.phase = "smoke";
        this.switching.t0 = now;
        this.checkSize();
      } else if (this.switching.phase === "smoke" && t >= SMOKE_MS) {
        this.finishSwitch();
      }
    }
    const cur = this.sprites!;

    if (this.over && now >= this.over.until) {
      this.over = null;
      this.play(this.target(), false);
    }
    if (this.wakeUntil && now >= this.wakeUntil) {
      this.wakeUntil = 0;
      this.play(this.target(), true);
    }
    let a = cur.action(this.action) ?? cur.action("idle")!;
    let { idx, finished } = cur.frameAt(a, now - this.actionStart);
    if (finished && !this.wakeUntil && !this.switching) {
      // Back to the state's own action if it is a held one, else follow `then`.
      const t = this.target();
      const next = t !== this.action && cur.action(t)?.loop ? t : a.then;
      if (next && next !== this.action) {
        this.play(next, false);
        a = cur.action(this.action)!;
        idx = 0;
      }
    }
    if (this.bubbleText && now > this.bubbleUntil) this.bubbleText = "";

    // Minis: ease toward their slots; leaving ones wave, then slip back inside.
    const { w } = this.size();
    const slots = this.slotXs(w, cur);
    let moving = false;
    for (const [k, m] of this.minis) {
      let tx = slots.get(k) ?? w / 2;
      let ta = 1;
      if (m.leaving) {
        const lt = now - m.leaving;
        if (lt > MINI_LEAVE_WAVE) {
          tx = w / 2;
          ta = 0;
        }
        if (lt > MINI_LEAVE_WAVE + MINI_LEAVE_MOVE) {
          this.minis.delete(k);
          this.checkSize();
          continue;
        }
      }
      const k2 = Math.min(1, dt * 10);
      m.x += (tx - m.x) * k2;
      m.alpha += (ta - m.alpha) * k2;
      if (Math.abs(tx - m.x) > 0.5 || Math.abs(ta - m.alpha) > 0.02) moving = true;
      const ma = cur.action(m.action) ?? cur.action("idle")!;
      const fr = cur.frameAt(ma, now - m.start);
      // One-shots (done) hand over to `then`; held actions loop forever.
      if (fr.finished && !m.leaving) {
        m.action = ma.then ?? "idle";
        m.start = now;
      }
    }

    const v = this.view;
    const miniKey = [...this.minis.values()]
      .map((m) => `${m.view.key}:${m.action}:${cur.frameAt(cur.action(m.action) ?? cur.action("idle")!, now - m.start).idx}:${Math.round(m.x)}:${m.alpha.toFixed(2)}`)
      .join(",");
    const smoke = this.switching?.phase === "smoke" ? Math.floor((now - this.switching.t0) / 50) : -1;
    if (this.tug && (now >= this.tug.until || v?.state === "attention")) this.tug = null;
    if (this.duo && now >= this.duo.start + this.duo.ms + 400) this.duo = null;
    const tugKey = this.tug || this.duo ? Math.floor(now / 33) : -1;
    // Crown sparkle and halo bob animate on their own.
    const hatKey = v?.hat === "crown" || v?.hat === "halo" || v?.stage ? Math.floor(now / 100) : v?.hat ?? "";
    if (this.breathe && now >= this.breathe.until) this.breathe = null;
    if (this.kegel) this.kegelTick(now);
    const fxKey = this.fx.busy(now) || this.breathe || this.kegel ? Math.floor(now / 33) : -1;
    const key = `${fxKey}|${tugKey}|${hatKey}|${this.action}|${idx}|${this.bubbleText}|${v?.online}|${v?.waiting}|${v?.hue}|${v?.marker}|${v?.label}|${miniKey}|${v?.minis_more}|${smoke}|${moving}`;
    if (key === this.lastKey) return;
    this.lastKey = key;
    this.draw(a, idx, now);
  }

  /** Mini slot centres: alternating right/left of the main pet, waiting ones closest. */
  private slotXs(w: number, sp: Sprites): Map<string, number> {
    const f = sp.pack.frame;
    const mainW = f.w * this.sc(sp);
    const miniW = f.w * this.sc(sp) * MINI_SCALE;
    const out = new Map<string, number>();
    const order = (this.view?.minis ?? []).map((m) => m.key);
    order.forEach((k, i) => out.set(k, slotX(i, w, mainW, miniW)));
    if (this.view?.minis_more) out.set("+more", slotX(order.length, w, mainW, miniW));
    return out;
  }

  private draw(a: ReturnType<Sprites["action"]> & object, idx: number, now: number): void {
    const sp = this.sprites!;
    const { w, h } = this.size();
    const c = this.ctx;
    c.clearRect(0, 0, w, h);
    const f = sp.pack.frame;
    const hue = this.view?.hue ?? 0;
    const bottom = h - FOOT_H;
    const mainW = f.w * this.sc(sp);
    const mainH = f.h * this.sc(sp);
    const miniS = this.sc(sp) * MINI_SCALE;
    const hits: HitRect[] = [];

    // Minis behind the main pet.
    for (const m of this.minis.values()) {
      const ma = sp.action(m.action) ?? sp.action("idle")!;
      const fr = sp.frameAt(ma, now - m.start);
      sp.draw(c, ma, fr.idx, m.x, bottom, miniS, hue, Math.max(0, Math.min(1, m.alpha)));
      const mw = f.w * miniS;
      const mh = f.h * miniS;
      if (!m.leaving) hits.push({ x: m.x - mw / 2, y: bottom - mh, w: mw, h: mh, kind: "mini", key: m.view.key });
      if (m.view.state === "attention" && !m.leaving) {
        // Packs usually draw their own "!"; add one only for packs that fall back.
        if (ma.from !== "attention") {
          c.fillStyle = "#ee5b3c";
          c.fillRect(Math.round(m.x + mw / 4), bottom - mh - 2, 3, 8);
          c.fillRect(Math.round(m.x + mw / 4), bottom - mh + 8, 3, 3);
        }
        this.footText(m.view.project, m.x, bottom, mw + GAP, "#ee5b3c", true);
      }
    }
    if (this.view?.minis_more) {
      const x = this.slotXs(w, sp).get("+more")!;
      c.fillStyle = "rgba(43,29,22,0.75)";
      c.beginPath();
      c.roundRect(x - 14, bottom - 22, 28, 18, 9);
      c.fill();
      c.fillStyle = "#fff";
      c.font = "bold 11px system-ui, sans-serif";
      c.textAlign = "center";
      c.textBaseline = "middle";
      c.fillText(`+${this.view.minis_more}`, x, bottom - 12.5);
    }

    // Main pet (or the smoke puff mid-switch).
    const cx = w / 2;
    if (this.switching?.phase === "smoke") {
      this.drawSmoke(cx, bottom - mainH / 2, mainW, (now - this.switching.t0) / SMOKE_MS);
    } else if (this.tug) {
      this.drawTug(a, idx, cx, bottom, mainW, mainH, w, now);
    } else if (this.duo && now >= this.duo.start - 100) {
      this.drawDuo(a, idx, cx, bottom, mainW, mainH, w, now);
    } else {
      if (this.view?.stage) this.drawAura(cx, bottom, mainW, mainH, this.view.stage, now);
      if (this.kegel) {
        // Turn around (the sprite folds away, the back view unfolds), then exercise.
        const turn = Math.min(1, (now - this.kegel.start) / 500);
        if (turn < 0.5) {
          c.save();
          c.translate(cx, bottom);
          c.scale(1 - turn * 2, 1);
          c.translate(-cx, -bottom);
          sp.draw(c, a, idx, cx, bottom, this.sc(sp), hue);
          c.restore();
        } else {
          this.drawRear(cx, bottom, mainW, mainH, kegelAt(this.kegel, now), now, (turn - 0.5) * 2, this.kegel.fur);
        }
      } else {
        sp.draw(c, a, idx, cx, bottom, this.sc(sp) * this.breath(now), hue);
        if (this.view?.hat) this.drawHatOn(a, idx, cx, bottom, now);
      }
    }
    hits.push({ x: cx - mainW / 2, y: bottom - mainH, w: mainW, h: mainH, kind: "main" });

    // Effects on top of the pet, under the bubble.
    this.fx.draw(c, this.anchor(a, idx, cx, bottom, w), now);
    if (this.kegel) this.drawKegel(kegelAt(this.kegel, now), cx, w, bottom, mainW, mainH, now);
    else if (this.bubbleText) this.drawBubble(this.bubbleText, w, bottom - mainH);
    // Badge at the main pet's top-left: clear of the minis and the pack's own "!".
    this.drawBadge(cx - mainW / 2, bottom - mainH + 4);
    // Camp tag (and project / rule label) under the feet.
    if (this.view?.marker) this.drawMarker(this.view.marker, this.view.label, cx, bottom);
    else if (this.view?.label) this.footText(this.view.label, cx, bottom, mainW, "#1f2245", false);

    const hk = JSON.stringify(hits.map((r) => [Math.round(r.x), Math.round(r.y), Math.round(r.w), Math.round(r.h), r.key ?? ""]));
    if (hk !== this.lastHits) {
      this.lastHits = hk;
      this.onHits(hits);
    }
  }

  /** Phase changes: a soft beep, and the closing cheer. */
  private kegelTick(now: number): void {
    const kg = this.kegel!;
    const st = kegelAt(kg, now);
    const mark = `${st.phase}${st.rep}`;
    if (mark === kg.last) return;
    kg.last = mark;
    if (st.phase === "squeeze") this.beep(660);
    else if (st.phase === "relax") this.beep(440);
    else if (st.phase === "done") {
      this.kegel = null;
      this.beep(880);
      this.override("cheer", 2600, "script", tr(`完成 ${kg.reps} 组！今天也很棒 💪`, `${kg.reps} reps done! Nice work 💪`));
    }
  }

  /** The pet's main fur colour, from its idle frame (warm orange if unreadable). */
  private furColor(): string {
    const fallback = "#f0a35e";
    try {
      const sp = this.sprites!, a = sp.action("idle")!, f = sp.pack.frame;
      const cv = document.createElement("canvas");
      cv.width = f.w; cv.height = f.h;
      const g = cv.getContext("2d", { willReadFrequently: true })!;
      sp.draw(g, a, 0, f.w / 2, f.h, 1, this.view?.hue ?? 0);
      const d = g.getImageData(0, 0, f.w, f.h).data;
      const bins = new Map<number, [number, number, number, number]>();
      for (let i = 0; i < d.length; i += 4) {
        if (d[i + 3] < 200) continue;
        const [r, gg, b] = [d[i], d[i + 1], d[i + 2]];
        const mx = Math.max(r, gg, b), mn = Math.min(r, gg, b);
        if (mx < 70 || mn > 235) continue; // outlines and highlights
        const key = ((r >> 5) << 6) | ((gg >> 5) << 3) | (b >> 5);
        const e = bins.get(key) ?? [0, 0, 0, 0];
        e[0] += r; e[1] += gg; e[2] += b; e[3]++;
        bins.set(key, e);
      }
      let best: [number, number, number, number] | null = null;
      for (const e of bins.values()) if (!best || e[3] > best[3]) best = e;
      if (!best || best[3] < 20) return fallback;
      return `rgb(${Math.round(best[0] / best[3])},${Math.round(best[1] / best[3])},${Math.round(best[2] / best[3])})`;
    } catch {
      return fallback; // cross-origin frames can't be read
    }
  }

  /**
   * The pet seen from behind, doing kegels: cheeks squeeze, the little
   * pucker tightens and lifts, the tail goes stiff; on relax it all lets go.
   */
  private drawRear(cx: number, bottom: number, mainW: number, mainH: number, st: KegelState, now: number, open: number, fur: string): void {
    const c = this.ctx;
    const H = mainH * 0.9, W = Math.min(mainW * 0.95, H);
    const k = st.k;
    const ink = "#4a2c20", dark = mix(fur, "#000000", 0.3), light = mix(fur, "#ffffff", 0.45);
    const pink = "#f0a0ae", deep = "#b8465f";
    const TAU = Math.PI * 2;
    const lw = Math.max(1.5, H / 55);
    c.save();
    c.translate(cx, bottom);
    c.scale(open * (1 + 0.05 * k), 1 - 0.06 * k);
    c.lineJoin = "round";
    c.lineCap = "round";
    c.lineWidth = lw;
    c.strokeStyle = ink;
    const blob = (draw: () => void, fill: string) => { c.beginPath(); draw(); c.fillStyle = fill; c.fill(); c.stroke(); };
    const wag = st.phase === "squeeze" ? 0 : Math.sin(now / 240);

    // Body (the back), with tabby stripes down the spine.
    const by = -H * 0.38;
    blob(() => c.ellipse(0, by, W * 0.36, H * 0.33, 0, 0, TAU), fur);
    c.strokeStyle = dark;
    for (let i = 0; i < 3; i++) {
      const y = by - H * 0.2 + i * H * 0.09, hw = W * (0.12 - i * 0.015);
      c.beginPath();
      c.moveTo(-hw, y);
      c.quadraticCurveTo(0, y + H * 0.04, hw, y);
      c.stroke();
    }
    c.strokeStyle = ink;

    // Head from behind: ears, round back of the head.
    const hr = H * 0.2, hy = -H * 0.79;
    for (const sd of [-1, 1]) {
      blob(() => { c.moveTo(sd * hr * 0.2, hy - hr * 0.75); c.lineTo(sd * hr * 0.95, hy - hr * 1.4); c.lineTo(sd * hr * 1.0, hy - hr * 0.15); c.closePath(); }, fur);
    }
    blob(() => c.arc(0, hy, hr, 0, TAU), fur);
    c.strokeStyle = dark;
    for (const dx of [-0.35, 0, 0.35]) {
      c.beginPath();
      c.moveTo(dx * hr, hy - hr * 0.9);
      c.lineTo(dx * hr * 0.8, hy - hr * 0.45);
      c.stroke();
    }
    c.strokeStyle = ink;

    // Hind paws, soles to us: pink beans.
    for (const sd of [-1, 1]) {
      const px = sd * W * 0.21, py = -H * 0.05;
      blob(() => c.ellipse(px, py, W * 0.11, H * 0.055, 0, 0, TAU), fur);
      c.fillStyle = pink;
      c.beginPath();
      c.ellipse(px, py + H * 0.012, W * 0.04, H * 0.022, 0, 0, TAU);
      c.fill();
      for (const t of [-1, 0, 1]) {
        c.beginPath();
        c.arc(px + t * W * 0.042, py - H * 0.026, W * 0.014, 0, TAU);
        c.fill();
      }
    }

    // Cheeks: they press together while squeezing.
    const cy = -H * 0.22, cr = W * 0.19, sep = W * 0.15 - k * W * 0.035;
    for (const sd of [-1, 1]) {
      c.beginPath();
      c.arc(sd * sep, cy, cr, 0, TAU);
      c.fillStyle = fur;
      c.fill();
      const hl = c.createRadialGradient(sd * (sep + cr * 0.25), cy - cr * 0.35, 0, sd * (sep + cr * 0.25), cy - cr * 0.35, cr * 0.7);
      hl.addColorStop(0, light);
      hl.addColorStop(1, "rgba(255,255,255,0)");
      c.fillStyle = hl;
      c.fill();
    }
    for (const sd of [-1, 1]) {
      c.beginPath();
      if (sd > 0) c.arc(sep, cy, cr, -0.3 * Math.PI, 0.82 * Math.PI);
      else c.arc(-sep, cy, cr, 0.18 * Math.PI, 1.3 * Math.PI);
      c.stroke();
    }
    // The crease.
    c.beginPath();
    c.moveTo(0, cy - cr * 0.95);
    c.quadraticCurveTo(lw * 0.6 * (1 - k), cy, 0, cy + cr * 0.85);
    c.stroke();

    // Tail: limp and wagging when relaxed, straight up when squeezing.
    const tx = 0, ty = cy - cr * 0.9;
    const ex = (1 - k) * (W * 0.36 + wag * W * 0.06), ey = ty - H * (0.32 + 0.2 * k);
    const qx = (1 - k) * (W * 0.3 + wag * W * 0.04), qy = ty - H * (0.06 + 0.18 * k);
    c.lineWidth = W * 0.1;
    c.beginPath();
    c.moveTo(tx, ty);
    c.quadraticCurveTo(qx, qy, ex, ey);
    c.stroke();
    c.strokeStyle = fur;
    c.lineWidth = W * 0.1 - lw * 2;
    c.stroke();
    c.strokeStyle = dark;
    c.lineWidth = W * 0.1 - lw * 2;
    c.beginPath();
    c.moveTo(ex + (qx - ex) * 0.18, ey + (qy - ey) * 0.18);
    c.lineTo(ex, ey);
    c.stroke();
    c.strokeStyle = ink;
    c.lineWidth = lw;

    // The pucker: a soft pink dot that tightens into a little star and lifts.
    const px = 0, py = cy - k * H * 0.03, pr = H * 0.045 * (1 - 0.5 * k);
    c.fillStyle = k > 0.5 ? deep : pink;
    c.beginPath();
    c.arc(px, py, pr, 0, TAU);
    c.fill();
    c.strokeStyle = deep;
    c.lineWidth = Math.max(1, lw * 0.7);
    for (let i = 0; i < 6; i++) {
      const a = (i / 6) * TAU + Math.PI / 6;
      c.beginPath();
      c.moveTo(px + Math.cos(a) * pr * 0.7, py + Math.sin(a) * pr * 0.7);
      c.lineTo(px + Math.cos(a) * pr * (1.5 + 0.9 * k), py + Math.sin(a) * pr * (1.5 + 0.9 * k));
      c.stroke();
    }

    // Effort: squeeze lines by the cheeks and a sweat drop.
    if (k > 0.4) {
      c.globalAlpha = Math.min(1, (k - 0.4) * 2.5);
      c.strokeStyle = ink;
      c.lineWidth = lw;
      for (const sd of [-1, 1]) {
        for (let i = 0; i < 3; i++) {
          const x0 = sd * (sep + cr + W * 0.05), y0 = cy - cr * 0.5 + i * cr * 0.45;
          c.beginPath();
          c.moveTo(x0, y0);
          c.lineTo(x0 + sd * W * 0.06, y0 - H * 0.01);
          c.stroke();
        }
      }
      c.fillStyle = "#7cc7f2";
      const dx = hr * 1.05, dy = hy - hr * 0.2 + ((now / 30) % 10) * 0.4;
      c.beginPath();
      c.moveTo(dx, dy - H * 0.04);
      c.quadraticCurveTo(dx + H * 0.025, dy, dx, dy + H * 0.015);
      c.quadraticCurveTo(dx - H * 0.025, dy, dx, dy - H * 0.04);
      c.fill();
      c.globalAlpha = 1;
    }
    c.restore();
  }

  private audio: AudioContext | null = null;
  /** A short soft tone, so the set can be followed without looking. */
  private beep(freq: number): void {
    if (this.muted) return;
    try {
      this.audio ??= new AudioContext();
      const a = this.audio, o = a.createOscillator(), g = a.createGain();
      o.frequency.value = freq;
      g.gain.setValueAtTime(0.0001, a.currentTime);
      g.gain.exponentialRampToValueAtTime(0.12, a.currentTime + 0.02);
      g.gain.exponentialRampToValueAtTime(0.0001, a.currentTime + 0.25);
      o.connect(g).connect(a.destination);
      o.start();
      o.stop(a.currentTime + 0.3);
    } catch {
      /* no audio: the overlay is enough */
    }
  }

  /** Coach overlay above the pet: what to do, a countdown, and progress dots. */
  private drawKegel(st: KegelState, cx: number, w: number, bottom: number, mainW: number, mainH: number, now: number): void {
    const c = this.ctx;
    const reps = this.kegel?.reps ?? 0;
    const [label, color] =
      st.phase === "ready" ? [tr(`提肛时间！准备 ${st.left}`, `Kegel time! Ready ${st.left}`), "#2b3275"]
      : st.phase === "squeeze" ? [tr(`收紧 ⬆ ${st.left}`, `Squeeze ⬆ ${st.left}`), "#ee5b3c"]
      : [tr(`放松 ⬇ ${st.left}`, `Relax ⬇ ${st.left}`), "#2f9a8a"];
    const count = st.phase === "ready" ? tr(`${reps} 组`, `${reps} reps`) : `${st.rep}/${reps}`;
    const big = "13px 'Segoe UI', 'Microsoft YaHei UI', 'PingFang SC', system-ui, sans-serif";
    const small = "11px 'Segoe UI', 'Microsoft YaHei UI', 'PingFang SC', system-ui, sans-serif";
    c.font = big;
    const lw = c.measureText(label).width;
    c.font = small;
    const cw = c.measureText(count).width;
    const pw = Math.min(w - 8, lw + cw + 30), ph = 24;
    const petTop = bottom - mainH;
    const px = Math.round((w - pw) / 2), py = Math.max(2, Math.min(BUBBLE_H - ph - 12, petTop - ph - 12));
    c.fillStyle = color;
    c.beginPath();
    c.roundRect(px, py, pw, ph, ph / 2);
    c.fill();
    c.fillStyle = "#fff";
    c.textBaseline = "middle";
    c.textAlign = "left";
    c.font = big;
    c.fillText(label, px + 12, py + ph / 2 + 1);
    c.textAlign = "right";
    c.font = small;
    c.globalAlpha = 0.9;
    c.fillText(count, px + pw - 10, py + ph / 2 + 1);
    c.globalAlpha = 1;
    // Progress: one dot per rep; a ring fills around the current one.
    const dots = Math.min(reps, 20), gap = Math.min(10, (pw - 16) / Math.max(1, dots));
    const dy = py + ph + 6, dx0 = w / 2 - (gap * (dots - 1)) / 2;
    for (let i = 0; i < dots; i++) {
      const done = st.phase !== "ready" && i < st.rep - 1;
      const cur = st.phase !== "ready" && i === st.rep - 1;
      c.fillStyle = done ? "#2b3275" : "rgba(31,34,69,0.18)";
      c.beginPath();
      c.arc(dx0 + i * gap, dy, cur ? 3.2 : 2.4, 0, Math.PI * 2);
      c.fill();
      if (cur) {
        c.strokeStyle = color;
        c.lineWidth = 2;
        c.beginPath();
        c.arc(dx0 + i * gap, dy, 4.5, -Math.PI / 2, -Math.PI / 2 + Math.PI * 2 * st.repT);
        c.stroke();
      }
    }
    // Arrows beside the body: rising while squeezing, sinking while relaxing.
    if (st.phase === "squeeze" || st.phase === "relax") {
      const up = st.phase === "squeeze";
      const t = ((now - this.kegel!.start) / 900) % 1;
      c.strokeStyle = color;
      c.lineWidth = 2.5;
      c.lineCap = "round";
      for (const side of [-1, 1]) {
        const x = cx + side * (mainW / 2 + 6);
        for (let j = 0; j < 2; j++) {
          const f = (t + j * 0.5) % 1;
          const y = up ? bottom - 8 - f * mainH * 0.45 : bottom - 8 - mainH * 0.45 + f * mainH * 0.45;
          c.globalAlpha = Math.sin(Math.PI * f);
          c.beginPath();
          c.moveTo(x - 5, y + (up ? 4 : -4));
          c.lineTo(x, y);
          c.lineTo(x + 5, y + (up ? 4 : -4));
          c.stroke();
        }
      }
      c.globalAlpha = 1;
      c.lineCap = "butt";
    }
  }

  /** Body scale for the breathing exercise: in 4 s, hold 7 s, out 8 s. */
  private breath(now: number): number {
    if (!this.breathe) return 1;
    const t = ((now - this.breathe.start) / 1000) % 19;
    const k = t < 4 ? t / 4 : t < 11 ? 1 : 1 - (t - 11) / 8;
    return 1 + 0.07 * (0.5 - 0.5 * Math.cos(Math.PI * k));
  }

  /** Where effects attach: the head of the current frame. */
  private anchor(a: NonNullable<ReturnType<Sprites["action"]>>, idx: number, cx: number, bottom: number, w: number): Anchor {
    const sp = this.sprites!;
    const f = sp.pack.frame;
    const sc = this.sc(sp);
    const h = sp.head(a, idx);
    return { cx: cx + (h.x - f.w / 2) * sc, bottom, top: bottom - (f.h - h.top) * sc, headW: h.w * sc, w };
  }

  /** Evolution aura behind the pet: blue, purple, gold for stages 1–3. */
  private drawAura(cx: number, bottom: number, w: number, h: number, stage: number, now: number): void {
    const c = this.ctx;
    const col = ["", "155,231,255", "195,155,255", "255,209,102"][Math.min(3, stage)];
    const pulse = 0.75 + 0.25 * Math.sin(now / 600);
    const r = Math.max(w, h) * 0.55;
    const g = c.createRadialGradient(cx, bottom - h * 0.45, r * 0.2, cx, bottom - h * 0.45, r);
    g.addColorStop(0, `rgba(${col},${0.6 * pulse})`);
    g.addColorStop(1, `rgba(${col},0)`);
    c.fillStyle = g;
    c.beginPath();
    c.ellipse(cx, bottom - h * 0.45, r, r * 0.95, 0, 0, Math.PI * 2);
    c.fill();
  }

  /** The equipped hat (or season crown) on the head of the current frame. */
  private drawHatOn(a: NonNullable<ReturnType<Sprites["action"]>>, idx: number, cx: number, bottom: number, now: number): void {
    const sp = this.sprites!;
    const f = sp.pack.frame;
    const sc = this.sc(sp);
    const h = sp.head(a, idx);
    drawHat(this.ctx, this.view!.hat!, cx + (h.x - f.w / 2) * sc, bottom - (f.h - h.top) * sc, h.w * sc, now);
  }

  /**
   * The pet leans back and holds a rope that runs to the window edge facing
   * the other pet; the two windows touch, so the halves join into one rope.
   * The trailing pet leans harder and gets dragged a little toward the rope.
   */
  private drawTug(a: NonNullable<ReturnType<Sprites["action"]>>, idx: number, cx: number, bottom: number, mainW: number, mainH: number, w: number, now: number): void {
    const t = this.tug!;
    const sp = this.sprites!;
    const c = this.ctx;
    const toward = t.side === "left" ? 1 : -1; // direction of the rope
    const k = Math.min(1, (now - t.start) / Math.max(1, t.until - t.start));
    const base = t.lead === true ? 0.12 : t.lead === false ? 0.3 : 0.2;
    const lean = -toward * (base + Math.sin(now / 140) * 0.04);
    const drag = (t.lead === false ? 12 : t.lead === true ? -4 : 0) * k * toward;
    const px = cx + drag;
    const handY = bottom - mainH * 0.42;
    // Hand position after leaning back around the feet.
    const hx0 = toward * mainW * 0.28;
    const hy0 = -mainH * 0.42;
    const hx = px + hx0 * Math.cos(lean) - hy0 * Math.sin(lean);
    const hy = bottom + hx0 * Math.sin(lean) + hy0 * Math.cos(lean);
    const edge = t.side === "left" ? w : 0;
    // Rope (behind the pet's hand, in front of the body is fine at this size).
    // Rope colour depends on the skin; draw in window coordinates so the
    // two halves (in two windows) line up.
    c.strokeStyle = ropeStyle(c, this.rope, Math.min(hx, edge), Math.max(hx, edge));
    c.lineWidth = 3;
    c.beginPath();
    c.moveTo(hx, hy);
    c.quadraticCurveTo((hx + edge) / 2, (hy + handY) / 2 + 4, edge, handY);
    c.stroke();
    c.save();
    c.translate(px, bottom);
    c.rotate(lean);
    sp.draw(c, a, idx, 0, 0, this.sc(sp), this.view?.hue ?? 0);
    if (this.view?.hat) this.drawHatOn(a, idx, 0, 0, now);
    c.restore();
    // The pet on the left carries the flag at the joint.
    if (t.side === "left") {
      const fx = w - 3;
      c.fillStyle = "#ee5b3c";
      c.fillRect(fx - 1, handY, 2, 12);
      c.beginPath();
      c.moveTo(fx - 1, handY + 1);
      c.lineTo(fx - 10, handY + 5);
      c.lineTo(fx - 1, handY + 9);
      c.fill();
    }
  }

  private drawDuo(a: NonNullable<ReturnType<Sprites["action"]>>, idx: number, cx: number, bottom: number, mainW: number, mainH: number, w: number, now: number): void {
    const d = this.duo!;
    const sp = this.sprites!;
    const c = this.ctx;
    const toward = d.side === "left" ? 1 : -1;
    const t = now - d.start;
    const hue = this.view?.hue ?? 0;
    if (d.game === "armwrestle") {
      // Clasped hands sway over the table, then the winner slams them down.
      const edge = d.side === "left" ? w : 0;
      const tableTop = bottom - mainH * 0.36;
      const k = Math.min(1, Math.max(0, t / d.ms));
      const fin = Math.max(0, (t - (d.ms - 900)) / 900);
      const winnerSide = d.winner ? d.side : d.side === "left" ? "right" : "left";
      const sway = 12 * Math.sin(t / 230 + d.seed) * Math.sin(t / 610 + d.seed * 2) * (1 - fin);
      const slam = (winnerSide === "left" ? 1 : -1) * 30 * Math.min(1, fin * fin * 1.6);
      const off = sway + slam; // + = rightward, same in both windows
      const pushing = (off * toward) < 0; // the clasp is moving toward me
      const lean = toward * (0.1 + (pushing ? 0.08 : 0.02) + 0.02 * Math.sin(now / 90));
      // Table half.
      c.fillStyle = "#8d5a3b";
      const tx = d.side === "left" ? edge - 42 : edge;
      c.fillRect(tx, tableTop, 42, 7);
      c.fillStyle = "#6e4329";
      c.fillRect(d.side === "left" ? edge - 36 : edge + 30, tableTop + 7, 5, bottom - tableTop - 7);
      // Pet, leaning in.
      c.save();
      c.translate(cx, bottom);
      c.rotate(lean);
      sp.draw(c, a, idx, 0, 0, this.sc(sp), hue);
      if (this.view?.hat) this.drawHatOn(a, idx, 0, 0, now);
      c.restore();
      // Arm from the shoulder to the clasp (the other window draws its half).
      const sx = cx + toward * mainW * 0.2 + Math.sin(lean) * mainH * 0.5;
      const sy = bottom - mainH * 0.5;
      const hx = edge + off;
      const hy = tableTop - 9 + (fin > 0 ? Math.abs(slam) * 0.2 : 0);
      c.lineCap = "round";
      c.strokeStyle = "#4a2c20";
      c.lineWidth = 8;
      c.beginPath();
      c.moveTo(sx, sy);
      c.quadraticCurveTo((sx + hx) / 2, sy + 6, hx, hy);
      c.stroke();
      c.strokeStyle = "#f2c9a0";
      c.lineWidth = 5;
      c.stroke();
      c.fillStyle = "#f2c9a0";
      c.strokeStyle = "#4a2c20";
      c.lineWidth = 1.5;
      c.beginPath();
      c.arc(hx, hy, 6, 0, Math.PI * 2);
      c.fill();
      c.stroke();
      if (k > 0.3 && k < 0.95 && Math.floor(now / 400) % 3 === 0) {
        c.font = `14px "Segoe UI Emoji", sans-serif`;
        c.fillText("💢", cx - toward * mainW * 0.3, bottom - mainH * 0.95);
      }
      return;
    }
    // Racket games: my shots are the even ones when I serve, else the odd.
    let swing = 0;
    let hop = 0;
    for (let i = d.serve ? 0 : 1; i < d.hits; i += 2) {
      const dt = t - i * d.each;
      if (dt > -180 && dt < 220) {
        swing = Math.sin(((dt + 180) / 400) * Math.PI);
        if (d.game === "badminton") hop = swing * 8;
      }
    }
    // The loser lunges for the last ball and misses.
    const missAt = d.hits * d.each - d.each * 0.15;
    let lunge = 0;
    if (!d.winner && t > missAt - 200 && t < missAt + 500) lunge = Math.sin(Math.min(1, (t - missAt + 200) / 500) * Math.PI);
    if (!d.winner && t >= missAt && t < missAt + 400) swing = Math.max(swing, Math.sin(((t - missAt) / 400) * Math.PI));
    const lean = toward * (lunge * 0.35 + swing * 0.06);
    c.save();
    c.translate(cx + toward * lunge * 10, bottom - hop);
    c.rotate(lean);
    sp.draw(c, a, idx, 0, 0, this.sc(sp), hue);
    if (this.view?.hat) this.drawHatOn(a, idx, 0, 0, now);
    // Racket at the hand facing the other pet.
    const hx = toward * mainW * 0.3;
    const hy = -mainH * 0.45;
    const rest = d.game === "badminton" ? -0.9 : -0.3;
    const ang = toward * (rest + swing * 1.9);
    c.translate(hx, hy);
    c.rotate(ang);
    const len = d.game === "badminton" ? Math.max(26, mainH * 0.36) : Math.max(8, mainH * 0.1);
    c.strokeStyle = "#5b3a29";
    c.lineWidth = 2.5;
    c.beginPath();
    c.moveTo(0, 0);
    c.lineTo(0, -len);
    c.stroke();
    if (d.game === "badminton") {
      c.strokeStyle = "#2a6fdb";
      c.lineWidth = 2;
      c.fillStyle = "rgba(255,255,255,0.7)";
      c.beginPath();
      c.ellipse(0, -len - 10, 8, 11, 0, 0, Math.PI * 2);
      c.fill();
      c.stroke();
      c.strokeStyle = "rgba(42,111,219,0.5)";
      c.lineWidth = 0.8;
      for (const o of [-4, 0, 4]) {
        c.beginPath();
        c.moveTo(o, -len - 20);
        c.lineTo(o, -len);
        c.stroke();
        c.beginPath();
        c.moveTo(-7, -len - 10 + o * 1.5);
        c.lineTo(7, -len - 10 + o * 1.5);
        c.stroke();
      }
    } else {
      c.fillStyle = "#d62828";
      c.strokeStyle = "#5b3a29";
      c.lineWidth = 1.5;
      c.beginPath();
      c.arc(0, -len - 8, 9, 0, Math.PI * 2);
      c.fill();
      c.stroke();
    }
    c.restore();
  }

  /** Which pet or mini is under a point (logical px). */
  hitTest(x: number, y: number): HitRect | null {
    const rects: [number, number, number, number, string][] = JSON.parse(this.lastHits || "[]");
    // Main pet last in the list but on top: check it first.
    for (let i = rects.length - 1; i >= 0; i--) {
      const [rx, ry, rw, rh, key] = rects[i];
      if (x >= rx && x < rx + rw && y >= ry && y < ry + rh) {
        return { x: rx, y: ry, w: rw, h: rh, kind: key ? "mini" : "main", key: key || undefined };
      }
    }
    return null;
  }

  private footText(text: string, cx: number, bottom: number, maxW: number, color: string, bold: boolean): void {
    const c = this.ctx;
    c.font = `${bold ? "600 " : ""}10.5px 'Segoe UI', 'Microsoft YaHei UI', 'PingFang SC', system-ui, sans-serif`;
    let t = text;
    while (c.measureText(t).width > maxW && t.length > 1) t = t.slice(0, -2) + "…";
    c.textAlign = "center";
    c.textBaseline = "top";
    c.lineWidth = 3;
    c.strokeStyle = "rgba(255,255,255,0.85)";
    c.strokeText(t, cx, bottom + 2);
    c.fillStyle = color;
    c.fillText(t, cx, bottom + 2);
  }

  private drawSmoke(cx: number, cy: number, size: number, t: number): void {
    const c = this.ctx;
    const k = Math.min(1, Math.max(0, t));
    const puffs: [number, number, number][] = [[0, 0, 0.32], [-0.22, 0.1, 0.22], [0.22, 0.08, 0.24], [-0.1, -0.2, 0.2], [0.14, -0.18, 0.18]];
    c.save();
    c.globalAlpha = 1 - k * 0.85;
    c.fillStyle = "#e8e4df";
    c.strokeStyle = "#a8a19a";
    c.lineWidth = 2;
    for (const [dx, dy, r] of puffs) {
      c.beginPath();
      c.arc(cx + dx * size * (1 + k), cy + dy * size * (1 + k), r * size * (0.6 + k * 0.6), 0, Math.PI * 2);
      c.fill();
      c.stroke();
    }
    c.restore();
  }

  private drawBubble(text: string, w: number, petTop: number): void {
    const c = this.ctx;
    c.font = "12.5px 'Segoe UI', 'Microsoft YaHei UI', 'PingFang SC', system-ui, sans-serif";
    const maxW = Math.min(w - 16, 260);
    let t = text;
    while (c.measureText(t).width > maxW - 16 && t.length > 1) t = t.slice(0, -2) + "…";
    const tw = Math.min(maxW, c.measureText(t).width + (this.view?.state === "attention" ? 30 : 22));
    const bh = 26;
    const bx = Math.round((w - tw) / 2);
    const by = Math.max(2, Math.min(BUBBLE_H - bh - 8, petTop - bh - 8));
    const attention = this.view?.state === "attention";
    // A rounded pill with a small tail: white normally, coral when you're needed.
    const fill = attention ? "#ee5b3c" : "#ffffff";
    c.save();
    c.shadowColor = attention ? "rgba(238,91,60,0.45)" : "rgba(31,34,69,0.22)";
    c.shadowBlur = 10;
    c.shadowOffsetY = 3;
    c.fillStyle = fill;
    c.beginPath();
    c.roundRect(bx, by, tw, bh, bh / 2);
    c.moveTo(w / 2 - 6, by + bh - 1);
    c.lineTo(w / 2, by + bh + 6);
    c.lineTo(w / 2 + 6, by + bh - 1);
    c.fill();
    c.restore();
    let tx = w / 2;
    if (attention) {
      c.fillStyle = "#ffffff";
      c.beginPath();
      c.arc(bx + 12, by + bh / 2, 3, 0, Math.PI * 2);
      c.fill();
      tx += 5;
    }
    c.fillStyle = attention ? "#ffffff" : "#1f2245";
    c.font = "600 12.5px 'Segoe UI', 'Microsoft YaHei UI', 'PingFang SC', system-ui, sans-serif";
    c.textBaseline = "middle";
    c.textAlign = "center";
    c.fillText(t, tx, by + bh / 2 + 1);
  }

  private drawBadge(left: number, top: number): void {
    const v = this.view;
    if (!v || v.online <= 1) return;
    const c = this.ctx;
    const text = v.waiting > 0 ? `!${v.waiting}` : `×${v.online}`;
    c.font = "bold 11px 'Segoe UI', system-ui, sans-serif";
    const tw = c.measureText(text).width + 12;
    const x = Math.round(left + 2), y = Math.round(top);
    c.fillStyle = v.waiting > 0 ? "#ee5b3c" : "#2b3275";
    c.beginPath();
    c.roundRect(x, y, tw, 16, 8);
    c.fill();
    c.fillStyle = "#fff";
    c.textAlign = "center";
    c.textBaseline = "middle";
    c.fillText(text, x + tw / 2, y + 8.5);
  }

  /** Camp tag under the feet: which agent this pet stands for. */
  private drawMarker(camp: string, label: string | undefined, cx: number, bottom: number): void {
    const c = this.ctx;
    const text = label ? `${camp} · ${label}` : camp;
    c.font = "600 10px 'Segoe UI', 'Microsoft YaHei UI', system-ui, sans-serif";
    let t = text;
    while (c.measureText(t).width > 150 && t.length > 4) t = t.slice(0, -2) + "…";
    const tw = c.measureText(t).width + 18;
    const x = Math.round(cx - tw / 2);
    // A pill in the camp colour with a white dot.
    c.fillStyle = camp === "Codex" ? "#2f9a8a" : "#e0794f";
    c.beginPath();
    c.roundRect(x, bottom + 2, tw, 15, 7.5);
    c.fill();
    c.fillStyle = "rgba(255,255,255,0.9)";
    c.beginPath();
    c.arc(x + 7, bottom + 9.5, 2, 0, Math.PI * 2);
    c.fill();
    c.fillStyle = "#ffffff";
    c.textAlign = "center";
    c.textBaseline = "middle";
    c.fillText(t, cx + 4, bottom + 9.5);
  }
}

function slotX(i: number, w: number, mainW: number, miniW: number): number {
  const side = i % 2 === 0 ? 1 : -1;
  const k = Math.floor(i / 2);
  return w / 2 + side * (mainW / 2 + GAP + miniW / 2 + k * (miniW + GAP));
}
