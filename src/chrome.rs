//! Where the window ends and the site begins.
//!
//! The site's toolbar is drawn as a window's title bar, and every page has it
//! -- the painter pages too -- so on Windows the window has no title bar of
//! its own: its minimise, maximise and close buttons are drawn at the
//! toolbar's right end instead (caption.js). On Linux the system's title bar
//! stays, over the toolbar.
//!
//! The site lays itself out for this on its own. The app names itself in the
//! user agent (`USER_AGENT_MARK`), from which the site marks its root
//! `data-desktop="windows"` and keeps room at the toolbar's end for the
//! buttons (theme_head.jinja and ds.css in oeee-cafe/web); and the toolbar
//! arrives marked `data-tauri-drag-region="deep"` (toolbar.jinja), so Tauri
//! drags the window by it without the app going looking for it. The loader
//! marks its own strip the same way (`loader/index.html`).

use std::sync::Mutex;

use tauri::window::Color;
use tauri::{Theme, WebviewWindow, WebviewWindowBuilder, Window, WindowEvent};

/// What the app adds to the webview's user agent, which is how the site
/// tells this window from a browser and from the other apps.
pub const USER_AGENT_MARK: &str = "OeeeCafeWindows";

/// The webview's own user agent with the app's name at its end, once.
#[cfg_attr(not(windows), allow(dead_code))]
pub fn user_agent(default: &str) -> String {
    if default.split(' ').any(|part| part == USER_AGENT_MARK) {
        return default.to_owned();
    }
    format!("{default} {USER_AGENT_MARK}")
}

/// Minimise, maximise or restore, and close, drawn into the toolbar on
/// Windows. See caption.js.
#[cfg_attr(not(windows), allow(dead_code))]
pub const WINDOWS_CAPTION: &str = include_str!("caption.js");

/// The right-click menu kept to where it earns its place, by the page
/// (context_menu.js). Windows does the same natively, and more exactly
/// (context_menu.rs), so there the page is left alone.
#[cfg_attr(windows, allow(dead_code))]
const QUIET_CONTEXT_MENU: &str = include_str!("context_menu.js");

/// The window's frame: on Windows no title bar, the toolbar drawing the
/// window's controls instead. The window keeps its shadow, and with it the
/// system's resize edges.
pub fn prepare<'a, R: tauri::Runtime, M: tauri::Manager<R>>(
    builder: WebviewWindowBuilder<'a, R, M>,
) -> WebviewWindowBuilder<'a, R, M> {
    #[cfg(not(windows))]
    let builder = builder.initialization_script(QUIET_CONTEXT_MENU);
    #[cfg(windows)]
    let builder = builder
        .initialization_script(WINDOWS_CAPTION)
        .decorations(false)
        .shadow(true);
    builder
}

/// The site's ground as the page last said it (`theme` on the bridge), which
/// is the one to paint from then on; none until a page has said.
static GROUND: Mutex<Option<Color>> = Mutex::new(None);

/// What the window shows before a page has painted, until the site has said
/// what its ground is: NEO's -- lavender, or its night blue -- so a load does
/// not flash.
fn background(theme: Theme) -> Color {
    match theme {
        Theme::Dark => Color(0x17, 0x17, 0x2b, 0xff),
        _ => Color(0xcc, 0xcc, 0xff, 0xff),
    }
}

fn said_ground() -> Option<Color> {
    *GROUND.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// Paints the window's ground for the system's theme now.
pub fn paint_background<R: tauri::Runtime>(window: &WebviewWindow<R>) {
    let ground = said_ground().or_else(|| window.theme().ok().map(background));
    if let Some(ground) = ground {
        let _ = window.set_background_color(Some(ground));
    }
}

/// Paints the ground the page says it has (`--ds-ground`, bridge.rs), which
/// follows the reader's choice of theme on the site as well as the system's,
/// and keeps the window's colours from having to be kept in step with the
/// site's by hand. A colour the app cannot read is ignored.
pub fn paint_ground<R: tauri::Runtime>(window: &WebviewWindow<R>, ground: &str) {
    let Some(ground) = hex_colour(ground) else {
        return;
    };
    *GROUND.lock().unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(ground);
    let _ = window.set_background_color(Some(ground));
}

/// Keeps the ground following the system's theme, until the site has said
/// what it is: after that the page says again when it changes, and knows
/// better, since a reader may have chosen a theme on the site that is not the
/// system's.
pub fn on_window_event<R: tauri::Runtime>(window: &Window<R>, event: &WindowEvent) {
    if let WindowEvent::ThemeChanged(theme) = event {
        if said_ground().is_none() {
            let _ = window.set_background_color(Some(background(*theme)));
        }
    }
}

/// A CSS hex colour -- `#rgb`, `#rgba`, `#rrggbb` or `#rrggbbaa` -- as the
/// window's.
fn hex_colour(text: &str) -> Option<Color> {
    let digits = text.trim().strip_prefix('#')?;
    if !digits.chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    let channel = |at: usize, width: usize| -> Option<u8> {
        let value = u8::from_str_radix(digits.get(at..at + width)?, 16).ok()?;
        Some(if width == 1 { value * 0x11 } else { value })
    };
    let (width, alpha) = match digits.len() {
        3 => (1, false),
        4 => (1, true),
        6 => (2, false),
        8 => (2, true),
        _ => return None,
    };
    Some(Color(
        channel(0, width)?,
        channel(width, width)?,
        channel(2 * width, width)?,
        if alpha { channel(3 * width, width)? } else { 0xff },
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_app_names_itself_once_after_the_webviews_own() {
        let edge = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/140.0.0.0 Safari/537.36 Edg/140.0.0.0";
        let named = user_agent(edge);
        assert_eq!(named, format!("{edge} OeeeCafeWindows"));
        assert_eq!(user_agent(&named), named);
    }

    #[test]
    fn the_page_is_left_to_lay_itself_out() {
        // The room for the buttons is ds.css's, and the drag region is in
        // the toolbar's markup; the app neither marks the root nor looks for
        // the toolbar by its class to do either.
        assert!(!WINDOWS_CAPTION.contains("#menubar"));
        assert!(!WINDOWS_CAPTION.contains(".toolbar-links"));
        assert!(!WINDOWS_CAPTION.contains(r#"setAttribute("data-desktop""#));
        assert!(!WINDOWS_CAPTION.contains("data-tauri-drag-region"));
    }

    #[test]
    fn the_page_no_longer_reports_its_own_unread_count() {
        // The site says it over the bridge (bridge.rs); reading the toolbar
        // for it is what broke when the toolbar changed.
        assert!(!WINDOWS_CAPTION.contains("toolbar-badge"));
        assert!(!WINDOWS_CAPTION.contains("oeee-unread"));
    }

    #[test]
    fn links_get_no_menu_where_the_page_decides() {
        assert!(QUIET_CONTEXT_MENU.contains(r#"closest("img")"#));
        assert!(!QUIET_CONTEXT_MENU.contains("a[href]"));
    }

    #[test]
    fn the_caption_draws_the_window_controls() {
        assert!(WINDOWS_CAPTION.contains("oeee-caption"));
        assert!(WINDOWS_CAPTION.contains(r#"invoke("close")"#));
    }

    #[test]
    fn the_sites_ground_is_read_as_it_says_it() {
        assert_eq!(hex_colour("#ccccff"), Some(Color(0xcc, 0xcc, 0xff, 0xff)));
        assert_eq!(hex_colour(" #17172b "), Some(Color(0x17, 0x17, 0x2b, 0xff)));
        assert_eq!(hex_colour("#ccf"), Some(Color(0xcc, 0xcc, 0xff, 0xff)));
        assert_eq!(hex_colour("#17172b80"), Some(Color(0x17, 0x17, 0x2b, 0x80)));
    }

    #[test]
    fn a_colour_that_is_not_hex_is_nothing() {
        for text in ["", "#", "ccccff", "#ccccf", "#gggggg", "rgb(204, 204, 255)", "lavender"] {
            assert_eq!(hex_colour(text), None, "{text}");
        }
    }

    #[test]
    fn until_the_site_says_the_ground_is_neos() {
        assert_eq!(background(Theme::Light), Color(0xcc, 0xcc, 0xff, 0xff));
        assert_eq!(background(Theme::Dark), Color(0x17, 0x17, 0x2b, 0xff));
    }
}
