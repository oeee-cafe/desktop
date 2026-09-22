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
  style.textContent = [
    // Room for the three buttons at the toolbar's end, and the sections'
    // row, when the toolbar stacks, reaching back under them.
    'html[data-desktop="windows"] .nav-bar #menubar { padding-right: 150px; }',
    'html[data-desktop="windows"] .nav-bar.is-stacked .toolbar-links { width: calc(100% + 138px); margin-right: -138px; }',
    ".oeee-caption { position: absolute; top: 0; right: 0; z-index: 60; display: flex; height: 52px; }",
    ".oeee-caption.is-loose { position: fixed; }",
    ".oeee-caption button { display: flex; align-items: center; justify-content: center;" +
      " width: 46px; height: 100%; margin: 0; padding: 0; border: 0; border-radius: 0;" +
      " background: transparent; color: var(--ds-ink, #22223a); box-shadow: none; cursor: default;" +
      ' font: 400 10px/1 "Segoe Fluent Icons", "Segoe MDL2 Assets"; -webkit-font-smoothing: auto; }',
    // Windows 11's own fills, in the toolbar's ink whichever the site's
    // theme: a faint wash under the pointer, fainter pressed with the glyph
    // dimmed, and on close its red, the glyph dimmed when pressed there too.
    ".oeee-caption button:hover { background: color-mix(in srgb, currentColor 6%, transparent); }",
    ".oeee-caption button:active { background: color-mix(in srgb, currentColor 4%, transparent); }",
    ".oeee-caption button:active span { opacity: 0.7; }",
    ".oeee-caption button.is-close:hover { background: #c42b1c; color: #ffffff; }",
    ".oeee-caption button.is-close:active { background: rgba(196, 43, 28, 0.9); color: #ffffff; }",
    // A window in the background greys its controls until one is pointed at.
    ".oeee-caption.is-inactive button:not(:hover) span { opacity: 0.4; }",
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
  function showMaximized() {
    var b = document.querySelector('#oeee-caption [data-caption="maximize"], #oeee-caption [data-caption="restore"]');
    if (!b) return;
    var name = maximized ? "restore" : "maximize";
    b.setAttribute("data-caption", name);
    b.title = words[name];
    b.setAttribute("aria-label", words[name]);
    b.innerHTML = glyph(name);
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
  // controls are put back whenever the document changes.
  function place() {
    addStyle();
    if (!document.body) return;
    var bar = document.querySelector(".nav-bar");
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
  window.addEventListener("resize", refreshMaximized);
  window.addEventListener("focus", function () { active = true; showActive(); });
  window.addEventListener("blur", function () { active = false; showActive(); });
})();
