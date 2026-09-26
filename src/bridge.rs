//! What the site tells the app, by the one channel it tells it by.
//!
//! The site used to be read: its toolbar's badge for the unread count, and a
//! `<meta>` evaluated out of each page as it finished loading for Steam. Each
//! of those broke silently when the site changed -- the badge read 0 for weeks
//! after the bell stopped rendering it, and a boosted navigation never
//! finishes loading, so Steam went on showing the page before it. Now the site
//! says these things itself (app_bridge.jinja in oeee-cafe/web): it calls
//! `oeeeBridge.postMessage` with a JSON string, which bridge.js hands to the
//! app as an `oeee-bridge` event.
//!
//! Every message is `{v, type, ...}`. Types the app has no use for --
//! haptics, a finger landing on a drawing, the painter being ready -- and
//! fields it does not read are ignored, so the site can add either without a
//! release of the app.

use serde::Deserialize;

use crate::words::Words;

/// Defines `window.oeeeBridge` before the page's first script runs.
pub const SCRIPT: &str = include_str!("bridge.js");

/// The event bridge.js emits each message in.
pub const EVENT: &str = "oeee-bridge";

/// The version of the contract this app speaks. A message of another is
/// ignored rather than guessed at: the site bumps it only for a change the
/// app could not read correctly.
const VERSION: u32 = 1;

#[derive(Debug, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum Message {
    /// On every page, and again whenever any of it changes.
    Page(Page),
    /// The number on the bell.
    Unread { count: i64 },
    /// The page's colours, of which the app uses the ground (chrome.rs).
    Theme(Theme),
    /// What the app says in its own dialogs and menus, in the page's
    /// language (words.rs). Sent once a page.
    Words(Words),
    /// Open this address in the system's browser: a sign-in the page is
    /// handing out (handoff.rs, app_sign_in.jinja).
    Browse { url: String },
    /// A sign-in the page is carrying (app_sign_in.jinja). The page decides
    /// which provider each app signs in with itself, and the only one it
    /// ever asks this app for is Steam's -- in the Steam build, whose user
    /// agent names Steam -- which is answered with a Web API ticket
    /// (steam.rs). Apple and Google go by `browse` here. So the provider it
    /// names is not read: the app does what it is asked.
    #[serde(rename = "signIn")]
    SignIn,
    /// What these products cost, through the store the build sells through
    /// (app_store.jinja), answered with `oeeeApp.store.prices` -- or not at
    /// all, and the page shows its buttons without a price. Steam app ids
    /// in the Steam build (steam.rs), Store IDs in the Microsoft Store's
    /// (microsoft.rs).
    Prices { products: Vec<String> },
    /// Sell this product, through the store the build sells through
    /// (app_store.jinja). In the Steam build the product is the Steam app id
    /// of a DLC, and selling it is showing its page in the overlay; in the
    /// Microsoft Store's it is an add-on's Store ID, and selling it is the
    /// Store's own purchase dialog.
    Purchase { product: String },
    /// The toolbar's window controls on Windows (app_caption.jinja):
    /// "minimize", "maximize" (which restores, too) or "close". The Mac's
    /// "drag" and "zoom" are the iOS app's and not asked here.
    Window { action: String },
    /// Where the toolbar's maximise button is on Windows, for the Snap
    /// Layouts stand-in over it (snap.rs): its place, or none.
    Caption {
        #[serde(default)]
        place: Option<Place>,
    },
    /// A new notification for the reader, heard by the page while the window
    /// was not in front: shown as Windows's own (notify.rs). `url` is a
    /// path on the site.
    Notify {
        title: String,
        body: String,
        url: String,
    },
    /// Anything else the site says -- `restore` among it, which neither
    /// store here has: Steam and the Microsoft Store both keep what was
    /// bought on the account, and the site asks them.
    #[serde(other)]
    Other,
}

/// The toolbar's maximise button's place in the window, in physical pixels
/// from the top left of the window's client area -- which is where the
/// webview starts.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
pub struct Place {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
}

/// The largest the stand-in may be on a side, so a page cannot spread it
/// over the rest of itself: a caption button is 46 by 52 at 100%, and this
/// is room for it at 400%.
#[cfg_attr(not(windows), allow(dead_code))]
const MOST: i32 = 256;

// Only Windows has the stand-in (snap.rs) that is placed by it.
#[cfg_attr(not(windows), allow(dead_code))]
impl Place {
    /// None for a button with no size, and one past a button's size as no
    /// more than one.
    pub fn bounded(self) -> Option<Place> {
        if self.width <= 0 || self.height <= 0 {
            return None;
        }
        Some(Place {
            x: self.x.max(0),
            y: self.y.max(0),
            width: self.width.min(MOST),
            height: self.height.min(MOST),
        })
    }
}

/// What the site says about the page showing, as far as the app uses it.
#[derive(Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Page {
    /// Whether anyone is signed in; none on a page without the toolbar,
    /// which cannot tell.
    #[serde(default)]
    pub signed_in: Option<bool>,
    /// What the player is doing (presence_meta.jinja), or none for browsing.
    #[serde(default)]
    pub presence: Option<String>,
    #[serde(default)]
    pub community: Option<String>,
    #[serde(default)]
    pub group: Option<String>,
}

/// What the site says about its colours, as far as the app uses them.
#[derive(Debug, Default, Deserialize, PartialEq, Eq)]
pub struct Theme {
    /// The design system's `--ds-ground` as a CSS colour, the field behind
    /// every page; none on a page without the design system's stylesheet.
    #[serde(default)]
    pub ground: Option<String>,
}

#[derive(Deserialize)]
struct Version {
    v: u32,
}

/// The message in an `oeee-bridge` event's payload. The payload is the JSON
/// of what the page emitted, which is itself a string of JSON -- a string
/// because that is all Android's channel carries, and the site sends every
/// app the same thing.
pub fn parse(payload: &str) -> Option<Message> {
    let text: String = serde_json::from_str(payload).ok()?;
    let Version { v } = serde_json::from_str(&text).ok()?;
    if v != VERSION {
        return None;
    }
    serde_json::from_str(&text).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A message as the event carries it: the page's string, as JSON.
    fn sent(message: &str) -> String {
        serde_json::to_string(message).unwrap()
    }

    /// bridge.js names the event in its own text, and main.rs listens by
    /// `EVENT`: renaming either alone leaves the page talking to nobody.
    #[test]
    fn the_script_emits_the_event_listened_for() {
        assert!(SCRIPT.contains(&format!(r#"event: "{EVENT}""#)));
    }

    #[test]
    fn the_unread_count_is_a_number() {
        assert_eq!(
            parse(&sent(r#"{"v":1,"type":"unread","count":3}"#)),
            Some(Message::Unread { count: 3 })
        );
        assert_eq!(parse(&sent(r#"{"v":1,"type":"unread","count":null}"#)), None);
        assert_eq!(parse(&sent(r#"{"v":1,"type":"unread","count":"x"}"#)), None);
    }

    #[test]
    fn the_theme_says_what_the_ground_is() {
        let message = sent(
            r##"{"v":1,"type":"theme","choice":"dark","dark":true,"top":"#000","bottom":"#000","ground":"#17172b","grid":"#22223f"}"##,
        );
        assert_eq!(
            parse(&message),
            Some(Message::Theme(Theme {
                ground: Some("#17172b".into()),
            }))
        );
        // A page without the design system's stylesheet has none.
        let message = sent(r#"{"v":1,"type":"theme","choice":"system","dark":false,"ground":null}"#);
        assert_eq!(parse(&message), Some(Message::Theme(Theme::default())));
    }

    #[test]
    fn words_the_page_leaves_out_are_the_apps_own() {
        let Some(Message::Words(words)) = parse(&sent(r#"{"v":1,"type":"words","leave":"떠나기"}"#)) else {
            panic!("not understood");
        };
        assert_eq!(words.leave, "떠나기");
        assert_eq!(words.stay, Words::default().stay);
    }

    #[test]
    fn a_page_asks_for_the_browser() {
        assert_eq!(
            parse(&sent(
                r#"{"v":1,"type":"browse","url":"https://oeee.cafe/auth/handoff/open?id=1"}"#
            )),
            Some(Message::Browse {
                url: "https://oeee.cafe/auth/handoff/open?id=1".into()
            })
        );
        assert_eq!(parse(&sent(r#"{"v":1,"type":"browse"}"#)), None);
    }

    #[test]
    fn a_page_asks_for_a_sign_in() {
        for message in [
            r#"{"v":1,"type":"signIn","provider":"steam"}"#,
            r#"{"v":1,"type":"signIn","provider":"apple","nonce":"n"}"#,
            r#"{"v":1,"type":"signIn"}"#,
        ] {
            assert_eq!(parse(&sent(message)), Some(Message::SignIn), "{message}");
        }
    }

    #[test]
    fn a_page_asks_to_buy_a_product() {
        assert_eq!(
            parse(&sent(r#"{"v":1,"type":"purchase","product":"3456780"}"#)),
            Some(Message::Purchase {
                product: "3456780".into()
            })
        );
        // Fields the site used to send, or adds later, change nothing.
        assert_eq!(
            parse(&sent(
                r#"{"v":1,"type":"purchase","product":"3456780","store":"steam","year":2026}"#
            )),
            Some(Message::Purchase {
                product: "3456780".into()
            })
        );
        assert_eq!(parse(&sent(r#"{"v":1,"type":"purchase"}"#)), None);
    }

    #[test]
    fn a_page_asks_what_products_cost() {
        assert_eq!(
            parse(&sent(r#"{"v":1,"type":"prices","products":["3456780","9NBLGGH4R315"]}"#)),
            Some(Message::Prices {
                products: vec!["3456780".into(), "9NBLGGH4R315".into()]
            })
        );
        assert_eq!(parse(&sent(r#"{"v":1,"type":"prices"}"#)), None);
        assert_eq!(parse(&sent(r#"{"v":1,"type":"prices","products":[1]}"#)), None);
    }

    #[test]
    fn what_the_app_does_not_know_is_ignored() {
        for message in [
            r#"{"v":1,"type":"restore"}"#,
            r#"{"v":1,"type":"steamTicket"}"#,
            r#"{"v":1,"type":"haptic","name":"light"}"#,
            r#"{"v":1,"type":"something-new"}"#,
        ] {
            assert_eq!(parse(&sent(message)), Some(Message::Other), "{message}");
        }
    }

    #[test]
    fn a_community_name_keeps_its_quotes() {
        let message = sent(
            r#"{"v":1,"type":"page","path":"/replay","signedIn":false,"presence":"watching-replay","community":"오이카페 \"모에화\"","group":null,"painting":false,"refreshable":false}"#,
        );
        assert_eq!(
            parse(&message),
            Some(Message::Page(Page {
                signed_in: Some(false),
                presence: Some("watching-replay".into()),
                community: Some("오이카페 \"모에화\"".into()),
                group: None,
            }))
        );
    }

    #[test]
    fn another_version_or_no_json_is_nothing() {
        assert_eq!(parse(&sent(r#"{"v":2,"type":"unread","count":3}"#)), None);
        assert_eq!(parse(&sent(r#"{"type":"unread","count":3}"#)), None);
        assert_eq!(parse(&sent("not json")), None);
        assert_eq!(parse("3"), None);
        assert_eq!(parse("null"), None);
    }

    #[test]
    fn the_page_says_where_its_button_is() {
        let place = Place { x: 1782, y: 0, width: 69, height: 78 };
        assert_eq!(
            parse(&sent(r#"{"v":1,"type":"caption","place":{"x":1782,"y":0,"width":69,"height":78}}"#)),
            Some(Message::Caption { place: Some(place) })
        );
        assert_eq!(
            parse(&sent(r#"{"v":1,"type":"caption","place":null}"#)),
            Some(Message::Caption { place: None })
        );
    }

    #[test]
    fn the_stand_in_is_never_bigger_than_a_button() {
        assert_eq!(
            Place { x: -5, y: -5, width: 5000, height: 5000 }.bounded(),
            Some(Place { x: 0, y: 0, width: MOST, height: MOST })
        );
        assert_eq!(Place { x: 0, y: 0, width: 0, height: 52 }.bounded(), None);
    }
}
