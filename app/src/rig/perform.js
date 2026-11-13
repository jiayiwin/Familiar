// The rig's performances: a piece on its own instrument (紫梦 the piano, with
// her ears; 麻薯 the cello, with its tail for a bow; 泡泡 the flute, in
// bubbles; 菇菇 the xylophone, on the spots of its cap; 茸茸 the harp, on the
// curls of its chest), and a dance to a short loop. Owns the music clock, so
// sound and motion stay on the same beat. The five pieces are five parts of
// one arrangement: started on the same clock in separate windows they play
// together.
// `Tone` is passed in: the app bundles it, the preview page loads it from a CDN.
// Lives in app/src/rig/; design/pet-art/preview keeps a copy (tools/sync_app.py).

import { drawCello, drawKeyboard, keyFor, keyCenter } from "./rig.js";

/** Piano samples (Salamander Grand Piano, CC BY 3.0): every minor third, A1..A6. */
export const SAMPLES = ["A1", "C2", "D#2", "F#2", "A2", "C3", "D#3", "F#3", "A3", "C4", "D#4", "F#4", "A4", "C5", "D#5", "F#5", "A5", "C6", "D#6", "F#6", "A6"];
/** Cello samples ("Real Cello Notes" by flcellogrl, CC BY; see the folder's LICENSE.txt). */
export const CELLO_SAMPLES = ["C2", "D2", "E2", "G2", "A2", "C3", "D3", "E3", "G3", "A3", "C4", "D4", "E4", "G4", "A4", "C5"];
/** Flute, xylophone and harp samples (VSCO 2 CE, CC0; the edited set from tonejs-instruments, CC BY 3.0). */
export const FLUTE_SAMPLES = ["C4", "E4", "A4", "C5", "E5", "A5", "C6", "E6", "A6", "C7"];
export const XYLOPHONE_SAMPLES = ["G4", "C5", "G5", "C6", "G6", "C7", "G7", "C8"];
export const HARP_SAMPLES = ["D2", "A2", "E3", "B3", "F4", "C5", "G5", "D6", "F6"];
const SAMPLE_SETS = { piano: SAMPLES, cello: CELLO_SAMPLES, flute: FLUTE_SAMPLES, xylophone: XYLOPHONE_SAMPLES, harp: HARP_SAMPLES };
// How each instrument speaks: how fast a note comes in, how long it rings on, how loud it sits (dB).
const VOICE = {
  piano: { attack: 0, release: 1.4, volume: 0 },
  cello: { attack: 0.06, release: 0.5, volume: 0 },
  flute: { attack: 0.03, release: 0.35, volume: -4 },
  xylophone: { attack: 0, release: 1.1, volume: -9 },
  harp: { attack: 0, release: 1.8, volume: -2 },
};
/** A note's place on the keyboard, for choosing which spot or string it lights. */
const pitchOf = (note) => {
  const m = /^([A-G])(#?)(\d)$/.exec(note);
  return "C.D.EF.G.A.B".indexOf(m[1]) + (m[2] ? 1 : 0) + 12 * Number(m[3]);
};

// Piano piece: an original two-hand arrangement over the (public-domain) canon progression.
const E8 = 0.42;
const ARP = { D: ["D3", "A3", "D4", "F#4"], A: ["A2", "E3", "A3", "C#4"], Bm: ["B2", "F#3", "B3", "D4"], "F#m": ["F#2", "C#3", "F#3", "A3"], G: ["G2", "D3", "G3", "B3"] };
const PROG = ["D", "A", "Bm", "F#m", "G", "D", "G", "A"];
const MEL1 = ["F#5", "E5", "D5", "C#5", "B4", "A4", "B4", "C#5"];
const MEL2 = [["F#5", "A5"], ["E5", "C#5"], ["D5", "F#5"], ["C#5", "A4"], ["B4", "D5"], ["A4", "F#4"], ["G4", "B4"], ["C#5", "E5"]];

function pianoScore() {
  const ev = [];
  const human = () => (Math.random() - 0.5) * 0.022; // nobody plays on a grid
  let t = 0.4;
  for (let pass = 0; pass < 2; pass++) {
    for (let i = 0; i < 8; i++) {
      ARP[PROG[i]].forEach((n, j) => ev.push({ t: t + j * E8 + human(), note: n, dur: 1.3, vel: (j === 0 ? 0.5 : 0.36) + Math.random() * 0.06, hand: "l", tap: j % 2 === 0 }));
      if (pass === 0) ev.push({ t: t + 0.012 + human(), note: MEL1[i], dur: E8 * 4 + 0.3, vel: 0.68 + Math.random() * 0.08, hand: "r", tap: true });
      else MEL2[i].forEach((n, j) => ev.push({ t: t + j * 2 * E8 + 0.012 + human(), note: n, dur: E8 * 2 + 0.3, vel: 0.66 + Math.random() * 0.1, hand: "r", tap: true }));
      t += 4 * E8;
    }
  }
  ARP.D.forEach((n, j) => ev.push({ t: t + j * E8 * (1 + j * 0.2), note: n, dur: 3.2, vel: 0.42, hand: "l", tap: j % 2 === 0 }));
  ev.push({ t: t + 0.02, note: "D5", dur: 3.4, vel: 0.7, hand: "r", tap: true });
  ev.sort((a, b) => a.t - b.t);
  return { ev, end: t + 4.4 };
}

// The cello's part of the same arrangement: the ground bass in long bows the
// first time through, then a tenor line a third under the piano's melody, and
// the last D held. Alone it is a slow solo; with the piano it is the duet.
const GROUND = ["D3", "A2", "B2", "F#2", "G2", "D2", "G2", "A2"];
const TENOR = [["D4", "F#4"], ["C#4", "E4"], ["B3", "D4"], ["A3", "C#4"], ["G3", "B3"], ["F#3", "A3"], ["G3", "B3"], ["A3", "C#4"]];

function celloScore() {
  const ev = [];
  const human = () => (Math.random() - 0.5) * 0.03;
  let t = 0.4;
  for (let pass = 0; pass < 2; pass++) {
    for (let i = 0; i < 8; i++) {
      if (pass === 0) ev.push({ t: t + human(), note: GROUND[i], dur: 4 * E8 - 0.06, vel: 0.62 + Math.random() * 0.06, bow: true });
      else TENOR[i].forEach((n, j) => ev.push({ t: t + j * 2 * E8 + human(), note: n, dur: 2 * E8 - 0.04, vel: 0.6 + Math.random() * 0.08, bow: true }));
      t += 4 * E8;
    }
  }
  ev.push({ t: t + 0.02, note: "D3", dur: 3.4, vel: 0.66, bow: true });
  ev.sort((a, b) => a.t - b.t);
  return { ev, end: t + 4.4 };
}

// The flute's part: the canon's second phrase high above the piano the first
// time through, then a rising figure on each chord, and the last D held.
const DESCANT = ["D6", "C#6", "B5", "A5", "G5", "F#5", "G5", "E5"];
const RISE = [["F#5", "A5", "D6", "A5"], ["E5", "A5", "C#6", "A5"], ["D5", "F#5", "B5", "F#5"], ["C#5", "F#5", "A5", "F#5"], ["D5", "G5", "B5", "G5"], ["D5", "F#5", "A5", "F#5"], ["D5", "G5", "B5", "G5"], ["E5", "A5", "C#6", "E6"]];

function fluteScore() {
  const ev = [];
  const human = () => (Math.random() - 0.5) * 0.03;
  let t = 0.4;
  for (let pass = 0; pass < 2; pass++) {
    for (let i = 0; i < 8; i++) {
      if (pass === 0) ev.push({ t: t + human(), note: DESCANT[i], dur: 4 * E8 - 0.1, vel: 0.5 + Math.random() * 0.06, own: true });
      else RISE[i].forEach((n, j) => ev.push({ t: t + j * E8 + human(), note: n, dur: E8 - 0.03, vel: 0.5 + Math.random() * 0.08, own: true }));
      t += 4 * E8;
    }
  }
  ev.push({ t: t + 0.02, note: "D6", dur: 3.2, vel: 0.56, own: true });
  ev.sort((a, b) => a.t - b.t);
  return { ev, end: t + 4.4 };
}

// The xylophone's part: two bright notes off the beat in each bar, then a
// falling figure on each chord, and a last roll.
const CHIME = [["A5", "D6"], ["E6", "C#6"], ["F#5", "B5"], ["C#6", "A5"], ["D6", "B5"], ["A5", "F#5"], ["B5", "D6"], ["C#6", "E6"]];
const FALL = [["D6", "A5", "F#5", "A5"], ["C#6", "A5", "E5", "A5"], ["B5", "F#5", "D5", "F#5"], ["A5", "F#5", "C#5", "F#5"], ["B5", "G5", "D5", "G5"], ["A5", "F#5", "D5", "F#5"], ["B5", "G5", "D5", "G5"], ["C#6", "A5", "E5", "A5"]];

function xylophoneScore() {
  const ev = [];
  const human = () => (Math.random() - 0.5) * 0.018;
  let t = 0.4;
  for (let pass = 0; pass < 2; pass++) {
    for (let i = 0; i < 8; i++) {
      if (pass === 0) CHIME[i].forEach((n, j) => ev.push({ t: t + (1 + 2 * j) * E8 + human(), note: n, dur: 0.6, vel: 0.5 + Math.random() * 0.08, own: true }));
      else FALL[i].forEach((n, j) => ev.push({ t: t + j * E8 + human(), note: n, dur: 0.5, vel: (j === 0 ? 0.58 : 0.44) + Math.random() * 0.06, own: true }));
      t += 4 * E8;
    }
  }
  ["D5", "F#5", "A5", "D6"].forEach((n, j) => ev.push({ t: t + j * 0.11, note: n, dur: 1.6, vel: 0.5, own: true }));
  ev.sort((a, b) => a.t - b.t);
  return { ev, end: t + 4.4 };
}

// The harp's part: one rolled chord to a bar, then a roll and two answering
// plucks above it, and a long roll up to the last D.
function harpScore() {
  const ev = [];
  const human = () => (Math.random() - 0.5) * 0.02;
  const up = (n) => n.replace(/\d/, (d) => String(Number(d) + 1));
  let t = 0.4;
  for (let pass = 0; pass < 2; pass++) {
    for (let i = 0; i < 8; i++) {
      const chord = ARP[PROG[i]];
      chord.forEach((n, j) => ev.push({ t: t + j * 0.075 + human(), note: n, dur: 4 * E8, vel: (j === 0 ? 0.56 : 0.42) + Math.random() * 0.06, own: true }));
      if (pass === 1) [up(chord[2]), up(chord[3])].forEach((n, j) => ev.push({ t: t + (2 + j) * E8 + human(), note: n, dur: 2 * E8, vel: 0.46 + Math.random() * 0.06, own: true }));
      t += 4 * E8;
    }
  }
  ["D2", "A2", "D3", "F#3", "A3", "D4", "F#4", "A4", "D5"].forEach((n, j) => ev.push({ t: t + j * 0.09, note: n, dur: 3.4, vel: 0.5, own: true }));
  ev.sort((a, b) => a.t - b.t);
  return { ev, end: t + 4.4 };
}

const SCORES = { piano: pianoScore, cello: celloScore, flute: fluteScore, xylophone: xylophoneScore, harp: harpScore };

// Dance loop: an original four-bar pattern, 120 bpm.
const CHORD = [["E4", "G4", "C5"], ["E4", "A4", "C5"], ["F4", "A4", "C5"], ["D4", "G4", "B4"]];
const BASS = ["C2", "A1", "F1", "G1"];
const LEAD = [{ 0: "E5", 2: "G5", 3: "A5", 5: "G5", 6: "E5" }, { 0: "C5", 2: "E5", 3: "A5", 5: "G5", 6: "E5" }, { 0: "F5", 2: "A5", 3: "C6", 5: "A5", 6: "F5" }, { 0: "D5", 2: "G5", 3: "B5", 5: "A5", 6: "G5", 7: "D5" }];
const DANCE_BEAT = 0.5;

/**
 * `opts.instrument`: "piano" (default) or "cello"; `opts.baseUrl`: where its
 * samples are; `opts.onReady` / `opts.onError` report the sample load. Without
 * `Tone`, or before the samples are in, the performances still run, silently,
 * on the wall clock.
 */
export function createPerformer(Tone, pet, opts = {}) {
  const instrument = SCORES[opts.instrument] ? opts.instrument : "piano";
  // The cello dances an octave lower than it is written, where it sounds like itself.
  const down = (n) => (instrument === "cello" ? n.replace(/\d/, (d) => String(Math.max(2, Number(d) - 1))) : n);
  let piano = null, kick = null, hat = null, ready = false;
  if (Tone) {
    const reverb = new Tone.Reverb({ decay: 2.4, wet: 0.2 }).toDestination();
    piano = new Tone.Sampler({
      urls: Object.fromEntries(SAMPLE_SETS[instrument].map((n) => [n, n.replace("#", "s") + ".mp3"])),
      release: VOICE[instrument].release,
      attack: VOICE[instrument].attack,
      volume: VOICE[instrument].volume,
      baseUrl: opts.baseUrl,
      onload: () => {
        ready = true;
        opts.onReady?.();
      },
      onerror: (e) => opts.onError?.(e),
    }).connect(reverb);
    kick = new Tone.MembraneSynth({ volume: -15, pitchDecay: 0.03, octaves: 4, envelope: { attack: 0.001, decay: 0.22, sustain: 0 } }).toDestination();
    hat = new Tone.NoiseSynth({ volume: -30, envelope: { attack: 0.001, decay: 0.04, sustain: 0 } }).connect(new Tone.Filter(7000, "highpass").toDestination());
  }

  let playing = null; // { kind, beat, end, done, ev?, cursor? }
  let audioClock = false;
  let clock0 = 0;
  let kbAlpha = 0;
  let stringGlow = 0;
  const lit = [];
  const sparks = [];

  function stopMusic() {
    if (!Tone) return;
    Tone.getTransport().stop();
    Tone.getTransport().cancel();
    piano.releaseAll();
  }

  function danceStep(i, time) {
    const bar = Math.floor(i / 8) % 4, s = i % 8;
    if (s % 2 === 0) kick.triggerAttackRelease("C1", 0.12, time, s === 0 ? 1 : 0.8);
    else hat.triggerAttackRelease(0.04, time);
    if (instrument === "piano" && (s === 0 || s === 3 || s === 6)) piano.triggerAttackRelease(CHORD[bar], 0.3, time, 0.4);
    // The bass line is for those that reach it: the flute and the xylophone only carry the tune.
    const low = instrument === "piano" || instrument === "cello" || instrument === "harp";
    if (low && (s === 0 || s === 4)) piano.triggerAttackRelease(instrument === "piano" ? BASS[bar] : BASS[bar].replace(/\d/, (d) => String(Number(d) + 1)), 0.5, time, 0.62);
    if (LEAD[bar][s]) piano.triggerAttackRelease(down(LEAD[bar][s]), 0.35, time, 0.6);
  }

  /**
   * `mode`: "idle" | "play" (its piece on its instrument; "piano" means the
   * same) | "dance". `silent`: no sound (motion on the wall clock). `bars`: how
   * long to dance (0 = until stopped). `loop`: start the piece over when it
   * ends. `at`: a wall-clock time (ms) to begin at, so two pets in two windows
   * start together. `done()` runs when the performance ends. `score`: play
   * these notes instead of its own piece ({ ev, end }, events as the pieces
   * write them: the piano's with `hand` and `tap`, the cello's with `bow`, the
   * others' with `own`); 收工曲 hands each its part this way.
   */
  async function start(mode, { silent = false, bars = 0, loop = false, at = 0, done, score } = {}) {
    if (mode === "play" || mode === "piano") mode = instrument;
    stopMusic();
    pet.setMode(mode);
    lit.length = sparks.length = 0;
    playing = null;
    if (mode === "idle") return;
    audioClock = ready && !silent;
    if (audioClock) {
      // Without a user gesture the context may never resume: do not wait on it forever.
      await Promise.race([Tone.start(), new Promise((r) => setTimeout(r, 400))]).catch(() => {});
      audioClock = Tone.getContext().state === "running";
    }
    if (pet.mode !== mode) return; // stopped or replaced while the audio was waking
    // A shared start: both windows count the same lead-in from the same wall-clock moment.
    const lead = at ? Math.max(0, (at - Date.now()) / 1000) : 0;
    clock0 = performance.now() + lead * 1000;
    const transport = Tone?.getTransport();
    if (mode !== "dance") {
      const sc = score ?? SCORES[instrument]();
      playing = { kind: mode, beat: 2 * E8, ev: sc.ev, cursor: 0, end: sc.end, again: loop ? { silent, loop, done } : null, done };
      for (const e of sc.ev) {
        if (e.tap) pet.tap(e.hand, e.t);
        if (e.bow) pet.bow(e.t, e.dur);
        if (e.own) pet.note(e.t, e.dur, pitchOf(e.note));
      }
      if (audioClock) for (const e of sc.ev) transport.schedule((time) => piano.triggerAttackRelease(e.note, e.dur, time, e.vel), e.t + lead);
      playing.lead = lead;
    } else {
      playing = { kind: "dance", beat: DANCE_BEAT, end: bars ? bars * 4 * DANCE_BEAT : Infinity, again: null, done, lead };
      let i = 0;
      // Bounded by the transport itself, so the loop ends even when no frames are drawn (window hidden).
      if (audioClock) transport.scheduleRepeat((time) => danceStep(i++, time), 0.25, lead, playing.end);
    }
    if (audioClock) transport.start();
  }

  /** End the performance now and go back to idling. */
  function stop() {
    const done = playing?.done;
    stopMusic();
    pet.setMode("idle");
    lit.length = sparks.length = 0;
    playing = null;
    done?.();
  }

  /** Advance one frame. `at` forces the music time (stills and tests). */
  function tick(dt, at) {
    const time = at !== undefined ? at : !playing ? null : audioClock ? Tone.getTransport().seconds - Tone.getContext().lookAhead - (playing.lead ?? 0) : (performance.now() - clock0) / 1000;
    const music = playing && time != null ? { time, beats: time / playing.beat } : null;
    if (music && playing.kind !== "dance") {
      while (playing.cursor < playing.ev.length && playing.ev[playing.cursor].t <= time) {
        const e = playing.ev[playing.cursor++];
        if (playing.kind !== "piano") {
          // Only the piano has keys to light; the others show their notes on themselves.
          if (playing.kind === "cello") stringGlow = 1;
          continue;
        }
        const key = keyFor(e.note, e.hand);
        lit.push({ ...key, glow: 1 });
        const [x, y] = keyCenter(key);
        sparks.push({ x, y, life: 0, vx: (Math.random() - 0.5) * 12 });
      }
    }
    stringGlow = Math.max(0, stringGlow - dt * 0.9);
    for (const k of lit) k.glow -= dt * 2.2;
    for (const s of sparks) {
      s.life += dt / 1.1;
      s.y -= 40 * dt;
      s.x += s.vx * dt;
    }
    for (let i = lit.length - 1; i >= 0; i--) if (lit[i].glow <= 0) lit.splice(i, 1);
    for (let i = sparks.length - 1; i >= 0; i--) if (sparks[i].life >= 1) sparks.splice(i, 1);
    kbAlpha += ((pet.mode === "piano" || pet.mode === "cello" ? 1 : 0) - kbAlpha) * (1 - Math.exp(-dt * 9));
    pet.update(dt, music);
    if (music && at === undefined && time > playing.end) {
      if (playing.again) void start(playing.kind, playing.again);
      else stop();
    }
  }

  function draw(ctx, scale) {
    if (instrument === "cello") {
      // The cello stands at its hip, nearly as tall as the player, where the tail can reach the strings.
      const [tx] = pet.rig.anchors.tail_root ?? [pet.foot[0] - 60, 0];
      const h = (pet.rig.bbox[3] - pet.rig.bbox[1]) * 0.76;
      pet.draw(ctx, scale, kbAlpha > 0.01 ? (c) => drawCello(c, tx - h * 0.265, pet.foot[1] + 2, h, kbAlpha, stringGlow, -5) : null);
      return;
    }
    pet.draw(ctx, scale, instrument === "piano" && kbAlpha > 0.01 ? (c) => drawKeyboard(c, lit, sparks, kbAlpha) : null);
  }

  return {
    start,
    stop,
    tick,
    draw,
    get ready() {
      return ready;
    },
    get playing() {
      return playing ? playing.kind : null;
    },
  };
}
