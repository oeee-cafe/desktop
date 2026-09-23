//! Steam, when Steam started the app.
//!
//! Two things are asked of it. What the player is doing, for their friends
//! list: the site says it in each `page` message it sends the app (bridge.rs;
//! `src/web/presence.rs` in oeee-cafe/web), and the app hands it to Steam as
//! rich presence, in the words of `steam/rich_presence.vdf`.
//!
//! And who the player is: a Web API ticket
//! the site takes to Steam to find out (`src/steam.rs` in oeee-cafe/web).
//! The site cannot ask for one itself -- it has no way into the app -- so the
//! app watches for the site's "Sign in with Steam" link, which goes to
//! `/auth/steam/app`, stops that navigation, gets a ticket, and posts it to
//! `/auth/steam` from the page, as the page's own form would.
//!
//! And, when Steam says a DLC has just been installed -- the Supporter Pack,
//! bought in the overlay or the store while the app was open -- a fresh
//! ticket, posted in the background to `/auth/steam/refresh`, so the site
//! asks Steam again what the player owns and the supporter badge follows at
//! once. That signs nobody in and moves no page, so it can happen mid-drawing.
//!
//! Without Steam -- started from a terminal, or Steam not running -- the app is
//! the same window onto the site it always was, and the link is never shown:
//! the site draws it only on a page the app has marked `data-steam-app`.

// Without the `steam` feature, what reads pages and writes scripts for Steam
// is still built and tested, and nothing calls it.
#![cfg_attr(not(feature = "steam"), allow(dead_code))]

use std::sync::Arc;

use tauri::webview::PageLoadEvent;
use tauri::{AppHandle, Manager, WebviewWindowBuilder};
use url::Url;

use crate::bridge::Page;
use crate::{dialogs, site, words, WINDOW};

#[cfg(feature = "steam")]
mod client;
#[cfg(feature = "steam")]
pub use client::{start, Steam};

/// The path of the site's "Sign in with Steam" link.
const SIGN_IN_PATH: &str = "/auth/steam/app";

/// Steam, in a build without it (the Microsoft Store's): there is never one,
/// so the app is only ever the window onto the site.
#[cfg(not(feature = "steam"))]
pub enum Steam {}

#[cfg(not(feature = "steam"))]
impl Steam {
    pub fn web_api_ticket(&self) -> Result<String, String> {
        match *self {}
    }

    pub fn show_presence(&self, _page: Option<&Page>) {
        match *self {}
    }

    pub fn on_dlc_installed(&self, _then: impl Fn(u32) + Send + 'static) {
        match *self {}
    }
}

#[cfg(not(feature = "steam"))]
pub fn start() -> Option<std::sync::Arc<Steam>> {
    None
}

/// The rich presence keys for what a page says, every key given a value or
/// cleared so nothing from the page before is left behind. `steam_display` is
/// a token of `steam/rich_presence.vdf`; `community` fills its `%community%`.
fn rich_presence(page: Option<&Page>) -> [(&'static str, Option<String>); 3] {
    let activity = page.and_then(|p| p.presence.as_deref()).unwrap_or("");
    let community = page
        .and_then(|p| p.community.as_deref())
        .map(presence_value)
        .filter(|c| !c.is_empty());
    let group = page
        .and_then(|p| p.group.as_deref())
        .map(presence_value)
        .filter(|g| !g.is_empty());

    let (token, in_community) = match activity {
        "drawing" => ("Drawing", true),
        "relaying" => ("Relaying", true),
        "collaborating" => ("Collaborating", true),
        "drawing-banner" => ("DrawingBanner", false),
        "watching-replay" => ("WatchingReplay", false),
        _ => ("Browsing", false),
    };
    let community = community.filter(|_| in_community);
    let display = match community {
        Some(_) => format!("#{token}In"),
        None => format!("#{token}"),
    };
    let group = group.filter(|_| activity == "collaborating");
    [
        ("steam_display", Some(display)),
        ("community", community),
        ("steam_player_group", group),
    ]
}

/// A value as Steam takes one: no NUL, which it cannot carry, and at most
/// 256 bytes, cut where a character ends.
fn presence_value(value: &str) -> String {
    let value: String = value.chars().filter(|c| *c != '\0').collect();
    let mut end = value.len().min(256);
    while !value.is_char_boundary(end) {
        end -= 1;
    }
    value[..end].to_string()
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// Whether a navigation is the site's "Sign in with Steam" link, and if so
/// where the site asked to go afterwards.
pub fn sign_in_link(url: &Url, site: &Url) -> Option<Option<String>> {
    if !site::is_site(url, site) || url.path() != SIGN_IN_PATH {
        return None;
    }
    let next = url
        .query_pairs()
        .find(|(key, _)| key == "next")
        .map(|(_, value)| value.into_owned());
    Some(next)
}

/// Runs at the start of every page: tells the site Steam is here, which is
/// what shows its "Sign in with Steam" link (mark_page.js).
pub const MARK_PAGE: &str = include_str!("steam/mark_page.js");

const POST_TICKET: &str = include_str!("steam/post_ticket.js");
const REFRESH: &str = include_str!("steam/refresh.js");

/// A value as JavaScript reads it: JSON, so a string arrives quoted and
/// escaped, and cannot close its own quotes to run as script.
fn quoted(value: &impl serde::Serialize) -> String {
    serde_json::to_string(value).expect("a string serialises")
}

/// Posts `ticket` to the site's `/auth/steam` from the page that is showing,
/// as a form on it would (post_ticket.js).
pub fn post_ticket_script(ticket: &str, next: Option<&str>) -> String {
    format!("{}({}, {})", POST_TICKET.trim_end(), quoted(&ticket), quoted(&next))
}

/// Posts `ticket` to the site's `/auth/steam/refresh` from the page that is
/// showing, without leaving it: the site asks Steam what the ticket's account
/// owns now and records it. Nothing is said to the player either way -- the
/// badge is simply there on the next page.
pub fn refresh_script(ticket: &str) -> String {
    format!("{}({})", REFRESH.trim_end(), quoted(&ticket))
}

/// Marks each page for Steam, and says the player is browsing whenever the
/// window shows a page that is not the site's -- the loader, or its "can't be
/// reached" page -- which sends no `page` message to say otherwise.
pub fn prepare<'a, M: Manager<tauri::Wry>>(
    builder: WebviewWindowBuilder<'a, tauri::Wry, M>,
    steam: &Option<Arc<Steam>>,
    site: &Url,
) -> WebviewWindowBuilder<'a, tauri::Wry, M> {
    let Some(steam) = steam.clone() else {
        return builder;
    };
    let site = site.clone();
    builder
        .initialization_script(MARK_PAGE)
        .on_page_load(move |_window, payload| {
            if payload.event() == PageLoadEvent::Finished && !site::is_site(payload.url(), &site) {
                steam.show_presence(None);
            }
        })
}

/// A DLC bought while the app is open -- the Supporter Pack -- is told to the
/// site at once, rather than at its daily recheck.
pub fn watch_dlc(app: &AppHandle, steam: &Option<Arc<Steam>>, site: &Url) {
    let Some(steam) = steam else {
        return;
    };
    let (app, site) = (app.clone(), site.clone());
    let weak = Arc::downgrade(steam);
    steam.on_dlc_installed(move |_app_id| {
        if let Some(steam) = weak.upgrade() {
            refresh_standing(&app, &steam, &site);
        }
    });
}

/// Gets a ticket from Steam and posts it to the site from the page showing,
/// or tells the player it could not. Off the main thread: Steam can take a
/// moment to answer.
pub fn sign_in(app: &AppHandle, steam: Arc<Steam>, next: Option<String>) {
    let app = app.clone();
    std::thread::spawn(move || match steam.web_api_ticket() {
        Ok(ticket) => {
            if let Some(window) = app.get_webview_window(WINDOW) {
                let _ = window.eval(post_ticket_script(&ticket, next.as_deref()));
            }
        }
        Err(error) => {
            eprintln!("no Steam ticket: {error}");
            let message = words::words().steam_sign_in_failed;
            dialogs::ask(&app, dialogs::Question::Alert(message), |_| {});
        }
    });
}

/// Gets a fresh ticket and posts it to the site in the background, from the
/// page showing, so the site asks Steam again what the player owns. Off the
/// thread Steam called from: the ticket arrives on that thread.
fn refresh_standing(app: &AppHandle, steam: &Arc<Steam>, site: &Url) {
    let (app, steam, site) = (app.clone(), steam.clone(), site.clone());
    std::thread::spawn(move || {
        let Some(window) = app.get_webview_window(WINDOW) else {
            return;
        };
        if !window.url().is_ok_and(|page| site::is_site(&page, &site)) {
            // The loader or its "can't be reached" page: the daily recheck
            // on the site will notice instead.
            return;
        }
        match steam.web_api_ticket() {
            Ok(ticket) => {
                let _ = window.eval(refresh_script(&ticket));
            }
            Err(error) => eprintln!("no Steam ticket to refresh with: {error}"),
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn site() -> Url {
        site::for_tests()
    }

    #[test]
    fn the_sign_in_link_is_recognised_with_where_it_goes_next() {
        let url = Url::parse("https://oeee.cafe/auth/steam/app?next=%2Fdraw%3Fa%3Db").unwrap();
        assert_eq!(sign_in_link(&url, &site()), Some(Some("/draw?a=b".to_string())));
        let url = Url::parse("https://oeee.cafe/auth/steam/app").unwrap();
        assert_eq!(sign_in_link(&url, &site()), Some(None));
    }

    #[test]
    fn nothing_else_is_the_sign_in_link() {
        for other in [
            "https://oeee.cafe/auth/steam",
            "https://oeee.cafe/login",
            "https://elsewhere.test/auth/steam/app",
            "http://oeee.cafe/auth/steam/app",
        ] {
            assert_eq!(sign_in_link(&Url::parse(other).unwrap(), &site()), None, "{other}");
        }
    }

    fn page(activity: &str, community: Option<&str>, group: Option<&str>) -> Page {
        Page {
            presence: Some(activity.to_string()),
            community: community.map(str::to_string),
            group: group.map(str::to_string),
            ..Page::default()
        }
    }

    #[test]
    fn a_page_says_what_the_player_is_doing() {
        let drawing = page("drawing", Some("오이카페 모에화"), None);
        assert_eq!(
            rich_presence(Some(&drawing)),
            [
                ("steam_display", Some("#DrawingIn".to_string())),
                ("community", Some("오이카페 모에화".to_string())),
                ("steam_player_group", None),
            ]
        );
        assert_eq!(
            rich_presence(Some(&page("relaying", None, None)))[0].1.as_deref(),
            Some("#Relaying")
        );
        let room = page("collaborating", None, Some("0123abcd"));
        assert_eq!(
            rich_presence(Some(&room)),
            [
                ("steam_display", Some("#Collaborating".to_string())),
                ("community", None),
                ("steam_player_group", Some("0123abcd".to_string())),
            ]
        );
    }

    #[test]
    fn anything_else_is_browsing_and_clears_what_came_before() {
        for answer in [None, Some(page("something-new", Some("x"), Some("y")))] {
            assert_eq!(
                rich_presence(answer.as_ref()),
                [
                    ("steam_display", Some("#Browsing".to_string())),
                    ("community", None),
                    ("steam_player_group", None),
                ]
            );
        }
        // A banner is not drawn in a community, whatever the page says.
        assert_eq!(
            rich_presence(Some(&page("drawing-banner", Some("x"), None)))[1].1,
            None
        );
    }

    /// Every token the app can set is in the file uploaded to Steam, in
    /// every language: one that is missing shows the player as doing nothing.
    #[test]
    fn every_token_is_worded_in_every_language() {
        let file = include_str!("../steam/rich_presence.vdf");
        let languages = ["english", "koreana", "japanese", "schinese"];
        let activities = [
            "drawing",
            "relaying",
            "collaborating",
            "drawing-banner",
            "watching-replay",
            "",
        ];
        for activity in activities {
            for community in [None, Some("x")] {
                let [(_, display), ..] = rich_presence(Some(&page(activity, community, None)));
                let token = format!("\"{}\"", display.unwrap());
                assert_eq!(
                    file.matches(&token).count(),
                    languages.len(),
                    "{token} should be worded once per language"
                );
            }
        }
        for language in languages {
            assert!(file.contains(&format!("\"{language}\"")), "{language}");
        }
    }

    #[test]
    fn a_long_name_is_cut_where_a_character_ends() {
        let long = "가".repeat(200); // 600 bytes
        let cut = presence_value(&long);
        assert!(cut.len() <= 256);
        assert_eq!(cut, "가".repeat(85));
        assert_eq!(presence_value("a\0b"), "ab");
    }

    #[test]
    fn a_ticket_is_hex() {
        assert_eq!(hex(&[0x14, 0x00, 0xab, 0xff]), "1400abff");
    }

    #[test]
    fn a_refresh_posts_the_ticket_from_the_page_it_is_on() {
        let script = refresh_script("14\"00ab");
        assert!(script.contains(r#"fetch("/auth/steam/refresh""#));
        assert!(script.ends_with(r#"})("14\"00ab")"#));
        assert!(!script.contains("location"), "the page stays where it is");
    }

    #[test]
    fn what_reaches_the_page_is_quoted_as_javascript() {
        let script = post_ticket_script("1400ab", Some("/a\"</script>"));
        assert!(script.contains(r#"form.action = "/auth/steam";"#));
        assert!(script.ends_with(r#"})("1400ab", "/a\"</script>")"#));
        assert!(post_ticket_script("1400ab", None).ends_with(r#"("1400ab", null)"#));
    }
}
