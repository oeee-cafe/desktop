//! Where the window ends and the site begins.
//!
//! The site's toolbar is drawn as a window's title bar, and every page has it
//! -- the painter pages too -- so on macOS the window's own title bar steps
//! aside: it goes transparent, and the traffic lights sit inside the toolbar,
//! at its left.
//!
//! On Windows the window has no title bar at all, and its minimise,
//! maximise and close buttons are drawn at the toolbar's right end instead
//! (caption.js).
//!
//! The site knows nothing about any of this. It is told only which platform
//! it is in (`data-desktop` on its root element), and the app marks the
//! toolbar as the handle the window is dragged by.

use url::Url;

/// Runs at the start of every page: tells the site where it is, makes room
/// for the traffic lights, and makes the toolbar's empty space drag the
/// window. `deep` lets the whole bar drag while its links, buttons and menu
/// stay clickable; htmx replaces the body on boosted navigation, so the
/// toolbar is marked again whenever the document changes.
pub fn script(os: &str) -> String {
    let mut script = platform_script(os);
    // On Windows the window has no title bar of its own, and the toolbar
    // carries the window's controls (caption.js).
    if os == "windows" {
        script.push('\n');
        script.push_str(WINDOWS_CAPTION);
    }
    script
}

/// Minimise, maximise or restore, and close, drawn into the toolbar on
/// Windows. See caption.js.
const WINDOWS_CAPTION: &str = include_str!("caption.js");

fn platform_script(os: &str) -> String {
    format!(
        r##"(function () {{
  var root = document.documentElement;
  root.setAttribute("data-desktop", "{os}");
  var style = document.createElement("style");
  style.textContent = 'html[data-desktop="macos"] .nav-bar #menubar {{ padding-left: 96px; }}';
  (document.head || root).appendChild(style);
  // The unread count the toolbar shows, sent to the app for its icon
  // (badge.rs) whenever it changes -- it is swapped in by the handlers that
  // change it, so watching the document catches every one.
  var unread = null;
  function reportUnread(bar) {{
    var badge = bar.querySelector("#nav-notifications .toolbar-badge");
    var count = badge ? parseInt(badge.textContent, 10) || 0 : 0;
    if (count === unread) return;
    unread = count;
    var ipc = window.__TAURI_INTERNALS__;
    if (ipc) ipc.invoke("plugin:event|emit", {{ event: "oeee-unread", payload: count }}).catch(function () {{}});
  }}
  function mark() {{
    var bar = document.querySelector(".nav-bar");
    if (!bar) return;
    if (bar.getAttribute("data-tauri-drag-region") !== "deep") {{
      bar.setAttribute("data-tauri-drag-region", "deep");
    }}
    reportUnread(bar);
  }}
  document.addEventListener("DOMContentLoaded", mark);
  new MutationObserver(mark).observe(root, {{ childList: true, subtree: true }});
}})();"##
    )
}

/// The site's pages may move, minimise, maximise and close the window --
/// what dragging the toolbar and the Windows caption buttons need -- and
/// nothing else.
pub fn site_pattern(site: &Url) -> String {
    let host = site.host_str().unwrap_or_default();
    match site.port() {
        Some(port) => format!("{}://{}:{}/*", site.scheme(), host, port),
        None => format!("{}://{}/*", site.scheme(), host),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn url(value: &str) -> Url {
        Url::parse(value).unwrap()
    }

    #[test]
    fn the_pattern_covers_the_site_and_nothing_else() {
        assert_eq!(
            site_pattern(&url("https://oeee.cafe/")),
            "https://oeee.cafe/*"
        );
        assert_eq!(
            site_pattern(&url("http://127.0.0.1:8765/")),
            "http://127.0.0.1:8765/*"
        );
    }

    #[test]
    fn the_script_names_the_platform() {
        assert!(script("macos").contains(r#"setAttribute("data-desktop", "macos")"#));
    }

    #[test]
    fn the_toolbar_reports_its_unread_count() {
        assert!(script("macos").contains(r#"event: "oeee-unread""#));
        assert_eq!(crate::badge::EVENT, "oeee-unread");
    }

    #[test]
    fn only_windows_draws_the_window_controls() {
        assert!(script("windows").contains("oeee-caption"));
        assert!(script("windows").contains(r#"invoke("close")"#));
        assert!(!script("macos").contains("oeee-caption"));
        assert!(!script("linux").contains("oeee-caption"));
    }
}
