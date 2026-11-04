// petd connection: port/token come from the Tauri host, or from the URL
// (?port=&token=) when the page is opened in a plain browser for debugging.

import { t } from "./i18n";

export type State =
  | "idle" | "thinking" | "tool" | "reading" | "attention" | "done"
  | "error" | "stone" | "aborted" | "sleeping" | "offline";

export interface SessionCard {
  key: string;
  source: string;
  session: string;
  project: string;
  pid: number;
  state: State;
  since: number;
  started_at: number;
  updated_at: number;
  tool?: string;
  file?: string;
}

export interface MiniView {
  key: string;
  source: string;
  project: string;
  state: State;
  since: number;
}

export interface PetView {
  id: string;
  pack: string;
  pack_rev: number;
  state: State;
  since: number;
  camp: string | null;
  target: string;
  label?: string;
  hue: number;
  marker?: string;
  attention_kind?: string;
  bubble?: { key: string; project?: string };
  online: number;
  waiting: number;
  sessions: SessionCard[];
  minis: MiniView[];
  minis_more: number;
  activity?: string;
  size?: number;
  hat?: string;
  stage?: number;
}

export interface Action {
  sheet: string;
  frames: number;
  fps: number;
  loop: boolean;
  then?: string;
  sound?: string;
  tint?: string;
  from: string;
}

export interface Pack {
  id: string;
  name: string;
  author?: string;
  tags?: string[];
  frame: { w: number; h: number; scale: number; anchor?: [number, number] };
  actions: Record<string, Action>;
  bubbles: Record<string, string[]>;
  sounds?: Record<string, string[]>;
  description?: string;
  evolutions?: { turns: number; dir?: string; name?: string }[];
}

export interface PackInfo {
  id: string;
  name: string;
  description?: string;
  version: string;
  author: string;
  tags: string[];
  builtin: boolean;
}

export interface BrokenPack {
  dir: string;
  id?: string;
  errors: string[];
}

export interface ConfigInfo {
  mode: string;
  selection: {
    "claude-code"?: string | null;
    codex?: string | null;
    projects: Record<string, string>;
    favorites: string[];
    rotation: string;
  };
  in_use: { "claude-code": string; codex: string };
  rules: { id: string; pack: string }[];
  projects_seen: string[];
  error?: string | null;
  current_target: string;
}

export interface Conn {
  base: string;
  token: string;
}

export const inTauri = "__TAURI_INTERNALS__" in window;

export async function connect(): Promise<Conn> {
  const q = new URLSearchParams(location.search);
  if (q.get("port") && q.get("token")) {
    return { base: `http://127.0.0.1:${q.get("port")}`, token: q.get("token")! };
  }
  const { invoke } = await import("@tauri-apps/api/core");
  const info = await invoke<{ port: number; token: string }>("petd_info");
  return { base: `http://127.0.0.1:${info.port}`, token: info.token };
}

export async function getJson<T>(c: Conn, path: string): Promise<T> {
  const r = await fetch(c.base + path, { headers: { "X-Familiar-Token": c.token } });
  if (!r.ok) throw new Error(`${path}: ${r.status}`);
  return r.json() as Promise<T>;
}

export async function postJson<T = { ok: boolean; message: string }>(c: Conn, path: string, body: unknown): Promise<T> {
  const r = await fetch(c.base + path, {
    method: "POST",
    headers: { "X-Familiar-Token": c.token, "Content-Type": "application/json" },
    body: JSON.stringify(body),
  });
  const text = await r.text();
  try {
    return JSON.parse(text) as T;
  } catch {
    return { ok: r.ok, message: text } as T;
  }
}

export function fileUrl(c: Conn, pack: string, path: string, rev = 0): string {
  return `${c.base}/v1/packs/${encodeURIComponent(pack)}/files/${path}?token=${c.token}&rev=${rev}`;
}

/** Subscribe to petd pushes; reconnects forever. */
export function subscribe(c: Conn, onMsg: (type: string, data: unknown) => void): void {
  const open = () => {
    const ws = new WebSocket(`${c.base.replace("http", "ws")}/v1/ws?token=${c.token}`);
    ws.onmessage = (e) => {
      try {
        const m = JSON.parse(e.data as string);
        onMsg(m.type, m.data);
      } catch {
        /* ignore malformed */
      }
    };
    ws.onclose = () => setTimeout(open, 1000);
  };
  open();
}

// Getters so the names follow the current language at read time.
export const STATE_NAMES: Record<State, string> = {
  get idle() { return t("空闲", "Idle"); },
  get thinking() { return t("思考中", "Thinking"); },
  get tool() { return t("干活中", "Working"); },
  get reading() { return t("读代码", "Reading"); },
  get attention() { return t("等你", "Waiting for you"); },
  get done() { return t("完成", "Done"); },
  get error() { return t("出错", "Error"); },
  get stone() { return t("石化", "Petrified"); },
  get aborted() { return t("被打断", "Interrupted"); },
  get sleeping() { return t("睡觉", "Sleeping"); },
  get offline() { return t("已离线", "Offline"); },
};

export const SOURCE_NAMES: Record<string, string> = { "claude-code": "Claude Code", codex: "Codex" };

export function targetName(target: string): string {
  if (target === "claude-code") return t("Claude Code 阵营", "Claude Code team");
  if (target === "codex") return t("Codex 阵营", "Codex team");
  if (target.startsWith("project:")) return t(`项目 ${target.slice(8)}`, `Project ${target.slice(8)}`);
  if (target.startsWith("rule:")) return t(`规则 ${target.slice(5)}`, `Rule ${target.slice(5)}`);
  return target;
}

export function duration(ms: number): string {
  const m = Math.max(0, Math.floor(ms / 60000));
  if (m < 1) return t("不到 1 分钟", "under 1 min");
  if (m < 60) return t(`${m} 分钟`, `${m} min`);
  return t(`${Math.floor(m / 60)} 小时 ${m % 60} 分`, `${Math.floor(m / 60)} h ${m % 60} min`);
}

export interface Bug {
  id: number;
  boss: boolean;
  hp: number;
  max_hp: number;
  pet: string;
  born: number;
}

export interface CampDay {
  turns: number;
  clean: number;
  errors: number;
}

export interface WorldState {
  frozen: boolean;
  focus: boolean;
  arena: boolean;
  bugs: Bug[];
  tug: null | {
    offset: number;
    scoring: string;
    "claude-code": { score: number; day: CampDay };
    codex: { score: number; day: CampDay };
  };
  shadow: null | { today: number; best_now: number; best_total: number; best_day: string; ahead: boolean };
  wallet: number;
  income_today: number;
  daily_cap: number;
}

export interface CatchRow {
  ts: number;
  outcome: string;
  fish_id: string;
  rarity: string;
  coins: number;
  source: string;
}

export interface Report {
  day: string;
  work_ms: number;
  turns: number;
  clean: number;
  errors: number;
  by_camp: Record<string, CampDay>;
  catches: CatchRow[];
  bugs: number;
  bosses: number;
  coins_earned: number;
  balance: number;
}

export const RARITY_NAMES: Record<string, string> = {
  get common() { return t("普通", "Common"); },
  get rare() { return t("稀有", "Rare"); },
  get epic() { return t("史诗", "Epic"); },
  get legendary() { return t("传说", "Legendary"); },
};
export const RARITY_COLORS: Record<string, string> = { common: "#8c969e", rare: "#4a6fa5", epic: "#7d5ba6", legendary: "#b8956a" };
