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
    /// A sign-in the page is carrying (app_sign_in.jinja). The desktop's
    /// only one of its own is Steam's, which the Steam build answers with a
    /// Web API ticket (steam.rs); Apple and Google go by `browse` here, so a
    /// page never asks for them, and anything but "steam" is left alone.
    #[serde(rename = "signIn")]
    SignIn { provider: String },
    /// Sell this product, through the store the build sells through
    /// (app_store.jinja). In the Steam build the product is the Steam app id
    /// of a DLC, and selling it is showing its page in the overlay; the
    /// Microsoft Store build sells nothing yet and marks no page, so no page
    /// asks it.
    Purchase { product: String },
    /// Anything else the site says -- `prices` among it: the page shows its
    /// button without a price when the app does not answer, which is what
    /// the contract allows while the app has no way to ask Steam for one.
    #[serde(other)]
    Other,
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

    #[test]
    fn the_script_emits_the_event_listened_for() {
        assert!(SCRIPT.contains(&format!(r#"event: "{EVENT}""#)));
        assert!(SCRIPT.contains("window.oeeeBridge = {"));
        assert!(SCRIPT.contains("postMessage: function (text)"));
    }

    #[test]
    fn a_page_says_what_the_player_is_doing() {
        let message = sent(
            r#"{"v":1,"type":"page","path":"/draw","signedIn":true,"presence":"drawing","community":"오이","group":null,"painting":true,"refreshable":false}"#,
        );
        assert_eq!(
            parse(&message),
            Some(Message::Page(Page {
                signed_in: Some(true),
                presence: Some("drawing".into()),
                community: Some("오이".into()),
                group: None,
            }))
        );
    }

    #[test]
    fn a_page_without_the_toolbar_cannot_say_who_is_signed_in() {
        let message = sent(r#"{"v":1,"type":"page","signedIn":null,"presence":null}"#);
        assert_eq!(parse(&message), Some(Message::Page(Page::default())));
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
    fn the_words_are_the_pages() {
        let message = sent(
            r#"{"v":1,"type":"words","leaveTitle":"이 페이지를 떠날까요?","leaveBody":"저장하지 않은 내용은 사라집니다.","leave":"떠나기","stay":"머무르기","ok":"확인","cancel":"취소","saveImage":"이미지 저장","copyImage":"이미지 복사","share":"공유…","copyLink":"링크 복사","savedImage":"사진에 저장했습니다","savedFile":"다운로드에 저장했습니다","saveFailed":"저장하지 못했습니다"}"#,
        );
        let Some(Message::Words(words)) = parse(&message) else {
            panic!("not understood: {message}");
        };
        assert_eq!(words.leave_title, "이 페이지를 떠날까요?");
        assert_eq!(words.leave, "떠나기");
        assert_eq!(words.stay, "머무르기");
        assert_eq!(words.copy_link, "링크 복사");
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
        assert_eq!(
            parse(&sent(r#"{"v":1,"type":"signIn","provider":"steam"}"#)),
            Some(Message::SignIn {
                provider: "steam".into()
            })
        );
        // Another provider is still a sign-in; the app decides it is not its.
        assert_eq!(
            parse(&sent(r#"{"v":1,"type":"signIn","provider":"apple","nonce":"n"}"#)),
            Some(Message::SignIn {
                provider: "apple".into()
            })
        );
        assert_eq!(parse(&sent(r#"{"v":1,"type":"signIn"}"#)), None);
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
    fn what_the_app_does_not_know_is_ignored() {
        for message in [
            r#"{"v":1,"type":"prices","products":["3456780"]}"#,
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
}
