// Festive background music for the Spring Festival page. The user's own song
// (e.g. 刘德华《恭喜发财》, put in ~/.familiar/music) plays when there is one;
// otherwise an original pentatonic gong-and-drum tune is synthesized here, so
// nothing copyrighted ships with the app.

const BPM = 140;
const EIGHTH = 60 / BPM / 2;
/** D major pentatonic: degree → semitones above D4. */
const DEG: Record<string, number> = { "1": 0, "2": 2, "3": 4, "5": 7, "6": 9 };
const D4 = 293.66;

// Each token is an eighth note: a degree (' = octave up, , = octave down),
// "-" holds the previous note, "0" is a rest.
const MELODY = [
  "1 2 3 5 6 5 3 -", "5 6 1' 6 5 - - -", "3 5 6 5 3 2 1 2", "3 - 2 1 2 - - -",
  "1 2 3 5 6 5 3 -", "5 6 1' 2' 1' - 6 -", "5 3 2 3 5 6 5 3", "2 3 2 1 1 - - -",
  "5 - 6 1' 2' - 1' 6", "5 6 5 3 5 - - -", "6 - 5 6 1' 2' 1' 6", "5 - - 3 5 - - -",
  "2' 2' 1' 6 5 6 1' -", "6 5 3 5 6 - - -", "5 6 5 3 2 3 2 1", "1 - - - 0 0 0 0",
].map((bar) => bar.split(" "));
/** Bass root per bar. */
const ROOTS = ["1", "6", "1", "5", "1", "6", "5", "1", "5", "5", "6", "5", "2", "6", "5", "1"];

function freq(tok: string): number {
  const up = (tok.match(/'/g) ?? []).length - (tok.match(/,/g) ?? []).length;
  return D4 * 2 ** ((DEG[tok.replace(/[',]/g, "")] + 12 * up) / 12);
}

export class Bgm {
  ctx: AudioContext | null = null;
  private master!: GainNode;
  private noise!: AudioBuffer;
  private timer = 0;
  private next = 0;
  private step = 0;
  private playing = false;
  private volume = 0.6;
  private file: HTMLAudioElement | null = null;

  /** The AudioContext, made on first use (sound effects share it). */
  audio(): AudioContext {
    if (!this.ctx) {
      const ctx = new AudioContext();
      this.ctx = ctx;
      const comp = ctx.createDynamicsCompressor();
      comp.threshold.value = -14;
      comp.connect(ctx.destination);
      this.master = ctx.createGain();
      this.master.gain.value = this.volume * 0.5;
      this.master.connect(comp);
      this.noise = ctx.createBuffer(1, ctx.sampleRate, ctx.sampleRate);
      const d = this.noise.getChannelData(0);
      for (let i = 0; i < d.length; i++) d[i] = Math.random() * 2 - 1;
    }
    if (this.ctx.state === "suspended") void this.ctx.resume();
    return this.ctx;
  }

  /** Seconds into the user's song, or null when the built-in tune plays. */
  songTime(): number | null {
    return this.file && !this.file.paused ? this.file.currentTime : null;
  }

  get on(): boolean {
    return this.playing;
  }

  /** Play the built-in tune, or `src` (an object URL of the user's file). */
  start(src?: string | null): void {
    this.stop();
    this.playing = true;
    if (src) {
      this.file = new Audio(src);
      this.file.loop = true;
      this.file.volume = this.volume;
      this.file.play().catch(() => {});
      return;
    }
    const ctx = this.audio();
    this.next = ctx.currentTime + 0.1;
    this.step = 0;
    this.timer = window.setInterval(() => this.schedule(), 25);
  }

  stop(): void {
    this.playing = false;
    clearInterval(this.timer);
    if (this.file) {
      this.file.pause();
      this.file = null;
    }
  }

  setVolume(v: number): void {
    this.volume = v;
    if (this.ctx) this.master.gain.setTargetAtTime(v * 0.5, this.ctx.currentTime, 0.05);
    if (this.file) this.file.volume = v;
  }

  /** Browsers start audio only after a click: call this from any click. */
  wake(): void {
    if (this.ctx?.state === "suspended") void this.ctx.resume();
    if (this.file?.paused && this.playing) this.file.play().catch(() => {});
  }

  private schedule(): void {
    const ctx = this.ctx!;
    while (this.next < ctx.currentTime + 0.12) {
      this.play(this.step, this.next);
      this.next += EIGHTH;
      this.step = (this.step + 1) % (MELODY.length * 8);
    }
  }

  private play(step: number, t: number): void {
    const bar = Math.floor(step / 8);
    const e = step % 8;
    const tok = MELODY[bar][e];
    if (tok !== "-" && tok !== "0") {
      // Hold through the following "-" tokens.
      let len = 1;
      while (e + len < 8 && MELODY[bar][e + len] === "-") len++;
      this.pluck(freq(tok), t, len * EIGHTH);
    }
    // Bass on beats 1 and 3.
    if (e === 0 || e === 4) this.bass(freq(ROOTS[bar]) / 4, t);
    // 锣鼓: 咚 · 锵 · 咚咚 锵 ·
    const last = bar === MELODY.length - 1;
    if (e === 0 || e === 4 || (e === 5 && !last)) this.drum(t, e === 5 ? 0.45 : 0.9);
    if (e === 2 || e === 6) {
      this.cymbal(t, 0.35);
      this.smallGong(t);
    }
    if (last && e >= 4) this.drum(t, 0.8);
    if (step === 0) this.bigGong(t);
    if (e % 2 === 1) this.block(t);
  }

  private env(t: number, peak: number, decay: number): GainNode {
    const g = this.ctx!.createGain();
    g.gain.setValueAtTime(0.0001, t);
    g.gain.exponentialRampToValueAtTime(peak, t + 0.006);
    g.gain.exponentialRampToValueAtTime(0.0001, t + decay);
    g.connect(this.master);
    return g;
  }

  private osc(type: OscillatorType, f: number, t: number, dur: number, out: AudioNode, bendFrom = 0): OscillatorNode {
    const o = this.ctx!.createOscillator();
    o.type = type;
    o.frequency.setValueAtTime(bendFrom || f, t);
    if (bendFrom) o.frequency.exponentialRampToValueAtTime(f, t + 0.04);
    o.connect(out);
    o.start(t);
    o.stop(t + dur + 0.05);
    return o;
  }

  /** Guzheng-like pluck, with a soft sine "flute" doubling the line. */
  private pluck(f: number, t: number, hold: number): void {
    const decay = Math.max(0.5, hold * 1.6);
    const lp = this.ctx!.createBiquadFilter();
    lp.type = "lowpass";
    lp.frequency.setValueAtTime(f * 8, t);
    lp.frequency.exponentialRampToValueAtTime(f * 2, t + decay);
    lp.connect(this.env(t, 0.32, decay));
    this.osc("triangle", f, t, decay, lp, f * 1.02);
    this.osc("sawtooth", f * 2, t, decay, this.env(t, 0.03, decay * 0.6));
    const fl = this.env(t, 0.07, hold + 0.15);
    const o = this.osc("sine", f * 2, t, hold + 0.15, fl);
    const lfo = this.ctx!.createOscillator();
    const depth = this.ctx!.createGain();
    lfo.frequency.value = 5.5;
    depth.gain.value = f * 0.012;
    lfo.connect(depth).connect(o.frequency);
    lfo.start(t);
    lfo.stop(t + hold + 0.2);
  }

  private bass(f: number, t: number): void {
    this.osc("sine", f, t, 0.45, this.env(t, 0.38, 0.45));
  }

  /** Big drum: a falling sine thump. */
  private drum(t: number, v: number): void {
    const g = this.env(t, v, 0.32);
    const o = this.osc("sine", 48, t, 0.32, g);
    o.frequency.setValueAtTime(130, t);
    o.frequency.exponentialRampToValueAtTime(48, t + 0.18);
    this.hiss(t, 0.05, 900, v * 0.25, "lowpass");
  }

  private hiss(t: number, decay: number, f: number, v: number, type: BiquadFilterType): void {
    const ctx = this.ctx!;
    const s = ctx.createBufferSource();
    s.buffer = this.noise;
    const bp = ctx.createBiquadFilter();
    bp.type = type;
    bp.frequency.value = f;
    s.connect(bp).connect(this.env(t, v, decay));
    s.start(t, Math.random() * 0.5);
    s.stop(t + decay + 0.05);
  }

  private cymbal(t: number, v: number): void {
    this.hiss(t, 0.28, 7000, v, "highpass");
    this.hiss(t, 0.18, 3500, v * 0.5, "bandpass");
  }

  /** 小锣 "镗": a bright tone that bends up a little. */
  private smallGong(t: number): void {
    const g = this.env(t, 0.1, 0.42);
    const o = this.osc("triangle", 1020, t, 0.42, g);
    o.frequency.setValueAtTime(880, t);
    o.frequency.exponentialRampToValueAtTime(1020, t + 0.12);
  }

  /** 大锣 at the top of each loop: low inharmonic partials, long ring. */
  private bigGong(t: number): void {
    for (const [r, v] of [[1, 0.25], [1.48, 0.12], [2.3, 0.08], [3.1, 0.05]] as const) {
      const o = this.osc("sine", 110 * r, t, 2.4, this.env(t, v, 2.4));
      o.frequency.setValueAtTime(118 * r, t);
      o.frequency.exponentialRampToValueAtTime(110 * r, t + 0.6);
    }
    this.hiss(t, 0.6, 1800, 0.15, "bandpass");
  }

  /** 梆子 woodblock on the off-beats. */
  private block(t: number): void {
    this.osc("sine", 1750, t, 0.05, this.env(t, 0.06, 0.05));
  }

  // ---- sound effects for the game (they play even when the music is off)

  ding(pitch = 1): void {
    const ctx = this.audio();
    const t = ctx.currentTime;
    for (const [m, v] of [[1, 0.25], [2.01, 0.1]] as const) this.osc("sine", 1320 * pitch * m, t, 0.35, this.env(t, v, 0.35));
  }

  chime(): void {
    const ctx = this.audio();
    [0, 0.07, 0.14].forEach((d, i) => {
      const t = ctx.currentTime + d;
      this.osc("triangle", 1046 * 2 ** (i * 4 / 12), t, 0.5, this.env(t, 0.22, 0.5));
    });
  }

  boom(): void {
    const ctx = this.audio();
    const t = ctx.currentTime;
    this.hiss(t, 0.5, 600, 0.9, "lowpass");
    this.hiss(t, 0.12, 4000, 0.5, "highpass");
    for (let i = 1; i <= 4; i++) this.hiss(t + i * 0.06 + Math.random() * 0.03, 0.06, 3000, 0.35, "bandpass");
  }
}
