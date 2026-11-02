// Chinese / English for the pages. The language comes from petd
// (`language:` in config.yaml); pages reload when it changes.
//
//   t("中文", "English")          inline strings
//   <span data-en="English">中文</span>   static HTML (also data-en-title,
//                                          data-en-placeholder)

import { getJson, subscribe, type Conn } from "./api";

export let EN = false;

export function t(zh: string, en: string): string {
  return EN ? en : zh;
}

/** Ask petd for the language, translate the static HTML, follow changes. */
export async function initLang(conn: Conn, opts: { reload?: boolean } = {}): Promise<void> {
  try {
    EN = (await getJson<{ lang: string }>(conn, "/v1/lang")).lang === "en";
  } catch {
    EN = false;
  }
  document.documentElement.lang = EN ? "en" : "zh-CN";
  applyDom();
  if (opts.reload !== false) {
    subscribe(conn, (type, data) => {
      if (type === "lang" && ((data as { lang: string }).lang === "en") !== EN) location.reload();
    });
  }
}

export function applyDom(root: ParentNode = document): void {
  if (!EN) return;
  root.querySelectorAll<HTMLElement>("[data-en]").forEach((el) => (el.textContent = el.dataset.en ?? el.textContent));
  root.querySelectorAll<HTMLElement>("[data-en-title]").forEach((el) => (el.title = el.dataset.enTitle ?? el.title));
  root.querySelectorAll<HTMLInputElement>("[data-en-placeholder]").forEach((el) => (el.placeholder = el.dataset.enPlaceholder ?? el.placeholder));
  const title = document.querySelector<HTMLTitleElement>("title[data-en]");
  if (title) document.title = title.dataset.en ?? document.title;
}
