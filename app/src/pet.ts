// Draws what goes around one pet: its bubble, the kegel coach, the badge that
// counts sessions, the tag that names its agent, and the effects. The body is
// the rig, on its own canvas under this one; this file only says where it
// stands and which rectangle takes the clicks (everything else lets them
// through).

import type { PetView } from "./api";
import { drawSlip, Fx, slipHand, type Anchor } from "./fx";
import { t as tr } from "./i18n"; // `t` is a common local name here
import { sampleUrl, worn } from "./skin";
import { line as voiceLine, lines as voiceLines } from "./voice";

export const BUBBLE_H = 48;
/** While it asks, the window grows this much taller (feet where they were), so the question and its answers sit above its head. */
const ASK_H = 42;
export const FOOT_H = 18;
/** The pet's own palette, for everything drawn around it. */
const paper = () => worn().colors.paper;
const ink = () => worn().colors.ink;
const outline = () => worn().colors.line;
const shade = () => `rgba(${worn().colors.shade},0.24)`;
const accent = () => worn().colors.accent;
/** The one loud colour: you are being waited on. */
const ALERT = "#c24e38";
/** The dot in the tag under a pet, by agent. */
const MARKER_COLORS: Record<string, string> = { Claude: "#e0794f", Codex: "#2f9a8a", Gemini: "#3f73d8", Cursor: "#4a4f6b", OpenCode: "#b7791f" };
const MIN_W = 200;
const KEGEL_READY = 3000;
const KEGEL_REP = 6000;
const FONT = "'Segoe UI Variable Text', 'Segoe UI', 'Microsoft YaHei UI', 'PingFang SC', system-ui, sans-serif";

/** Something the pet asks, with a yes and a no on its bubble (and where those were drawn, for clicks). */
interface Offer {
  text: string;
  yes: string;
  no: string;
  until: number;
  onYes: () => void;
  /** Asked even while its agent waits on you (the question is about that wait). */
  always?: boolean;
  box?: [number, number, number, number];
  yesBox?: [number, number, number, number];
  noBox?: [number, number, number, number];
}

interface KegelState { phase: "ready" | "squeeze" | "relax" | "done"; rep: number; left: number; k: number; repT: number }

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

/** How long a state line stays up; 0 = until the state changes. */
const BUBBLE_MS: Record<string, number> = { attention: 0, stone: 0, done: 4000, error: 4000 };

/** What the body can be asked to do; the rig acts it out. */
export type Gesture = "poke" | "perk" | "cheer";

export interface HitRect {
  x: number;
  y: number;
  w: number;
  h: number;
}

export class PetRenderer {
  private view: PetView | null = null;
  private bubbleText = "";
  /** Something it asks, with a yes and a no on the bubble: what "yes" does, and until when it waits. */
  private offer: Offer | null = null;
  /** The bubble is something the pet was told to say (not its state line): it stays its full time through state changes. */
  private bubbleSaid = false;
  private bubbleUntil = 0;
  private muted = false;
  private focus = false;
  /** Being dragged, thrown or walked about by the host: nothing else plays meanwhile. */
  private moving = false;
  /** Guided kegel set: 3 s to get ready, then squeeze 3 s / relax 3 s per rep. */
  private kegel: { start: number; reps: number; last: string } | null = null;
  /** Effects around the pet (the fish, hearts, confetti, the reset card). */
  readonly fx = new Fx();
  private lastKey = "";
  private lastHits = "";
  private lastSize = "";
  private chimes: number[] = [];

  /** How big the rig draws the pet (logical px), standing on the foot line. */
  rigBox = { w: 122, h: 104 };
  /** How many things it holds for you (手上的事): a slip at its side when any, a second behind it when more. */
  notes = 0;
  /** Called when the body should do something. */
  onGesture: (g: Gesture) => void = () => {};
  /** A kegel set was done to the end (not stopped half way). */
  onKegelDone: () => void = () => {};
  /** Called when the logical window size changes. */
  onResize: (w: number, h: number) => void = () => {};
  /** Called when the clickable rectangle changes (logical px). */
  onHits: (rects: HitRect[]) => void = () => {};

  constructor(private ctx: CanvasRenderingContext2D) {}

  /** Logical window size: the pet, room for its ears to swing, the bubble band above and the tag below. */
  size(): { w: number; h: number } {
    const asking = this.offer && performance.now() < this.offer.until && this.offerable(this.offer);
    return {
      w: Math.ceil(Math.max(MIN_W, this.rigBox.w * 1.18 + 16)),
      // Raised ears and hops reach into the bubble band; give them a little air.
      h: Math.ceil(BUBBLE_H + (asking ? ASK_H : 0) + this.rigBox.h + 12 + FOOT_H),
    };
  }

  /** The rig changed size (the user's size setting). */
  fit(): void {
    this.checkSize();
  }

  setView(v: PetView): void {
    const prev = this.view;
    this.view = v;
    this.lastKey = "";
    if (prev && prev.state === v.state && prev.since === v.since) return;

    const now = performance.now();
    const waking = prev?.state === "sleeping" && v.state !== "sleeping" && v.state !== "offline";
    if (v.state === "attention") {
      this.onGesture("perk");
      this.chime();
    } else if (v.state === "done") this.onGesture("cheer");
    else if (waking) this.onGesture("perk");

    const key = v.bubble?.key;
    // What it says when its agent's state changes, in its own voice.
    const lines = key ? voiceLines(key) : [];
    if (key && lines.length) {
      this.bubbleText = (v.bubble?.project ? `[${v.bubble.project}] ` : "") + lines[Math.floor(Math.random() * lines.length)];
      this.bubbleSaid = false;
      const ms = BUBBLE_MS[key] ?? 4000;
      this.bubbleUntil = ms === 0 ? Infinity : now + ms;
    } else if (!this.bubbleSaid || now > this.bubbleUntil) {
      this.bubbleText = "";
      this.bubbleSaid = false;
    }
  }

  /** User poked the pet: it squishes. A poke during a kegel set calls the set off. */
  poke(): void {
    if (this.kegel) {
      this.kegel = null;
      this.lastKey = "";
      this.say(voiceLine("later"), 2500);
      return;
    }
    if (this.view?.state === "attention") return;
    this.onGesture("poke");
  }

  /** While a kegel set runs: how hard the pet is squeezing, 0..1. Null when no set is running. */
  kegelClench(now: number): number | null {
    return this.kegel ? kegelAt(this.kegel, now).k : null;
  }

  /** Lead a kegel set of `reps` (squeeze 3 s, relax 3 s each). */
  startKegel(reps: number): void {
    if (this.view?.state === "attention" || this.moving) return;
    this.kegel = { start: performance.now(), reps: Math.max(1, Math.round(reps)), last: "" };
    this.bubbleText = "";
  }

  /** A gesture, with a line if it has one. Being waited on comes first. */
  act(g: Gesture, bubble?: string | null, ms = 3000): void {
    if (this.view?.state === "attention") return;
    this.onGesture(g);
    if (bubble) this.say(bubble, Math.max(2500, ms));
  }

  /** The host is moving the pet about (drag, throw, a walk to the other pet), or has let go. */
  setMoving(on: boolean): void {
    this.moving = on;
  }

  isMoving(): boolean {
    return this.moving;
  }

  /** Ask something: the line, and a "yes" and a "no" on the bubble. Unanswered, it goes after `ms`. */
  ask(text: string, yes: string, no: string, ms: number, onYes: () => void, always = false): void {
    if (this.view?.state === "attention" && !always) return;
    this.offer = { text, yes, no, until: performance.now() + ms, onYes, always };
    this.bubbleText = "";
    this.lastKey = "";
  }

  /** Being waited on comes first: only a question about that wait is asked meanwhile. */
  private offerable(o: Offer): boolean {
    return !!o.always || this.view?.state !== "attention";
  }

  /** Which answer is under a point, if the pet is asking. */
  offerAt(x: number, y: number): "yes" | "no" | "bubble" | null {
    const o = this.offer;
    if (!o?.box) return null;
    const inside = (b?: [number, number, number, number]) => !!b && x >= b[0] && x < b[0] + b[2] && y >= b[1] && y < b[1] + b[3];
    return inside(o.yesBox) ? "yes" : inside(o.noBox) ? "no" : inside(o.box) ? "bubble" : null;
  }

  answer(yes: boolean): void {
    const o = this.offer;
    this.offer = null;
    this.lastKey = "";
    if (o && yes) o.onYes();
  }

  say(text: string, ms: number): void {
    if (this.view?.state === "attention") return;
    this.bubbleText = text;
    this.bubbleSaid = true;
    this.bubbleUntil = ms > 0 ? performance.now() + ms : Infinity;
    this.lastKey = "";
  }

  setMuted(on: boolean): void {
    this.muted = on;
  }

  setFocus(on: boolean): void {
    this.focus = on;
  }

  currentView(): PetView | null {
    return this.view;
  }

  /** What goes after the agent's name in the tag: the project it last heard from. */
  private watching(): string | undefined {
    const v = this.view;
    if (!v) return undefined;
    const latest = [...v.sessions].sort((a, b) => b.updated_at - a.updated_at)[0];
    return latest?.project || undefined;
  }

  /** What a finished turn brought: a sweet, an empty wrapper, or one that rolled away. */
  reel(items: { outcome: string; name: string; rarity: string }[]): void {
    if (!items.length) return;
    const fish = items.filter((i) => i.outcome === "fish");
    let text: string;
    if (items.length > 1) {
      text = tr(`得了 ${fish.length} 颗。`, `${fish.length} for the jar.`);
    } else if (items[0].outcome === "fish") {
      const tag = { rare: tr("。稀有的。", ". Rare."), epic: tr("。史诗。", ". Epic."), legendary: tr("……传说里的那条。", "… the one from the stories.") }[items[0].rarity] ?? tr("。", ".");
      text = tr(`得了一颗${items[0].name}${tag}`, `Got ${items[0].name}${tag}`);
    } else if (items[0].outcome === "boot") {
      text = tr("只有一张空糖纸。", "Only an empty wrapper.");
    } else {
      text = tr("滚到桌子底下去了。", "It rolled away under the desk.");
    }
    if (fish.length) this.act("cheer", text, 3500);
    else this.say(text, 3500);
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

  /** "Your turn": two soft piano notes. Boss key mutes it; at most four a minute. */
  private chime(): void {
    if (this.muted) return;
    const now = Date.now();
    this.chimes = this.chimes.filter((t) => now - t < 60_000);
    if (this.chimes.length >= 4) return;
    this.chimes.push(now);
    // In its own instrument.
    const s = worn();
    s.chime.forEach((note, i) => {
      window.setTimeout(() => {
        const a = new Audio(`${sampleUrl(s.instrument)}${note}.mp3`);
        a.volume = 0.45;
        a.play().catch(() => {});
      }, i * 170);
    });
  }

  /** Advance and redraw if anything visible changed. */
  update(now: number): void {
    this.checkSize();
    if (this.bubbleText && now > this.bubbleUntil) this.bubbleText = "";
    if (this.kegel) this.kegelTick(now);
    const v = this.view;
    const moving = this.fx.busy(now) || this.kegel ? Math.floor(now / 33) : -1;
    // An offer redraws when it comes and when it runs out.
    const asking = this.offer ? (now < this.offer.until ? "ask" : "gone") : "";
    const key = `${moving}|${this.notes > 1 ? 2 : this.notes}|${asking}|${this.bubbleText}|${v?.state}|${v?.online}|${v?.waiting}|${v?.marker}|${this.watching()}|${this.rigBox.w}`;
    if (key === this.lastKey) return;
    this.lastKey = key;
    this.draw(now);
  }

  private draw(now: number): void {
    const { w, h } = this.size();
    const c = this.ctx;
    c.clearRect(0, 0, w, h);
    const bottom = h - FOOT_H;
    const { w: mainW, h: mainH } = this.rigBox;
    const cx = w / 2;
    const anchor: Anchor = { cx, bottom, top: bottom - mainH, headW: mainW * 0.7, w };

    // The slips it holds for you, then effects on top of the pet, under the bubble.
    if (this.notes > 0) {
      const hand = slipHand(anchor);
      if (this.notes > 1) drawSlip(c, hand.x + 4, hand.y + 3, 0.28, 0.92);
      drawSlip(c, hand.x, hand.y, 0.12);
    }
    this.fx.draw(c, anchor, now);
    if (this.kegel) this.drawKegel(kegelAt(this.kegel, now), cx, w, bottom, mainW, mainH, now);
    else if (this.offer && now < this.offer.until && this.offerable(this.offer)) this.drawOffer(this.offer, w, bottom - mainH);
    else if (this.bubbleText) this.drawBubble(this.bubbleText, w, bottom - mainH);
    if (this.offer && (now >= this.offer.until || !this.offerable(this.offer))) this.offer = null;
    this.drawBadge(cx - mainW / 2, bottom - mainH + 4);
    if (this.view?.marker) this.drawMarker(this.view.marker, this.watching(), cx, bottom);

    // The body is what takes the clicks.
    const hits: HitRect[] = [{ x: cx - mainW / 2, y: bottom - mainH, w: mainW, h: mainH }];
    // An offer's bubble takes clicks too: its yes and its no.
    if (this.offer?.box) hits.push({ x: this.offer.box[0], y: this.offer.box[1], w: this.offer.box[2], h: this.offer.box[3] });
    const hk = JSON.stringify(hits.map((r) => [Math.round(r.x), Math.round(r.y), Math.round(r.w), Math.round(r.h)]));
    if (hk !== this.lastHits) {
      this.lastHits = hk;
      this.onHits(hits);
    }
  }

  /** Phase changes: a soft note, and the closing line. */
  private kegelTick(now: number): void {
    const kg = this.kegel!;
    const st = kegelAt(kg, now);
    const mark = `${st.phase}${st.rep}`;
    if (mark === kg.last) return;
    kg.last = mark;
    if (st.phase === "squeeze") this.note(1);
    else if (st.phase === "relax") this.note(0);
    else if (st.phase === "done") {
      this.kegel = null;
      this.lastKey = "";
      this.act("cheer", tr(`${kg.reps} 下。没人看见。`, `${kg.reps} reps. Nobody saw a thing.`), 2600);
      this.onKegelDone();
    }
  }

  /** One quiet note of its instrument (the higher of its two for a squeeze), so the set can be followed without looking. */
  private note(which: 0 | 1): void {
    if (this.muted || this.focus) return;
    const s = worn();
    const a = new Audio(`${sampleUrl(s.instrument)}${s.chime[which]}.mp3`);
    a.volume = 0.3;
    a.play().catch(() => {});
  }

  /** Coach overlay above the pet: what to do, a countdown, and progress dots. */
  private drawKegel(st: KegelState, cx: number, w: number, bottom: number, mainW: number, mainH: number, now: number): void {
    const c = this.ctx;
    const reps = this.kegel?.reps ?? 0;
    const [label, color] =
      st.phase === "ready" ? [tr(`准备 ${st.left}`, `Ready ${st.left}`), "#8d7fb5"]
      : st.phase === "squeeze" ? [tr(`收紧 ${st.left}`, `Squeeze ${st.left}`), worn().colors.accent]
      : [tr(`放松 ${st.left}`, `Relax ${st.left}`), "#5d8f86"];
    const count = st.phase === "ready" ? tr(`${reps} 下`, `${reps} reps`) : `${st.rep}/${reps}`;
    const big = `700 13px ${FONT}`;
    const small = `600 11px ${FONT}`;
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
      c.fillStyle = done ? worn().colors.accentDeep : "rgba(91,74,87,0.2)";
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

  /** Whether a point (logical px) is on the pet. */
  /** Everything of the pet that takes the pointer (body, bubble, tag). */
  hitRects(): HitRect[] {
    const rects: [number, number, number, number][] = JSON.parse(this.lastHits || "[]");
    return rects.length ? rects.map(([x, y, w, h]) => ({ x, y, w, h })) : [{ x: 0, y: 0, w: 1, h: 1 }];
  }

  hitTest(x: number, y: number): HitRect | null {
    const rects: [number, number, number, number][] = JSON.parse(this.lastHits || "[]");
    for (const [rx, ry, rw, rh] of rects) {
      if (x >= rx && x < rx + rw && y >= ry && y < ry + rh) return { x: rx, y: ry, w: rw, h: rh };
    }
    return null;
  }

  /** The asking bubble: its line, and under it a filled "yes" and a plain "no". */
  private drawOffer(o: Offer, w: number, petTop: number): void {
    const c = this.ctx;
    c.font = `600 12.5px ${FONT}`;
    // The question wraps once if it must, at a pause (a full stop or comma) when there is one near the middle.
    const room = w - 12 - 22;
    let lines = [o.text];
    if (c.measureText(o.text).width > room) {
      const chars = [...o.text];
      let n = 0;
      let first = "";
      while (n < chars.length && c.measureText(first + chars[n]).width <= room) first += chars[n++];
      const pause = Math.max(first.lastIndexOf("。"), first.lastIndexOf("，"), first.lastIndexOf("？"), first.lastIndexOf("……") + 1, first.lastIndexOf(". "), first.lastIndexOf(", "));
      if (pause >= first.length * 0.4 && pause < first.length - 1) {
        n = [...first.slice(0, pause + 1)].length;
        first = first.slice(0, pause + 1);
      }
      let rest = chars.slice(n).join("").trimStart();
      // A line never starts with a stop or a comma: the last character of the first line comes down with it.
      if (/^[，。？！、：；…」,.?!]/.test(rest) && [...first].length > 1) {
        const f = [...first];
        rest = f.pop() + rest;
        first = f.join("");
      }
      while (c.measureText(rest).width > room && rest.length > 2) rest = rest.slice(0, -2) + "…";
      lines = [first.trimEnd(), rest];
    }
    const textW = Math.max(...lines.map((l) => c.measureText(l).width));
    c.font = `700 12px ${FONT}`;
    const yw = c.measureText(o.yes).width + 26, nw = c.measureText(o.no).width + 22;
    const tw = Math.min(w - 12, Math.max(textW + 26, yw + nw + 30));
    const two = lines.length > 1;
    const bh = two ? 74 : 58;
    const bx = Math.round((w - tw) / 2);
    const by = Math.max(2, petTop - bh - 8);
    c.save();
    c.shadowColor = shade();
    c.shadowBlur = 10;
    c.shadowOffsetY = 3;
    c.fillStyle = paper();
    c.beginPath();
    c.roundRect(bx, by, tw, bh, 14);
    c.moveTo(w / 2 - 6, by + bh - 1);
    c.lineTo(w / 2, by + bh + 6);
    c.lineTo(w / 2 + 6, by + bh - 1);
    c.fill();
    c.restore();
    c.strokeStyle = outline();
    c.lineWidth = 1;
    c.beginPath();
    c.roundRect(bx + 0.5, by + 0.5, tw - 1, bh - 1, 13.5);
    c.stroke();
    c.fillStyle = ink();
    c.textBaseline = "middle";
    c.textAlign = "center";
    c.font = `600 12.5px ${FONT}`;
    lines.forEach((l, i) => c.fillText(l, w / 2, by + 17 + i * 16));
    // The two answers, side by side under the line.
    const y = by + (two ? 47 : 31), h = 20, gap = 8;
    const x0 = Math.round(w / 2 - (yw + nw + gap) / 2);
    c.font = `700 12px ${FONT}`;
    c.fillStyle = accent();
    c.beginPath();
    c.roundRect(x0, y, yw, h, 10);
    c.fill();
    c.fillStyle = "#ffffff";
    c.fillText(o.yes, x0 + yw / 2, y + h / 2 + 0.5);
    c.strokeStyle = outline();
    c.beginPath();
    c.roundRect(x0 + yw + gap + 0.5, y + 0.5, nw - 1, h - 1, 9.5);
    c.stroke();
    c.fillStyle = ink();
    c.fillText(o.no, x0 + yw + gap + nw / 2, y + h / 2 + 0.5);
    o.box = [bx, by, tw, bh];
    o.yesBox = [x0 - 4, y - 4, yw + 8, h + 8];
    o.noBox = [x0 + yw + gap - 4, y - 4, nw + 8, h + 8];
  }

  private drawBubble(text: string, w: number, petTop: number): void {
    const c = this.ctx;
    c.font = `600 12.5px ${FONT}`;
    const maxW = Math.min(w - 16, 260);
    const attention = this.view?.state === "attention";
    const fits = (s: string) => c.measureText(s).width <= maxW - 16;
    const clip = (s: string) => {
      while (!fits(s) && s.length > 1) s = s.slice(0, -2) + "…";
      return s;
    };
    // A long line wraps once; what still does not fit is cut. The "needs you"
    // pill stays one line.
    let lines = [text];
    if (!attention && !fits(text)) {
      const chars = [...text];
      let n = 0;
      let first = "";
      while (n < chars.length && fits(first + chars[n])) first += chars[n++];
      // Latin text breaks at a space when one is close to the edge.
      const sp = first.lastIndexOf(" ");
      if (sp > first.length * 0.6) {
        n -= [...first.slice(sp + 1)].length;
        first = first.slice(0, sp);
      }
      lines = [first, clip(chars.slice(n).join("").trimStart())];
    } else {
      lines = [clip(text)];
    }
    const two = lines.length > 1;
    const tw = Math.min(maxW, Math.max(...lines.map((l) => c.measureText(l).width)) + (attention ? 30 : 22));
    const bh = two ? 42 : 26;
    const bx = Math.round((w - tw) / 2);
    const by = Math.max(2, Math.min(BUBBLE_H - bh - 8, petTop - bh - 8));
    // A rounded pill with a small tail: paper normally, coral when you're needed.
    c.save();
    c.shadowColor = attention ? "rgba(194,78,56,0.4)" : shade();
    c.shadowBlur = 10;
    c.shadowOffsetY = 3;
    c.fillStyle = attention ? ALERT : paper();
    c.beginPath();
    c.roundRect(bx, by, tw, bh, 13);
    c.moveTo(w / 2 - 6, by + bh - 1);
    c.lineTo(w / 2, by + bh + 6);
    c.lineTo(w / 2 + 6, by + bh - 1);
    c.fill();
    c.restore();
    if (!attention) {
      // The same soft outline the pet is drawn with.
      c.strokeStyle = outline();
      c.lineWidth = 1;
      c.beginPath();
      c.roundRect(bx + 0.5, by + 0.5, tw - 1, bh - 1, 12.5);
      c.stroke();
    }
    let tx = w / 2;
    if (attention) {
      c.fillStyle = "#ffffff";
      c.beginPath();
      c.arc(bx + 12, by + bh / 2, 3, 0, Math.PI * 2);
      c.fill();
      tx += 5;
    }
    c.fillStyle = attention ? "#ffffff" : ink();
    c.textBaseline = "middle";
    c.textAlign = "center";
    if (two) {
      c.fillText(lines[0], tx, by + 14);
      c.fillText(lines[1], tx, by + 30);
    } else {
      c.fillText(lines[0], tx, by + bh / 2 + 1);
    }
  }

  /** How many sessions this pet stands for, when more than one; how many of them wait on you. */
  private drawBadge(left: number, top: number): void {
    const v = this.view;
    if (!v || v.online <= 1) return;
    const c = this.ctx;
    const text = v.waiting > 0 ? `!${v.waiting}` : `×${v.online}`;
    c.font = `700 11px ${FONT}`;
    const tw = c.measureText(text).width + 12;
    const x = Math.round(left + 2), y = Math.round(top);
    c.fillStyle = v.waiting > 0 ? ALERT : "#8d7fb5";
    c.beginPath();
    c.roundRect(x, y, tw, 16, 8);
    c.fill();
    c.fillStyle = "#fff";
    c.textAlign = "center";
    c.textBaseline = "middle";
    c.fillText(text, x + tw / 2, y + 8.5);
  }

  /** The tag under the feet: which agent this pet stands for, and the project it watches. */
  private drawMarker(camp: string, label: string | undefined, cx: number, bottom: number): void {
    const c = this.ctx;
    const text = label ? `${camp} · ${label}` : camp;
    c.font = `600 10px ${FONT}`;
    let t = text;
    while (c.measureText(t).width > 150 && t.length > 4) t = t.slice(0, -2) + "…";
    const tw = c.measureText(t).width + 18;
    const x = Math.round(cx - tw / 2);
    // A paper pill with a dot in the agent's colour.
    c.save();
    c.shadowColor = shade();
    c.shadowBlur = 6;
    c.shadowOffsetY = 1.5;
    c.fillStyle = paper();
    c.beginPath();
    c.roundRect(x, bottom + 2, tw, 15, 7.5);
    c.fill();
    c.restore();
    c.strokeStyle = outline();
    c.lineWidth = 1;
    c.beginPath();
    c.roundRect(x + 0.5, bottom + 2.5, tw - 1, 14, 7);
    c.stroke();
    c.fillStyle = MARKER_COLORS[camp] ?? "#8d7fb5";
    c.beginPath();
    c.arc(x + 7.5, bottom + 9.5, 2.5, 0, Math.PI * 2);
    c.fill();
    c.fillStyle = ink();
    c.textAlign = "center";
    c.textBaseline = "middle";
    c.fillText(t, cx + 4, bottom + 9.5);
  }
}
