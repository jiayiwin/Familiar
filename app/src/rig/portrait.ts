// What the windows share about the pet: which face goes with what the agent is
// doing, and a small idling pet for the windows that only show it (the room, the tray, the gallery).

import { loadRig, type RigExpr, type RigPet } from "./rig.js";

/** What the agent is up to, as the windows colour it. */
export type Mood = "work" | "wait" | "done" | "rest" | "night" | "worry";

const FACES: Record<Mood, RigExpr> = {
  work: "neutral",
  wait: "neutral",
  done: "happy",
  rest: "sleepy",
  night: "sleepy",
  worry: "worried",
};

/** The idle face for a mood; `state` is petd's own state, which knows when the pet is asleep. */
export function faceFor(mood: Mood, state: string): RigExpr {
  // Asleep is its own face: eyes shut, not just heavy.
  if (state === "sleeping" || state === "offline") return "asleep";
  return FACES[mood];
}

/** What a change of mood looks like: a hop for good news, ears up when it needs you. */
export function greet(pet: RigPet, mood: Mood): void {
  if (mood === "done") pet.react("cheer");
  else if (mood === "wait") pet.react("perk");
}

/**
 * The pet idling in `canvas`: fitted to it (with room for its ears to swing and
 * a hop) and standing on its bottom edge. The canvas takes its size from CSS.
 * `room`: the canvas is bigger than the pet (it has the room to jump about in):
 * fit it to a `box` [w, h] of CSS px instead, standing `floor` px above the
 * bottom edge.
 */
export async function mountPortrait(canvas: HTMLCanvasElement, base = "rig/zimeng/", room?: { box: [number, number]; floor: number }): Promise<RigPet> {
  const pet = await loadRig(base);
  const ctx = canvas.getContext("2d")!;
  const [x0, y0, x1, y1] = pet.rig.bbox;
  let last = 0;
  const frame = (t: number) => {
    // Taken off the page (another pet took the seat): this one stops.
    if (!canvas.isConnected) return;
    const dpr = window.devicePixelRatio || 1;
    const w = canvas.clientWidth;
    const h = canvas.clientHeight;
    if (canvas.width !== Math.round(w * dpr) || canvas.height !== Math.round(h * dpr)) {
      canvas.width = Math.round(w * dpr);
      canvas.height = Math.round(h * dpr);
    }
    pet.update(Math.min(0.1, (t - (last || t)) / 1000));
    last = t;
    const [fw, fh] = room?.box ?? [w, h];
    const s = Math.min(fw / ((x1 - x0) * 1.16), fh / ((y1 - y0) * 1.22));
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
    ctx.clearRect(0, 0, w, h);
    ctx.translate(w / 2 - pet.foot[0] * s, h - (room?.floor ?? 0) - 3 - pet.foot[1] * s);
    pet.draw(ctx, s, null);
    requestAnimationFrame(frame);
  };
  requestAnimationFrame(frame);
  return pet;
}
