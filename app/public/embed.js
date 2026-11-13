// Inside the tabbed window each page is a frame of it. A frame has no line to
// the host of its own, so it borrows the window's; the page's own script then
// finds everything where it expects it. This has to run before that script,
// which is why it is a file of its own (the release build allows no inline
// scripts).
(function () {
  if (window.parent === window) return;
  try {
    if (!("__TAURI_INTERNALS__" in window) && window.parent.__TAURI_INTERNALS__) window.__TAURI_INTERNALS__ = window.parent.__TAURI_INTERNALS__;
    document.documentElement.dataset.embedded = "";
  } catch (e) {
    // A frame of some other page: nothing to borrow.
  }
})();
