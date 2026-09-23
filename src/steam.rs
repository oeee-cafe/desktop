//! Steam, when Steam started the app.
//!
//! Three things are asked of it. What the player is doing, for their friends
//! list: the site says it in each `page` message it sends the app (bridge.rs;
//! `src/web/presence.rs` in oeee-cafe/web), and the app hands it to Steam as
//! rich presence, in the words of `steam/rich_presence.vdf`.
//!
//! Who the player is: a Web API ticket the site takes to Steam to find out
//! (`src/steam.rs` in oeee-cafe/web). The page cannot ask Steam for one -- it
//! has no way to Steam but the app -- so it sends `signIn` with the provider
//! "steam" (bridge.rs), and the app answers `oeeeApp.signIn.answer` with the
//! ticket, or with nothing when Steam would not give one. Everything done
//! with the ticket is the page's (app_sign_in.jinja in oeee-cafe/web): it
//! takes the press on its own "Sign in with Steam" button, posts the ticket,
//! and says so itself when there is none. The app knows no route of the
//! site's, so the site can change any of them without a release of the app.
//!
//! And the Supporter Pack, a DLC, which the page sells as every app's page
//! sells (app_store.jinja): a `purchase` of its app id opens its store page
//! in the overlay, where the buying happens, out of the app's sight. What
//! the app hears is Steam saying a DLC has been installed, and then it hands
//! the page a fresh ticket as proof with `oeeeApp.store.purchased`; the site
//! asks Steam what that account owns, rather than taking the app's word, so
//! the supporter badge follows at once and nothing the app says can forge
//! it.
//!
//! Without Steam -- started from a terminal, or Steam not running, or the
//! Microsoft Store's build, which has none -- the app is the same window
//! onto the site it always was, and the page never asks. The site shows
//! Steam's buttons only in a window whose user agent ends ` store/steam`,
//! which the app adds only when Steam is there to answer them (`store`,
//! and `user_agent` in chrome.rs); the site marks the root
//! `data-store="steam"` from it, before the page paints, so neither button
//! is drawn in a window that could do nothing with it.

// Without the `steam` feature, what reads pages and writes scripts for Steam
// is still built and tested, and nothing calls it.
#![cfg_attr(not(feature = "steam"), allow(dead_code))]

use std::sync::Arc;

use tauri::webview::PageLoadEvent;
use tauri::{AppHandle, Manager, WebviewWindowBuilder};
use url::Url;

use crate::bridge::Page;
use crate::{site, WINDOW};

#[cfg(feature = "steam")]
mod client;
#[cfg(feature = "steam")]
pub use client::{start, Steam};

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

    pub fn show_store(&self, _app_id: u32) {
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

/// The store to name in the user agent (chrome.rs): Steam, when Steam
/// started the app and so can answer a sign-in or sell the Supporter Pack,
/// and none otherwise -- the Microsoft Store's build included, which has no
/// Steam, and the Steam build started without it, which cannot reach it.
#[cfg_attr(not(windows), allow(dead_code))]
pub fn store(steam: &Option<Arc<Steam>>) -> Option<&'static str> {
    steam.as_ref().map(|_| "steam")
}

/// A value as JavaScript reads it: JSON, so a string arrives quoted and
/// escaped, and cannot close its own quotes to run as script.
fn quoted(value: &impl serde::Serialize) -> String {
    serde_json::to_string(value).expect("a value serialises")
}

/// Answers the page's Steam sign-in with a ticket, or with `{}` when Steam
/// would not give one, which the page tells the player about in its own
/// words. Guarded, as every call into the page is: a page that has gone
/// since it asked -- or the loader, which never asks -- has no
/// `oeeeApp.signIn` to answer.
fn sign_in_script(ticket: Option<&str>) -> String {
    let told = match ticket {
        Some(ticket) => serde_json::json!({ "ticket": ticket }),
        None => serde_json::json!({}),
    };
    format!(
        "window.oeeeApp && window.oeeeApp.signIn && window.oeeeApp.signIn.answer({});",
        quoted(&told)
    )
}

/// Hands the page a ticket as proof of a DLC bought. The page posts it to
/// the site, which asks Steam what the account owns; it signs nobody in, and
/// reloads only when the site took something, so it can come mid-drawing
/// with the same care as any purchase.
fn purchased_script(ticket: &str) -> String {
    format!(
        "window.oeeeApp && window.oeeeApp.store && window.oeeeApp.store.purchased({});",
        quoted(&[ticket])
    )
}

/// Says the player is browsing whenever the window shows a page that is not
/// the site's -- the loader, or its "can't be reached" page -- which sends no
/// `page` message to say otherwise.
pub fn prepare<'a, M: Manager<tauri::Wry>>(
    builder: WebviewWindowBuilder<'a, tauri::Wry, M>,
    steam: &Option<Arc<Steam>>,
    site: &Url,
) -> WebviewWindowBuilder<'a, tauri::Wry, M> {
    let Some(steam) = steam.clone() else {
        return builder;
    };
    let site = site.clone();
    builder.on_page_load(move |_window, payload| {
            if payload.event() == PageLoadEvent::Finished && !site::is_site(payload.url(), &site) {
                steam.show_presence(None);
            }
        })
}

/// A DLC bought while the app is open -- the Supporter Pack, in the overlay
/// `open_store` showed -- is handed to the page as a ticket at once, so the
/// site hears of it now rather than at its daily recheck.
pub fn watch_dlc(app: &AppHandle, steam: &Option<Arc<Steam>>) {
    let Some(steam) = steam else {
        return;
    };
    let app = app.clone();
    let asker = steam.clone();
    steam.on_dlc_installed(move |_app_id| {
        // Off the thread that pumps Steam's callbacks: the ticket arrives by
        // one, so waiting for it there would wait forever.
        let (app, steam) = (app.clone(), asker.clone());
        std::thread::spawn(move || match steam.web_api_ticket() {
            Ok(ticket) => {
                if let Some(window) = app.get_webview_window(WINDOW) {
                    let _ = window.eval(purchased_script(&ticket));
                }
            }
            // The site rechecks daily, so the badge still comes, only later.
            Err(error) => eprintln!("a DLC was installed, but no Steam ticket: {error}"),
        });
    });
}

/// Gets a ticket from Steam and answers the page's sign-in with it. Off the
/// main thread: Steam can take a moment to answer.
pub fn answer_sign_in(app: &AppHandle, steam: Arc<Steam>) {
    let app = app.clone();
    std::thread::spawn(move || {
        let ticket = steam
            .web_api_ticket()
            .inspect_err(|error| eprintln!("no Steam ticket: {error}"))
            .ok();
        if let Some(window) = app.get_webview_window(WINDOW) {
            let _ = window.eval(sign_in_script(ticket.as_deref()));
        }
    });
}

/// Opens the overlay on a DLC's store page, where the player buys it; what
/// comes of that arrives by `watch_dlc`. The page names the DLC by its app
/// id, and anything else is the page's mistake, said and left.
pub fn open_store(steam: &Steam, product: &str) {
    match product.parse::<u32>() {
        Ok(app_id) => steam.show_store(app_id),
        Err(_) => eprintln!("not a Steam app id to sell: {product:?}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn a_ticket_answers_the_sign_in_quoted_as_javascript() {
        assert_eq!(
            sign_in_script(Some("14\"00ab")),
            r#"window.oeeeApp && window.oeeeApp.signIn && window.oeeeApp.signIn.answer({"ticket":"14\"00ab"});"#
        );
        assert!(sign_in_script(None).ends_with(".answer({});"));
    }

    #[test]
    fn a_ticket_is_proof_of_a_purchase() {
        assert_eq!(
            purchased_script("1400abff"),
            r#"window.oeeeApp && window.oeeeApp.store && window.oeeeApp.store.purchased(["1400abff"]);"#
        );
    }
}
