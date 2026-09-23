//! The window's keys on Windows: the ones any Windows program answers to,
//! and shortcuts for the site's own commands.
//!
//! WebView2's own browser keys are off (webview2.rs), because each of them is
//! the browser speaking -- Ctrl+F's find bar, Ctrl+P's print preview. That
//! took going back, forward and reloading with them, so those are answered
//! here instead, as the app's: Alt+Left and Alt+Right, F5 and Ctrl+R, the
//! keyboard's own Back, Forward and Refresh keys, and Ctrl+W to close the
//! window, which asks first over a drawing as the close button does.
//!
//! Ctrl+F is the site's search rather than a find bar, and the rest are the
//! site's sections by number.

/// What a key does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    Back,
    Forward,
    Reload,
    Close,
    /// One of the site's commands, by the name `window.oeeeApp.command` knows it
    /// by (toolbar.jinja in oeee-cafe/web).
    Command(&'static str),
}

/// The modifier keys held with it.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Modifiers {
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
}

// Windows' virtual-key codes, which are what WebView2 reports.
const VK_F4: u32 = 0x73;
const VK_F5: u32 = 0x74;
const VK_LEFT: u32 = 0x25;
const VK_RIGHT: u32 = 0x27;
const VK_BROWSER_BACK: u32 = 0xA6;
const VK_BROWSER_FORWARD: u32 = 0xA7;
const VK_BROWSER_REFRESH: u32 = 0xA8;
const VK_OEM_COMMA: u32 = 0xBC;
/// The `/?` key on US layouts.
const VK_OEM_2: u32 = 0xBF;

/// The action for `key` pressed with `held`, if it is one of the window's.
pub fn action(key: u32, held: Modifiers) -> Option<Action> {
    let Modifiers { ctrl, alt, shift } = held;
    let letter = char::from_u32(key).filter(char::is_ascii_alphanumeric);
    match (ctrl, alt, shift) {
        (false, false, false) => match key {
            VK_F5 | VK_BROWSER_REFRESH => Some(Action::Reload),
            VK_BROWSER_BACK => Some(Action::Back),
            VK_BROWSER_FORWARD => Some(Action::Forward),
            _ => None,
        },
        (false, true, false) => match key {
            VK_LEFT => Some(Action::Back),
            VK_RIGHT => Some(Action::Forward),
            _ => None,
        },
        (true, false, false) => match (key, letter) {
            (VK_F5, _) | (_, Some('R')) => Some(Action::Reload),
            (VK_F4, _) | (_, Some('W')) => Some(Action::Close),
            (_, Some('F')) => Some(Action::Command("search")),
            (_, Some('N')) => Some(Action::Command("new-drawing")),
            (VK_OEM_COMMA, _) => Some(Action::Command("account")),
            (VK_OEM_2, _) => Some(Action::Command("shortcuts")),
            (_, Some('1')) => Some(Action::Command("recent")),
            (_, Some('2')) => Some(Action::Command("following")),
            (_, Some('3')) => Some(Action::Command("communities")),
            (_, Some('4')) => Some(Action::Command("together")),
            (_, Some('5')) => Some(Action::Command("tags")),
            _ => None,
        },
        // Ctrl+Shift+R, the hard reload a player's hands may know.
        (true, false, true) => match letter {
            Some('R') => Some(Action::Reload),
            _ => None,
        },
        _ => None,
    }
}

/// The script that carries out a site command in the page showing
/// (keys_command.js): the site's own `window.oeeeApp.command` (toolbar.jinja in
/// oeee-cafe/web), which is the one list of what each command does.
///
/// A page without the toolbar -- a replay, or the loader before the site
/// has arrived -- has no `oeeeApp.command`, and there the key does nothing.
/// This used to fall back to loading the command's page from a table of
/// the site's routes kept here, which was a second copy of them to keep in
/// step; going nowhere from a replay was judged the smaller cost.
pub fn command_script(command: &str) -> String {
    format!(
        "{}({});",
        COMMAND.trim_end(),
        serde_json::to_string(command).expect("a string serialises"),
    )
}

const COMMAND: &str = include_str!("keys_command.js");

/// Carries out one of the window's keys in the page showing. Going back,
/// forward or reloading is the page's own, so a page holding a drawing asks
/// first; closing asks as the close button does (close_guard.rs).
#[cfg(windows)]
fn act(window: &tauri::WebviewWindow, action: Action) {
    let script = match action {
        Action::Back => "history.back();".to_owned(),
        Action::Forward => "history.forward();".to_owned(),
        Action::Reload => "location.reload();".to_owned(),
        Action::Close => {
            let _ = window.close();
            return;
        }
        Action::Command(name) => command_script(name),
    };
    let _ = window.eval(script);
}

/// Answers the window's keys, which WebView2 hands over (webview2.rs).
#[cfg(windows)]
pub fn attach(window: &tauri::WebviewWindow) -> tauri::Result<()> {
    let keys_window = window.clone();
    window.with_webview(move |webview| {
        crate::webview2::on_keys(&webview, move |action| act(&keys_window, action));
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const NONE: Modifiers = Modifiers {
        ctrl: false,
        alt: false,
        shift: false,
    };
    const CTRL: Modifiers = Modifiers {
        ctrl: true,
        alt: false,
        shift: false,
    };
    const ALT: Modifiers = Modifiers {
        ctrl: false,
        alt: true,
        shift: false,
    };
    const CTRL_SHIFT: Modifiers = Modifiers {
        ctrl: true,
        alt: false,
        shift: true,
    };

    #[test]
    fn going_back_and_forward() {
        assert_eq!(action(VK_LEFT, ALT), Some(Action::Back));
        assert_eq!(action(VK_RIGHT, ALT), Some(Action::Forward));
        assert_eq!(action(VK_BROWSER_BACK, NONE), Some(Action::Back));
        assert_eq!(action(VK_BROWSER_FORWARD, NONE), Some(Action::Forward));
    }

    #[test]
    fn reloading() {
        assert_eq!(action(VK_F5, NONE), Some(Action::Reload));
        assert_eq!(action(VK_F5, CTRL), Some(Action::Reload));
        assert_eq!(action('R' as u32, CTRL), Some(Action::Reload));
        assert_eq!(action('R' as u32, CTRL_SHIFT), Some(Action::Reload));
        assert_eq!(action(VK_BROWSER_REFRESH, NONE), Some(Action::Reload));
    }

    #[test]
    fn closing() {
        assert_eq!(action('W' as u32, CTRL), Some(Action::Close));
        assert_eq!(action(VK_F4, CTRL), Some(Action::Close));
    }

    #[test]
    fn the_sites_commands() {
        assert_eq!(action('F' as u32, CTRL), Some(Action::Command("search")));
        assert_eq!(
            action('N' as u32, CTRL),
            Some(Action::Command("new-drawing"))
        );
        assert_eq!(action(VK_OEM_COMMA, CTRL), Some(Action::Command("account")));
        assert_eq!(
            action('3' as u32, CTRL),
            Some(Action::Command("communities"))
        );
    }

    #[test]
    fn keys_that_are_not_the_windows_are_left_alone() {
        // Typing, editing, and the painter's own keys.
        assert_eq!(action('R' as u32, NONE), None);
        assert_eq!(action(VK_LEFT, NONE), None);
        assert_eq!(action('C' as u32, CTRL), None);
        assert_eq!(action('Z' as u32, CTRL), None);
        assert_eq!(action('V' as u32, CTRL_SHIFT), None);
        // Alt+F4 is the system's, which already closes the window through
        // the same question.
        assert_eq!(action(VK_F4, ALT), None);
    }

    #[test]
    fn a_command_is_the_site_s_to_carry_out() {
        let script = command_script("search");
        assert!(script.contains("window.oeeeApp.command(command)"));
        assert!(script.ends_with(r#"})("search");"#));
        assert!(!script.contains("location"));
    }
}
