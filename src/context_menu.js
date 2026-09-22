// The browser's own right-click menu -- Back, Reload, Save as, Print -- is the
// plainest sign that a window is a browser, so it is kept to where it earns
// its place: text fields (Cut, Copy, Paste, spelling), a selection (Copy)
// and images (Save, Copy). Anywhere else a right click does nothing, unless
// the page has its own use for it, as the painter does; this listens last
// and steps aside when the page has already answered.
//
// Not on Windows, where WebView2's menu is trimmed to those same items
// natively and a link gets the app's Copy link (context_menu.rs). Here a
// link's menu would be the browser's, opening windows the app does not have,
// so a link gets none.
window.addEventListener("contextmenu", function (event) {
  if (event.defaultPrevented) return;
  var target = event.target;
  if (target && target.closest) {
    if (target.closest("input, textarea, select, [contenteditable]")) return;
    if (target.closest("img")) return;
  }
  if (window.getSelection && String(window.getSelection()) !== "") return;
  event.preventDefault();
});
