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

use std::sync::mpsc::{self, Sender};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde::Deserialize;
use steamworks::{AuthTicket, CallbackHandle, Client, TicketForWebApiResponse};
use url::Url;

/// Names the service the ticket is for; the site accepts no other. Has to
/// match `TICKET_IDENTITY` in oeee-cafe/web's `src/steam.rs`.
const STEAM_TICKET_IDENTITY: &str = "oeee-cafe";

/// The path of the site's "Sign in with Steam" link.
const SIGN_IN_PATH: &str = "/auth/steam/app";

/// How long Steam gets to hand over a ticket. It usually answers within a
/// second; a player waiting longer than this is better told it failed.
const TICKET_TIMEOUT: Duration = Duration::from_secs(15);

type Waiting = Arc<Mutex<Vec<(AuthTicket, Sender<Result<Vec<u8>, String>>)>>>;

pub struct Steam {
    client: Client,
    waiting: Waiting,
    _ticket_callback: CallbackHandle,
}

/// Starts Steam's API, or says why not and carries on without it.
pub fn start() -> Option<Arc<Steam>> {
    let client = match Client::init() {
        Ok(client) => client,
        Err(error) => {
            eprintln!("running without Steam: {error}");
            return None;
        }
    };

    let waiting: Waiting = Arc::default();
    let ticket_callback = client.register_callback({
        let waiting = waiting.clone();
        move |response: TicketForWebApiResponse| {
            let reply = {
                let mut waiting = waiting.lock().unwrap();
                let Some(at) = waiting.iter().position(|(h, _)| *h == response.ticket_handle) else {
                    return;
                };
                waiting.swap_remove(at).1
            };
            let _ = reply.send(match response.result {
                Ok(()) => {
                    let len = usize::try_from(response.ticket_len).unwrap_or(0);
                    Ok(response.ticket.get(..len).unwrap_or_default().to_vec())
                }
                Err(error) => Err(error.to_string()),
            });
        }
    });

    // Callbacks arrive only when asked for. Nothing here is urgent, so ten
    // times a second is plenty.
    let pump = client.clone();
    std::thread::Builder::new()
        .name("steam-callbacks".into())
        .spawn(move || loop {
            pump.run_callbacks();
            std::thread::sleep(Duration::from_millis(100));
        })
        .ok()?;

    Some(Arc::new(Steam {
        client,
        waiting,
        _ticket_callback: ticket_callback,
    }))
}

impl Steam {
    /// A Web API ticket for the site, hex-encoded. Blocks until Steam
    /// answers, so it is called off the main thread.
    pub fn web_api_ticket(&self) -> Result<String, String> {
        let (reply, answer) = mpsc::channel();
        {
            // Held across the request, so the callback cannot look for the
            // handle before it is here to be found.
            let mut waiting = self.waiting.lock().unwrap();
            let handle = self
                .client
                .user()
                .authentication_session_ticket_for_webapi(STEAM_TICKET_IDENTITY);
            waiting.push((handle, reply));
        }
        let ticket = answer
            .recv_timeout(TICKET_TIMEOUT)
            .map_err(|_| "Steam did not answer".to_string())??;
        if ticket.is_empty() {
            return Err("Steam gave an empty ticket".to_string());
        }
        Ok(hex(&ticket))
    }
}

impl Steam {
    /// Tells Steam what the page says the player is doing. `answer` is what
    /// [`READ_PRESENCE`] evaluated to, as JSON.
    pub fn show_presence(&self, answer: &str) {
        let page: Option<PagePresence> = serde_json::from_str(answer).unwrap_or(None);
        let friends = self.client.friends();
        for (key, value) in rich_presence(page.as_ref()) {
            friends.set_rich_presence(key, value.as_deref());
        }
    }
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
