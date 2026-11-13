// The five pets as skins. A skin is everything about the product that changes
// with the pet: the colours of every window (taken from that pet's own art),
// the motif that seals its letters, the instrument it plays and the notes it
// calls you with, and where its body's layers live. Windows opened from a pet
// wear that pet's skin; windows that belong to no pet wear the main pet's.

import { getJson, subscribe, type Conn, type PetView } from "./api";

export type SkinId = "zimeng" | "mashu" | "paopao" | "gugu" | "rongrong";
export type Instrument = "piano" | "cello" | "flute" | "xylophone" | "harp";

export interface Skin {
  id: SkinId;
  zh: string;
  en: string;
  about: [string, string];
  instrument: Instrument;
  /** How it plays, in a few words (the gallery's "plays" line). */
  plays: [string, string];
  /** Folder of its rig layers, or null while it has only been drawn. */
  rig: string | null;
  /** A still, for the pets that cannot move yet. */
  art: string | null;
  /** The two notes of "waiting for you", in its own instrument's sample folder. */
  chime: [string, string];
  /** Its accessory, as a 24-unit path: seals its letters and marks its dates. */
  motif: string;
  colors: {
    ground: string; paper: string; ink: string; ink2: string; muted: string; line: string; rule: string; well: string;
    accent: string; accentDeep: string; accentSoft: string; accentText: string; accentOff: string; hover: string;
    room: string; roomNight: string; roomLine: string; cushion: string;
    /** The card it sits on in the radio. */
    stage: string; stageInk: string; stageLine: string;
    /** r,g,b of the soft shadow under cards. */
    shade: string;
    /** The pale fill of its motif. */
    motifFill: string; motifLine: string;
  };
}

const STAR = "M12 3.2l2.5 5.4 5.9.7-4.4 4 1.2 5.8L12 16.2l-5.2 2.9 1.2-5.8-4.4-4 5.9-.7z";
const BOW = "M12 12.2c-2.6-3.4-6.3-4.6-8.2-3.9-1 .4-1 7 0 7.4 1.9.7 5.6-.5 8.2-3.5zm0 0c2.6-3.4 6.3-4.6 8.2-3.9 1 .4 1 7 0 7.4-1.9.7-5.6-.5-8.2-3.5zM10.3 10.3h3.4v3.8h-3.4z";
const SHELL = "M12 4.5c4.7 0 8.2 3.6 8.2 8.2 0 1.3-.5 2.4-1.3 3.2l-3.4 3.1H8.5l-3.4-3.1c-.8-.8-1.3-1.9-1.3-3.2 0-4.6 3.5-8.2 8.2-8.2zM12 5v13.5M8.3 6.3l2 12.2M15.7 6.3l-2 12.2";
const CAP = "M3.5 13.5c0-5.2 3.8-9 8.5-9s8.5 3.8 8.5 9c0 1.1-3.8 2-8.5 2s-8.5-.9-8.5-2zM9 15.4v2.9a3 3 0 0 0 6 0v-2.9";
const FLOWER = "M12 9.3a2.7 2.7 0 1 0 0 5.4 2.7 2.7 0 0 0 0-5.4zM12 3.5a2.8 2.8 0 0 1 2.3 4.4M19.4 8.5a2.8 2.8 0 0 1-1.9 4.6M17.3 18.2a2.8 2.8 0 0 1-4.9-.6M6.7 18.2a2.8 2.8 0 0 0 4.9-.6M4.6 8.5a2.8 2.8 0 0 0 1.9 4.6M12 3.5a2.8 2.8 0 0 0-2.3 4.4";

export const SKINS: Record<SkinId, Skin> = {
  zimeng: {
    id: "zimeng", zh: "紫梦", en: "Zimeng",
    about: ["奶紫色的垂耳兔。话少，看得久。", "A milk-purple lop rabbit. Says little, watches long."],
    instrument: "piano", plays: ["钢琴，用两只耳朵弹", "The piano, with both ears"],
    rig: "rig/zimeng/", art: null, chime: ["A4", "Ds5"], motif: STAR,
    colors: {
      ground: "#faf6f0", paper: "#fffaf2", ink: "#5b4a57", ink2: "#6d5d6a", muted: "#776775", line: "#d9cbc8", rule: "#efe6e2", well: "#f0e9e4",
      accent: "#7a5fc0", accentDeep: "#6a50b0", accentSoft: "#ece4ff", accentText: "#5b4696", accentOff: "#cfc3e6", hover: "#f5eefc",
      room: "#f3ecf9", roomNight: "#ebe3f4", roomLine: "#e3d8ea", cushion: "#e0d4ff",
      stage: "#e9eff8", stageInk: "#5d6c85", stageLine: "#cfd9e8",
      shade: "110,88,140", motifFill: "#f3e9cf", motifLine: "#cdbdba",
    },
  },
  mashu: {
    id: "mashu", zh: "麻薯", en: "Mashu",
    about: ["奶油色的猫，系着薄荷绿的领结。黏人，爱凑近。", "A cream cat in a mint bow. Clingy; likes to be close."],
    instrument: "cello", plays: ["大提琴，尾巴当弓", "The cello, with its tail for a bow"],
    rig: "rig/mashu/", art: "art/mashu.png", chime: ["D3", "A3"], motif: BOW,
    colors: {
      ground: "#fbf7ef", paper: "#fffbf3", ink: "#5e4a3e", ink2: "#705c50", muted: "#7a6a5e", line: "#e0d0bf", rule: "#f1e8dc", well: "#f3ebdf",
      accent: "#2a7b69", accentDeep: "#236a5a", accentSoft: "#d9f1e8", accentText: "#1f6455", accentOff: "#b9d9cf", hover: "#edf8f3",
      room: "#f8efe1", roomNight: "#f1e6d5", roomLine: "#ecdfcc", cushion: "#cfeadd",
      stage: "#fbe9e3", stageInk: "#8a5f56", stageLine: "#f0d2c9",
      shade: "120,96,70", motifFill: "#cfeadd", motifLine: "#9fcdbb",
    },
  },
  paopao: {
    id: "paopao", zh: "泡泡", en: "Paopao",
    about: ["雾蓝色的水滴，头顶一根卷须，别着一枚贝壳。慢半拍。", "A mist-blue drop with a curl on top and a shell on its head. Half a beat slow."],
    instrument: "flute", plays: ["长笛，吹泡泡吹出来的", "The flute, blown like bubbles"],
    rig: "rig/paopao/", art: "art/paopao.png", chime: ["E5", "A5"], motif: SHELL,
    colors: {
      ground: "#f7f9fb", paper: "#fcfdff", ink: "#44506a", ink2: "#55617a", muted: "#66728a", line: "#cfd8e4", rule: "#e6ecf3", well: "#e9eef5",
      accent: "#3f6ea8", accentDeep: "#33598a", accentSoft: "#dce8f7", accentText: "#2f5688", accentOff: "#bccde4", hover: "#eef4fb",
      room: "#e3ebf6", roomNight: "#d9e3f0", roomLine: "#d0dcec", cushion: "#c9d9ef",
      stage: "#e6f2f1", stageInk: "#4d6f6c", stageLine: "#cfe3e0",
      shade: "70,90,130", motifFill: "#f4ecd8", motifLine: "#cfc3a6",
    },
  },
  gugu: {
    id: "gugu", zh: "菇菇", en: "Gugu",
    about: ["藕粉色的小蘑菇。胆小，躲在菌盖下面看。", "A lotus-pink mushroom. Shy; watches from under its cap."],
    instrument: "xylophone", plays: ["木琴，敲自己的菌盖", "The xylophone, tapped out on its own cap"],
    rig: "rig/gugu/", art: "art/gugu.png", chime: ["C5", "G5"], motif: CAP,
    colors: {
      ground: "#fbf6f5", paper: "#fffaf8", ink: "#5e4650", ink2: "#705761", muted: "#7c6670", line: "#e3cfd3", rule: "#f3e6e8", well: "#f4e9ea",
      accent: "#a84f72", accentDeep: "#8f3f5f", accentSoft: "#f8e1ea", accentText: "#8a3d5c", accentOff: "#e3c3cf", hover: "#fcf0f4",
      room: "#f8e8ec", roomNight: "#f1dde2", roomLine: "#eed5db", cushion: "#f1d3dc",
      stage: "#f6efe2", stageInk: "#7a684c", stageLine: "#e9dcc4",
      shade: "130,90,100", motifFill: "#f8e1ea", motifLine: "#dcb6c2",
    },
  },
  rongrong: {
    id: "rongrong", zh: "茸茸", en: "Rongrong",
    about: ["鼠尾草绿的小羊，耳边别着一朵花。不急。", "A sage-green lamb with a flower by its ear. In no hurry."],
    instrument: "harp", plays: ["竖琴，拨自己的卷毛", "The harp, plucked on its own curls"],
    rig: "rig/rongrong/", art: "art/rongrong.png", chime: ["C5", "G5"], motif: FLOWER,
    colors: {
      ground: "#f7f8f3", paper: "#fcfdf8", ink: "#46503f", ink2: "#57624f", muted: "#68735f", line: "#d2dbc9", rule: "#e8eee2", well: "#ebf0e5",
      accent: "#4f7548", accentDeep: "#3f613a", accentSoft: "#e0ecda", accentText: "#3d5f37", accentOff: "#c3d5bd", hover: "#f0f6ec",
      room: "#e9efe3", roomNight: "#dfe7d8", roomLine: "#d7e0cf", cushion: "#d3e2cb",
      stage: "#f3ece4", stageInk: "#75634f", stageLine: "#e5d8c7",
      shade: "90,110,80", motifFill: "#f1dbe6", motifLine: "#d3b2c2",
    },
  },
};

export const ORDER: SkinId[] = ["zimeng", "mashu", "paopao", "gugu", "rongrong"];

export function skin(id: string | null | undefined): Skin {
  return SKINS[(id ?? "") as SkinId] ?? SKINS.zimeng;
}

/** "弹首曲子" / "拉首曲子": the menu's name for a pet playing its piece. */
export const PLAY_LABEL: Record<Instrument, [zh: string, en: string]> = {
  piano: ["弹首曲子", "Play the piano"],
  cello: ["拉首曲子", "Play the cello"],
  flute: ["吹首曲子", "Play the flute"],
  xylophone: ["敲首曲子", "Play the xylophone"],
  harp: ["拨首曲子", "Play the harp"],
};

/** Where the samples of an instrument live. */
export const sampleUrl = (i: Instrument) => `audio/${i}/`;

let current: Skin = SKINS.zimeng;
/** The skin this window wears. */
export const worn = (): Skin => current;

/** Dress this window: every colour the stylesheets use comes from here. */
export function applySkin(id: string | null | undefined): Skin {
  const s = skin(id);
  current = s;
  const c = s.colors;
  const st = document.documentElement.style;
  const set = (k: string, v: string) => st.setProperty(k, v);
  for (const [k, v] of Object.entries({
    "--ground": c.ground, "--paper": c.paper, "--ink": c.ink, "--ink-2": c.ink2, "--muted": c.muted, "--line": c.line, "--rule": c.rule, "--well": c.well,
    "--accent": c.accent, "--accent-deep": c.accentDeep, "--accent-soft": c.accentSoft, "--accent-text": c.accentText, "--accent-off": c.accentOff, "--hover": c.hover,
    "--room": c.room, "--room-night": c.roomNight, "--room-line": c.roomLine, "--cushion": c.cushion,
    "--stage": c.stage, "--stage-ink": c.stageInk, "--stage-line": c.stageLine,
    "--shade": c.shade, "--motif-fill": c.motifFill, "--motif-line": c.motifLine,
    // The pages built on ui.css name the same things differently.
    "--bg": c.ground, "--panel": c.paper, "--raised": c.paper, "--line-2": c.line, "--chip": c.well, "--faint": c.accentOff,
    "--indigo": c.accent, "--indigo-2": c.accentDeep, "--accent-2": c.accentDeep, "--t-lilac": c.accentSoft, "--t-sky": c.stage,
    "--shadow": `0 1px 2px rgba(${c.shade},.06), 0 8px 22px rgba(${c.shade},.1)`,
    "--shadow-lift": `0 2px 4px rgba(${c.shade},.08), 0 14px 34px rgba(${c.shade},.2)`,
  })) set(k, v);
  // ui.css calls the hairline `--line` where the other pages call it `--rule`.
  if (document.documentElement.dataset.sheet === "ui") {
    set("--line", c.rule);
    set("--accent-soft", `color-mix(in srgb, ${c.accent} 12%, transparent)`);
  }
  document.documentElement.dataset.skin = s.id;
  for (const el of document.querySelectorAll<SVGPathElement>("path[data-motif]")) el.setAttribute("d", s.motif);
  return s;
}

/** The skin of the pet a window was opened from (`?pet=`), or the main pet's. */
/**
 * Keep an open window in the right skin. When the agent it belongs to gets
 * another pet (`petId()` says which agent; none means the main one, Claude
 * Code's), `then` runs: by default the page loads again as that pet's.
 */
export function followSkin(conn: Conn, petId: () => string | null = () => null, then: () => void = () => location.reload()): void {
  subscribe(conn, (type, data) => {
    if (type !== "pet.state") return;
    const v = data as PetView;
    if (v?.id === (petId() ?? "claude-code") && v.skin && v.skin !== current.id) then();
  });
}

export async function skinFor(conn: Conn, petId?: string | null): Promise<{ skin: Skin; pet: PetView | null }> {
  if (petId) {
    const { pets } = await getJson<{ pets: PetView[] }>(conn, "/v1/pets").catch(() => ({ pets: [] as PetView[] }));
    const pet = pets.find((p) => p.id === petId) ?? null;
    if (pet) return { skin: skin(pet.skin), pet };
  }
  const s = await getJson<{ pets?: { main?: string } }>(conn, "/v1/settings").catch(() => null);
  return { skin: skin(s?.pets?.main), pet: null };
}
