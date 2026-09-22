// Runs at the start of every page: tells the site where it is, and makes
// the toolbar's empty space drag the window (chrome.rs). `deep` lets the
// whole bar drag while its links, buttons and menu stay clickable; htmx
// replaces the body on boosted navigation, so the toolbar is marked again
// whenever the document changes.
(function () {
  // WebView2 runs this before the document has its root element, so the
  // page is marked once the root arrives, and the document itself is what
  // is watched.
  var started = false;
  function start() {
    var root = document.documentElement;
    if (started || !root) return;
    started = true;
    root.setAttribute("data-desktop", "__OS__");
  }
  function mark() {
    start();
    var bar = document.querySelector(".nav-bar");
    if (!bar) return;
    if (bar.getAttribute("data-tauri-drag-region") !== "deep") {
      bar.setAttribute("data-tauri-drag-region", "deep");
    }
  }
  start();
  document.addEventListener("DOMContentLoaded", mark);
  new MutationObserver(mark).observe(document, { childList: true, subtree: true });
})();
