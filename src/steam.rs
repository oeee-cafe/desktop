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
//! it. Before any of that the page asks `prices`, and the app asks Steam's
//! public store API what each DLC costs in the player's country (Steam says
//! which, from where they are), and answers `oeeeApp.store.prices` with
//! Steam's own formatting of it.
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

use std::collections::BTreeMap;
use std::sync::Arc;

use tauri::webview::PageLoadEvent;
use tauri::{AppHandle, Manager, WebviewWindowBuilder};
use url::Url;

use crate::bridge::Page;
use crate::store::{self, quoted};
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

    pub fn country(&self) -> String {
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
/// and none otherwise: the Microsoft Store's build has no Steam (and names
/// its own store, microsoft.rs), and the Steam build started without it
/// cannot reach it.
#[cfg_attr(not(windows), allow(dead_code))]
pub fn store(steam: &Option<Arc<Steam>>) -> Option<&'static str> {
    steam.as_ref().map(|_| "steam")
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
                    let _ = window.eval(store::purchased_script(&[&ticket]));
                }
            }
            // The site rechecks daily, so the badge still comes, only later.
            Err(error) => eprintln!("a DLC was installed, but no Steam ticket: {error}"),
        });
    });
}

/// Gets a ticket from Steam and answers the page's sign-in with it, or with
/// nothing when there is no Steam to ask. Off the main thread: Steam can
/// take a moment to answer.
pub fn answer_sign_in(app: &AppHandle, steam: Option<Arc<Steam>>) {
    let app = app.clone();
    std::thread::spawn(move || {
        let ticket = match &steam {
            Some(steam) => steam
                .web_api_ticket()
                .inspect_err(|error| eprintln!("no Steam ticket: {error}"))
                .ok(),
            None => {
                eprintln!("a page asked for Steam's sign-in, and Steam is not here");
                None
            }
        };
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

/// Steam's public store API: what an app costs, and without a key, so the
/// app can ask it and the site need not be asked on its behalf.
const APPDETAILS: &str = "https://store.steampowered.com/api/appdetails";

/// How long Steam's store gets to say what things cost. The page shows its
/// button without a price until then, so there is no hurry beyond not
/// leaving a thread waiting for ever.
const PRICES_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(15);

/// The address to ask the prices of several apps at once -- which Steam
/// allows only when all that is asked for is `price_overview` -- in a
/// country's currency, or in Steam's default when it could not say where
/// the player is.
fn appdetails_url(app_ids: &[u32], country: &str) -> String {
    let ids: Vec<String> = app_ids.iter().map(u32::to_string).collect();
    let mut url = format!("{APPDETAILS}?appids={}&filters=price_overview", ids.join(","));
    if country.len() == 2 && country.chars().all(|c| c.is_ascii_alphabetic()) {
        url.push_str("&cc=");
        url.push_str(country);
    }
    url
}

/// Each app's price as Steam formats it for the player -- "₩5,500", "$4.99"
/// -- from what `appdetails` answers. An app Steam did not find, or has no
/// price for (it answers `"data": []` then), is left out.
fn prices_from(body: &str) -> Result<BTreeMap<u32, String>, String> {
    let answer: BTreeMap<String, serde_json::Value> =
        serde_json::from_str(body).map_err(|error| format!("not an appdetails answer: {error}"))?;
    Ok(answer
        .into_iter()
        .filter_map(|(id, details)| {
            let id = id.parse::<u32>().ok()?;
            let price = details
                .pointer("/data/price_overview/final_formatted")?
                .as_str()?
                .trim();
            (!price.is_empty()).then(|| (id, price.to_string()))
        })
        .collect())
}

#[cfg(feature = "steam")]
fn fetch(url: &str) -> Result<String, String> {
    let config = ureq::Agent::config_builder().timeout_global(Some(PRICES_TIMEOUT));
    // Windows' own TLS, trusting what Windows trusts (Cargo.toml).
    #[cfg(windows)]
    let config = config.tls_config(
        ureq::tls::TlsConfig::builder()
            .provider(ureq::tls::TlsProvider::NativeTls)
            .root_certs(ureq::tls::RootCerts::PlatformVerifier)
            .build(),
    );
    let agent: ureq::Agent = config.build().into();
    agent
        .get(url)
        .header("Accept", "application/json")
        .call()
        .map_err(|error| error.to_string())?
        .body_mut()
        .read_to_string()
        .map_err(|error| error.to_string())
}

#[cfg(not(feature = "steam"))]
fn fetch(_url: &str) -> Result<String, String> {
    Err("built without Steam".to_string())
}

/// Answers the page's `prices` with what Steam's store says each DLC costs
/// where the player is. Off the main thread, and in one request for all of
/// them. The page names each DLC by its app id; anything else is left
/// without a price, as is everything when Steam's store cannot be reached,
/// and the page shows its button without one.
pub fn answer_prices(app: &AppHandle, steam: &Steam, products: Vec<String>) {
    let asked: Vec<(String, u32)> = products
        .into_iter()
        .filter_map(|product| match product.parse::<u32>() {
            Ok(app_id) => Some((product, app_id)),
            Err(_) => {
                eprintln!("not a Steam app id to price: {product:?}");
                None
            }
        })
        .collect();
    if asked.is_empty() {
        return;
    }
    let country = steam.country();
    let app = app.clone();
    std::thread::spawn(move || {
        let app_ids: Vec<u32> = asked.iter().map(|(_, app_id)| *app_id).collect();
        let found = match fetch(&appdetails_url(&app_ids, &country)).and_then(|body| prices_from(&body)) {
            Ok(found) => found,
            Err(error) => {
                eprintln!("no prices from Steam: {error}");
                return;
            }
        };
        let prices: BTreeMap<String, String> = asked
            .into_iter()
            .filter_map(|(product, app_id)| Some((product, found.get(&app_id)?.clone())))
            .collect();
        if prices.is_empty() {
            eprintln!("Steam has no price for {app_ids:?} in {country:?}");
            return;
        }
        if let Some(window) = app.get_webview_window(WINDOW) {
            let _ = window.eval(store::prices_script(&prices));
        }
    });
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
    fn prices_are_asked_for_all_at_once_in_the_players_country() {
        assert_eq!(
            appdetails_url(&[3456780, 3456790], "KR"),
            "https://store.steampowered.com/api/appdetails?appids=3456780,3456790&filters=price_overview&cc=KR"
        );
        // Steam could not say where the player is, or said something odd.
        for country in ["", "K", "K&", "KOR"] {
            assert!(appdetails_url(&[3456780], country).ends_with("filters=price_overview"));
        }
    }

    #[test]
    fn a_price_is_steams_own_formatting() {
        // As Steam answers, trimmed: one app with a price, one Steam has no
        // price for (free, or not on sale in that country), one it does not
        // know.
        let body = r#"{
            "3456780": {"success": true, "data": {"price_overview": {
                "currency": "KRW", "initial": 550000, "final": 550000,
                "discount_percent": 0, "initial_formatted": "",
                "final_formatted": "₩ 5,500"
            }}},
            "3456790": {"success": true, "data": []},
            "1": {"success": false}
        }"#;
        assert_eq!(
            prices_from(body),
            Ok(BTreeMap::from([(3456780, "₩ 5,500".to_string())]))
        );
        // What Steam answers a request it cannot read, and an error page.
        assert!(prices_from("null").is_err());
        assert!(prices_from("<html>").is_err());
    }

    #[test]
    fn a_ticket_answers_the_sign_in_quoted_as_javascript() {
        assert_eq!(
            sign_in_script(Some("14\"00ab")),
            r#"window.oeeeApp && window.oeeeApp.signIn && window.oeeeApp.signIn.answer({"ticket":"14\"00ab"});"#
        );
        assert!(sign_in_script(None).ends_with(".answer({});"));
    }
}
