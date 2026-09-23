// Runs at the start of every page: tells the site this build sells through
// Steam, which is what shows its "Sign in with Steam" button and the
// Supporter Pack's, and lets the page ask the app for a ticket (steam.rs).
// Before the page paints, so neither button is drawn in a window that could
// do nothing with it. WebView2 runs this before the document has its root
// element, so it waits for one.
(function () {
  function mark() {
    var root = document.documentElement;
    if (root) root.setAttribute("data-store", "steam");
    return !!root;
  }
  if (mark()) return;
  new MutationObserver(function (_, observer) {
    if (mark()) observer.disconnect();
  }).observe(document, { childList: true });
})();
