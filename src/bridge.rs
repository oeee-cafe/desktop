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
//! Every message is `{v, type, ...}`. Types the app has no use for -- the
//! page's theme, haptics, a finger landing on a drawing, the painter being
//! ready -- and fields it does not read are ignored, so the site can add
//! either without a release of the app.

use serde::Deserialize;

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
    /// Anything else the site says.
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
    fn what_the_app_does_not_know_is_ignored() {
        for message in [
            r##"{"v":1,"type":"theme","choice":"dark","dark":true,"top":"#000","bottom":"#000"}"##,
            r#"{"v":1,"type":"haptic","name":"light"}"#,
            r#"{"v":1,"type":"something-new"}"#,
        ] {
            assert_eq!(parse(&sent(message)), Some(Message::Other), "{message}");
        }
    }

    /// What app_bridge.jinja (oeee-cafe-web) really sends, captured from its
    /// script running in Chromium over pages shaped like the site's: a feed
    /// signed in, a collaborative painter, a replay signed out, and a page
    /// without the toolbar. Recapture it when the site's script changes.
    const CAPTURED: &str = include_str!("bridge-messages.jsonl");

    #[test]
    fn what_the_site_really_sends_is_understood() {
        let parsed: Vec<Message> = CAPTURED
            .lines()
            .filter(|line| !line.trim().is_empty())
            .map(|line| parse(&sent(line)).unwrap_or_else(|| panic!("not understood: {line}")))
            .collect();
        let unread: Vec<i64> = parsed
            .iter()
            .filter_map(|m| match m {
                Message::Unread { count } => Some(*count),
                _ => None,
            })
            .collect();
        assert_eq!(unread, [12, 0]);
        let pages: Vec<&Page> = parsed
            .iter()
            .filter_map(|m| match m {
                Message::Page(page) => Some(page),
                _ => None,
            })
            .collect();
        assert_eq!(
            pages.iter().map(|p| p.signed_in).collect::<Vec<_>>(),
            [Some(true), Some(true), Some(false), None]
        );
        assert_eq!(pages[1].presence.as_deref(), Some("collaborating"));
        assert_eq!(pages[1].group.as_deref(), Some("0123456789abcdef"));
        assert_eq!(pages[2].community.as_deref(), Some("오이카페 \"모에화\""));
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
