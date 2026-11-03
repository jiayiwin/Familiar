// Gallery dialog: make a pet from one picture. Pick a picture, name it,
// optionally give it lines, watch the preview, then install (or cancel).

import { fileUrl, inTauri, type Conn } from "./api";
import { canvasPng, CutoutError, cutout, manifest, PLAN, render, type Built } from "./petmaker";
import { EN, t as tr } from "./i18n";

type Outcome =
  | { status: "installed"; id: string; name: string; replaced: boolean }
  | { status: "conflict"; id: string; name: string; new_version: string; existing_version: string; existing_builtin: boolean }
  | { status: "invalid"; errors: string[] };

const ACTION_NAMES: Record<string, [string, string]> = {
  idle: ["待机", "Idle"], thinking: ["思考", "Thinking"], tool: ["干活", "Working"], attention: ["提醒", "Your turn"],
  done: ["完成", "Done"], error: ["出错", "Error"], sleep: ["睡觉", "Sleep"], wake: ["醒来", "Wake"],
  squish: ["被捏", "Squish"], petted: ["被摸", "Petted"], annoyed: ["生气", "Annoyed"], walk: ["走路", "Walk"],
  fall: ["下落", "Fall"], dizzy: ["晕了", "Dizzy"], cheer: ["欢呼", "Cheer"], sulk: ["闹别扭", "Sulk"],
};
const SOUNDS = ["attention.wav", "done.wav", "error.wav", "squish1.wav", "squish2.wav", "squish3.wav"];
export const IMAGE_EXT = /\.(png|jpe?g|webp|bmp|gif)$/i;

const $ = <T extends HTMLElement = HTMLElement>(id: string) => document.getElementById(id) as T;
const esc = (s: string) => s.replace(/[&<>"]/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;" })[c]!);

/** A stable pack id from the names: ascii slug, else "pet-" + a short hash. */
function makeId(name: string, nameEn: string): string {
  const slug = (nameEn || name).toLowerCase().replace(/[^a-z0-9]+/g, "-").replace(/^-+|-+$/g, "").slice(0, 32);
  if (slug.length >= 2) return slug;
  let h = 2166136261;
  for (const ch of name) h = Math.imul(h ^ ch.codePointAt(0)!, 16777619) >>> 0;
  return `pet-${h.toString(36)}`;
}

/** A picture with a busy background, kept as a framed sticker. */
function sticker(img: ImageBitmap): HTMLCanvasElement {
  const k = Math.min(1, 640 / Math.max(img.width, img.height));
  const w = Math.round(img.width * k), h = Math.round(img.height * k), pad = Math.round(Math.max(w, h) * 0.04);
  const c = document.createElement("canvas");
  c.width = w + pad * 2; c.height = h + pad * 2;
  const g = c.getContext("2d")!;
  const r = Math.round(Math.min(w, h) * 0.12);
  g.fillStyle = "#fff";
  g.beginPath(); g.roundRect(0, 0, c.width, c.height, r + pad); g.fill();
  g.save(); g.beginPath(); g.roundRect(pad, pad, w, h, r); g.clip();
  g.drawImage(img, pad, pad, w, h); g.restore();
  return c;
}

export interface MakerDeps {
  conn: Conn;
  /** Called after a successful install. */
  installed: (id: string, name: string, replaced: boolean) => void;
}

export function setupMaker(deps: MakerDeps): { open: (path?: string) => void } {
  let built: Built | null = null;
  let bitmap: ImageBitmap | null = null;
  let act = "idle";
  let auto = true;
  let shownAt = performance.now();
  let busy = false;

  const box = $("maker");
  const prev = $<HTMLCanvasElement>("mk-preview");
  const pg = prev.getContext("2d")!;
  const msg = (html: string, kind = "") => { $("mk-msg").className = `mkmsg ${kind}`; $("mk-msg").innerHTML = html; };
  const update = () => {
    ($("mk-install") as HTMLButtonElement).disabled = !built || !$<HTMLInputElement>("mk-name").value.trim() || busy;
  };

  // Action chips under the preview.
  $("mk-acts").innerHTML = Object.keys(PLAN).map((a) => `<span class="chip" data-a="${a}">${ACTION_NAMES[a][EN ? 1 : 0]}</span>`).join("");
  $("mk-acts").onclick = (e) => {
    const a = (e.target as HTMLElement).dataset.a;
    if (!a) return;
    act = a; auto = false; shownAt = performance.now();
  };

  const loop = (t: number) => {
    if (!box.hidden && built) {
      if (auto && t - shownAt > 1800) {
        const keys = Object.keys(PLAN);
        act = keys[(keys.indexOf(act) + 1) % keys.length];
        shownAt = t;
      }
      const [n, fps, loopy] = PLAN[act];
      const raw = Math.floor(((t - shownAt) / 1000) * fps);
      const idx = loopy ? raw % n : Math.min(n - 1, raw % (n + Math.round(fps * 0.6)));
      pg.clearRect(0, 0, prev.width, prev.height);
      pg.imageSmoothingQuality = "high";
      pg.drawImage(built.strips[act], idx * built.size, 0, built.size, built.size, 0, 0, prev.width, prev.height);
      document.querySelectorAll<HTMLElement>("#mk-acts .chip").forEach((c) => c.classList.toggle("on", c.dataset.a === act));
    }
    requestAnimationFrame(loop);
  };
  requestAnimationFrame(loop);

  const showBuilt = () => {
    $("mk-drop").hidden = true;
    prev.hidden = false;
    $("mk-acts").hidden = false;
    $("mk-change").hidden = false;
    act = "idle"; auto = true; shownAt = performance.now();
    update();
  };

  const useImage = async (blob: Blob) => {
    msg("");
    try {
      bitmap = await createImageBitmap(blob);
    } catch {
      msg(tr("这张图片打不开，换一张 PNG 或 JPG 试试。", "Couldn't open that picture. Try a PNG or JPG."), "err");
      return;
    }
    try {
      built = render(cutout(bitmap));
      msg(tr("预览会自动轮播所有动作，也可以点下面的动作单独看。", "The preview cycles through every action; click one to see it alone."));
    } catch (e) {
      built = null;
      if (e instanceof CutoutError && e.message === "busy") {
        msg(tr(
          `背景不是纯色，抠不干净。<br>可以换一张<b>透明或纯色背景</b>的图；或者先用 Windows「画图」/「照片」的<b>删除背景</b>、Mac 访达右键的<b>移除背景</b>处理一下。<br><button id="mk-sticker">保留背景，做成贴纸</button>`,
          `The background isn't one flat colour, so it can't be cut out cleanly.<br>Use a picture with a <b>transparent or plain</b> background, or remove it first (Paint or Photos on Windows: <b>Remove background</b>; Finder on a Mac: <b>Remove Background</b>).<br><button id="mk-sticker">Keep the background as a sticker</button>`,
        ), "err");
        $("mk-sticker").onclick = () => { built = render(sticker(bitmap!)); msg(""); showBuilt(); };
      } else {
        msg(tr("图片里没找到人物。", "Couldn't find a character in the picture."), "err");
      }
      update();
      return;
    }
    showBuilt();
  };

  const choose = async () => {
    if (!inTauri) return;
    const { open } = await import("@tauri-apps/plugin-dialog");
    const p = await open({ title: tr("选择一张图片", "Choose a picture"), filters: [{ name: tr("图片", "Pictures"), extensions: ["png", "jpg", "jpeg", "webp", "bmp", "gif"] }] });
    if (typeof p === "string") await loadPath(p);
  };
  const loadPath = async (path: string) => {
    const { invoke } = await import("@tauri-apps/api/core");
    try {
      const bytes = await invoke<number[]>("read_image", { path });
      await useImage(new Blob([new Uint8Array(bytes)]));
    } catch (e) {
      msg(esc(String(e)), "err");
    }
  };

  const install = async (overwrite = false) => {
    if (!built) return;
    const name = $<HTMLInputElement>("mk-name").value.trim();
    const nameEn = $<HTMLInputElement>("mk-name-en").value.trim();
    const lines = (id: string) => $<HTMLInputElement>(id).value.split(/[/／|]/).map((s) => s.trim()).filter(Boolean);
    const bubbles: Record<string, string[]> = {};
    for (const k of ["attention", "done", "error"]) { const l = lines(`mk-b-${k}`); if (l.length) bubbles[k] = l; }
    const id = makeId(name, nameEn);
    busy = true; update();
    msg(tr("正在生成…", "Building…"));
    try {
      const files: [string, number[]][] = [];
      for (const [a, c] of Object.entries(built.strips)) files.push([`sprites/${a}.png`, Array.from(await canvasPng(c))]);
      const pv = document.createElement("canvas"); pv.width = pv.height = 256;
      const pvg = pv.getContext("2d")!; pvg.imageSmoothingQuality = "high";
      pvg.drawImage(built.strips.idle, 0, 0, built.size, built.size, 0, 0, 256, 256);
      files.push(["preview.png", Array.from(await canvasPng(pv))]);
      const sounds: string[] = [];
      for (const s of SOUNDS) {
        try {
          const r = await fetch(fileUrl(deps.conn, "pixel-cat", `sounds/${s}`));
          if (r.ok) { files.push([`sounds/${s}`, Array.from(new Uint8Array(await r.arrayBuffer()))]); sounds.push(s); }
        } catch { /* a pet without sounds is fine */ }
      }
      const m = manifest(built, {
        id, name, name_en: nameEn || undefined, scale: Number($<HTMLSelectElement>("mk-scale").value),
        bubbles, bubbles_en: EN && Object.keys(bubbles).length ? bubbles : undefined,
      }, sounds);
      files.push(["pet.json", Array.from(new TextEncoder().encode(JSON.stringify(m, null, 2)))]);
      const { invoke } = await import("@tauri-apps/api/core");
      const r = await invoke<Outcome>("create_pack", { files, overwrite });
      if (r.status === "installed") {
        close();
        deps.installed(r.id, r.name || name, r.replaced);
      } else if (r.status === "conflict") {
        msg(tr(
          `已经有一只 id 为 <b>${esc(r.id)}</b> 的宠物（${esc(r.name)}）。要覆盖它吗？<br><button class="primary" id="mk-ow">覆盖</button> 或者换个名字。`,
          `There's already a pet with id <b>${esc(r.id)}</b> (${esc(r.name)}). Replace it?<br><button class="primary" id="mk-ow">Replace</button> or pick another name.`,
        ), "err");
        $("mk-ow").onclick = () => void install(true);
      } else {
        msg(`${tr("没能安装：", "Couldn't install: ")}<ul>${r.errors.map((e) => `<li>${esc(e)}</li>`).join("")}</ul>`, "err");
      }
    } catch (e) {
      msg(`${tr("没能安装：", "Couldn't install: ")}${esc(String(e))}`, "err");
    } finally {
      busy = false; update();
    }
  };

  const reset = () => {
    built = null; bitmap = null;
    $("mk-drop").hidden = false; prev.hidden = true; $("mk-acts").hidden = true; $("mk-change").hidden = true;
    for (const id of ["mk-name", "mk-name-en", "mk-b-attention", "mk-b-done", "mk-b-error"]) $<HTMLInputElement>(id).value = "";
    $<HTMLSelectElement>("mk-scale").value = "1";
    msg("");
    update();
  };
  const close = () => { box.hidden = true; reset(); };

  $("mk-drop").onclick = () => void choose();
  $("mk-change").onclick = () => void choose();
  $("mk-cancel").onclick = close;
  $("mk-install").onclick = () => void install();
  $("mk-name").oninput = update;
  box.onclick = (e) => { if (e.target === box && !busy) close(); };
  document.addEventListener("keydown", (e) => { if (e.key === "Escape" && !box.hidden && !busy) close(); });

  return {
    open: (path?: string) => {
      box.hidden = false;
      if (path) void loadPath(path);
    },
  };
}
