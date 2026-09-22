//! Where the window ends and the site begins.
//!
//! The site's toolbar is drawn as a window's title bar, and every page has it
//! -- the painter pages too -- so on Windows the window has no title bar of
//! its own: its minimise, maximise and close buttons are drawn at the
//! toolbar's right end instead (caption.js). On Linux the system's title bar
//! stays, over the toolbar.
//!
//! The site knows nothing about any of this. It is told only which platform
//! it is in (`data-desktop` on its root element), and the app marks the
//! toolbar as the handle the window is dragged by.

use tauri::{Theme, WebviewWindow, WebviewWindowBuilder, Window, WindowEvent};

/// Runs at the start of every page: tells the site where it is, and makes
/// the toolbar's empty space drag the window (chrome.js).
pub fn script(os: &str) -> String {
    let mut script = CHROME.replace("__OS__", os);
    // On Windows the window has no title bar of its own, and the toolbar
    // carries the window's controls (caption.js).
    if os == "windows" {
        script.push('\n');
        script.push_str(WINDOWS_CAPTION);
    }
    script
}

const CHROME: &str = include_str!("chrome.js");

/// Minimise, maximise or restore, and close, drawn into the toolbar on
/// Windows. See caption.js.
const WINDOWS_CAPTION: &str = include_str!("caption.js");

/// The right-click menu kept to where it earns its place, by the page
/// (context_menu.js). Windows does the same natively, and more exactly
/// (context_menu.rs), so there the page is left alone.
#[cfg_attr(windows, allow(dead_code))]
const QUIET_CONTEXT_MENU: &str = include_str!("context_menu.js");

/// The window's frame and what the page is told about it: the platform, the
/// toolbar as the handle it is dragged by, and on Windows no title bar, the
/// toolbar drawing the window's controls instead. The window keeps its
/// shadow, and with it the system's resize edges.
pub fn prepare<'a, R: tauri::Runtime, M: tauri::Manager<R>>(
    builder: WebviewWindowBuilder<'a, R, M>,
) -> WebviewWindowBuilder<'a, R, M> {
    #[cfg(not(windows))]
    let builder = builder.initialization_script(QUIET_CONTEXT_MENU);
    let builder = builder.initialization_script(script(std::env::consts::OS));
    #[cfg(windows)]
    let builder = builder.decorations(false).shadow(true);
    builder
}

/// What the window shows before a page has painted: the site's ground, which
/// is NEO's -- lavender, or its night blue -- so a load does not flash.
fn background(theme: Theme) -> tauri::window::Color {
    match theme {
        Theme::Dark => tauri::window::Color(0x17, 0x17, 0x2b, 0xff),
        _ => tauri::window::Color(0xcc, 0xcc, 0xff, 0xff),
    }
}

/// Paints the window's ground for the system's theme now.
pub fn paint_background<R: tauri::Runtime>(window: &WebviewWindow<R>) {
    if let Ok(theme) = window.theme() {
        let _ = window.set_background_color(Some(background(theme)));
    }
}

/// Keeps the ground following the system's theme.
pub fn on_window_event<R: tauri::Runtime>(window: &Window<R>, event: &WindowEvent) {
    if let WindowEvent::ThemeChanged(theme) = event {
        let _ = window.set_background_color(Some(background(*theme)));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_script_names_the_platform() {
        assert!(script("linux").contains(r#"setAttribute("data-desktop", "linux")"#));
    }

    #[test]
    fn the_page_no_longer_reports_its_own_unread_count() {
        // The site says it over the bridge (bridge.rs); reading the toolbar
        // for it is what broke when the toolbar changed.
        assert!(!script("linux").contains("toolbar-badge"));
        assert!(!script("windows").contains("oeee-unread"));
    }

    #[test]
    fn links_get_no_menu_where_the_page_decides() {
        assert!(QUIET_CONTEXT_MENU.contains(r#"closest("img")"#));
        assert!(!QUIET_CONTEXT_MENU.contains("a[href]"));
    }

    #[test]
    fn only_windows_draws_the_window_controls() {
        assert!(script("windows").contains("oeee-caption"));
        assert!(script("windows").contains(r#"invoke("close")"#));
        assert!(!script("linux").contains("oeee-caption"));
    }
}
