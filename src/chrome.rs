//! Where the window ends and the site begins.
//!
//! The site's toolbar is drawn as a window's title bar, and every page has it
//! -- the painter pages too -- so on Windows the window has no title bar of
//! its own: its minimise, maximise and close buttons are drawn at the
//! toolbar's right end instead, by the site (app_caption.jinja in oeee-cafe/web).
//!
//! The site lays itself out for this on its own. The app names itself at the
//! end of the user agent (`user_agent`), from which the site marks its root
//! `data-app="windows"` and `data-form="desktop"` before the page paints, and
//! keeps room at the toolbar's end for the buttons (theme_head.jinja and
//! ds.css in oeee-cafe/web); and the toolbar
//! arrives marked `data-tauri-drag-region="deep"` (toolbar.jinja), so Tauri
//! drags the window by it without the app going looking for it. The loader
//! marks its own strip the same way (`loader/index.html`).

use std::sync::Mutex;

use tauri::window::Color;
use tauri::{Theme, WebviewWindow, WebviewWindowBuilder, Window, WindowEvent};

/// What the app adds to the webview's user agent, which is how the site
/// tells this window from a browser and from the other apps. Every app ends
/// its user agent with a mark of this one shape, `OeeeCafe platform/<app>`,
/// so the site has one place to read which app it is in (theme_head.jinja
/// in oeee-cafe/web).
pub const USER_AGENT_MARK: &str = "OeeeCafe platform/windows";

/// The word every app's mark starts with, as a word of its own.
const APP: &str = "OeeeCafe";

/// The webview's own user agent with the app's mark at its end, and straight
/// after it the store this build sells through, if it sells at all:
/// ` store/steam` when Steam started it, ` store/microsoft` when it is the
/// Microsoft Store's package, and nothing otherwise, so the site offers
/// nothing to buy in a window that could not sell it. The site marks the
/// root `data-store` from that, before the page paints.
///
/// Added once: a user agent that already carries an app's mark is left as
/// it is.
#[cfg_attr(not(windows), allow(dead_code))]
pub fn user_agent(default: &str, store: Option<&str>) -> String {
    if default.split(' ').any(|part| part == APP) {
        return default.to_owned();
    }
    match store {
        Some(store) => format!("{default} {USER_AGENT_MARK} store/{store}"),
        None => format!("{default} {USER_AGENT_MARK}"),
    }
}

/// The window's frame: on Windows no title bar, the toolbar drawing the
/// window's controls instead. The window keeps its shadow, and with it the
/// system's resize edges.
///
/// The right-click menu is trimmed on Windows only, natively
/// (context_menu.rs). The app ships nowhere else -- macOS is the iOS app's
/// -- so a build for another system is a developer's, and keeps the
/// browser's whole menu, Inspect included.
pub fn prepare<'a, R: tauri::Runtime, M: tauri::Manager<R>>(
    builder: WebviewWindowBuilder<'a, R, M>,
) -> WebviewWindowBuilder<'a, R, M> {
    #[cfg(windows)]
    let builder = builder.decorations(false).shadow(true);
    builder
}

/// The toolbar's window controls asking for the window (a `window` message,
/// app_caption.jinja in oeee-cafe/web): minimise, maximise or restore, and
/// close -- which goes through CloseRequested as the system's button did,
/// so a page holding a drawing is asked first (close_guard.rs).
pub fn window_asked(window: &WebviewWindow, action: &str) {
    let _ = match action {
        "minimize" => window.minimize(),
        "maximize" if window.is_maximized().unwrap_or(false) => window.unmaximize(),
        "maximize" => window.maximize(),
        "close" => window.close(),
        _ => Ok(()),
    };
}

/// Where the toolbar's maximise button is (a `caption` message): the Snap
/// Layouts stand-in goes over it (snap.rs), and the page is told whether the
/// window is maximised, which it cannot see for itself. The page says where
/// its button is again whenever the window changes size, which is also when
/// that may have changed.
pub fn caption_placed(window: &WebviewWindow, place: Option<crate::bridge::Place>) {
    #[cfg(windows)]
    {
        use tauri::Manager;
        let place = place.and_then(crate::bridge::Place::bounded);
        let _ = window
            .app_handle()
            .run_on_main_thread(move || crate::snap::place(place));
    }
    #[cfg(not(windows))]
    let _ = place;
    let maximized = window.is_maximized().unwrap_or(false);
    let _ = window.eval(format!(
        "window.oeeeApp && window.oeeeApp.caption && window.oeeeApp.caption({{maximized: {maximized}}})"
    ));
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
        let named = user_agent(edge, None);
        assert_eq!(named, format!("{edge} OeeeCafe platform/windows"));
        assert_eq!(user_agent(&named, None), named);
        // Nor does a store change a mark already there.
        assert_eq!(user_agent(&named, Some("steam")), named);
    }

    #[test]
    fn a_build_that_sells_names_its_store_after_the_app() {
        let edge = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/140.0.0.0 Safari/537.36 Edg/140.0.0.0";
        let named = user_agent(edge, Some("steam"));
        assert_eq!(named, format!("{edge} OeeeCafe platform/windows store/steam"));
        assert_eq!(user_agent(&named, Some("steam")), named);
        assert_eq!(
            user_agent(edge, Some("microsoft")),
            format!("{edge} OeeeCafe platform/windows store/microsoft")
        );
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
