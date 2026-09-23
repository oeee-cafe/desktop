// The window's own controls on Windows -- minimise, maximise or restore,
// close -- at the right end of the site's toolbar, where the window's title
// bar would have put them above it.
//
// The window has no title bar of its own there (main.rs, decorations off),
// so the toolbar is the title bar: these are drawn to Windows 11's measure,
// 46px wide and the height of the toolbar's first row, in the system's own
// glyphs and fills, greyed while the window is in the background, and close
// turns red under the pointer. On a page with no toolbar -- the loader --
// they sit at the top right of the window instead, so it can still be
// closed before the site arrives.
//
// They ask the app to do the thing (Tauri's window commands, which the
// window's capabilities allow and nothing more); close goes through the same
// CloseRequested as the system's button did, so a page holding a drawing is
// still asked before it is left.
(function () {
  var invoke = function (command) {
    var ipc = window.__TAURI_INTERNALS__;
    if (!ipc) return Promise.resolve();
    return ipc.invoke("plugin:window|" + command, { label: "main" });
  };

  var style = document.createElement("style");
  // Only the buttons' own look. The room the toolbar keeps for them at its
  // end is the site's (ds.css in oeee-cafe/web), keyed on the root's
  // data-app="windows", which the site sets from the user agent before the
  // page paints (theme_head.jinja; chrome.rs names the app there).
  style.textContent = [
    ".oeee-caption { position: absolute; top: 0; right: 0; z-index: 60; display: flex; height: 52px; }",
    ".oeee-caption.is-loose { position: fixed; }",
    ".oeee-caption button { display: flex; align-items: center; justify-content: center;" +
      " width: 46px; height: 100%; margin: 0; padding: 0; border: 0; border-radius: 0;" +
      " background: transparent; color: var(--ds-ink, #22223a); box-shadow: none; cursor: default;" +
      ' font: 400 10px/1 "Segoe Fluent Icons", "Segoe MDL2 Assets"; -webkit-font-smoothing: auto; }',
    // Windows 11's own fills, in the toolbar's ink whichever the site's
    // theme: a faint wash under the pointer, fainter pressed with the glyph
    // dimmed, and on close its red, the glyph dimmed when pressed there too.
    // Maximise is under the app's stand-in (snap.rs), so the pointer never
    // reaches it, and it is shown hot and pressed when the app says.
    ".oeee-caption button:hover, .oeee-caption button.is-hot { background: color-mix(in srgb, currentColor 6%, transparent); }",
    ".oeee-caption button:active, .oeee-caption button.is-pressed { background: color-mix(in srgb, currentColor 4%, transparent); }",
    ".oeee-caption button:active span, .oeee-caption button.is-pressed span { opacity: 0.7; }",
    ".oeee-caption button.is-close:hover { background: #c42b1c; color: #ffffff; }",
    ".oeee-caption button.is-close:active { background: rgba(196, 43, 28, 0.9); color: #ffffff; }",
    // A window in the background greys its controls until one is pointed at.
    ".oeee-caption.is-inactive button:not(:hover):not(.is-hot) span { opacity: 0.4; }",
    ".oeee-caption span { display: block; pointer-events: none; }",
  ].join("\n");
  // This runs before the document has its root element (chrome.rs), so the
  // style goes in once there is somewhere to put it.
  function addStyle() {
    var root = document.documentElement;
    if (style.parentNode || !root) return;
    (document.head || root).appendChild(style);
  }

  // The system's own glyphs, from the font Windows draws its title bars
  // with (Segoe Fluent Icons; Segoe MDL2 Assets on Windows 10, at the same
  // code points), so they are hairlines at every scale as the system's are.
  var GLYPHS = {
    minimize: "",
    maximize: "",
    restore: "",
    close: "",
  };
  function glyph(name) {
    return '<span aria-hidden="true">' + GLYPHS[name] + "</span>";
  }
  var LABELS = {
    en: { minimize: "Minimize", maximize: "Maximize", restore: "Restore", close: "Close" },
    ko: { minimize: "최소화", maximize: "최대화", restore: "이전 크기로", close: "닫기" },
    ja: { minimize: "最小化", maximize: "最大化", restore: "元のサイズに戻す", close: "閉じる" },
    zh: { minimize: "最小化", maximize: "最大化", restore: "向下还原", close: "关闭" },
  };
  var words = LABELS[(navigator.language || "en").slice(0, 2)] || LABELS.en;

  var maximized = false;
  function button(name, onClick) {
    var b = document.createElement("button");
    b.type = "button";
    b.className = "is-" + name;
    b.setAttribute("data-caption", name);
    b.title = words[name];
    b.setAttribute("aria-label", words[name]);
    b.innerHTML = glyph(name);
    b.addEventListener("click", onClick);
    // A press here is the button's, not the start of a drag of the window.
    b.addEventListener("mousedown", function (event) { event.stopPropagation(); });
    return b;
  }
  var active = document.hasFocus();
  function showActive() {
    var box = document.getElementById("oeee-caption");
    if (box) box.classList.toggle("is-inactive", !active);
  }
  function build() {
    var box = document.createElement("div");
    box.className = active ? "oeee-caption" : "oeee-caption is-inactive";
    box.id = "oeee-caption";
    box.appendChild(button("minimize", function () { invoke("minimize"); }));
    box.appendChild(button("maximize", function () { invoke("toggle_maximize"); }));
    box.appendChild(button("close", function () { invoke("close"); }));
    return box;
  }
  function maximizeButton() {
    return document.querySelector("#oeee-caption .is-maximize");
  }
  function showMaximized() {
    var b = maximizeButton();
    if (!b) return;
    var name = maximized ? "restore" : "maximize";
    b.setAttribute("data-caption", name);
    b.title = words[name];
    b.setAttribute("aria-label", words[name]);
    b.innerHTML = glyph(name);
    b.classList.toggle("is-hot", pointer === "hot");
    b.classList.toggle("is-pressed", pointer === "pressed");
  }

  // Windows offers its Snap Layouts only over what it knows for a maximise
  // button, so the app keeps a stand-in of its own over this one (snap.rs):
  // told where the button is, in the window's pixels, whenever that moves,
  // and telling it back when the pointer is on it or pressing it.
  var pointer = "";
  window.__oeeeCaption = {
    maximizeState: function (state) {
      pointer = state;
      showMaximized();
    },
  };
  var reported = "";
  var reporting = false;
  function report() {
    if (reporting) return;
    reporting = true;
    requestAnimationFrame(function () {
      reporting = false;
      var b = maximizeButton();
      var r = b && b.getBoundingClientRect();
      var scale = window.devicePixelRatio || 1;
      var place = r && r.width > 0 && r.height > 0 ? {
        x: Math.round(r.left * scale),
        y: Math.round(r.top * scale),
        width: Math.round(r.right * scale) - Math.round(r.left * scale),
        height: Math.round(r.bottom * scale) - Math.round(r.top * scale),
      } : null;
      var json = JSON.stringify(place);
      if (json === reported) return;
      reported = json;
      var ipc = window.__TAURI_INTERNALS__;
      if (ipc) ipc.invoke("plugin:event|emit", { event: "oeee-caption-maximize", payload: place }).catch(function () {});
    });
  }
  function refreshMaximized() {
    invoke("is_maximized").then(function (value) {
      if (typeof value === "boolean" && value !== maximized) {
        maximized = value;
        showMaximized();
      }
    }, function () {});
  }

  // htmx swaps the body on boosted navigation, toolbar and all, so the
  // controls are put back whenever the document changes. The toolbar is
  // found by the mark the site gives the bar that moves the window
  // (toolbar.jinja), not by its class.
  function place() {
    addStyle();
    if (!document.body) return;
    report();
    var bar = document.querySelector("[data-window-drag]");
    var existing = document.getElementById("oeee-caption");
    if (existing && (bar ? existing.parentNode === bar : existing.parentNode === document.body)) return;
    if (existing) existing.parentNode.removeChild(existing);
    var box = build();
    if (bar) bar.appendChild(box);
    else {
      box.classList.add("is-loose");
      document.body.appendChild(box);
    }
    showMaximized();
  }
  document.addEventListener("DOMContentLoaded", function () {
    place();
    refreshMaximized();
  });
  new MutationObserver(place).observe(document, { childList: true, subtree: true });
  window.addEventListener("resize", function () {
    refreshMaximized();
    report();
  });
  window.addEventListener("focus", function () { active = true; showActive(); });
  window.addEventListener("blur", function () { active = false; showActive(); });
})();
