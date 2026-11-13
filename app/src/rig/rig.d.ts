export interface RigLayer {
  name: string;
  file: string;
  x: number;
  y: number;
  w: number;
  h: number;
  z?: number;
  pivot?: [number, number];
}

export interface Rig {
  id: string;
  size: [number, number];
  /** x0, y0, x1, y1 of the figure at rest, in rig units. */
  bbox: [number, number, number, number];
  anchors: Record<string, [number, number]>;
  layers: RigLayer[];
}

export type RigMode = "idle" | "piano" | "cello" | "flute" | "xylophone" | "harp" | "dance";
/** The idle face. */
export type RigExpr = "neutral" | "happy" | "sleepy" | "asleep" | "calm" | "worried" | "surprised";
export type RigReaction = "poke" | "perk" | "cheer";

export class RigPet {
  readonly rig: Rig;
  /** The anatomy: the lop-eared body, or one of the upright ones. */
  readonly kind: "lop" | "cat" | "blob" | "mushroom" | "sheep";
  /** Everything but the lop: a head that tilts on a small body. */
  readonly upright: boolean;
  mode: RigMode;
  expr: RigExpr;
  /** Where the pet stands, in rig units. */
  readonly foot: [number, number];
  /**
   * Driven from outside while it idles: where it stands and how high (rig px),
   * a lean (degrees), a turn about itself (radians), where its face turns
   * (-1..1) and its head tips (degrees), what it looks at ([x, y] px of eye
   * travel) and the face it makes. Null or 0 leaves that to the pet.
   */
  play: { x: number; hop: number; rot: number; twirl: number; yaw: number | null; pitch: number | null; head: number | null; look: [number, number] | null; face: RigExpr | null };
  setMode(mode: RigMode): void;
  react(name: RigReaction): void;
  /** Turn its back to the viewer while idling; `clench` 0..1 squeezes and lifts the tail. */
  setBack(on: boolean, clench?: number): void;
  update(dt: number, music?: { time: number; beats: number } | null): void;
  draw(ctx: CanvasRenderingContext2D, scale: number, mid?: ((ctx: CanvasRenderingContext2D) => void) | null): void;
}

/** `base`: the folder holding rig.json and its layers, with a trailing slash. */
export function loadRig(base: string): Promise<RigPet>;
