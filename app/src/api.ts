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

export interface PetView {
  /** One pet per agent: `claude-code`, `codex`, … */
  id: string;
  state: State;
  since: number;
  camp: string | null;
  /** The agent's short name for the tag under the pet ("Claude", "Codex"). */
  marker?: string;
  attention_kind?: string;
  bubble?: { key: string; project?: string };
  online: number;
  waiting: number;
  sessions: SessionCard[];
  activity?: string;
  /** The user's size setting, as a scale. */
  size?: number;
  /** Which of the five pets this is: its body, its skin, its voice. */
  skin?: string;
}

export interface Conn {
  base: string;
  token: string;
}

export const inTauri = "__TAURI_INTERNALS__" in window;

/** Inside the tabbed window each page is a frame of it (see shell.ts). */
export const embedded = window.parent !== window;

/** Hear `event` from the host: sent to this window, or handed down by the tabbed window that holds this page. */
export async function onHost<T>(event: string, cb: (payload: T) => void): Promise<void> {
  if (embedded) {
    window.addEventListener("message", (e) => {
      if (e.origin === location.origin && e.source === window.parent && e.data?.type === event) cb(e.data.payload as T);
    });
    return;
  }
  if (!inTauri) return;
  const { getCurrentWebviewWindow } = await import("@tauri-apps/api/webviewWindow");
  await getCurrentWebviewWindow().listen<T>(event, (e) => cb(e.payload));
}

/** Close this page: its tab in the tabbed window, or the window it has to itself. */
export async function closeSelf(): Promise<void> {
  if (embedded) return void window.parent.postMessage({ type: "close-tab" }, location.origin);
  if (!inTauri) return;
  const { getCurrentWindow } = await import("@tauri-apps/api/window");
  await getCurrentWindow().close();
}

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
  const r = await fetch(c.base + path, { headers: { "X-RillBuddy-Token": c.token } });
  if (!r.ok) throw new Error(`${path}: ${r.status}`);
  return r.json() as Promise<T>;
}

export async function postJson<T = { ok: boolean; message: string }>(c: Conn, path: string, body: unknown): Promise<T> {
  const r = await fetch(c.base + path, {
    method: "POST",
    headers: { "X-RillBuddy-Token": c.token, "Content-Type": "application/json" },
    body: JSON.stringify(body),
  });
  const text = await r.text();
  try {
    return JSON.parse(text) as T;
  } catch {
    return { ok: r.ok, message: text } as T;
  }
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
/** One thing in 手上的事 (petd/todo.rs). */
export interface Todo {
  id: number;
  text: string;
  at: number;
  /** "you", or the agent that left it. */
  from: string;
  project?: string;
  /** The agent it waits on; `ready` once that agent has stopped. */
  wait?: string;
  ready?: boolean;
  told?: boolean;
  done_at?: number;
  /** The pet that holds it. */
  pet?: string;
}

/** The pet a thing is held by: its own, else the first pet there is. */
export function holderOf(t: Todo, pets: { id: string }[]): string | undefined {
  return t.pet && pets.some((p) => p.id === t.pet) ? t.pet : pets[0]?.id;
}

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

export const SOURCE_NAMES: Record<string, string> = { "claude-code": "Claude Code", codex: "Codex", gemini: "Gemini CLI", cursor: "Cursor", opencode: "OpenCode" };

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
  sounds?: boolean;
  bugs: Bug[];
}

export interface CatchRow {
  ts: number;
  outcome: string;
  fish_id: string;
  rarity: string;
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
  radio_s: number;
  kegel_sets: number;
  kegel_reps: number;
  letters: number;
  peak_agents: number;
}

export const RARITY_NAMES: Record<string, string> = {
  get common() { return t("普通", "Common"); },
  get rare() { return t("稀有", "Rare"); },
  get epic() { return t("史诗", "Epic"); },
  get legendary() { return t("传说", "Legendary"); },
};
export const RARITY_COLORS: Record<string, string> = { common: "#7f8aa3", rare: "#3c7ee6", epic: "#9a5ce6", legendary: "#f2a53a" };
