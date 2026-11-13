// Layered-rig runtime: canvas 2D. The art is cut into layers that overlap at the
// joints; everything here is transforms on those layers plus a few vector face
// parts, so the painted texture is never redrawn.
// Two anatomies, by `rig.kind`: "lop" (紫梦: one body, two long hanging ears it
// plays the piano with), and everything else, which stands upright: a head
// that tilts on a small body, plus whatever that pet has. "cat" (麻薯) has ears
// that flick and a tail it bows a cello with; "blob" (泡泡) a curl on its head
// and a flute whose notes leave it as bubbles; "mushroom" (菇菇) a cap that
// rocks and a small xylophone (the spots on its cap light with the bars);
// "sheep" (茸茸) two hanging ears and a harp it stands beside. An upright rig lists its
// layers back to front in `order`.
// Lives in app/src/rig/; design/pet-art/preview keeps a copy (tools/sync_app.py).

const DEG = Math.PI / 180;
const clamp = (v, a, b) => Math.min(b, Math.max(a, v));
const easeOut = (p) => 1 - (1 - p) ** 3;

class Spring {
  constructor(k, c) {
    this.k = k;
    this.c = c;
    this.x = 0;
    this.v = 0;
  }
  step(target, dt) {
    this.v += (this.k * (target - this.x) - this.c * this.v) * dt;
    this.x += this.v * dt;
  }
}

// ---- Dance: one move per bar (4 beats), eight bars to a round ----
// Each move returns offsets from the rest pose. `yaw` / `pitch` turn the head by
// sliding the face and ears against the body; `scale` is distance to the viewer;
// `twirl` is a turn about the vertical axis (the back shows on the far half);
// `shear` leans the body sideways from the feet; `tail` wags it (degrees).

const sin = Math.sin, PI = Math.PI;

function hop(lb, h) {
  const p = lb - Math.floor(lb);
  const air = 1 - (2 * p - 1) ** 2;
  const land = Math.exp(-p * 9);
  const k = h / 20;
  return { y: -h * air, sy: 1 - 0.085 * land * k + 0.035 * air * k, sx: 1 + 0.07 * land * k - 0.025 * air * k };
}

const MOVES = {
  bounce: (lb) => ({ ...hop(lb, Math.floor(lb) === 3 ? 29 : 20), rot: 6.5 * sin(PI * lb), x: 9 * sin(PI * lb) }),
  sidestep: (lb) => {
    const dir = Math.cos((PI * lb) / 2);
    return { ...hop(lb, 9), x: 34 * sin((PI * lb) / 2), rot: 4.5 * dir, yaw: 0.85 * dir };
  },
  earWave: (lb) => {
    const s = sin(PI * lb);
    return { ...hop(lb, 8), rot: 3 * s, pitch: -0.35, earL: 38 * Math.max(0, s) ** 0.7, earR: 38 * Math.max(0, -s) ** 0.7 };
  },
  twist: (lb) => {
    const s = sin(PI * lb);
    const yaw = Math.sign(s) * Math.abs(s) ** 0.45;
    return { ...hop(lb, 6), yaw, rot: 2.5 * yaw, x: 6 * yaw };
  },
  wiggle: (lb) => {
    const s = sin(2 * PI * lb);
    return { y: 5, sy: 0.965 - 0.02 * Math.abs(s), sx: 1.03, rot: 4.2 * s, x: 6 * s, pitch: 0.45 };
  },
  pop: (lb) => {
    const s = sin(PI * lb);
    return { ...hop(lb, 10), scale: 1 + 0.1 * s, pitch: -0.3 * s, earL: 10 * Math.max(0, s), earR: 10 * Math.max(0, s) };
  },
  jumpTwirl: (lb) => {
    if (lb < 1) {
      const a = easeOut(lb);
      return { y: 7 * a, sy: 1 - 0.1 * a, sx: 1 + 0.08 * a, pitch: 0.3 * a };
    }
    if (lb < 3) {
      const p = (lb - 1) / 2;
      const air = 1 - (2 * p - 1) ** 2;
      const tw = clamp((p - 0.12) / 0.7, 0, 1);
      return { y: -50 * air, sy: 1 + 0.07 * air, sx: 1 - 0.05 * air, twirl: 2 * PI * tw * tw * (3 - 2 * tw), earL: 16, earR: 16 };
    }
    const p = lb - 3;
    const land = Math.exp(-p * 7);
    const up = sin(PI * Math.min(p * 1.2, 1));
    return { sy: 1 - 0.13 * land, sx: 1 + 0.1 * land, pitch: -0.3 * up, earL: 28 * up, earR: 28 * up };
  },
  // Two bars with its back to the viewer: hop round and shake the tail...
  backWag: (lb) => {
    if (lb < 0.75) {
      const p = lb / 0.75;
      const air = 1 - (2 * p - 1) ** 2;
      return { y: -22 * air, sy: 1 + 0.04 * air, sx: 1 - 0.03 * air, twirl: PI * p * p * (3 - 2 * p), earL: 14 * air, earR: 14 * air };
    }
    const s = sin(2 * PI * lb);
    // The rump swings while the head stays put: lean from the feet, slide back under it.
    return { twirl: PI, y: 4, sy: 0.97 - 0.015 * Math.abs(s), sx: 1.03, shear: 0.034 * s, x: 10 * s, tail: 26 * s };
  },
  // ...then swing the ears out one at a time, and hop back round on the last beat.
  backEars: (lb) => {
    if (lb >= 3.1) {
      const p = (lb - 3.1) / 0.9;
      const air = 1 - (2 * p - 1) ** 2;
      const tw = clamp(p / 0.85, 0, 1);
      return { y: -26 * air, sy: 1 + 0.05 * air, sx: 1 - 0.03 * air, twirl: PI * (1 + tw * tw * (3 - 2 * tw)), earL: 16 * air, earR: 16 * air };
    }
    const s = sin(PI * lb);
    return { ...hop(lb, 9), twirl: PI, rot: 3 * s, earL: 42 * Math.max(0, s) ** 0.7, earR: 42 * Math.max(0, -s) ** 0.7, tail: -14 * s };
  },
};
const ROUND = [
  "bounce", "sidestep", "earWave", "twist",
  "backWag", "backEars", "bounce", "wiggle",
  "pop", "sidestep", "earWave", "jumpTwirl",
];

// One-off reactions on top of idling. `p` runs 0..1 over `dur` seconds.
const REACTIONS = {
  // Pressed with a finger: squashes and springs back.
  poke: {
    dur: 0.6,
    pose: (p) => {
      const e = Math.exp(-p * 5) * Math.cos(p * 13);
      return { y: 4 * e, sy: 1 - 0.1 * e, sx: 1 + 0.08 * e, earL: 9 * e, earR: 9 * e, head: 6 * e };
    },
  },
  // Noticed something: ears up, a small lift, a little "oh".
  perk: {
    dur: 0.9,
    face: "surprised",
    pose: (p) => {
      const s = sin(PI * p) ** 0.6;
      return { y: -5 * s, earL: 24 * s, earR: 24 * s, pitch: -0.3 * s, head: -7 * s };
    },
  },
  // Pleased: two quick hops, smiling.
  cheer: { dur: 1.3, face: "happy", pose: (p) => ({ ...hop(Math.min(p * 2, 1.999), 14), earL: 12, earR: 12, head: 5 * sin(4 * PI * p), tail: 24 * sin(6 * PI * p) }) },
};

// The cat's round: the same floor pattern, danced with what a cat has. `head`
// tilts it on the neck (degrees), `headY` nods it, `tail` swishes, the ears flick.
const MOVES_CAT = {
  bounce: (lb) => {
    const s = sin(PI * lb);
    return { ...hop(lb, Math.floor(lb) === 3 ? 26 : 18), rot: 4 * s, x: 7 * s, head: -7 * s, tail: 22 * s };
  },
  sway: (lb) => {
    const dir = Math.cos((PI * lb) / 2);
    return { ...hop(lb, 7), x: 30 * sin((PI * lb) / 2), rot: 3.5 * dir, head: 8 * dir, yaw: 0.7 * dir, tail: -26 * dir };
  },
  nod: (lb) => {
    const dip = Math.exp(-(lb - Math.floor(lb)) * 6);
    return { y: 3 * dip, sy: 1 - 0.03 * dip, sx: 1 + 0.025 * dip, headY: 7 * dip, pitch: 0.5 * dip, head: 3 * sin((PI * lb) / 2), tail: 14 * sin(PI * lb), earL: 8 * dip, earR: 8 * dip };
  },
  tailSwish: (lb) => {
    const s = sin(PI * lb);
    return { y: 4, sy: 0.97, sx: 1.03, tail: 36 * s, head: -5 * s, shear: 0.02 * s, earL: 10 * Math.max(0, s), earR: 10 * Math.max(0, -s), pitch: 0.2 };
  },
  earFlick: (lb) => {
    const s = sin(PI * lb);
    return { ...hop(lb, 8), rot: 2.5 * s, head: 4 * s, earL: 14 * Math.max(0, s) ** 0.7, earR: 14 * Math.max(0, -s) ** 0.7, tail: -12 * s };
  },
  pop: (lb) => {
    const s = sin(PI * lb);
    return { ...hop(lb, 10), scale: 1 + 0.1 * s, pitch: -0.3 * s, earL: 10 * Math.max(0, s), earR: 10 * Math.max(0, s), headY: -3 * Math.max(0, s), tail: 20 * s };
  },
  jumpTwirl: (lb) => ({ ...MOVES.jumpTwirl(lb), tail: 20 }),
  backWag: (lb) => ({ ...MOVES.backWag(lb), head: lb < 0.75 ? 0 : -4 * sin(2 * PI * lb) }),
  // Back still turned: the head tips side to side, one ear at a time; hop back round on the last beat.
  backPeek: (lb) => {
    if (lb >= 3.1) return { ...MOVES.backEars(lb), earL: 0, earR: 0 };
    const s = sin(PI * lb);
    return { ...hop(lb, 8), twirl: PI, head: 9 * s, earL: 12 * Math.max(0, s), earR: 12 * Math.max(0, -s), tail: -18 * s };
  },
};
const ROUND_CAT = [
  "bounce", "sway", "nod", "tailSwish",
  "backWag", "backPeek", "bounce", "earFlick",
  "pop", "sway", "tailSwish", "jumpTwirl",
];
// The other three dance with what they have. `top` swings what rides on the
// head (the curl, the cap) directly, in degrees.
const MOVES_BLOB = {
  ...MOVES_CAT,
  // A drop of water: it wobbles like jelly, and the curl on top whips about.
  jelly: (lb) => {
    const s = sin(2 * PI * lb);
    return { y: 3 * Math.max(0, -s), sy: 1 + 0.09 * s, sx: 1 - 0.08 * s, head: 3 * s, top: -22 * s, pitch: 0.15 * s };
  },
  // Up on its toes and down, a beat at a time, the curl springing after it.
  boing: (lb) => {
    const h = hop(lb, 22);
    return { ...h, sy: h.sy * (1 + 0.05 * sin(PI * lb)), top: 24 * Math.cos(2 * PI * lb), rot: 3 * sin(PI * lb) };
  },
  // A slow sway, half a beat behind itself.
  drift: (lb) => {
    const s = sin((PI * (lb - 0.5)) / 2);
    return { x: 26 * s, rot: 5 * s, yaw: 0.6 * s, head: -6 * s, top: 18 * s, y: 2 };
  },
};
const ROUND_BLOB = ["boing", "drift", "jelly", "nod", "backWag", "backPeek", "boing", "jelly", "pop", "drift", "jelly", "jumpTwirl"];

const MOVES_MUSHROOM = {
  ...MOVES_CAT,
  // The cap tips one way and the other on the beat, the body bobbing under it.
  capTip: (lb) => {
    const s = sin(PI * lb);
    return { ...hop(lb, 7), top: 8 * s, head: -5 * s, rot: 2 * s, x: 8 * s };
  },
  // Shy: it ducks under its cap, then peeks out on the last beat.
  duck: (lb) => {
    const down = lb < 3 ? Math.min(1, lb / 0.5) : Math.max(0, 1 - (lb - 3) / 0.5);
    return { y: 9 * down, sy: 1 - 0.09 * down, sx: 1 + 0.06 * down, headY: 6 * down, pitch: 0.6 * down, top: lb > 3 ? 6 * sin(PI * (lb - 3) * 2) : 0 };
  },
  // A little spin of the cap alone: the body stays, the cap goes round in a wobble.
  twirlCap: (lb) => {
    const s = sin(2 * PI * lb);
    return { ...hop(lb, 5), top: 8 * s, sx: 1 + 0.02 * s, head: 3 * Math.cos(2 * PI * lb) };
  },
};
const ROUND_MUSHROOM = ["capTip", "nod", "duck", "twirlCap", "backWag", "backPeek", "capTip", "bounce", "pop", "duck", "twirlCap", "jumpTwirl"];

const MOVES_SHEEP = {
  ...MOVES_CAT,
  // Its long ears swing out on the beat, one side and then the other.
  earSwing: (lb) => {
    const s = sin(PI * lb);
    return { ...hop(lb, 9), rot: 3.5 * s, head: 5 * s, earL: 34 * Math.max(0, s) ** 0.7, earR: 34 * Math.max(0, -s) ** 0.7 };
  },
  // It puffs up, round and woolly, and lets the air out.
  fluff: (lb) => {
    const p = lb - Math.floor(lb);
    const puff = Math.exp(-p * 4) * (Math.floor(lb) % 2 ? 1 : 0.7);
    return { scale: 1 + 0.08 * puff, sx: 1 + 0.05 * puff, sy: 1 + 0.03 * puff, earL: 20 * puff, earR: 20 * puff, y: -3 * puff };
  },
  // A slow, unhurried rock from foot to foot.
  rock: (lb) => {
    const s = sin((PI * lb) / 2);
    return { x: 16 * s, rot: 6 * s, shear: 0.02 * s, head: -4 * s, earL: 10 * Math.max(0, -s), earR: 10 * Math.max(0, s), y: 2 };
  },
};
const ROUND_SHEEP = ["rock", "earSwing", "nod", "fluff", "backWag", "backPeek", "rock", "earSwing", "pop", "fluff", "sway", "jumpTwirl"];

const CHOREO = { lop: [MOVES, ROUND], cat: [MOVES_CAT, ROUND_CAT], blob: [MOVES_BLOB, ROUND_BLOB], mushroom: [MOVES_MUSHROOM, ROUND_MUSHROOM], sheep: [MOVES_SHEEP, ROUND_SHEEP] };

/** Each body has its own round; one we have no round for dances the cat's. */
const choreoOf = (kind) => CHOREO[kind] ?? (!kind || kind === "lop" ? CHOREO.lop : CHOREO.cat);
/** The strings of the harp, and the bars of the xylophone. */
const STRINGS = 7;
const BARS = 8;
/** Where the flute ends, from the mouth: its notes leave from there as bubbles. */
const FLUTE = { len: 150, tilt: 8, dx: 6, dy: 9 };
const UPRIGHT_ORDER = ["tail", "body", "ear_l", "ear_r", "head", "bow", "eye_l", "eye_r", "mouth"];
const PLAYED = { flute: 1, xylophone: 1, harp: 1 };

export function danceMoveAt(beats, kind = "lop") {
  const round = choreoOf(kind)[1];
  return round[Math.floor(Math.max(0, beats) / 4) % round.length];
}

export async function loadRig(base) {
  // Revalidate everything: a re-cut rig must never be mixed with cached layers.
  const get = (file) => fetch(base + file, { cache: "no-cache" });
  const rig = await (await get("rig.json")).json();
  // The back view is optional: without it a turn shows the front with no face.
  const back = await get("back.json").then((r) => (r.ok ? r.json() : null), () => null);
  if (back) rig.back = back;
  // So are the painted closed eyes and smile: without them those are drawn as plain strokes.
  const faces = await get("faces.json").then((r) => (r.ok ? r.json() : null), () => null);
  if (faces) rig.faces = faces;
  const images = {};
  await Promise.all(
    [...rig.layers, ...(back?.layers ?? []), ...(faces?.layers ?? [])].map(
      async (l) => {
        const blob = await (await get(l.file)).blob();
        const im = new Image();
        images[l.name] = im;
        await new Promise((ok, fail) => {
          im.onload = ok;
          im.onerror = fail;
          im.src = URL.createObjectURL(blob);
        });
      },
    ),
  );
  return new RigPet(rig, images);
}

export class RigPet {
  constructor(rig, images) {
    this.rig = rig;
    this.img = images;
    this.layer = Object.fromEntries([...rig.layers, ...(rig.back?.layers ?? []), ...(rig.faces?.layers ?? [])].map((l) => [l.name, l]));
    /** The anatomy: "lop", or one of the upright ones ("cat", "blob", "mushroom", "sheep"). */
    this.kind = rig.kind ?? "lop";
    this.upright = this.kind !== "lop";
    // What rides on the head and swings on its own: a curl (springy), a mushroom cap (heavy).
    this.top = this.kind === "mushroom" ? new Spring(150, 11) : new Spring(70, 5.5);
    this.topY = new Spring(260, 16);
    // Notes waiting to be played (flute, xylophone, harp), and what they leave behind.
    this.notes = [];
    this.pulse = 0;
    this.glow = [];
    this.bubbles = [];
    this.propA = 0;
    // The instrument last played (it fades out after the piece), which of its bars or strings ring, and the two mallets.
    this.inst = null;
    // Driven from outside while it idles (played with in its room, clinging to a screen edge):
    // where it stands and how high (rig px), a lean (degrees), a turn about itself (radians),
    // where its face turns (yaw / pitch, -1..1) and its head tips (degrees), what it looks at
    // ([x, y] in px of eye travel) and the face it makes. All null or 0 when left alone.
    this.play = { x: 0, hop: 0, rot: 0, twirl: 0, yaw: null, pitch: null, head: null, look: null, face: null };
    this.bars = [];
    this.mallets = [{ at: 1.5, to: 1.5, hit: 0 }, { at: 5.5, to: 5.5, hit: 0 }];
    // The harp is plucked with the ear on its side: how far that ear has reached into the strings (0..1), and the glints a plucked string throws off.
    this.reach = 0;
    this.glints = [];
    /** The pale spots on a mushroom's cap, [x, y, r] each: its xylophone bars. */
    this.spots = rig.spots ?? [];
    this.mode = "idle";
    this.expr = "neutral";
    this.t = 0;
    this.root = { x: 0, y: 0, rot: 0, sx: 1, sy: 1, scale: 1, yaw: 0, pitch: 0, twirl: 0, shear: 0, headY: 0 };
    // A cat's head tilts on its neck, its bow tie jiggles, and its tail is a bow.
    this.head = new Spring(120, 12);
    this.tilt = { to: 0, next: 3 };
    this.bowTie = new Spring(160, 9);
    this.strokes = [];
    this.bowPos = 0;
    this.tail = new Spring(210, 11);
    this.gest = null;
    this.back = 0; // 0 facing you .. 1 back turned (idle only)
    this.backTo = 0;
    this.clench = 0;
    this.wasClenched = false;
    this.prevY = 0;
    this.ears = {
      l: { sway: new Spring(90, 8.5), spread: new Spring(110, 10), angle: 0, stretch: 1, taps: [] },
      r: { sway: new Spring(76, 8), spread: new Spring(100, 9.5), angle: 0, stretch: 1, taps: [] },
    };
    this.blink = { next: 1.5, t: -1 };
    this.open = 1;
    this.look = { x: new Spring(40, 11), y: new Spring(40, 11), tx: 0, ty: 0, next: 2 };
    this.twitch = 4;
    const [x0, , x1, y1] = rig.bbox;
    // Where it stands: under the middle of its body (the cat's tail sticks out to one side).
    const body = this.upright ? this.layer.body : null;
    this.foot = [body ? body.x + body.w / 2 : (x0 + x1) / 2 + 2, y1 - 4];
  }

  setMode(mode) {
    this.mode = mode;
    this.gest = null;
    this.ears.l.taps = [];
    this.ears.r.taps = [];
    this.strokes = [];
    this.notes = [];
    if (PLAYED[mode]) this.inst = mode;
  }

  /** Turn its back to the viewer while idling; `clench` 0..1 squeezes and lifts the tail (a kegel set). */
  setBack(on, clench = 0) {
    this.backTo = on ? 1 : 0;
    this.clench = on ? clench : 0;
  }

  /** Play a one-off reaction ("poke", "perk", "cheer") over the idle pose. */
  react(name) {
    if (REACTIONS[name] && this.mode === "idle") this.gest = { name, t: 0 };
  }

  /** An ear strike landing at music time `at` (seconds). */
  tap(side, at) {
    this.ears[side].taps.push(at);
  }

  /** One stroke of the bow (the tail), from music time `at` for `dur` seconds. Strokes alternate direction. */
  bow(at, dur) {
    this.strokes.push({ at, dur, dir: this.strokes.length % 2 ? -1 : 1 });
  }

  /** One note of its own instrument at music time `at` (seconds), `dur` long; `i` picks which bubble size, spot or string. */
  note(at, dur, i = 0) {
    this.notes.push({ at, dur, i });
  }

  /** A note sounds now: a bubble leaves its mouth, a spot on its cap lights, a string on its chest rings. */
  strike(n) {
    this.pulse = 1;
    if (this.mode === "flute") {
      // The note leaves the far end of the flute as a bubble.
      const [mx, my] = this.rig.anchors.mouth;
      const ex = mx + this.root.x + FLUTE.dx - FLUTE.len * Math.cos(FLUTE.tilt * DEG);
      const ey = my + this.root.y + FLUTE.dy + FLUTE.len * Math.sin(FLUTE.tilt * DEG);
      this.bubbles.push({ x: ex - 6, y: ey - 4, r: 9 + 9 * Math.min(n.dur, 1.7), life: 0, wob: Math.random() * 6.28, vx: -26 - Math.random() * 16 });
      this.top.v += 110;
    } else if (this.mode === "xylophone") {
      const spots = this.spots.length || 1;
      this.glow[n.i % spots] = 1;
      this.topY.v += 60; // its cap bobs with the strike
      this.top.v += (n.i % 2 ? 1 : -1) * 34;
      // One mallet to a hand: the low bars are the left hand's.
      const bar = n.i % BARS;
      this.bars[bar] = 1;
      const m = this.mallets[bar < BARS / 2 ? 0 : 1];
      m.to = bar;
      m.hit = 1;
    } else if (this.mode === "harp") {
      // The ear that reached into the strings lets this one go: it snaps back, and the string rings.
      const string = n.i % STRINGS;
      this.bars[string] = 1;
      this.reach = 0;
      this.ears.l.spread.v -= 420;
      this.ears.r.spread.v += 60;
      const [hx, hy, u] = this.harpAt();
      // ...and a note leaves it, as bubbles leave the flute.
      this.glints.push({ x: hx + (13 + (33 * string) / (STRINGS - 1)) * u, y: hy - 58 * u, life: 0, vx: 8 + (Math.random() - 0.5) * 22, sign: n.i % 3 ? "♪" : "♫", tip: (Math.random() - 0.5) * 0.5 });
    }
  }

  get face() {
    if (this.mode !== "idle") return this.mode === "dance" ? "happy" : "calm";
    return (this.gest && REACTIONS[this.gest.name].face) || this.play.face || this.expr;
  }

  /** `music`: { time, beats } on the music clock, when something is playing. */
  update(dt, music) {
    dt = clamp(dt, 0.001, 0.05);
    this.t += dt;
    const t = this.t;
    const r = this.root;
    const breath = Math.sin((t * 2 * Math.PI) / 3.4);
    let x = 0, y = 0, rot = 0, sx = 1, sy = 1;
    let pose = {};
    if (this.mode === "dance") {
      const b = Math.max(0, music ? music.beats : t / 0.5);
      pose = choreoOf(this.kind)[0][danceMoveAt(b, this.kind)](b - Math.floor(b / 4) * 4);
      ({ x = 0, y = 0, rot = 0, sx = 1, sy = 1 } = pose);
    } else if (this.mode === "piano") {
      const b = music ? music.beats : t / 0.84;
      const dip = Math.exp(-(b - Math.floor(b)) * 5);
      y = 2.5 * dip;
      rot = 2.2 * Math.sin((Math.PI * b) / 2);
      sy = 1 + 0.008 * breath - 0.006 * dip;
      sx = 1 - 0.005 * breath;
    } else if (this.mode === "cello") {
      // Leaning into the cello, swaying a bar at a time; the tail does the bowing.
      const b = music ? music.beats : t / 0.84;
      rot = 1.6 * Math.sin((Math.PI * b) / 4);
      y = 1.5;
      sy = 1 + 0.008 * breath;
      sx = 1 - 0.005 * breath;
      if (music) {
        this.strokes = this.strokes.filter((k) => music.time < k.at + k.dur + 4);
        for (const k of this.strokes) {
          if (music.time < k.at) break;
          const p = clamp((music.time - k.at) / k.dur, 0, 1);
          this.bowPos = k.dir * (2 * (p * p * (3 - 2 * p)) - 1);
        }
      }
      // A bow is drawn along its own length: the tail slides across the strings and only tips a little.
      pose = { head: -6 + 2.5 * Math.sin((Math.PI * b) / 4), pitch: 0.3, tail: 13 + 6 * this.bowPos };
    } else if (PLAYED[this.mode]) {
      // Flute, xylophone, harp: it sways a bar at a time and each note shows on its own body.
      const b = music ? music.beats : t / 0.84;
      if (music) while (this.notes.length && this.notes[0].at <= music.time) this.strike(this.notes.shift());
      rot = 1.5 * Math.sin((Math.PI * b) / 4);
      y = this.mode === "xylophone" ? 2.4 * this.pulse : 0.6;
      sy = 1 + 0.008 * breath + (this.mode === "flute" ? 0.012 * this.pulse : 0);
      sx = 1 - 0.005 * breath;
      // The harp is at its left: it leans to it, and a moment before each note the near ear reaches into the strings.
      const harp = this.mode === "harp";
      if (harp) x = -5;
      const lead = harp && music && this.notes.length ? this.notes[0].at - music.time : 9;
      this.reach += ((lead < 0.2 ? 1 : 0) - this.reach) * (1 - Math.exp(-dt * 26));
      pose = { head: (this.mode === "flute" ? -3 : harp ? -6 : 2.5) + 2.5 * Math.sin((Math.PI * b) / 4), pitch: this.mode === "flute" ? -0.25 : 0.3, tail: 10 * Math.sin((Math.PI * b) / 2) };
    } else if (this.expr === "asleep") {
      const slow = Math.sin((t * 2 * Math.PI) / 4.8);
      sy = 1 + 0.02 * slow;
      sx = 1 - 0.012 * slow;
      pose = { pitch: 0.5 };
    } else {
      sy = 1 + 0.013 * breath;
      sx = 1 - 0.008 * breath;
      rot = 0.7 * Math.sin((t * 2 * Math.PI) / 7.3);
    }
    if (this.mode === "idle") {
      this.back += (this.backTo - this.back) * (1 - Math.exp(-dt * 6));
      if (this.back > 0.002) {
        const b = this.back * this.back * (3 - 2 * this.back);
        const c = this.clench * b;
        pose = { ...pose, twirl: PI * b };
        sx *= 1 - 0.05 * c;
        sy *= 1 + 0.035 * c;
        y -= 3 * c;
        if (this.wasClenched && this.clench < 0.5) this.tail.v += 300; // letting go: the tail wags
        this.wasClenched = this.clench >= 0.5;
      }
    }
    if (this.gest && this.mode === "idle") {
      const def = REACTIONS[this.gest.name];
      this.gest.t += dt;
      const p = this.gest.t / def.dur;
      if (p >= 1) this.gest = null;
      else {
        const g = def.pose(p);
        x += g.x ?? 0;
        y += g.y ?? 0;
        sx *= g.sx ?? 1;
        sy *= g.sy ?? 1;
        pose = { ...pose, ...g };
      }
    }
    if (this.mode === "idle") {
      // Whatever is driving it from outside adds to the pose it would have had.
      const pl = this.play;
      x += pl.x;
      y -= pl.hop;
      rot += pl.rot;
      if (pl.twirl) pose = { ...pose, twirl: (pose.twirl ?? 0) + pl.twirl };
      if (pl.yaw != null) pose = { ...pose, yaw: (pose.yaw ?? 0) + pl.yaw };
      if (pl.pitch != null) pose = { ...pose, pitch: (pose.pitch ?? 0) + pl.pitch };
    }
    const k = 1 - Math.exp(-dt * 30);
    r.x += (x - r.x) * k;
    r.y += (y - r.y) * k;
    r.rot += (rot - r.rot) * k;
    r.sx += (sx - r.sx) * k;
    r.sy += (sy - r.sy) * k;
    r.scale += ((pose.scale ?? 1) - r.scale) * k;
    r.yaw += ((pose.yaw ?? 0) - r.yaw) * k;
    r.pitch += ((pose.pitch ?? 0) - r.pitch) * k;
    r.shear += ((pose.shear ?? 0) - r.shear) * k;
    r.twirl = pose.twirl ?? 0;
    r.headY += ((pose.headY ?? 0) - r.headY) * k;
    // What the notes leave behind fades; the instrument's own marks come and go with the mode.
    this.pulse = Math.max(0, this.pulse - dt * 3.2);
    for (let i = 0; i < this.glow.length; i++) this.glow[i] = Math.max(0, (this.glow[i] ?? 0) - dt * 2.2);
    const ring = this.inst === "harp" ? 1.25 : 2.2; // a plucked string rings on
    for (let i = 0; i < this.bars.length; i++) this.bars[i] = Math.max(0, (this.bars[i] ?? 0) - dt * ring);
    for (const m of this.mallets) {
      m.at += (m.to - m.at) * (1 - Math.exp(-dt * 26));
      m.hit = Math.max(0, m.hit - dt * 5);
    }
    if (this.mode !== "harp") this.reach = 0;
    for (const gl of this.glints) {
      gl.life += dt / 1.5;
      gl.y -= 62 * dt;
      gl.x += (gl.vx + 12 * Math.sin(t * 3 + gl.tip * 9)) * dt;
    }
    for (let i = this.glints.length - 1; i >= 0; i--) if (this.glints[i].life >= 1) this.glints.splice(i, 1);
    this.propA += ((PLAYED[this.mode] ? 1 : 0) - this.propA) * (1 - Math.exp(-dt * 9));
    for (const bub of this.bubbles) {
      bub.life += dt / 2.8;
      bub.y -= (26 + bub.r) * dt;
      bub.vx *= 1 - 0.9 * dt; // it leaves sideways, then only rises
      bub.x += (bub.vx + 9 * Math.sin(t * 2.3 + bub.wob)) * dt;
    }
    for (let i = this.bubbles.length - 1; i >= 0; i--) if (this.bubbles[i].life >= 1) this.bubbles.splice(i, 1);
    const cat = this.upright; // the upright body: a head on a neck, ears that are not hands
    const hang = this.kind === "sheep"; // ears that hang and swing
    if (cat) {
      // Now and then it tips its head; a tail swishes slowly when nothing else moves it.
      this.tilt.next -= dt;
      if (this.tilt.next <= 0) {
        this.tilt.next = 3 + Math.random() * 5;
        this.tilt.to = this.mode === "idle" && this.expr !== "asleep" ? (Math.random() * 2 - 1) * 4.5 : 0;
      }
      const led = this.mode === "idle" ? this.play.head : null;
      this.head.step((pose.head ?? led ?? (this.mode === "idle" ? this.tilt.to : 0)) - (led == null ? r.rot * 0.6 : 0), dt);
      this.bowTie.step(-r.rot * 1.5 - this.head.x * 0.5, dt);
      const swish = this.mode === "idle" && this.expr !== "asleep" ? 7 * Math.sin(t * 1.1) : 0;
      // Bowing is led, not sprung: the tail follows the stroke closely.
      this.tail.k = this.mode === "cello" ? 900 : 210;
      this.tail.c = this.mode === "cello" ? 46 : 11;
      this.tail.step((pose.tail ?? swish) - r.rot * 1.4, dt);
    } else this.tail.step((pose.tail ?? 0) - r.rot * 1.4, dt);

    // Ears: hanging weights. They lag the body's tilt and lift when it drops.
    const vy = (r.y - this.prevY) / dt;
    this.prevY = r.y;
    const lift = clamp(vy * 0.055, -3, 15);
    const piano = this.mode === "piano";
    if (this.mode === "idle") {
      this.twitch -= dt;
      if (this.twitch <= 0) {
        this.ears[Math.random() < 0.5 ? "l" : "r"].spread.v += cat ? 190 : 150;
        this.twitch = 4 + Math.random() * 6;
      }
    }
    for (const side of ["l", "r"]) {
      const e = this.ears[side];
      let tap = 0;
      let stretch = 1;
      if (music) {
        e.taps = e.taps.filter((at) => music.time < at + 0.5);
        for (const at of e.taps) {
          const d = music.time - at;
          if (d > -0.17 && d < 0) tap = Math.max(tap, 12 * easeOut((d + 0.17) / 0.17));
          else if (d >= 0 && d < 0.07) tap = -4;
          if (d >= 0) stretch = Math.max(stretch, 1 + 0.07 * Math.exp(-d * 14));
        }
      }
      // A cat's ears stand up and flick outward; a lop's hang and swing.
      e.spread.k = piano ? 420 : hang ? 130 : cat ? 300 : side === "l" ? 110 : 100;
      e.spread.c = piano ? 26 : hang ? 9 : cat ? 15 : side === "l" ? 10 : 9.5;
      const idle = cat && !hang ? 0 : 1.1 * Math.sin(t * 0.9 + (side === "l" ? 0 : 1.7));
      const steps = Math.ceil(dt / (1 / 120));
      for (let i = 0; i < steps; i++) {
        e.sway.step(hang ? -r.rot * 0.9 - this.head.x * 0.7 + idle : cat ? 0 : -r.rot * 0.9 - r.yaw * 7 + idle, dt / steps);
        e.spread.step((cat ? lift * (hang ? 0.7 : 0.3) : lift) + tap + ((side === "l" ? pose.earL : pose.earR) ?? 0), dt / steps);
      }
      e.stretch = stretch;
      if (hang) {
        // They hang beside the face: a hop lifts them outward, a tilt leaves them behind.
        const reach = side === "l" ? 38 * this.reach : 0; // into the harp's strings
        e.angle = clamp((side === "l" ? 1 : -1) * e.spread.x * 0.8 + e.sway.x + reach, -18, reach > 0 || this.mode === "harp" ? 46 : 18);
        continue;
      }
      if (cat) {
        e.angle = (side === "l" ? -1 : 1) * clamp(e.spread.x, -5, 16);
        continue;
      }
      const a = (side === "l" ? 1 : -1) * e.spread.x + e.sway.x;
      // Never swing in far enough to cover an eye.
      e.angle = side === "l" ? clamp(a, -9, 60) : clamp(a, -60, 9);
    }

    // What rides on the head lags its tilt, and lifts off a little when the body drops.
    if (cat) {
      const lim = this.kind === "mushroom" ? 8 : 25;
      const idleTop = this.mode === "idle" && this.expr !== "asleep" ? 3 * Math.sin(t * 0.8) : 0;
      this.top.step(clamp(-this.head.x * 0.9 - r.rot * 1.4 + (pose.top ?? (pose.tail ?? idleTop) * (this.kind === "mushroom" ? 0.2 : 0.6)), -lim, lim), dt);
      this.top.x = clamp(this.top.x, -lim, lim);
      this.topY.step(clamp(lift * 0.4, -1.5, 6), dt);
      this.topY.x = clamp(this.topY.x, -5, 6);
    }

    // Eyes.
    const face = this.face;
    const b = this.blink;
    b.next -= dt;
    if (b.next <= 0 && b.t < 0) {
      b.t = 0;
      b.next = Math.random() < 0.18 ? 0.3 : 2.2 + Math.random() * 3.8;
    }
    let lid = 1;
    if (b.t >= 0) {
      b.t += dt;
      lid = b.t < 0.07 ? 1 - b.t / 0.07 : b.t < 0.11 ? 0 : Math.min(1, (b.t - 0.11) / 0.13);
      if (b.t > 0.24) b.t = -1;
    }
    this.open = face === "asleep" ? 0 : lid * (face === "sleepy" ? 0.48 : face === "calm" ? 0.84 : 1);
    const lk = this.look;
    lk.next -= dt;
    if (lk.next <= 0) {
      lk.next = 2.5 + Math.random() * 3.5;
      lk.tx = (Math.random() * 2 - 1) * 3;
      lk.ty = Math.random() * 3 - 1;
    }
    const at = this.mode === "idle" ? this.play.look : null; // something to watch
    lk.x.step(at ? at[0] : face === "calm" || face === "happy" || face === "surprised" ? 0 : lk.tx, dt);
    lk.y.step(at ? at[1] : face === "calm" ? 2.5 : face === "sleepy" || face === "worried" ? 2 : face === "surprised" ? -1.5 : lk.ty, dt);
  }

  put(ctx, name) {
    const l = this.layer[name];
    ctx.drawImage(this.img[name], l.x, l.y, l.w, l.h);
  }

  /** A painted face part, slid with the head turn and narrowed about `cx`. */
  putPart(ctx, name, cx, dx, dy, sx = 1) {
    ctx.save();
    ctx.translate(cx + dx, dy);
    ctx.scale(sx, 1);
    ctx.translate(-cx, 0);
    this.put(ctx, name);
    ctx.restore();
  }

  drawFace(ctx) {
    const a = this.rig.anchors;
    const face = this.face;
    const dx = this.root.yaw * 13;
    const dy = this.root.pitch * 6;
    const near = (k) => 1 + 0.11 * this.root.yaw * (k === "eye_l" ? 1 : -1); // the eye turning away narrows
    if (face === "happy" && this.kind === "lop") {
      for (const k of ["eye_l", "eye_r"]) {
        const [cx, cy] = a[k];
        const g = ctx.createRadialGradient(0, 0, 0, 0, 0, 24);
        g.addColorStop(0, "rgba(240,150,170,.34)");
        g.addColorStop(1, "rgba(240,150,170,0)");
        ctx.save();
        ctx.translate(cx + dx + (k === "eye_l" ? -6 : 8), cy + dy + 30);
        ctx.scale(1, 0.62);
        ctx.fillStyle = g;
        ctx.beginPath();
        ctx.arc(0, 0, 24, 0, 7);
        ctx.fill();
        ctx.restore();
      }
    }
    for (const k of ["eye_l", "eye_r"]) {
      const [cx, cy] = a[k];
      const l = this.layer[k];
      const s = k === "eye_l" ? "l" : "r";
      if (face === "happy" && this.layer["smile_" + s]) {
        this.putPart(ctx, "smile_" + s, cx, dx, dy, near(k));
      } else if (face !== "happy" && this.open < 0.3 && this.layer["lid_" + s]) {
        this.putPart(ctx, "lid_" + s, cx, dx, dy, near(k)); // shut: blinking or asleep
      } else if (face === "happy") {
        const rr = l.w * 0.36;
        ctx.save();
        ctx.translate(cx + dx, cy + dy);
        ctx.scale(near(k), 1);
        ctx.beginPath();
        ctx.arc(0, rr * 0.5, rr, Math.PI * 1.13, Math.PI * 1.87);
        ctx.restore();
        ctx.lineWidth = rr * 0.44;
        ctx.lineCap = "round";
        ctx.strokeStyle = "#2e2528";
        ctx.stroke();
      } else {
        const open = Math.max(this.open, 0.07);
        const wide = face === "surprised" ? 1.08 : 1;
        ctx.save();
        ctx.translate(cx + dx + this.look.x.x, cy + dy + this.look.y.x + (1 - open) * 4);
        ctx.scale(near(k) * wide, open * wide);
        ctx.translate(-cx, -cy);
        this.put(ctx, k);
        ctx.restore();
      }
    }
    // The sheep's own face has slanted brows; they go only when it smiles, sleeps or startles.
    const brows = face === "worried" || (this.kind === "sheep" && face !== "happy" && face !== "asleep" && face !== "surprised" && face !== "calm");
    if (brows && this.layer.brow_l) {
      this.putPart(ctx, "brow_l", a.eye_l[0], dx, dy);
      this.putPart(ctx, "brow_r", a.eye_r[0], dx, dy);
    }
    const mx = a.mouth[0] + dx * 1.15;
    const my = a.mouth[1] + dy;
    if ((face === "surprised" || this.mode === "flute") && this.layer.mouth_o) {
      this.putPart(ctx, "mouth_o", a.mouth[0], dx * 1.15, dy);
    } else if (face === "neutral" || face === "sleepy" || face === "asleep" || face === "worried" || face === "surprised" || (face === "calm" && this.upright)) {
      ctx.save();
      ctx.translate(dx * 1.15, dy);
      this.put(ctx, "mouth");
      ctx.restore();
    } else if (face === "happy" && this.layer.smile_mouth) {
      this.putPart(ctx, "smile_mouth", a.mouth[0], dx * 1.15, dy);
    } else {
      const w = face === "happy" ? 9 : 5.5;
      const d = face === "happy" ? 8 : 4.5;
      ctx.beginPath();
      ctx.moveTo(mx - w, my - 1);
      ctx.quadraticCurveTo(mx, my + d, mx + w, my - 1);
      ctx.lineWidth = 2.5;
      ctx.lineCap = "round";
      ctx.strokeStyle = "rgb(172,126,138)";
      ctx.stroke();
    }
  }

  drawEar(ctx, side) {
    const l = this.layer["ear_" + side];
    const e = this.ears[side];
    const [px, py] = l.pivot;
    ctx.save();
    ctx.translate(px - this.root.yaw * 6, py - this.root.pitch * 2);
    ctx.rotate(e.angle * DEG);
    ctx.scale(1 + 0.1 * this.root.yaw * (side === "l" ? 1 : -1), e.stretch);
    ctx.translate(-px, -py);
    this.put(ctx, "ear_" + side);
    ctx.restore();
  }

  /** Seen from behind, the ear on the left of the picture is the other ear. */
  drawBackEar(ctx, side) {
    const l = this.layer["back_ear_" + side];
    const e = this.ears[side === "l" ? "r" : "l"];
    const [px, py] = l.pivot;
    ctx.save();
    ctx.translate(px, py);
    ctx.rotate(-e.angle * DEG);
    ctx.scale(1, e.stretch);
    ctx.translate(-px, -py);
    this.put(ctx, "back_ear_" + side);
    ctx.restore();
  }

  drawTail(ctx) {
    if (!this.layer.back_tail) return;
    const [px, py] = this.layer.back_tail.pivot;
    ctx.save();
    ctx.translate(px + this.tail.x * 0.22, py - 7 * this.clench);
    ctx.rotate(this.tail.x * DEG);
    ctx.translate(-px, -py);
    this.put(ctx, "back_tail");
    ctx.restore();
  }

  /** A layer turned about its pivot. */
  putTurned(ctx, name, deg, dx = 0, dy = 0) {
    const [px, py] = this.layer[name].pivot;
    ctx.save();
    ctx.translate(px + dx, py + dy);
    ctx.rotate(deg * DEG);
    ctx.translate(-px, -py);
    this.put(ctx, name);
    ctx.restore();
  }

  /** An upright pet: `mid(ctx)` draws what stands behind it (the cello). */
  drawUpright(ctx, scale, mid) {
    const r = this.root;
    const [fx, fy] = this.foot;
    ctx.save();
    ctx.scale(scale, scale);
    const up = clamp(-r.y / 30, 0, 1);
    const turn = Math.cos(r.twirl);
    const g = ctx.createRadialGradient(0, 0, 0, 0, 0, 120);
    g.addColorStop(0, `rgba(110,90,80,${0.2 * (1 - 0.5 * up)})`);
    g.addColorStop(1, "rgba(110,90,80,0)");
    ctx.save();
    ctx.translate(fx + r.x, fy + 4);
    ctx.scale((1 - 0.2 * up) * r.scale * (0.7 + 0.3 * Math.abs(turn)), 0.15 * r.scale);
    ctx.fillStyle = g;
    ctx.beginPath();
    ctx.arc(0, 0, 120, 0, 7);
    ctx.fill();
    ctx.restore();
    if (mid) mid(ctx);
    // The harp stands beside it, a little behind.
    if (this.propA > 0.01 && this.inst === "harp") this.drawHarp(ctx);

    const front = turn >= 0;
    const back = !front && !!this.rig.back;
    ctx.save();
    ctx.translate(fx + r.x, fy + r.y);
    ctx.rotate(r.rot * DEG);
    ctx.scale(r.sx * r.scale * (1 - 0.035 * Math.abs(r.yaw)) * (front || back ? 1 : -1) * Math.max(Math.abs(turn), 0.04), r.sy * r.scale);
    ctx.transform(1, 0, r.shear, 1, 0, 0);
    ctx.translate(-fx, -fy);
    // The head and everything on it turn together about the neck.
    const inHead = (name, deg, draw) => {
      const [nx, ny] = this.layer[name].pivot;
      ctx.save();
      ctx.translate(nx, ny + r.headY);
      ctx.rotate(deg * DEG);
      ctx.translate(-nx, -ny);
      draw();
      ctx.restore();
    };
    const has = (name) => !!this.layer[name];
    if (back) {
      this.put(ctx, "back_body");
      inHead("back_head", -this.head.x, () => {
        if (has("back_curl")) this.putTurned(ctx, "back_curl", -this.top.x);
        this.put(ctx, "back_head");
        if (has("back_cap")) this.putTurned(ctx, "back_cap", -this.top.x, 0, -this.topY.x);
      });
      this.drawTail(ctx);
    } else {
      let faced = false;
      for (const name of this.rig.order ?? UPRIGHT_ORDER) {
        if (!has(name)) continue;
        if (name === "tail") this.putTurned(ctx, "tail", this.tail.x, this.mode === "cello" ? -17 + 10 * this.bowPos : 0, 0);
        else if (name === "body") {
          this.put(ctx, "body");
        } else if (name === "bow") {
          // The bow tie sits on the collar, over the chin, and stays with the body.
          if (front) this.putTurned(ctx, "bow", this.bowTie.x);
        } else if (name === "ear_l" || name === "ear_r") inHead("head", this.head.x, () => this.putTurned(ctx, name, this.ears[name === "ear_l" ? "l" : "r"].angle));
        else if (name === "head") inHead("head", this.head.x, () => this.put(ctx, "head"));
        else if (name === "curl") inHead("head", this.head.x, () => this.putTurned(ctx, name, this.top.x));
        else if (name.startsWith("cap"))
          inHead("head", this.head.x, () => {
            this.putTurned(ctx, name, this.top.x, 0, -this.topY.x);
            if (name === "cap" && front) this.drawSpots(ctx);
          });
        else if (!faced && front) {
          // The first of the face layers stands for the whole face.
          faced = true;
          inHead("head", this.head.x, () => this.drawFace(ctx));
        }
      }
    }
    ctx.restore();
    // The flute is at its lips and the xylophone floats before it, as 紫梦's keys do.
    if (this.propA > 0.01 && this.inst === "flute") this.drawFlute(ctx);
    if (this.propA > 0.01 && this.inst === "xylophone") this.drawXylophone(ctx);
    // Flute notes are bubbles: they leave the figure and rise on their own.
    for (const bub of this.bubbles) {
      const a = Math.min(1, bub.life * 8) * (1 - bub.life) ** 0.7;
      const rad = bub.r * (0.5 + 0.5 * Math.min(1, bub.life * 5));
      ctx.beginPath();
      ctx.arc(bub.x, bub.y, rad, 0, 7);
      ctx.fillStyle = `rgba(196,222,244,${0.3 * a})`;
      ctx.fill();
      // A rim of its own blue reads on paper; the white glint reads on dark.
      ctx.lineWidth = 1.8;
      ctx.strokeStyle = `rgba(108,150,196,${0.8 * a})`;
      ctx.stroke();
      ctx.beginPath();
      ctx.arc(bub.x, bub.y, rad * 0.6, Math.PI * 1.05, Math.PI * 1.45);
      ctx.lineWidth = 2.2;
      ctx.lineCap = "round";
      ctx.strokeStyle = `rgba(255,255,255,${0.95 * a})`;
      ctx.stroke();
    }
    ctx.restore();
  }

  /** The spots on the mushroom's cap, lit as they are struck (drawn inside the cap's own turn). */
  drawSpots(ctx) {
    const spots = this.spots;
    const [px, py] = this.layer.cap.pivot;
    ctx.save();
    ctx.translate(px, py - this.topY.x);
    ctx.rotate(this.top.x * DEG);
    ctx.translate(-px, -py);
    spots.forEach(([x, y, rad], i) => {
      const gl = this.glow[i] ?? 0;
      if (gl <= 0.01) return;
      const g = ctx.createRadialGradient(x, y, 0, x, y, rad * 1.7);
      g.addColorStop(0, `rgba(255,214,110,${0.95 * gl})`);
      g.addColorStop(0.55, `rgba(255,190,84,${0.62 * gl})`);
      g.addColorStop(1, "rgba(255,190,84,0)");
      ctx.fillStyle = g;
      ctx.beginPath();
      ctx.arc(x, y, rad * 1.7, 0, 7);
      ctx.fill();
      // The strike rings outward as it fades.
      ctx.beginPath();
      ctx.arc(x, y, rad * (1.1 + 1.3 * (1 - gl)), 0, 7);
      ctx.lineWidth = 2.4 * gl;
      ctx.strokeStyle = `rgba(255,236,170,${0.9 * gl})`;
      ctx.stroke();
    });
    ctx.restore();
  }

  /** The flute, held sideways at its lips; it follows the head. Drawn in the pets' manner: flat fills, one soft line. */
  drawFlute(ctx) {
    const [mx, my] = this.rig.anchors.mouth;
    const r = this.root;
    ctx.save();
    ctx.globalAlpha = this.propA;
    ctx.translate(mx + r.x + r.yaw * 13 + FLUTE.dx, my + r.y + r.headY + FLUTE.dy + 2 * this.pulse);
    ctx.scale(-1, 1); // it points to the left of the picture
    ctx.rotate((FLUTE.tilt + this.head.x * 0.4) * DEG);
    const line = "rgba(104,124,152,.9)";
    const g = ctx.createLinearGradient(0, -6.5, 0, 6.5);
    g.addColorStop(0, "#f7f9fc");
    g.addColorStop(0.55, "#dbe4ef");
    g.addColorStop(1, "#b8c6d9");
    roundRect(ctx, -14, -6.2, FLUTE.len + 14, 12.4, 6.2);
    ctx.fillStyle = g;
    ctx.fill();
    ctx.lineWidth = 1.6;
    ctx.strokeStyle = line;
    ctx.stroke();
    // the lip plate, the joints, the keys
    ctx.beginPath();
    ctx.ellipse(0, -1.6, 9, 4.4, 0, 0, 7);
    ctx.fillStyle = "#c9d5e4";
    ctx.fill();
    ctx.stroke();
    ctx.lineWidth = 1.2;
    for (const x of [26, 112]) {
      ctx.beginPath();
      ctx.moveTo(x, -6.2);
      ctx.lineTo(x, 6.2);
      ctx.stroke();
    }
    for (const x of [42, 56, 70, 84, 98, 124, 136]) {
      ctx.beginPath();
      ctx.arc(x, -0.8, 4.2, 0, 7);
      ctx.fillStyle = "#eef3f8";
      ctx.fill();
      ctx.stroke();
    }
    ctx.restore();
  }

  /** A small xylophone floating before it: eight bars, low to high, and a mallet to each hand. */
  drawXylophone(ctx) {
    const r = this.root;
    const [fx, fy] = this.foot;
    const H = this.rig.bbox[3] - this.rig.bbox[1];
    const bw = 27, gap = 6.5, w = BARS * bw + (BARS - 1) * gap;
    const x0 = fx + r.x - w / 2;
    const top = fy - H * 0.26;
    const COLORS = ["#f2b3bd", "#f6c7a4", "#f5e0a0", "#cde5b0", "#b0decf", "#b2d1ee", "#c7c0ee", "#e6bde6"];
    const line = "rgba(146,110,112,.85)";
    ctx.save();
    ctx.globalAlpha = this.propA;
    ctx.lineJoin = ctx.lineCap = "round";
    // the two rails the bars lie on
    ctx.strokeStyle = "rgba(160,124,104,.9)";
    ctx.lineWidth = 5;
    for (const k of [0.26, 0.76]) {
      ctx.beginPath();
      ctx.moveTo(x0 - 8, top + 58 * k + 2);
      ctx.lineTo(x0 + w + 8, top + 38 * k + 10 + 2);
      ctx.stroke();
    }
    for (let i = 0; i < BARS; i++) {
      const gl = this.bars[i] ?? 0;
      const h = 58 - (20 * i) / (BARS - 1); // longer bars sound lower
      const x = x0 + i * (bw + gap);
      const y = top + (58 - h) / 2 + 2.5 * gl;
      if (gl > 0.02) {
        ctx.shadowColor = `rgba(255,236,170,${0.9 * gl})`;
        ctx.shadowBlur = 16 * gl;
      }
      roundRect(ctx, x, y, bw, h, 7);
      ctx.fillStyle = COLORS[i];
      ctx.fill();
      ctx.shadowColor = "transparent";
      ctx.shadowBlur = 0;
      if (gl > 0.02) {
        ctx.fillStyle = `rgba(255,252,236,${0.75 * gl})`;
        ctx.fill();
      }
      ctx.lineWidth = 1.5;
      ctx.strokeStyle = line;
      ctx.stroke();
      // the two pins that hold it
      ctx.fillStyle = "rgba(146,110,112,.55)";
      for (const k of [0.26, 0.76]) {
        ctx.beginPath();
        ctx.arc(x + bw / 2, y + h * k, 2, 0, 7);
        ctx.fill();
      }
    }
    // the mallets: each hovers over the bar it struck last, and dips on a strike
    this.mallets.forEach((m, hand) => {
      const x = x0 + m.at * (bw + gap) + bw / 2;
      const y = top - 17 + 19 * m.hit;
      const lean = hand === 0 ? -1 : 1;
      ctx.beginPath();
      ctx.moveTo(x, y);
      ctx.lineTo(x + lean * 18, y - 40 + 8 * m.hit);
      ctx.lineWidth = 3;
      ctx.strokeStyle = "rgba(160,124,104,.95)";
      ctx.stroke();
      ctx.beginPath();
      ctx.arc(x, y, 8, 0, 7);
      ctx.fillStyle = "#fff4e6";
      ctx.fill();
      ctx.lineWidth = 1.5;
      ctx.strokeStyle = line;
      ctx.stroke();
    });
    ctx.restore();
  }

  /** Where the harp stands: [x of its pillar's foot, y of the floor, rig px per unit of its drawing (which is 100 tall)]. */
  harpAt() {
    const [x0, y0, , y1] = this.rig.bbox;
    const h = (y1 - y0) * 0.88; // as tall as fits beside it in its window on the desktop
    return [x0 - h * 0.35, this.foot[1] + 2, h / 100];
  }

  /** A small harp standing beside it: pillar, curved neck, sound box, and seven strings. A plucked string shivers wide, glows, and throws off a glint. */
  drawHarp(ctx) {
    const [hx, hy, u] = this.harpAt(); // the drawing below stands on y = 0, its pillar on the left
    const line = "rgba(138,98,72,.9)";
    ctx.save();
    ctx.globalAlpha = this.propA;
    // the notes the plucked strings let go, rising: gold, with a pale edge so they read on any desktop
    ctx.textAlign = "center";
    ctx.textBaseline = "middle";
    ctx.lineJoin = "round";
    for (const gl of this.glints) {
      const a = Math.min(1, gl.life * 6) * (1 - gl.life) ** 0.8;
      ctx.save();
      ctx.translate(gl.x, gl.y);
      ctx.rotate(gl.tip);
      ctx.font = `700 ${Math.round(34 + 12 * Math.min(1, gl.life * 4))}px "Segoe UI Symbol", "Segoe UI", sans-serif`;
      ctx.lineWidth = 5;
      ctx.strokeStyle = `rgba(255,250,236,${0.9 * a})`;
      ctx.strokeText(gl.sign, 0, 0);
      ctx.fillStyle = `rgba(190,138,52,${a})`;
      ctx.fillText(gl.sign, 0, 0);
      ctx.restore();
    }
    ctx.translate(hx, hy);
    ctx.rotate((3 + 1.6 * this.pulse * Math.sin(this.t * 38)) * DEG); // it leans to the player's shoulder, and shakes a little when plucked
    ctx.scale(u, -u); // y up from the floor
    ctx.lineJoin = ctx.lineCap = "round";
    // strings first: the frame closes over their ends
    for (let i = 0; i < STRINGS; i++) {
      const gl = this.bars[i] ?? 0;
      const x = 13 + (33 * i) / (STRINGS - 1);
      const top = 93.5 - (x - 13) * 0.52;
      const foot = Math.max(5, 4 + (x - 14) * 1.84);
      const bend = gl * 5.5 * Math.sin(this.t * 52 + i * 1.7);
      ctx.beginPath();
      ctx.moveTo(x, top);
      ctx.quadraticCurveTo(x + bend, (top + foot) / 2, x, foot);
      if (gl > 0.02) {
        ctx.lineWidth = 6 * gl;
        ctx.strokeStyle = `rgba(255,214,110,${0.7 * gl})`;
        ctx.stroke();
      }
      ctx.lineWidth = 1.15 + 1.1 * gl;
      ctx.strokeStyle = `rgba(${186 + 60 * gl},${146 + 64 * gl},${78 + 60 * gl},.98)`;
      ctx.stroke();
    }
    ctx.strokeStyle = line;
    ctx.fillStyle = "#d9a36f";
    // the sound box: from the base up to the neck, leaning in
    ctx.beginPath();
    ctx.moveTo(9, 2);
    ctx.lineTo(19, 2);
    ctx.lineTo(56, 71);
    ctx.lineTo(49, 75);
    ctx.closePath();
    ctx.fill();
    ctx.lineWidth = 1.8;
    ctx.stroke();
    // the pillar
    roundRect(ctx, 3, 2, 6.5, 92, 3);
    ctx.fill();
    ctx.stroke();
    // the neck: the harmonic curve
    ctx.beginPath();
    ctx.moveTo(4, 91);
    ctx.bezierCurveTo(18, 106, 34, 80, 55, 76);
    ctx.lineTo(56, 69);
    ctx.bezierCurveTo(36, 72, 20, 96, 8, 86);
    ctx.closePath();
    ctx.fill();
    ctx.stroke();
    // the crown and the feet
    ctx.beginPath();
    ctx.arc(6.2, 96, 4.2, 0, 7);
    ctx.fill();
    ctx.stroke();
    roundRect(ctx, 0, 0, 24, 4, 2);
    ctx.fillStyle = "#8f6a4f";
    ctx.fill();
    ctx.stroke();
    // a soft light down the pillar
    ctx.strokeStyle = "rgba(255,240,215,.5)";
    ctx.lineWidth = 1.3;
    ctx.beginPath();
    ctx.moveTo(5.2, 12);
    ctx.lineTo(5.2, 84);
    ctx.stroke();
    ctx.restore();
  }

  /** `mid(ctx)` draws between the body and the ears (the keyboard). */
  draw(ctx, scale, mid) {
    if (this.upright) return this.drawUpright(ctx, scale, mid);
    const r = this.root;
    const [fx, fy] = this.foot;
    ctx.save();
    ctx.scale(scale, scale);

    const up = clamp(-r.y / 30, 0, 1);
    const g = ctx.createRadialGradient(0, 0, 0, 0, 0, 150);
    g.addColorStop(0, `rgba(90,70,110,${0.2 * (1 - 0.5 * up)})`);
    g.addColorStop(1, "rgba(90,70,110,0)");
    ctx.save();
    ctx.translate(fx + r.x, fy + 4);
    const turn = Math.cos(r.twirl);
    ctx.scale((1 - 0.2 * up) * r.scale * (0.7 + 0.3 * Math.abs(turn)), 0.15 * r.scale);
    ctx.fillStyle = g;
    ctx.beginPath();
    ctx.arc(0, 0, 150, 0, 7);
    ctx.fill();
    ctx.restore();

    const front = turn >= 0; // past a quarter turn the back is showing: no face, no hair clip
    const back = !front && !!this.rig.back; // painted back view; else the front, mirrored and faceless
    const enter = () => {
      ctx.save();
      ctx.translate(fx + r.x, fy + r.y);
      ctx.rotate(r.rot * DEG);
      ctx.scale(r.sx * r.scale * (1 - 0.035 * Math.abs(r.yaw)) * (front || back ? 1 : -1) * Math.max(Math.abs(turn), 0.04), r.sy * r.scale);
      ctx.transform(1, 0, r.shear, 1, 0, 0);
      ctx.translate(-fx, -fy);
    };
    enter();
    if (back) {
      this.put(ctx, "back_body");
      this.drawTail(ctx);
    } else {
      this.drawEar(ctx, "l"); // this ear hangs behind the head
      this.put(ctx, "body");
      // Its root melts into the head while it hangs, and gets the head's edge once it lifts away.
      const lifted = clamp((this.ears.l.angle - 4) / 10, 0, 1);
      ctx.globalAlpha = 1 - lifted;
      this.put(ctx, "root_l_melt");
      ctx.globalAlpha = lifted;
      this.put(ctx, "root_l_line");
      ctx.globalAlpha = 1;
      if (front) this.drawFace(ctx);
    }
    ctx.restore();
    if (mid) {
      mid(ctx);
      // The tip of the ear behind the head still plays in front of the keys.
      ctx.save();
      ctx.beginPath();
      ctx.rect(0, KB.y - 8, this.rig.size[0], this.rig.size[1]);
      ctx.clip();
      enter();
      this.drawEar(ctx, "l");
      ctx.restore();
      ctx.restore();
    }
    enter();
    if (back) {
      this.drawBackEar(ctx, "l");
      this.drawBackEar(ctx, "r");
    } else {
      this.drawEar(ctx, "r");
      if (front) {
        ctx.translate(r.yaw * 8, r.pitch * 3);
        this.put(ctx, "star");
      }
    }
    ctx.restore();
    ctx.restore();
  }
}

// ---- The floating keyboard the ears play on (rig space, 512) ----

const KB = { x: 50, y: 300, w: 412, h: 52, whites: 14 };
const BLACK_AFTER = [0, 1, 3, 4, 5];

/** Which key a note lights: bass on the left half, melody on the right. */
export function keyFor(note, hand) {
  const m = /^([A-G])(#?)(\d)$/.exec(note);
  const white = "CDEFGAB".indexOf(m[1]) + (hand === "r" ? 7 : 0);
  return { white, black: m[2] === "#" };
}

function roundRect(ctx, x, y, w, h, r) {
  ctx.beginPath();
  ctx.moveTo(x + r, y);
  ctx.arcTo(x + w, y, x + w, y + h, r);
  ctx.arcTo(x + w, y + h, x, y + h, r);
  ctx.arcTo(x, y + h, x, y, r);
  ctx.arcTo(x, y, x + w, y, r);
  ctx.closePath();
}

/** `lit`: [{ white, black, glow 0..1 }]; `sparks`: [{ x, y, life 0..1 }]. */
export function drawKeyboard(ctx, lit, sparks, alpha = 1) {
  const kw = KB.w / KB.whites;
  const glowOf = (white, black) => lit.reduce((g, k) => (k.white === white && k.black === black ? Math.max(g, k.glow) : g), 0);
  ctx.save();
  ctx.globalAlpha = alpha;
  ctx.shadowColor = "rgba(90,70,120,.28)";
  ctx.shadowBlur = 14;
  ctx.shadowOffsetY = 5;
  roundRect(ctx, KB.x - 5, KB.y - 5, KB.w + 10, KB.h + 10, 12);
  ctx.fillStyle = "rgba(214,200,232,.92)";
  ctx.fill();
  ctx.shadowColor = "transparent";
  for (let i = 0; i < KB.whites; i++) {
    const g = glowOf(i, false);
    roundRect(ctx, KB.x + i * kw + 1, KB.y, kw - 2, KB.h, 5);
    ctx.fillStyle = `rgb(${255 - 62 * g},${251 - 80 * g},${246 - 4 * g})`;
    ctx.fill();
    ctx.lineWidth = 1;
    ctx.strokeStyle = "rgba(160,132,150,.5)";
    ctx.stroke();
  }
  for (let i = 0; i < KB.whites; i++) {
    if (!BLACK_AFTER.includes(i % 7)) continue;
    const g = glowOf(i, true);
    roundRect(ctx, KB.x + (i + 1) * kw - 8.5, KB.y, 17, KB.h * 0.6, 4);
    ctx.fillStyle = `rgb(${128 + 60 * g},${106 + 44 * g},${140 + 90 * g})`;
    ctx.fill();
  }
  for (const s of sparks) {
    const a = Math.sin(Math.PI * s.life);
    const rad = 3 + 3 * a;
    ctx.fillStyle = `rgba(255,250,240,${0.9 * a})`;
    ctx.beginPath();
    for (let i = 0; i < 8; i++) {
      const ang = (i * Math.PI) / 4;
      const rr = i % 2 ? rad * 0.38 : rad;
      ctx.lineTo(s.x + Math.cos(ang) * rr, s.y + Math.sin(ang) * rr);
    }
    ctx.closePath();
    ctx.fill();
  }
  ctx.restore();
}

export function keyCenter(key) {
  const kw = KB.w / KB.whites;
  return key.black ? [KB.x + (key.white + 1) * kw, KB.y + 6] : [KB.x + (key.white + 0.5) * kw, KB.y + 10];
}

// ---- The cello the cat bows with its tail (rig space) ----

/**
 * A small cello standing at (x, floor), `h` tall, leaning a little. `glow` 0..1
 * brightens the strings while a note sounds. Drawn in the pets' own manner:
 * flat warm fills and one soft brown line.
 */
export function drawCello(ctx, x, floor, h, alpha = 1, glow = 0, lean = -5) {
  const u = h / 100; // the drawing below is 100 tall, standing on y = 0
  ctx.save();
  ctx.globalAlpha = alpha;
  ctx.translate(x, floor);
  ctx.rotate(lean * DEG); // its neck leans past the player's cheek
  ctx.scale(u, u);
  ctx.lineJoin = ctx.lineCap = "round";
  const line = "rgba(138,98,72,.9)";
  // end pin
  ctx.strokeStyle = line;
  ctx.lineWidth = 1.6;
  ctx.beginPath();
  ctx.moveTo(0, -6);
  ctx.lineTo(0, 0);
  ctx.stroke();
  // body: two bouts and a waist
  ctx.beginPath();
  ctx.moveTo(0, -6);
  ctx.bezierCurveTo(19, -6, 21, -26, 13, -31);
  ctx.bezierCurveTo(9, -34, 9, -38, 12, -41);
  ctx.bezierCurveTo(18, -47, 15, -60, 0, -60);
  ctx.bezierCurveTo(-15, -60, -18, -47, -12, -41);
  ctx.bezierCurveTo(-9, -38, -9, -34, -13, -31);
  ctx.bezierCurveTo(-21, -26, -19, -6, 0, -6);
  ctx.closePath();
  ctx.fillStyle = "#d9a36f";
  ctx.fill();
  ctx.lineWidth = 2;
  ctx.stroke();
  // a soft light on the upper bout
  ctx.fillStyle = "rgba(255,240,215,.45)";
  ctx.beginPath();
  ctx.ellipse(-5, -50, 5, 7, -0.3, 0, 7);
  ctx.fill();
  // f-holes
  ctx.lineWidth = 1.3;
  for (const sgn of [-1, 1]) {
    ctx.beginPath();
    ctx.moveTo(sgn * 6.5, -40);
    ctx.quadraticCurveTo(sgn * 9, -34, sgn * 6.5, -28);
    ctx.stroke();
  }
  // neck and scroll
  ctx.fillStyle = "#8f6a4f";
  ctx.beginPath();
  ctx.roundRect(-2.2, -92, 4.4, 34, 2);
  ctx.fill();
  ctx.stroke();
  ctx.beginPath();
  ctx.arc(0, -95, 4.2, 0, 7);
  ctx.fillStyle = "#d9a36f";
  ctx.fill();
  ctx.lineWidth = 1.6;
  ctx.stroke();
  // tailpiece, bridge, strings
  ctx.fillStyle = "#6f5240";
  ctx.beginPath();
  ctx.moveTo(-3, -22);
  ctx.lineTo(3, -22);
  ctx.lineTo(1.6, -9);
  ctx.lineTo(-1.6, -9);
  ctx.closePath();
  ctx.fill();
  ctx.strokeStyle = line;
  ctx.lineWidth = 1.6;
  ctx.beginPath();
  ctx.moveTo(-5, -30);
  ctx.lineTo(5, -30);
  ctx.stroke();
  ctx.lineWidth = 0.7;
  ctx.strokeStyle = `rgba(${255 - 40 * (1 - glow)},${246 - 60 * (1 - glow)},${226 - 90 * (1 - glow)},.95)`;
  for (const k of [-1.8, -0.6, 0.6, 1.8]) {
    ctx.beginPath();
    ctx.moveTo(k * 1.1, -22);
    ctx.lineTo(k * 0.7, -90);
    ctx.stroke();
  }
  ctx.restore();
}
