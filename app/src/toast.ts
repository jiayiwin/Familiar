// A short line at the bottom of a page that goes away by itself.

export function toast(text: string): void {
  let el = document.querySelector<HTMLDivElement>(".toast");
  if (!el) {
    el = document.createElement("div");
    el.className = "toast";
    document.body.appendChild(el);
  }
  el.textContent = text;
  el.classList.add("show");
  clearTimeout(Number(el.dataset.t));
  el.dataset.t = String(window.setTimeout(() => el!.classList.remove("show"), 2600));
}
