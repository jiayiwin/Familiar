import type { RigPet } from "./rig.js";

export const SAMPLES: string[];
export const CELLO_SAMPLES: string[];
export const FLUTE_SAMPLES: string[];
export const XYLOPHONE_SAMPLES: string[];
export const HARP_SAMPLES: string[];

/** A part to play instead of its own piece: when each note sounds (s), and what the player does with it. */
export interface Score {
  ev: { t: number; note: string; dur: number; vel: number; hand?: "l" | "r"; tap?: boolean; bow?: boolean; own?: boolean }[];
  end: number;
}

export interface Performer {
  /**
   * "play": its piece on its own instrument. `silent`: no sound. `bars`: how
   * long to dance (0 = until stopped). `loop`: start the piece over when it
   * ends. `at`: wall-clock ms to begin at, so two windows start together.
   * `done` runs when it ends.
   */
  start(mode: "idle" | "play" | "dance", opts?: { silent?: boolean; bars?: number; loop?: boolean; at?: number; done?: () => void; score?: Score }): Promise<void>;
  stop(): void;
  tick(dt: number, at?: number): void;
  draw(ctx: CanvasRenderingContext2D, scale: number): void;
  readonly ready: boolean;
  readonly playing: "piano" | "cello" | "flute" | "xylophone" | "harp" | "dance" | null;
}

/** `tone`: the Tone.js module, or null for a silent performer. */
export function createPerformer(
  tone: unknown,
  pet: RigPet,
  opts?: { instrument?: string; baseUrl?: string; onReady?: () => void; onError?: (e: unknown) => void },
): Performer;
