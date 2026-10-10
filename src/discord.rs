//! Discord, when it is running beside the app.
//!
//! What the player is doing shows on their Discord profile, as it does for
//! their Steam friends (steam.rs): the site says it in each `page` message
//! (bridge.rs; `src/web/presence.rs` in oeee-cafe/web), in words the site
//! also sends, in the page's language (words.rs). Discord has no file of
//! words of its own to look them up in, as Steam has, so what the player's
//! friends read is in the player's language rather than each their own.
//!
//! And in a collaborative room a friend can be asked in. The page says how
//! to join it (`join`, a path on the site), and the app hands that to
//! Discord as the activity's join secret: Discord never shows it, and gives
//! it only to a friend the player invited from Discord or let in when they
//! asked. Discord then opens the friend's own Oeee Cafe app -- through Steam
//! in the Steam build, through `oeee-cafe://` otherwise -- and the app, told
//! the secret, opens the room. Everything past that is the site's: whether
//! they are signed in, whether the room has a seat, whether they may enter
//! it. Only a path into a room is ever opened ([`room_path`]): the secret
//! comes from someone else's app.
//!
//! The Social SDK talks to the Discord app on the same computer, without the
//! player signing in to Discord here or the site learning their Discord
//! account. Without Discord running, or in a build without the `discord`
//! feature, there is nobody to tell and nothing happens.

// Without the `discord` feature, what reads pages and checks secrets is
// still built and tested, and nothing calls it.
#![cfg_attr(not(feature = "discord"), allow(dead_code))]

use crate::bridge::Page;
use crate::words::Words;

#[cfg(feature = "discord")]
mod client;
#[cfg(feature = "discord")]
pub use client::{start, Discord};

/// Discord, in a build without it: there is never one.
#[cfg(not(feature = "discord"))]
pub enum Discord {}

#[cfg(not(feature = "discord"))]
impl Discord {
    pub fn show_presence(&self, _page: Option<&Page>) {
        match *self {}
    }

    pub fn page_shown(&self) {
        match *self {}
    }
}

#[cfg(not(feature = "discord"))]
pub fn start(
    _app: &tauri::AppHandle,
    _site: &url::Url,
    _steam_app: Option<u32>,
) -> Option<std::sync::Arc<Discord>> {
    None
}

/// The application Oeee Cafe is on Discord's Developer Portal.
pub const APPLICATION_ID: u64 = 1558602531133722646;

/// How Discord opens the app for a friend who was asked in, outside Steam:
/// the scheme both builds answer (handoff.rs). It carries nothing; the
/// secret comes by the SDK once the app is up.
pub const LAUNCH: &str = "oeee-cafe://discord";

/// What the player is doing, as Discord is to show it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Shown {
    /// The first line: what they are doing.
    pub details: String,
    /// The second: the community it is in, when it is public.
    pub state: Option<String>,
    /// The room they are in, when a friend could join it.
    pub room: Option<Room>,
}

/// A collaborative room, as a party friends can be asked into.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Room {
    /// The same for everyone in the room, and not the room's address
    /// (`group`, presence.rs in oeee-cafe/web).
    pub party: String,
    /// The path that joins it, handed to Discord as the join secret.
    pub join: String,
    pub size: i32,
    pub seats: i32,
}

/// What Discord is to show for a page, in the page's words, or what it is
/// to show with no page of the site to say -- the loader -- which is
/// browsing.
pub fn shown(page: Option<&Page>, words: &Words) -> Shown {
    let activity = page.and_then(|p| p.presence.as_deref()).unwrap_or("");
    let (details, in_community) = match activity {
        "drawing" => (&words.presence_drawing, true),
        "relaying" => (&words.presence_relaying, true),
        "collaborating" => (&words.presence_collaborating, true),
        "drawing-banner" => (&words.presence_drawing_banner, false),
        "watching-replay" => (&words.presence_watching_replay, false),
        _ => (&words.presence_browsing, false),
    };
    let state = page
        .and_then(|p| p.community.as_deref())
        .filter(|_| in_community)
        .map(|c| text(c, STATE))
        .filter(|c| !c.is_empty());
    let room = page.filter(|_| activity == "collaborating").and_then(room);
    Shown {
        details: text(details, DETAILS),
        state,
        room,
    }
}

fn room(page: &Page) -> Option<Room> {
    let party = page.group.as_deref().filter(|g| !g.is_empty())?;
    let join = page.join.as_deref().and_then(room_path)?;
    let seats = page.seats.filter(|s| *s > 0)?;
    // A room the page has not counted yet has the player in it, at least.
    let size = page.party.unwrap_or(1).clamp(1, seats);
    Some(Room {
        party: text(party, PARTY),
        join,
        size,
        seats,
    })
}

/// The path into a collaborative room that `secret` names, or none for
/// anything else: a join secret is another player's app's word, and the
/// window opens nothing on the site but a room by it.
pub fn room_path(secret: &str) -> Option<String> {
    let id = secret.strip_prefix("/collaborate/")?;
    is_uuid(id).then(|| format!("/collaborate/{id}"))
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

/// The longest Discord takes each field, in bytes.
const DETAILS: usize = 128;
const STATE: usize = 128;
const PARTY: usize = 128;

/// `value` as Discord takes it: at most `most` bytes, cut where a character
/// ends, and at least two characters, which it insists on, or nothing.
fn text(value: &str, most: usize) -> String {
    let mut end = value.len().min(most);
    while !value.is_char_boundary(end) {
        end -= 1;
    }
    let value = &value[..end];
    if value.chars().count() < 2 {
        return String::new();
    }
    value.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    const ROOM: &str = "/collaborate/9c881320-2b43-4afa-b2bb-7128c8a3e985";

    fn room_page(join: Option<&str>, party: Option<i32>) -> Page {
        Page {
            presence: Some("collaborating".into()),
            community: Some("오이카페 모에화".into()),
            group: Some("0123456789abcdef".into()),
            join: join.map(str::to_string),
            seats: Some(4),
            party,
            ..Page::default()
        }
    }

    #[test]
    fn a_room_a_friend_can_join_is_a_party_they_can_be_asked_into() {
        assert_eq!(
            shown(Some(&room_page(Some(ROOM), Some(2))), &Words::default()),
            Shown {
                details: "Drawing together".into(),
                state: Some("오이카페 모에화".into()),
                room: Some(Room {
                    party: "0123456789abcdef".into(),
                    join: ROOM.into(),
                    size: 2,
                    seats: 4,
                }),
            }
        );
    }

    #[test]
    fn a_room_not_counted_yet_has_the_player_in_it_and_no_more_than_fit() {
        let size = |party| {
            shown(Some(&room_page(Some(ROOM), party)), &Words::default())
                .room
                .unwrap()
                .size
        };
        assert_eq!(size(None), 1);
        assert_eq!(size(Some(0)), 1);
        assert_eq!(size(Some(9)), 4);
    }

    #[test]
    fn a_room_a_friend_could_not_enter_is_shown_but_not_offered() {
        let page = room_page(None, Some(2));
        let shown = shown(Some(&page), &Words::default());
        assert_eq!(shown.details, "Drawing together");
        assert_eq!(shown.room, None);
    }

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
    fn anything_but_a_painter_is_browsing_and_names_no_community() {
        let page = Page {
            presence: Some("watching-replay".into()),
            community: Some("오이카페 모에화".into()),
            ..Page::default()
        };
        let replay = shown(Some(&page), &Words::default());
        assert_eq!(
            (replay.details.as_str(), replay.state),
            ("Watching a replay", None)
        );
        assert_eq!(
            shown(None, &Words::default()),
            Shown {
                details: "Browsing Oeee Cafe".into(),
                state: None,
                room: None
            }
        );
    }

    #[test]
    fn text_is_cut_where_a_character_ends_and_too_short_is_none() {
        assert_eq!(text(&"가".repeat(50), 128), "가".repeat(42));
        assert_eq!(text("a", 128), "");
        assert_eq!(text("ab", 128), "ab");
    }
}
