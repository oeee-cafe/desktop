//! Taking the player into a collaborative room a friend asked them into,
//! from Discord (discord.rs) or Steam (steam.rs).
//!
//! Either hands over a path on the site, which another player's app chose,
//! so only a path into a room is ever opened ([`room_path`]). The window
//! goes there as a link in the page goes, so a drawing in progress is asked
//! about first -- or, when it is not showing the site yet (the app was just
//! started to join), once a page of the site says it is
//! ([`Joining::page_shown`]). Everything past that is the site's: whether
//! the player is signed in, whether the room has a seat, whether they may
//! enter it.

use std::sync::Mutex;

use tauri::{AppHandle, Manager};
use url::Url;

use crate::{site, WINDOW};

/// A friend's way into a room, and where to take them.
pub struct Joining {
    app: AppHandle,
    site: Url,
    /// A room the window could not be sent to yet, because it was not
    /// showing the site -- or not there at all -- when the app was started
    /// to join it.
    waiting: Mutex<Option<String>>,
}

impl Joining {
    pub fn new(app: &AppHandle, site: &Url) -> Self {
        Joining {
            app: app.clone(),
            site: site.clone(),
            waiting: Mutex::default(),
        }
    }

    // Discord evals its own scripts into the window through it.
    #[cfg_attr(not(feature = "discord"), allow(dead_code))]
    pub fn app(&self) -> &AppHandle {
        &self.app
    }

    /// Brings the window forward and takes it to `path`, a room's, now or
    /// once it shows the site.
    pub fn join(&self, path: String) {
        let window = self.app.get_webview_window(WINDOW);
        if let Some(window) = &window {
            let _ = window.unminimize();
            let _ = window.show();
            let _ = window.set_focus();
        }
        let on_site = window
            .and_then(|window| window.url().ok())
            .is_some_and(|page| site::is_site(&page, &self.site));
        if on_site {
            self.go(&path);
        } else {
            *self.waiting.lock().unwrap() = Some(path);
        }
    }

    /// A page of the site is showing and speaking: the room a friend was
    /// asked into before it was can be opened now.
    pub fn page_shown(&self) {
        let waiting = self.waiting.lock().unwrap().take();
        if let Some(path) = waiting {
            self.go(&path);
        }
    }

    /// As a link in the page goes, so a drawing in progress is asked about
    /// first.
    fn go(&self, path: &str) {
        let (Some(window), Ok(url)) = (self.app.get_webview_window(WINDOW), self.site.join(path))
        else {
            return;
        };
        let url = serde_json::to_string(url.as_str()).unwrap_or_default();
        let _ = window.eval(format!("location.assign({url});"));
    }
}

/// The path into a collaborative room that `secret` names, or none for
/// anything else: a join secret, or a connect string, is another player's
/// app's word, and the window opens nothing on the site but a room by it.
pub fn room_path(secret: &str) -> Option<String> {
    let id = secret.strip_prefix("/collaborate/")?;
    is_uuid(id).then(|| format!("/collaborate/{id}"))
}

/// The room among a launch's arguments: Steam starts the app with a friend's
/// connect string on its command line when the player joins them from the
/// friends list, or accepts their invitation, and the app is not running
/// (steam.rs).
pub fn room_in_args(args: impl IntoIterator<Item = String>) -> Option<String> {
    args.into_iter().find_map(|arg| room_path(arg.trim()))
}

/// Whether `id` is a UUID as the site writes one: lowercase hex in groups
/// of 8, 4, 4, 4 and 12.
fn is_uuid(id: &str) -> bool {
    id.len() == 36
        && id.char_indices().all(|(i, c)| match i {
            8 | 13 | 18 | 23 => c == '-',
            _ => c.is_ascii_digit() || ('a'..='f').contains(&c),
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    const ROOM: &str = "/collaborate/9c881320-2b43-4afa-b2bb-7128c8a3e985";

    #[test]
    fn only_a_room_is_ever_joined() {
        assert_eq!(room_path(ROOM).as_deref(), Some(ROOM));
        for secret in [
            "",
            "/collaborate/",
            "/collaborate/9C881320-2B43-4AFA-B2BB-7128C8A3E985",
            "/collaborate/9c881320-2b43-4afa-b2bb-7128c8a3e98",
            "/collaborate/9c881320-2b43-4afa-b2bb-7128c8a3e985/ws",
            "/collaborate/9c881320-2b43-4afa-b2bb-7128c8a3e985?x",
            "/collaborate/../logout/2b43-4afa-b2bb-7128c8a3e98",
            "https://example.com/collaborate/9c881320-2b43-4afa-b2bb-7128c8a3e985",
            "//example.com/collaborate/9c881320-2b43-4afa-b2bb-7128c8a3e985",
            "/settings",
        ] {
            assert_eq!(room_path(secret), None, "{secret}");
        }
    }

    #[test]
    fn a_launch_names_the_room_among_whatever_else_it_carries() {
        let args = |list: &[&str]| list.iter().map(|a| a.to_string()).collect::<Vec<_>>();
        assert_eq!(
            room_in_args(args(&["oeee-cafe://discord", ROOM])).as_deref(),
            Some(ROOM)
        );
        assert_eq!(room_in_args(args(&[ROOM])).as_deref(), Some(ROOM));
        assert_eq!(room_in_args(args(&[])), None);
        assert_eq!(
            room_in_args(args(&["oeee-cafe://discord", "/settings"])),
            None
        );
    }
}
