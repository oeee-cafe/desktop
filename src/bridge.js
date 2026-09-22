// The one channel the site tells the app things by (app_bridge.jinja in
// oeee-cafe/web). The site calls `oeeeBridge.postMessage` with a JSON
// string, and the string arrives in the app as the payload of an
// `oeee-bridge` event (bridge.rs).
//
// Defined in every page the window loads, not only the site's: the window
// only ever holds the site and the bundled loader (navigation.rs), and the
// loader never calls it. Tauri's own scripts run before this one, so a page
// without `__TAURI_INTERNALS__` is one Tauri has not given IPC at all, and
// is left without the channel rather than given one that cannot carry.
(function () {
  if (!window.__TAURI_INTERNALS__) return;
  window.oeeeBridge = {
    postMessage: function (text) {
      window.__TAURI_INTERNALS__
        .invoke("plugin:event|emit", { event: "oeee-bridge", payload: text })
        .catch(function () {});
    }
  };
})();
