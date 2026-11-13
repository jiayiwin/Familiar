// Playing with the pet in its room. Nothing here is a game with a score: it is
// the pet answering your hand, and it has the whole of the room to do it in.
//
//   - it watches the pointer: eyes, face and head turn to follow it;
//   - a click is a poke (four quick ones and it spins itself dizzy);
//   - a drag across it turns it round, as far and as fast as you drag;
//   - strokes over its head make it shut its eyes and smile;
//   - "逗它" hangs a toy from the pointer (the pet's own emblem on a string):
//     it runs after it along the floor, and when the toy is overhead it
//     crouches and jumps for it. If it gets its mouth on it in the air it
//     holds on, and you can swing it about until it lets go and drops. Held
//     too high for its legs, it jumps, misses, and says so;
//   - "转一圈" has it turn once on the spot;
//   - "接糖" is the one thing here that is a little game: for half a minute
//     sweets drift down from the top of the room, the pet runs where your
//     pointer is and a click makes it jump, and what it catches it eats.
//     Enough in one round and one goes into the jar (a couple a day at most).
//
// A sleepy pet wakes up to play and goes back to its own face afterwards.

import type { RigPet } from "./rig/rig.js";
import { drawTreat, type TreatLook } from "./treats";

export interface PlayOpts {
  /** The area that takes the pointer (the pet's canvas fills it), and a canvas over it for the toy. */
  stage: HTMLElement;
  canvas: HTMLCanvasElement;
  toy: HTMLCanvasElement;
  tease: HTMLButtonElement;
  spin: HTMLButtonElement;
  /** The box the pet is fitted to (CSS px), and how far above the bottom of the stage it stands. */
  box: [number, number];
  floor: number;
  /** The pet's emblem as a 24-unit path, and its colours. */
  motif: string;
  fill: string;
  line: string;
  /** How quick this pet is on its feet, and how high it jumps (1 = 紫梦). */
  pace: number;
  spring: number;
  /** Say one of its play lines ("play.catch", …). */
  say: (key: string) => void;
  /** 接糖: its button, the kinds of sweet there are, and what a round earns (the pet's line, or null). */
  game: HTMLButtonElement;
  looks: () => TreatLook[];
  earn: (caught: number) => Promise<string | null>;
  sayText: (text: string) => void;
}

/** One round of 接糖, and how many in it earn one for the jar. */
const ROUND_MS = 30_000;
export const CATCH_GOAL = 20;
interface Sweet { x: number; y: number; vy: number; sway: number; phase: number; rot: number; look: TreatLook; caught?: number; landed?: number }

const TAU = Math.PI * 2;
const GRAVITY = 1700; // CSS px/s²
const clamp = (v: number, a: number, b: number) => Math.min(b, Math.max(a, v));

export function mountPlay(pet: RigPet, o: PlayOpts): void {
  const [bx0, by0, bx1, by1] = pet.rig.bbox;
  const ptr = { x: 0, y: 0, in: false, down: false, moved: 0, startX: 0 };
  const toy = { on: false, x: 0, y: 0, vx: 0, vy: 0 };
  const turn = { angle: 0, to: 0, v: 0, dragging: false, turns: [] as number[] };
  const bursts: { x: number; y: number; t: number }[] = [];
  let pokes: number[] = [];
  let stroked = 0;
  let happyUntil = 0;
  let dizzyUntil = 0;
  let pettedAt = 0;
  let active = 0; // when the pointer last did something
  // Where it is: CSS px from the middle of the stage, and above the floor.
  let x = 0;
  let up = 0;
  let vy = 0;
  let stride = 0;
  /** "ground" | "crouch" (about to jump) | "air" | "hang" (its mouth on the toy). */
  let mode: "ground" | "crouch" | "air" | "hang" = "ground";
  let modeAt = 0;
  let holdFor = 0;
  let restUntil = 0; // no new jump before this
  let misses = 0;
  let grabbed = false; // whether this jump got the toy
  const game = { on: false, t0: 0, next: 0, caught: 0, sweets: [] as Sweet[], yumAt: 0 };

  /** CSS px per rig px, as the portrait fits it. */
  const scale = () => Math.min(o.box[0] / ((bx1 - bx0) * 1.16), o.box[1] / ((by1 - by0) * 1.22));
  /** Where the pet is in the stage's own CSS px: its middle, its feet, the top of its head, its mouth. */
  const where = () => {
    const s = scale();
    const st = o.stage.getBoundingClientRect();
    const tall = (by1 - by0) * s;
    const feet = st.height - o.floor - 3 - up;
    return { s, cx: st.width / 2 + x, feet, tall, top: feet - tall, head: feet - tall * 0.66, mouth: feet - tall * 0.5, half: ((bx1 - bx0) * s) / 2, width: st.width, height: st.height };
  };
  const onPet = (px: number, py: number) => {
    const w = where();
    return Math.abs(px - w.cx) < w.half && py > w.top - 6 && py < w.feet + 6;
  };
  const dizzy = (now: number) => {
    if (now < dizzyUntil) return;
    dizzyUntil = now + 2600;
    o.say("play.dizzy");
  };

  const at = (e: PointerEvent) => {
    const st = o.stage.getBoundingClientRect();
    return [e.clientX - st.left, e.clientY - st.top] as const;
  };
  o.stage.addEventListener("pointermove", (e) => {
    const [px, py] = at(e);
    const dx = px - ptr.x;
    ptr.x = px;
    ptr.y = py;
    ptr.in = true;
    active = performance.now();
    if (ptr.down) {
      ptr.moved += Math.abs(dx);
      // A drag across it turns it round.
      if (turn.dragging || (ptr.moved > 6 && onPet(ptr.startX, py))) {
        turn.dragging = true;
        turn.angle += dx * 0.03;
        turn.v = dx * 0.03 * 60;
      }
      return;
    }
    // Strokes over its head.
    const w = where();
    if (!toy.on && Math.abs(px - w.cx) < w.half * 0.9 && py > w.top - 8 && py < w.head + 14) stroked = Math.min(260, stroked + Math.abs(dx) + Math.abs(e.movementY) * 0.5);
  });
  o.stage.addEventListener("pointerdown", (e) => {
    if (e.button !== 0 || (e.target as HTMLElement).closest("button")) return;
    if (game.on) {
      // In the game a click is a jump.
      if (mode === "ground") {
        mode = "crouch";
        modeAt = performance.now();
      }
      return;
    }
    const [px] = at(e);
    Object.assign(ptr, { down: true, moved: 0, startX: px });
    o.stage.setPointerCapture(e.pointerId);
  });
  const release = (e: PointerEvent) => {
    if (!ptr.down) return;
    ptr.down = false;
    const now = performance.now();
    if (turn.dragging) {
      // Let go: it carries on a little, then comes to rest facing you.
      turn.dragging = false;
      turn.to = Math.round((turn.angle + turn.v * 0.3) / TAU) * TAU;
      if (Math.abs(turn.to - turn.angle) / TAU > 0.3) turn.turns.push(now);
    } else if (ptr.moved <= 6) {
      const [px, py] = at(e);
      if (onPet(px, py)) {
        pet.react("poke");
        pokes = pokes.filter((t) => now - t < 2400).concat(now);
        if (pokes.length >= 4) {
          // Poked too many times: round and round it goes.
          pokes = [];
          turn.to += TAU * 2;
          dizzy(now);
        }
      }
    }
  };
  o.stage.addEventListener("pointerup", release);
  o.stage.addEventListener("pointercancel", release);
  o.stage.addEventListener("pointerleave", () => (ptr.in = false));

  o.tease.addEventListener("click", () => {
    if (game.on) return;
    toy.on = !toy.on;
    o.tease.setAttribute("aria-pressed", String(toy.on));
    o.stage.classList.toggle("teasing", toy.on);
    if (toy.on) {
      const w = where();
      Object.assign(toy, { x: w.width - 40, y: 60, vx: 0, vy: 0 });
      misses = 0;
      o.say("play.toy");
      active = performance.now();
    }
  });
  const endGame = () => {
    game.on = false;
    o.game.setAttribute("aria-pressed", "false");
    o.stage.classList.remove("gaming");
    const caught = game.caught;
    o.say("play.over");
    happyUntil = performance.now() + 1500;
    if (caught >= CATCH_GOAL) void o.earn(caught).then((text) => text && window.setTimeout(() => o.sayText(text), 3000));
  };
  o.game.addEventListener("click", () => {
    if (game.on) return endGame();
    toy.on = false;
    o.tease.setAttribute("aria-pressed", "false");
    o.stage.classList.remove("teasing");
    const now = performance.now();
    Object.assign(game, { on: true, t0: now, next: now + 900, caught: 0, sweets: [], yumAt: now });
    o.game.setAttribute("aria-pressed", "true");
    o.stage.classList.add("gaming");
    o.say("play.game");
    active = now;
  });
  o.spin.addEventListener("click", () => {
    const now = performance.now();
    turn.to += TAU;
    turn.turns = turn.turns.filter((t) => now - t < 4000).concat(now);
    if (turn.turns.length >= 4) {
      turn.turns = [];
      dizzy(now);
    } else o.say("play.spin");
    active = now;
  });

  const ctx = o.toy.getContext("2d")!;
  const emblem = new Path2D(o.motif);
  let last = 0;
  const frame = (now: number) => {
    if (!o.canvas.isConnected) return;
    requestAnimationFrame(frame);
    const dt = Math.min(0.05, (now - (last || now)) / 1000);
    last = now;
    let w = where();
    const playing = now - active < 5000 || toy.on || game.on || mode !== "ground";
    o.stage.classList.toggle("playing", playing);
    const pl = pet.play;
    const reach = w.width / 2 - w.half - 4;

    // ---- the toy on its string: it hangs from the pointer and swings behind it.
    // With the pet hanging on it, it is heavy: it sags, and swings slowly.
    const hangLen = 44;
    const ax = ptr.in ? ptr.x : w.width - 40;
    const ay = ptr.in ? ptr.y : 18;
    if (toy.on) {
      const heavy = mode === "hang";
      const k = heavy ? 30 : 46;
      toy.vx += ((ax - toy.x) * k - toy.vx * (heavy ? 5 : 6.5)) * dt;
      toy.vy += ((ay + hangLen - toy.y) * k - toy.vy * (heavy ? 5 : 6.5) + (heavy ? 900 : 300)) * dt;
      toy.x += toy.vx * dt;
      toy.y += toy.vy * dt;
    } else if (mode === "hang") mode = "air"; // the toy was put away under it

    // ---- chasing the toy: along the floor, and up after it
    const chasing = toy.on && ptr.in;
    const dx = toy.x - w.cx;
    const above = w.top - toy.y; // how far over its head the toy is
    // What it catches with is its mouth, a little under the middle of its face.
    const grab = () => Math.hypot(toy.x - w.cx, toy.y - (w.top + w.tall * 0.4)) < 38;
    if (mode === "hang") {
      // Its mouth is on the toy: where the toy goes, it goes.
      x = clamp(toy.x - w.width / 2, -reach, reach);
      up = Math.max(0, w.height - o.floor - 3 - (toy.y + w.tall * 0.6));
      vy = 0;
      if (now - modeAt > holdFor || !chasing) {
        // It lets go, and the toy springs back up its string.
        mode = up > 2 ? "air" : "ground";
        restUntil = now + 900;
        toy.vy = -420;
        toy.vx += (toy.x < ax ? 1 : -1) * 160;
      }
    } else if (mode === "air") {
      vy -= GRAVITY * dt;
      up += vy * dt;
      // A little steering in the air, toward the toy.
      if (chasing) x = clamp(x + clamp(dx, -150 * o.pace * dt, 150 * o.pace * dt), -reach, reach);
      else if (game.on && ptr.in) x = clamp(x + clamp(ptr.x - w.cx, -170 * o.pace * dt, 170 * o.pace * dt), -reach, reach);
      if (chasing && !grabbed && grab()) {
        grabbed = true;
        misses = 0;
        mode = "hang";
        modeAt = now;
        holdFor = 900 + Math.random() * 900;
        happyUntil = now + holdFor + 600;
        bursts.push({ x: toy.x, y: toy.y, t: now });
        o.say("play.catch");
      } else if (up <= 0) {
        // Down again: a squash as it lands.
        up = 0;
        mode = "ground";
        restUntil = now + (grabbed ? 700 : 450);
        pet.react("poke");
        if (!grabbed && chasing && ++misses >= 2) {
          misses = 0;
          o.say("play.miss");
        }
      }
    } else if (mode === "crouch") {
      // A beat to gather itself, then up: as hard as the toy is high, as far as its legs allow.
      if (now - modeAt > 150) {
        const most = Math.min(w.height - o.floor - w.tall - 14, 250 * o.spring);
        const need = game.on ? Math.min(most, 170 * o.spring) : clamp(above + w.tall * 0.45, 30, most);
        vy = Math.sqrt(2 * GRAVITY * need);
        mode = "air";
        grabbed = false;
      }
    } else {
      // On the floor: run to stand under it; jump when it is overhead; take it at once when it is in reach.
      // In the game it runs to stand under your pointer (and stays put when the pointer is outside).
      const want = chasing ? clamp(toy.x - w.width / 2, -reach, reach) : game.on ? (ptr.in ? clamp(ptr.x - w.width / 2, -reach, reach) : x) : 0;
      const step = clamp(want - x, -280 * o.pace * dt, 280 * o.pace * dt);
      x += step;
      stride += Math.abs(step) * 0.075;
      // It hops as it runs, and settles when it stops.
      up = Math.abs(step) > 0.2 ? 12 * Math.abs(Math.sin(stride)) * w.s * 2.4 : up * (1 - Math.min(1, dt * 12));
      if (chasing && now > restUntil) {
        if (grab()) {
          grabbed = true;
          misses = 0;
          mode = "hang";
          modeAt = now;
          holdFor = 700 + Math.random() * 700;
          happyUntil = now + holdFor + 600;
          bursts.push({ x: toy.x, y: toy.y, t: now });
          pet.react("cheer");
          o.say("play.catch");
        } else if (above > 14 && Math.abs(dx) < 54) {
          mode = "crouch";
          modeAt = now;
        }
      }
    }
    w = where();
    pl.x = x / w.s;
    // Crouching, it sinks a little; in the air it is as high as it is.
    pl.hop = mode === "crouch" ? -7 * Math.min(1, (now - modeAt) / 150) : up / w.s;

    // ---- 接糖: sweets drift down; the ones that reach its mouth it eats
    if (game.on) {
      const t = now - game.t0;
      if (t >= ROUND_MS) endGame();
      else if (now >= game.next) {
        // A little quicker as the round goes on.
        game.next = now + 950 - 420 * (t / ROUND_MS) + Math.random() * 300;
        const kinds = o.looks();
        const look = kinds.length ? kinds[Math.floor(Math.random() * kinds.length)] : {};
        game.sweets.push({ x: 30 + Math.random() * (w.width - 60), y: -18, vy: 95 + Math.random() * 45 + 70 * (t / ROUND_MS), sway: 10 + Math.random() * 18, phase: Math.random() * TAU, rot: (Math.random() - 0.5) * 0.6, look });
      }
    }
    const mouthX = w.cx, mouthY = w.top + w.tall * 0.42;
    const ground = w.height - o.floor - 6;
    for (let i = game.sweets.length - 1; i >= 0; i--) {
      const s = game.sweets[i];
      if (s.caught || s.landed) {
        if (now - (s.caught ?? s.landed!) > (s.caught ? 260 : 700)) game.sweets.splice(i, 1);
        continue;
      }
      s.y += s.vy * dt;
      const sx = s.x + Math.sin(now / 520 + s.phase) * s.sway;
      if (game.on && Math.hypot(sx - mouthX, s.y - mouthY) < Math.max(26, w.half * 0.42)) {
        s.caught = now;
        s.x = sx;
        game.caught++;
        happyUntil = now + 700;
        bursts.push({ x: sx, y: s.y, t: now });
        if (now - game.yumAt > 6500 && Math.random() < 0.5) {
          game.yumAt = now;
          o.say("play.yum");
        }
      } else if (s.y >= ground) {
        s.y = ground;
        s.x = sx;
        s.landed = now;
      }
    }

    // ---- what it watches: a sweet coming down in the game, the toy if there is one, else your pointer
    const falling = game.on ? game.sweets.filter((s) => !s.caught && !s.landed).sort((a, b) => b.y - a.y)[0] : undefined;
    const tx = falling ? falling.x : toy.on ? toy.x : ptr.x;
    const ty = falling ? falling.y : toy.on ? toy.y : ptr.y;
    if (playing && (falling || toy.on || ptr.in)) {
      const lx = tx - w.cx;
      const ly = ty - w.head;
      pl.yaw = clamp(lx / 150, -1, 1) * 0.8;
      pl.pitch = clamp(ly / 160, -1, 1) * 0.7;
      pl.look = [clamp(lx / 40, -3.5, 3.5), clamp(ly / 50, -2.5, 2.5)];
      pl.head = clamp(lx / 24, -7, 7);
    } else Object.assign(pl, { yaw: null, pitch: null, look: null, head: null });

    // ---- turning round
    if (!turn.dragging) {
      turn.v += ((turn.to - turn.angle) * 34 - turn.v * 9.5) * dt;
      turn.angle += turn.v * dt;
      if (Math.abs(turn.to - turn.angle) < 0.002 && Math.abs(turn.v) < 0.02) {
        // At rest facing you: forget how many times round it has been.
        turn.angle = turn.to = turn.v = 0;
      }
    }
    pl.twirl = turn.angle;
    // Dizzy, it sways until its head clears; hanging from the toy, it swings with it.
    pl.rot = now < dizzyUntil ? 9 * Math.sin(now / 130) * ((dizzyUntil - now) / 2600) : mode === "hang" ? clamp(-toy.vx / 26, -16, 16) : 0;

    // ---- strokes
    stroked = Math.max(0, stroked - 150 * dt);
    if (stroked > 150) {
      happyUntil = now + 1200;
      if (now - pettedAt > 20_000) {
        pettedAt = now;
        o.say("play.pet");
      }
    }

    // ---- its face: startled while dizzy, smiling when pleased, awake while you play
    pl.face = now < dizzyUntil ? "surprised" : now < happyUntil ? "happy" : playing && (pet.expr === "asleep" || pet.expr === "sleepy") ? "neutral" : null;

    // ---- draw the toy
    const dpr = window.devicePixelRatio || 1;
    const cw = o.toy.clientWidth;
    const ch = o.toy.clientHeight;
    if (o.toy.width !== Math.round(cw * dpr) || o.toy.height !== Math.round(ch * dpr)) {
      o.toy.width = Math.round(cw * dpr);
      o.toy.height = Math.round(ch * dpr);
    }
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
    ctx.clearRect(0, 0, cw, ch);
    for (let i = bursts.length - 1; i >= 0; i--) {
      const b = bursts[i];
      const p = (now - b.t) / 520;
      if (p >= 1) {
        bursts.splice(i, 1);
        continue;
      }
      ctx.beginPath();
      ctx.arc(b.x, b.y, 10 + 30 * p, 0, 7);
      ctx.lineWidth = 3 * (1 - p);
      ctx.strokeStyle = o.line;
      ctx.globalAlpha = 1 - p;
      ctx.stroke();
      ctx.globalAlpha = 1;
    }
    for (const s of game.sweets) {
      const sx = s.caught || s.landed ? s.x : s.x + Math.sin(now / 520 + s.phase) * s.sway;
      ctx.save();
      if (s.caught) {
        // Into its mouth: smaller and gone.
        const k = Math.min(1, (now - s.caught) / 260);
        ctx.globalAlpha = 1 - k;
        drawTreat(ctx, s.look, sx + (mouthX - sx) * k, s.y + (mouthY - s.y) * k, 0.9 * (1 - k * 0.7));
      } else if (s.landed) {
        // Dropped: it lies there a moment and fades.
        ctx.globalAlpha = 1 - Math.min(1, (now - s.landed) / 700);
        drawTreat(ctx, s.look, sx, s.y, 0.8);
      } else {
        ctx.translate(sx, s.y);
        ctx.rotate(s.rot + Math.sin(now / 400 + s.phase) * 0.2);
        drawTreat(ctx, s.look, 0, 0, 0.9);
      }
      ctx.restore();
    }
    if (game.on) {
      // The round: seconds left and how many it has eaten, on a small pill at the top.
      const left = Math.max(0, Math.ceil((ROUND_MS - (now - game.t0)) / 1000));
      const text = (document.documentElement.lang || "zh").startsWith("en") ? `${left}s · caught ${game.caught}` : `还剩 ${left} 秒 · 接住 ${game.caught}`;
      ctx.font = "700 13px 'Segoe UI', 'Microsoft YaHei UI', sans-serif";
      const tw = ctx.measureText(text).width + 28;
      const px = cw / 2 - tw / 2, py = 10;
      ctx.fillStyle = "rgba(255, 250, 242, 0.92)";
      ctx.strokeStyle = o.line;
      ctx.lineWidth = 1;
      ctx.beginPath();
      ctx.roundRect(px, py, tw, 28, 14);
      ctx.fill();
      ctx.stroke();
      // How far through the round, along its bottom edge.
      ctx.fillStyle = o.fill;
      ctx.beginPath();
      ctx.roundRect(px + 8, py + 23, (tw - 16) * Math.min(1, (now - game.t0) / ROUND_MS), 2.5, 1.25);
      ctx.fill();
      ctx.fillStyle = getComputedStyle(o.stage).getPropertyValue("--ink").trim() || "#5b4a57";
      ctx.textAlign = "center";
      ctx.textBaseline = "middle";
      ctx.fillText(text, cw / 2, py + 13);
      ctx.textAlign = "start";
      ctx.textBaseline = "alphabetic";
    }
    if (!toy.on) return;
    ctx.lineCap = "round";
    if (ptr.in) {
      // the wand in your hand
      ctx.beginPath();
      ctx.moveTo(ax + 20, ay + 24);
      ctx.lineTo(ax, ay);
      ctx.lineWidth = 4;
      ctx.strokeStyle = o.line;
      ctx.stroke();
    }
    // the string: slack when the toy is close, taut with the pet on it
    ctx.beginPath();
    ctx.moveTo(ax, ay);
    ctx.quadraticCurveTo((ax + toy.x) / 2, mode === "hang" ? (ay + toy.y) / 2 : Math.max(ay, toy.y) + 8, toy.x, toy.y);
    ctx.lineWidth = 1.4;
    ctx.strokeStyle = o.line;
    ctx.stroke();
    // the emblem on its end, tipping with the swing
    ctx.save();
    ctx.translate(toy.x, toy.y);
    ctx.rotate(clamp(toy.vx / 500, -0.6, 0.6));
    ctx.scale(1.5, 1.5);
    ctx.translate(-12, -12);
    ctx.fillStyle = o.fill;
    ctx.fill(emblem);
    ctx.lineWidth = 1.3;
    ctx.lineJoin = "round";
    ctx.strokeStyle = o.line;
    ctx.stroke(emblem);
    ctx.restore();
  };
  requestAnimationFrame(frame);
}
