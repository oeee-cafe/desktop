// Runs at the start of every page: tells the site Steam is here, which is
// what shows its "Sign in with Steam" link (steam.rs). WebView2 runs it
// before the document has its root element, so it waits for one.
(function () {
  function mark() {
    var root = document.documentElement;
    if (root) root.setAttribute("data-steam-app", "");
    return !!root;
  }
  if (mark()) return;
  new MutationObserver(function (_, observer) {
    if (mark()) observer.disconnect();
  }).observe(document, { childList: true });
})();
