//! The menu bar, on macOS.
//!
//! Every command the site offers is here with the key an application would
//! give it, and each one asks the page to do it (`window.oeeeCommand`, in the
//! site's toolbar.jinja): going somewhere is the page's own navigation, so a
//! page holding an unsaved drawing still asks before it is left, and the menu
//! and the site's keyboard shortcuts are one set of commands rather than two.
//! The Edit menu is what makes copy and paste work in the site's text fields
//! at all -- WebKit takes those from the menu, not from the keyboard.

use tauri::menu::MenuEvent;
#[cfg(target_os = "macos")]
use tauri::menu::{Menu, MenuItemBuilder, PredefinedMenuItem, SubmenuBuilder};
use tauri::{AppHandle, Manager, Runtime};
use url::Url;

#[cfg(target_os = "macos")]
use crate::menu_words::menu_words;

/// A menu item that asks the site for `command`.
#[cfg(target_os = "macos")]
fn command<R: Runtime>(
    app: &AppHandle<R>,
    name: &str,
    text: &str,
    accelerator: Option<&str>,
) -> tauri::Result<tauri::menu::MenuItem<R>> {
    let mut item = MenuItemBuilder::with_id(format!("site:{name}"), text);
    if let Some(accelerator) = accelerator {
        item = item.accelerator(accelerator);
    }
    item.build(app)
}

#[cfg(target_os = "macos")]
pub fn build<R: Runtime>(app: &AppHandle<R>) -> tauri::Result<Menu<R>> {
    let w = menu_words();
    let separator = || PredefinedMenuItem::separator(app);

    let app_menu = SubmenuBuilder::new(app, "Oeee Cafe")
        .item(&command(app, "about", w.about, None)?)
        .item(&separator()?)
        .item(&command(app, "account", w.settings, Some("CmdOrCtrl+,"))?)
        .item(&separator()?)
        .item(&PredefinedMenuItem::services(app, Some(w.services))?)
        .item(&separator()?)
        .item(&PredefinedMenuItem::hide(app, Some(w.hide))?)
        .item(&PredefinedMenuItem::hide_others(app, Some(w.hide_others))?)
        .item(&PredefinedMenuItem::show_all(app, Some(w.show_all))?)
        .item(&separator()?)
        // Sends `terminate:`, which asks the page first (macos::guard_quit).
        .item(&PredefinedMenuItem::quit(app, Some(w.quit))?)
        .build()?;

    let file = SubmenuBuilder::new(app, w.file)
        .item(&command(
            app,
            "new-drawing",
            w.new_drawing,
            Some("CmdOrCtrl+N"),
        )?)
        .item(&separator()?)
        .item(&PredefinedMenuItem::close_window(
            app,
            Some(w.close_window),
        )?)
        .build()?;

    let edit = SubmenuBuilder::new(app, w.edit)
        .item(&PredefinedMenuItem::undo(app, Some(w.undo))?)
        .item(&PredefinedMenuItem::redo(app, Some(w.redo))?)
        .item(&separator()?)
        .item(&PredefinedMenuItem::cut(app, Some(w.cut))?)
        .item(&PredefinedMenuItem::copy(app, Some(w.copy))?)
        .item(&PredefinedMenuItem::paste(app, Some(w.paste))?)
        .item(&PredefinedMenuItem::select_all(app, Some(w.select_all))?)
        .item(&separator()?)
        // The site's search, where an application keeps Find: the web view
        // has no find bar of its own, so the key is free.
        .item(&command(app, "search", w.search, Some("CmdOrCtrl+F"))?)
        .build()?;

    let theme = SubmenuBuilder::new(app, w.theme)
        .item(&command(app, "theme-light", w.theme_light, None)?)
        .item(&command(app, "theme-dark", w.theme_dark, None)?)
        .item(&command(app, "theme-system", w.theme_system, None)?)
        .build()?;

    let view = SubmenuBuilder::new(app, w.view)
        .item(&command(app, "recent", w.recent, Some("CmdOrCtrl+1"))?)
        .item(&command(
            app,
            "following",
            w.following,
            Some("CmdOrCtrl+2"),
        )?)
        .item(&command(
            app,
            "communities",
            w.communities,
            Some("CmdOrCtrl+3"),
        )?)
        .item(&command(app, "together", w.together, Some("CmdOrCtrl+4"))?)
        .item(&command(app, "hashtags", w.hashtags, Some("CmdOrCtrl+5"))?)
        .item(&separator()?)
        .item(&command(
            app,
            "notifications",
            w.notifications,
            Some("CmdOrCtrl+Shift+N"),
        )?)
        .item(&command(
            app,
            "drafts",
            w.drafts,
            Some("CmdOrCtrl+Shift+D"),
        )?)
        .item(&command(
            app,
            "profile",
            w.profile,
            Some("CmdOrCtrl+Shift+P"),
        )?)
        .item(&separator()?)
        .item(
            &MenuItemBuilder::with_id("page:back", w.back)
                .accelerator("CmdOrCtrl+[")
                .build(app)?,
        )
        .item(
            &MenuItemBuilder::with_id("page:forward", w.forward)
                .accelerator("CmdOrCtrl+]")
                .build(app)?,
        )
        .item(
            &MenuItemBuilder::with_id("page:reload", w.reload)
                .accelerator("CmdOrCtrl+R")
                .build(app)?,
        )
        .item(&separator()?)
        .item(&theme)
        .item(&separator()?)
        .item(&PredefinedMenuItem::fullscreen(app, Some(w.full_screen))?)
        .build()?;

    let window = SubmenuBuilder::new(app, w.window)
        .item(&PredefinedMenuItem::minimize(app, Some(w.minimize))?)
        .item(&PredefinedMenuItem::maximize(app, Some(w.zoom))?)
        .item(&separator()?)
        .item(&PredefinedMenuItem::bring_all_to_front(
            app,
            Some(w.bring_all_to_front),
        )?)
        .build()?;

    let help = SubmenuBuilder::new(app, w.help)
        .item(&command(
            app,
            "shortcuts",
            w.shortcuts,
            Some("CmdOrCtrl+/"),
        )?)
        .build()?;

    Menu::with_items(app, &[&app_menu, &file, &edit, &view, &window, &help])
}

/// Where a command goes when the page has no toolbar to ask -- the bundled
/// loader, say. Commands that are not a place do nothing there.
fn fallback(name: &str) -> Option<&'static str> {
    Some(match name {
        "recent" | "about" => "/",
        "following" => "/home",
        "communities" => "/communities",
        "together" => "/collaborate",
        "hashtags" => "/hashtags",
        "search" => "/search",
        "notifications" => "/notifications",
        "drafts" => "/posts/drafts",
        "account" => "/account",
        _ => return None,
    })
}

/// The script that carries out a menu item, or none for an id that is not
/// ours (the predefined items are handled by the system).
pub fn script_for(id: &str, site: &Url) -> Option<String> {
    if let Some(name) = id.strip_prefix("site:") {
        let go = match fallback(name) {
            Some(path) => format!("location.href = {:?};", site.join(path).ok()?.as_str()),
            None => String::new(),
        };
        return Some(format!(
            "if (!(window.oeeeCommand && window.oeeeCommand({name:?}))) {{ {go} }}"
        ));
    }
    match id {
        "page:back" => Some("history.back();".into()),
        "page:forward" => Some("history.forward();".into()),
        // A plain reload, so a page holding a drawing asks first.
        "page:reload" => Some("location.reload();".into()),
        _ => None,
    }
}

pub fn handle<R: Runtime>(app: &AppHandle<R>, event: MenuEvent, site: &Url) {
    let Some(script) = script_for(event.id().as_ref(), site) else {
        return;
    };
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.eval(script);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn site() -> Url {
        Url::parse("https://oeee.cafe/").unwrap()
    }

    #[test]
    fn a_site_command_asks_the_page_and_falls_back_to_the_place() {
        let script = script_for("site:communities", &site()).unwrap();
        assert!(script.contains(r#"window.oeeeCommand("communities")"#));
        assert!(script.contains(r#"location.href = "https://oeee.cafe/communities""#));
    }

    #[test]
    fn search_asks_the_page_and_falls_back_to_the_search_page() {
        let script = script_for("site:search", &site()).unwrap();
        assert!(script.contains(r#"window.oeeeCommand("search")"#));
        assert!(script.contains(r#"location.href = "https://oeee.cafe/search""#));
    }

    #[test]
    fn a_command_that_is_not_a_place_only_asks() {
        let script = script_for("site:theme-dark", &site()).unwrap();
        assert!(script.contains(r#"window.oeeeCommand("theme-dark")"#));
        assert!(!script.contains("location.href"));
    }

    #[test]
    fn page_commands_and_unknown_ids() {
        assert_eq!(
            script_for("page:back", &site()).as_deref(),
            Some("history.back();")
        );
        assert!(script_for("copy", &site()).is_none());
    }
}
