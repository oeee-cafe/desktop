//! Steam, when Steam started the app.
//!
//! Two things are asked of it. What the player is doing, for their friends
//! list: the site says it in `<meta name="oeee-presence">` on each page
//! (`src/web/presence.rs` in oeee-cafe/web), and the app hands it to Steam as
//! rich presence, in the words of `steam/rich_presence.vdf`.
//!
//! And who the player is: a Web API ticket
//! the site takes to Steam to find out (`src/steam.rs` in oeee-cafe/web).
//! The site cannot ask for one itself -- it has no way into the app -- so the
//! app watches for the site's "Sign in with Steam" link, which goes to
//! `/auth/steam/app`, stops that navigation, gets a ticket, and posts it to
//! `/auth/steam` from the page, as the page's own form would.
//!
//! Without Steam -- started from a terminal, or Steam not running -- the app is
//! the same window onto the site it always was, and the link is never shown:
//! the site draws it only on a page the app has marked `data-steam-app`.

// Without the `steam` feature, what reads pages and writes scripts for Steam
// is still built and tested, and nothing calls it.
#![cfg_attr(not(feature = "steam"), allow(dead_code))]

use serde::Deserialize;
use url::Url;

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

    pub fn show_presence(&self, _answer: &str) {
        match *self {}
    }
}

#[cfg(not(feature = "steam"))]
pub fn start() -> Option<std::sync::Arc<Steam>> {
    None
}

/// Reads the page's presence tag, or null when it has none.
pub const READ_PRESENCE: &str = r#"(function () {
  var tag = document.querySelector('meta[name="oeee-presence"]');
  if (!tag) return null;
  return {
    activity: tag.getAttribute("content"),
    community: tag.getAttribute("data-community"),
    group: tag.getAttribute("data-group")
  };
})()"#;

#[derive(Debug, Deserialize, PartialEq, Eq)]
struct PagePresence {
    activity: Option<String>,
    community: Option<String>,
    group: Option<String>,
}

/// The rich presence keys for what a page says, every key given a value or
/// cleared so nothing from the page before is left behind. `steam_display` is
/// a token of `steam/rich_presence.vdf`; `community` fills its `%community%`.
fn rich_presence(page: Option<&PagePresence>) -> [(&'static str, Option<String>); 3] {
    let activity = page.and_then(|p| p.activity.as_deref()).unwrap_or("");
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
    if url.origin() != site.origin() || url.path() != SIGN_IN_PATH {
        return None;
    }
    let next = url
        .query_pairs()
        .find(|(key, _)| key == "next")
        .map(|(_, value)| value.into_owned());
    Some(next)
}

/// Runs at the start of every page: tells the site Steam is here, which is
/// what shows its "Sign in with Steam" link.
pub const MARK_PAGE: &str = r#"document.documentElement.setAttribute("data-steam-app", "");"#;

/// Posts `ticket` to the site's `/auth/steam` from the page that is showing,
/// as a form on it would. Same-origin, so the session cookie goes with it and
/// the site can link the ticket's account to the one already signed in.
pub fn post_ticket_script(ticket: &str, next: Option<&str>) -> String {
    format!(
        r#"(function (ticket, next) {{
  var form = document.createElement("form");
  form.method = "post";
  form.action = "/auth/steam";
  function field(name, value) {{
    var input = document.createElement("input");
    input.type = "hidden";
    input.name = name;
    input.value = value;
    form.appendChild(input);
  }}
  field("ticket", ticket);
  if (next) field("next", next);
  document.body.appendChild(form);
  form.submit();
}})({}, {})"#,
        serde_json::to_string(ticket).expect("a string serialises"),
        serde_json::to_string(&next).expect("a string serialises"),
    )
}

/// Tells the player, in the page, that Steam did not sign them in.
pub fn failed_script(message: &str) -> String {
    format!(
        "window.alert({})",
        serde_json::to_string(message).expect("a string serialises")
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn site() -> Url {
        Url::parse("https://oeee.cafe/").unwrap()
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

    fn page(activity: &str, community: Option<&str>, group: Option<&str>) -> PagePresence {
        PagePresence {
            activity: Some(activity.to_string()),
            community: community.map(str::to_string),
            group: group.map(str::to_string),
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

    #[test]
    fn what_the_page_evaluates_to_is_read_as_json() {
        let read: Option<PagePresence> = serde_json::from_str(
            r#"{"activity":"drawing","community":"오이","group":null}"#,
        )
        .unwrap();
        assert_eq!(read, Some(page("drawing", Some("오이"), None)));
        let none: Option<PagePresence> = serde_json::from_str("null").unwrap();
        assert_eq!(none, None);
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
    fn what_reaches_the_page_is_quoted_as_javascript() {
        let script = post_ticket_script("1400ab", Some("/a\"</script>"));
        assert!(script.contains(r#"("1400ab", "/a\"</script>")"#));
        assert!(post_ticket_script("1400ab", None).ends_with(r#"("1400ab", null)"#));
    }
}
