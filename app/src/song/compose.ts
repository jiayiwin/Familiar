// 收工曲: the day, played back by the five. The working day (first turn to
// last) is laid over a fixed number of bars of the pets' own progression, the
// one their pieces share (D major, the canon's chords), and each bar is played
// by what really happened in that stretch of the day:
//
//   - Claude Code is the piano's right hand, Codex the cello, Gemini the flute:
//     the busier an agent was, the more notes its voice has in the bar;
//   - a failure is a clash held over the chord (a semitone above where the
//     line wants to be), and it stays until that agent's next turn goes
//     through, which resolves it (a 4–3 suspension);
//   - every turn that finished rings a note on the xylophone; a long job
//     finished, or several at once, brings the flute's descant in;
//   - the harp is the room: one rolled chord a bar, and when nobody worked
//     (you were away, or it was quiet) it is all there is;
//   - the end is the same for every day: G, A, D, all five together.
//
// Nothing is random except a small human looseness in timing, seeded by the
// day, so the same day always plays the same.

export type Inst = "piano" | "cello" | "flute" | "xylophone" | "harp";
export interface Turn { source: string; started_at: number; ended_at: number; result: string; tool_calls: number; tool_errors: number }
export interface Note { inst: Inst; t: number; note: string; dur: number; vel: number }
export interface Bar {
  i: number;
  /** The stretch of the day it stands for (ms since 1970). */
  from: number;
  to: number;
  /** Per agent: how busy (tool calls in the stretch), failures, finished turns. */
  work: Record<string, number>;
  errors: Record<string, number>;
  done: Record<string, number>;
  /** A failure from earlier that this bar resolves, per agent. */
  resolves: string[];
  quiet: boolean;
}
export interface Song { notes: Note[]; bars: Bar[]; /** seconds */ start: number; barSec: number; end: number; from: number; to: number }

/** An eighth note, as in the pets' pieces. */
export const E8 = 0.42;
const BAR = 4 * E8;
/** Bars the day is spread over (the cadence comes after). */
export const BARS = 26;

const PROG = ["D", "A", "Bm", "F#m", "G", "D", "G", "A"];
const ARP: Record<string, string[]> = { D: ["D3", "A3", "D4", "F#4"], A: ["A2", "E3", "A3", "C#4"], Bm: ["B2", "F#3", "B3", "D4"], "F#m": ["F#2", "C#3", "F#3", "A3"], G: ["G2", "D3", "G3", "B3"], Em: ["E2", "B2", "E3", "G3"] };
const ROOT: Record<string, string> = { D: "D3", A: "A2", Bm: "B2", "F#m": "F#2", G: "G2", Em: "E2" };
const TENOR = [["D4", "F#4"], ["C#4", "E4"], ["B3", "D4"], ["A3", "C#4"], ["G3", "B3"], ["F#3", "A3"], ["G3", "B3"], ["A3", "C#4"]];
const DESCANT = ["D6", "C#6", "B5", "A5", "G5", "F#5", "G5", "E5"];
const CHIME = [["A5", "D6"], ["E6", "C#6"], ["F#5", "B5"], ["C#6", "A5"], ["D6", "B5"], ["A5", "F#5"], ["B5", "D6"], ["C#6", "E6"]];
/** The scale the right hand walks on, D4 to D6. */
const SCALE = ["D4", "E4", "F#4", "G4", "A4", "B4", "C#5", "D5", "E5", "F#5", "G5", "A5", "B5", "C#6", "D6"];
const MEL = ["F#5", "E5", "D5", "C#5", "B4", "A4", "B4", "C#5"];

/** Which voice an agent sings with: its pet's instrument (the default cast). */
export const VOICE_OF: Record<string, Inst> = { "claude-code": "piano", codex: "cello", gemini: "flute", cursor: "xylophone", opencode: "harp" };

const NAMES = ["C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B"];
const midi = (n: string) => {
  const m = /^([A-G]#?)(\d)$/.exec(n)!;
  return NAMES.indexOf(m[1]) + 12 * (Number(m[2]) + 1);
};
const name = (m: number) => `${NAMES[m % 12]}${Math.floor(m / 12) - 1}`;
const up = (n: string, s: number) => name(midi(n) + s);

/** A small seeded generator: the same day plays the same. */
function rng(seed: number) {
  let s = seed >>> 0 || 1;
  return () => {
    s ^= s << 13;
    s ^= s >>> 17;
    s ^= s << 5;
    return ((s >>> 0) % 10000) / 10000;
  };
}

/** How much of [a, b] falls inside [x, y]. */
const overlap = (a: number, b: number, x: number, y: number) => Math.max(0, Math.min(b, y) - Math.max(a, x));

export function compose(turns: Turn[], seed = 1): Song {
  const r = rng(seed);
  const loose = (w = 0.022) => (r() - 0.5) * w;
  const notes: Note[] = [];
  const add = (inst: Inst, t: number, note: string, dur: number, vel: number) => notes.push({ inst, t, note, dur, vel: Math.max(0.05, Math.min(1, vel)) });
  const sorted = [...turns].filter((t) => t.ended_at > t.started_at).sort((a, b) => a.started_at - b.started_at);
  const from = sorted.length ? sorted[0].started_at : Date.now() - 8 * 3_600_000;
  const to = sorted.length ? Math.max(...sorted.map((t) => t.ended_at)) : Date.now();
  const span = Math.max(3_600_000, to - from);
  const sources = [...new Set(sorted.map((t) => t.source))];

  // ---- what happened in each stretch
  const bars: Bar[] = [];
  for (let i = 0; i < BARS; i++) {
    const a = from + (span * i) / BARS, b = from + (span * (i + 1)) / BARS;
    const bar: Bar = { i, from: a, to: b, work: {}, errors: {}, done: {}, resolves: [], quiet: true };
    for (const t of sorted) {
      const o = overlap(t.started_at, t.ended_at, a, b);
      if (o > 0) {
        // Its tool calls, spread over the turn, counted for the part inside this stretch.
        bar.work[t.source] = (bar.work[t.source] ?? 0) + (t.tool_calls + 1) * (o / (t.ended_at - t.started_at));
        bar.quiet = false;
      }
      if (t.ended_at >= a && t.ended_at < b) {
        if (t.result === "ok" && t.tool_errors === 0) bar.done[t.source] = (bar.done[t.source] ?? 0) + 1;
        if (t.result === "error" || t.tool_errors > 0) bar.errors[t.source] = (bar.errors[t.source] ?? 0) + 1;
        bar.quiet = false;
      }
    }
    bars.push(bar);
  }
  // How busy each agent was at its busiest: the scale for its notes.
  const peak: Record<string, number> = {};
  for (const s of sources) peak[s] = Math.max(1e-6, ...bars.map((b) => b.work[s] ?? 0));
  // A failure stays until that agent's next clean finish.
  const open: Record<string, boolean> = {};
  for (const bar of bars) {
    for (const s of sources) {
      if (open[s] && (bar.done[s] ?? 0) > 0) {
        bar.resolves.push(s);
        open[s] = false;
      }
      if ((bar.errors[s] ?? 0) > (bar.done[s] ?? 0)) open[s] = true;
    }
  }

  // ---- the opening: the harp rolls the home chord, the piano answers.
  const start = 0.3;
  ARP.D.forEach((n, j) => add("harp", start + j * 0.09, n, 2.6, 0.42));
  add("piano", start + 0.5, "D5", 1.6, 0.42);
  let t = start + BAR;

  let walk = SCALE.indexOf("F#5");
  bars.forEach((bar, i) => {
    const chord = PROG[i % 8];
    const tones = ARP[chord];
    const any = !bar.quiet;
    const busy = (s: string) => Math.min(1, (bar.work[s] ?? 0) / peak[s]);

    // The room: the harp rolls the chord; alone, slower and softer, it is all you hear.
    if (any) tones.forEach((n, j) => add("harp", t + j * 0.075 + loose(), n, BAR, (j === 0 ? 0.4 : 0.3) + loose(0.06)));
    else [tones[0], tones[2]].forEach((n, j) => add("harp", t + j * 0.5 + loose(), n, BAR * 1.4, 0.26));

    // Claude Code: the piano. Left hand when anyone works, right hand as busy as Claude was.
    if (any) tones.forEach((n, j) => add("piano", t + j * E8 + loose(), n, 1.2, (j === 0 ? 0.36 : 0.26) + loose(0.05)));
    const cc = busy("claude-code");
    if (cc > 0.02) {
      const count = 1 + Math.round(cc * 3);
      // Walk the scale toward this bar's tune note, landing on it at the start of the bar.
      const target = SCALE.indexOf(MEL[i % 8]);
      walk = target;
      for (let k = 0; k < count; k++) {
        const n = SCALE[Math.max(0, Math.min(SCALE.length - 1, walk))];
        add("piano", t + (k * 4 * E8) / count + 0.012 + loose(), n, (4 * E8) / count + 0.25, 0.5 + cc * 0.2 + loose(0.06));
        walk += r() < 0.55 ? -1 : 1;
      }
    }
    // Codex: the cello. A bowed root when it worked, a tenor line when it worked hard; a soft pedal under the others.
    const cx = busy("codex");
    if (cx > 0.02) {
      add("cello", t + loose(0.03), ROOT[chord], BAR - 0.06, 0.5 + cx * 0.15);
      if (cx > 0.45) TENOR[i % 8].forEach((n, j) => add("cello", t + j * 2 * E8 + loose(0.03), n, 2 * E8 - 0.04, 0.48 + cx * 0.12));
    } else if (any && i % 2 === 0) add("cello", t + loose(0.03), ROOT[chord], 2 * BAR - 0.1, 0.3);
    // Gemini (when it was there): the flute, a rising figure.
    const gm = busy("gemini");
    if (gm > 0.02) tones.slice(1).forEach((n, j) => add("flute", t + j * E8 + loose(), up(n, 24), E8 - 0.03, 0.4 + gm * 0.15));

    // A failure: a clash a semitone above the tune, held over the bar. Resolved later by a 4–3.
    for (const s of sources) {
      if ((bar.errors[s] ?? 0) > (bar.done[s] ?? 0)) {
        const inst = VOICE_OF[s] ?? "piano";
        const hi = inst === "cello" ? up(ROOT[chord], 13) : up(MEL[i % 8], 1);
        add(inst === "piano" || inst === "cello" ? inst : "piano", t + E8 + loose(), hi, 3 * E8, 0.42);
      }
    }
    for (const s of bar.resolves) {
      const inst = VOICE_OF[s] === "cello" ? "cello" : "piano";
      const third = inst === "cello" ? up(tones[3], -12) : up(tones[3], 12);
      // The fourth above the root sits a semitone over a major third, a tone over a minor one.
      add(inst, t + loose(), up(third, chord.endsWith("m") ? 2 : 1), E8 * 1.2, 0.5);
      add(inst, t + 1.3 * E8 + loose(), third, 2.5 * E8, 0.52);
    }

    // Each finished turn: a note on the xylophone, off the beat (three at most).
    const done = Object.values(bar.done).reduce((s, n) => s + n, 0);
    for (let k = 0; k < Math.min(3, done); k++) add("xylophone", t + (1 + 2 * (k % 2)) * E8 + (k >= 2 ? E8 / 2 : 0) + loose(), CHIME[i % 8][k % 2], 0.6, 0.42 + loose(0.06));
    // Several finished at once, or a long job done: the flute's descant comes in.
    const long = sorted.some((x) => x.result === "ok" && x.ended_at >= bar.from && x.ended_at < bar.to && x.ended_at - x.started_at >= 15 * 60_000);
    if (done >= 3 || long) add("flute", t + loose(), DESCANT[i % 8], BAR - 0.1, 0.42);

    t += BAR;
  });

  // ---- the end, the same every day: G, A, D, all five.
  const close = ["G", "A", "D"];
  close.forEach((c, k) => {
    const tones = ARP[c];
    tones.forEach((n, j) => add("piano", t + j * E8 + loose(), n, 1.4, 0.4));
    add("piano", t + 0.012, k === 2 ? "D5" : k === 1 ? "C#5" : "B4", k === 2 ? 3.6 : BAR, 0.62);
    add("cello", t, ROOT[c], k === 2 ? 3.6 : BAR - 0.06, 0.55);
    add("flute", t + loose(), k === 2 ? "D6" : k === 1 ? "E6" : "D6", k === 2 ? 3.2 : BAR - 0.1, 0.46);
    if (k < 2) CHIME[k === 0 ? 4 : 7].forEach((n, j) => add("xylophone", t + (1 + 2 * j) * E8, n, 0.6, 0.44));
    t += BAR;
  });
  t -= BAR;
  ["D2", "A2", "D3", "F#3", "A3", "D4", "F#4", "A4", "D5"].forEach((n, j) => add("harp", t + j * 0.09, n, 3.4, 0.46));
  ["D5", "F#5", "A5", "D6"].forEach((n, j) => add("xylophone", t + 0.4 + j * 0.11, n, 1.6, 0.42));
  notes.sort((a, b) => a.t - b.t);
  return { notes, bars, start: start + BAR, barSec: BAR, end: t + 4.4, from, to };
}
