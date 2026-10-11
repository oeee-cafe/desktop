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
//! it (joining.rs). Only a path into a room is ever opened ([`room_path`]): the secret
//! comes from someone else's app.
//!
//! The Social SDK talks to the Discord app on the same computer, without the
//! player signing in to Discord here or the site learning their Discord
//! account. Without Discord running, or in a build without the `discord`
//! feature, there is nobody to tell and nothing happens.
//!
//! A player who wants to ask friends in from the room itself connects their
//! Discord account to the app, once (linking.rs): the room's Invite button
//! (SessionHeader.tsx in oeee-cafe/web) asks for that the first time, and
//! the account page shows it and lets it go. Connected, the app lists their
//! Discord friends for the room's picker and sends the one picked an
//! invitation, as Discord's own + would. The page asks with `discord` on
//! the bridge ([`Asked`]), and is answered on `oeeeApp.discord`.
//!
//! The other way round, a friend who sees the player drawing together can
//! press Ask to Join on their activity in Discord. Connected, the app hears
//! the request and the room lists it (`oeeeApp.discord.requests`), with Let
//! in, which answers it with an invitation, and Not now, which only takes it
//! off the list. Not connected, the request is Discord's own to show.
//!
//! The clock beside the activity is the room's: when it opened, as the site
//! says it (`since`), so everyone in it shows the same time and a reload
//! does not start it again. Anywhere else it starts when what the player is
//! doing changes.

// Without the `discord` feature, what reads pages and checks secrets is
// still built and tested, and nothing calls it.
#![cfg_attr(not(feature = "discord"), allow(dead_code))]

use crate::bridge::Page;
use crate::joining::room_path;
use crate::words::Words;

#[cfg(feature = "discord")]
mod client;
#[cfg(feature = "discord")]
mod linking;
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

    pub fn asked(&self, _asked: Asked) {
        match *self {}
    }
}

#[cfg(not(feature = "discord"))]
pub fn start(
    _site: &url::Url,
    _steam_app: Option<u32>,
    _joining: std::sync::Arc<crate::joining::Joining>,
) -> Option<std::sync::Arc<Discord>> {
    None
}

/// The application Oeee Cafe is on Discord's Developer Portal.
pub const APPLICATION_ID: u64 = 1558602531133722646;

/// How Discord opens the app for a friend who was asked in, outside Steam:
/// the scheme both builds answer (handoff.rs), given to the app itself. It
/// carries nothing; the secret comes by the SDK once the app is up.
pub const LAUNCH: &str = "oeee-cafe://discord";

/// The command Discord is told to run: on Windows and Linux an executable
/// with its arguments, not a URL -- given a bare scheme, Discord takes the
/// app for one that cannot be started on that computer, and offers a friend
/// there only "Join From a Supported Device". This executable, quoted, with
/// [`LAUNCH`], which the running app takes as it takes the scheme opened.
pub fn launch_command() -> String {
    match std::env::current_exe() {
        Ok(exe) => format!("{} {LAUNCH}", quoted_exe(&exe.display().to_string())),
        // The SDK registers the running executable for an empty command.
        Err(_) => String::new(),
    }
}

/// `exe` in double quotes, as the SDK's own example of a command has it
/// (`RegisterLaunchCommand` in discordpp.h), which says nothing more of how
/// the command is read. On Windows a path cannot hold a `"`, and nothing in
/// it needs escaping. A Linux path can hold anything, and `"`, `` ` ``, `$`
/// and `\` are escaped with a `\`: inside double quotes that is what both a
/// shell and a desktop entry's `Exec` -- the two ways Discord could be
/// running it -- take them by. Single quotes would do for a shell, and are
/// not quotes at all to a desktop entry.
fn quoted_exe(exe: &str) -> String {
    let mut quoted = String::with_capacity(exe.len() + 2);
    quoted.push('"');
    for c in exe.chars() {
        if cfg!(not(windows)) && matches!(c, '"' | '`' | '$' | '\\') {
            quoted.push('\\');
        }
        quoted.push(c);
    }
    quoted.push('"');
    quoted
}

/// What the player is doing, as Discord is to show it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Shown {
    /// The first line: what they are doing.
    pub details: String,
    /// The second: the community it is in, when it is public.
    pub state: Option<String>,
    /// The room they are in, when a friend could join it.
    pub room: Option<Room>,
    /// When the room opened, in milliseconds since the epoch.
    pub since: Option<u64>,
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
    let in_room = page.filter(|_| activity == "collaborating");
    let room = in_room.and_then(room);
    // Not a time to come: a clock Discord would count down.
    let since = in_room
        .and_then(|p| p.since)
        .filter(|since| *since > 0 && *since <= now());
    Shown {
        details: text(details, DETAILS),
        state,
        room,
        since,
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

/// Milliseconds since the epoch, as Discord counts an activity's start.
pub fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as u64)
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

/// What the page asks of the player's Discord account (`discord` on the
/// bridge, bridge.rs).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Asked {
    /// Connect it: Discord's consent screen, then the account.
    Connect,
    /// Let it go, and forget it on this computer.
    Disconnect,
    /// The friends to offer an invitation, answered with
    /// `oeeeApp.discord.friends` now and whenever they change.
    Friends,
    /// Invite this Discord user to the room showing.
    Invite(u64),
    /// Let this Discord user, who asked to join, into the room showing: an
    /// invitation in answer to their request.
    LetIn(u64),
    /// Take their request off the list. Discord is not told.
    Dismiss(u64),
}

impl Asked {
    /// What `action` asks, with `user` for an invitation: a Discord id,
    /// which the page carries as a string, since a JavaScript number cannot
    /// hold one.
    pub fn from(action: &str, user: Option<&str>) -> Option<Asked> {
        match action {
            "connect" => Some(Asked::Connect),
            "disconnect" => Some(Asked::Disconnect),
            "friends" => Some(Asked::Friends),
            "invite" => user_id(user).map(Asked::Invite),
            "letIn" => user_id(user).map(Asked::LetIn),
            "dismiss" => user_id(user).map(Asked::Dismiss),
            _ => None,
        }
    }
}

fn user_id(user: Option<&str>) -> Option<u64> {
    user?.parse().ok().filter(|id| *id != 0)
}

/// What the page is told of the player's Discord account in this app.
#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize)]
pub struct Linked {
    pub connected: bool,
    /// Asking Discord, trading the code or connecting: the page's buttons
    /// say so rather than offering to start again.
    pub connecting: bool,
    /// What Discord calls them, once connected.
    pub name: Option<String>,
}

/// A friend, as the room's picker lists them.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
pub struct Friend {
    pub id: String,
    pub name: String,
    /// "online", "idle", "dnd" or "offline".
    pub status: &'static str,
    /// In Oeee Cafe now.
    pub playing: bool,
}

impl Friend {
    /// From what shim.cpp says of one: `status` is the SDK's StatusType and
    /// `group` its RelationshipGroupType.
    pub fn from_sdk(id: u64, name: &str, status: i32, group: i32) -> Friend {
        Friend {
            id: id.to_string(),
            name: name.to_string(),
            status: match status {
                0 | 6 => "online",
                3 => "idle",
                4 => "dnd",
                _ => "offline",
            },
            playing: group == 0,
        }
    }
}

/// A friend who asked to join the room, as the room lists them.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
pub struct Request {
    pub id: String,
    pub name: String,
}

/// Tells the page where the player's Discord account stands. Guarded, as
/// every call into the page is.
pub fn state_script(linked: &Linked) -> String {
    format!(
        "window.oeeeApp && window.oeeeApp.discord && window.oeeeApp.discord.state && window.oeeeApp.discord.state({});",
        quoted(linked)
    )
}

/// Hands the page the player's friends.
pub fn friends_script(friends: &[Friend]) -> String {
    format!(
        "window.oeeeApp && window.oeeeApp.discord && window.oeeeApp.discord.friends && window.oeeeApp.discord.friends({});",
        quoted(&friends)
    )
}

/// Hands the page the friends waiting to be let into the room.
pub fn requests_script(requests: &[Request]) -> String {
    format!(
        "window.oeeeApp && window.oeeeApp.discord && window.oeeeApp.discord.requests && window.oeeeApp.discord.requests({});",
        quoted(&requests)
    )
}

/// Tells the page whether an invitation reached `user`.
pub fn invited_script(user: u64, sent: bool) -> String {
    format!(
        "window.oeeeApp && window.oeeeApp.discord && window.oeeeApp.discord.invited && window.oeeeApp.discord.invited({});",
        quoted(&serde_json::json!({ "user": user.to_string(), "sent": sent }))
    )
}

use crate::store::quoted;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_page_asks_for_what_it_can_and_names_a_user_by_a_string() {
        assert_eq!(Asked::from("connect", None), Some(Asked::Connect));
        assert_eq!(Asked::from("friends", Some("x")), Some(Asked::Friends));
        assert_eq!(
            Asked::from("invite", Some("80351110224678912")),
            Some(Asked::Invite(80351110224678912))
        );
        assert_eq!(
            Asked::from("letIn", Some("80351110224678912")),
            Some(Asked::LetIn(80351110224678912))
        );
        assert_eq!(Asked::from("dismiss", Some("2")), Some(Asked::Dismiss(2)));
        for (action, user) in [
            ("letIn", None),
            ("dismiss", Some("0")),
            ("invite", None),
            ("invite", Some("")),
            ("invite", Some("0")),
            ("invite", Some("-1")),
            ("invite", Some("8.0e16")),
            ("invite", Some("99999999999999999999")),
            ("send money", None),
        ] {
            assert_eq!(Asked::from(action, user), None, "{action} {user:?}");
        }
    }

    #[test]
    fn a_friend_is_named_as_the_picker_reads_them() {
        let friend = Friend::from_sdk(80351110224678912, "넬리", 3, 0);
        assert_eq!(
            serde_json::to_value(&friend).unwrap(),
            serde_json::json!({"id": "80351110224678912", "name": "넬리", "status": "idle", "playing": true})
        );
        assert_eq!(Friend::from_sdk(1, "a", 6, 1).status, "online");
        assert_eq!(Friend::from_sdk(1, "a", 1, 2).status, "offline");
        assert!(!Friend::from_sdk(1, "a", 0, 1).playing);
    }

    #[test]
    fn the_page_is_told_only_if_it_is_listening() {
        let script = state_script(&Linked {
            connected: true,
            connecting: false,
            name: Some("넬리".into()),
        });
        assert!(script.starts_with("window.oeeeApp && window.oeeeApp.discord && window.oeeeApp.discord.state && window.oeeeApp.discord.state("));
        assert!(script.contains(r#""name":"넬리""#));
        assert!(invited_script(42, false).contains(r#"{"sent":false,"user":"42"}"#));
        let requests = requests_script(&[Request {
            id: "80351110224678912".into(),
            name: "넬리".into(),
        }]);
        assert!(requests.starts_with("window.oeeeApp && window.oeeeApp.discord && window.oeeeApp.discord.requests && window.oeeeApp.discord.requests("));
        assert!(requests.contains(r#"[{"id":"80351110224678912","name":"넬리"}]"#));
    }

    const ROOM: &str = "/collaborate/9c881320-2b43-4afa-b2bb-7128c8a3e985";

    /// The Store's package declares the scheme Discord opens the app by,
    /// which the SDK's own registration cannot reach from inside it:
    /// changing the application alone leaves the package unopenable.
    #[test]
    fn the_package_answers_the_scheme_discord_opens_the_app_by() {
        let manifest = include_str!("../msstore/AppxManifest.xml");
        assert!(manifest.contains(&format!(
            r#"<uap:Protocol Name="discord-{APPLICATION_ID}">"#
        )));
    }

    fn room_page(join: Option<&str>, party: Option<i32>) -> Page {
        Page {
            presence: Some("collaborating".into()),
            community: Some("오이카페 모에화".into()),
            group: Some("0123456789abcdef".into()),
            join: join.map(str::to_string),
            seats: Some(4),
            party,
            since: Some(OPENED),
            ..Page::default()
        }
    }

    /// 2025-10-11, when a room in these tests opened.
    const OPENED: u64 = 1760140800000;

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
                since: Some(OPENED),
            }
        );
    }

    #[test]
    fn the_clock_is_the_rooms_and_never_one_to_come() {
        let since = |since| {
            let page = Page {
                since,
                ..room_page(Some(ROOM), Some(2))
            };
            shown(Some(&page), &Words::default()).since
        };
        assert_eq!(since(Some(OPENED)), Some(OPENED));
        assert_eq!(since(None), None);
        assert_eq!(since(Some(0)), None);
        assert_eq!(since(Some(now() + 60 * 60 * 1000)), None);
        // A room a friend could not enter keeps its clock all the same.
        let closed = Page {
            join: None,
            ..room_page(None, Some(2))
        };
        assert_eq!(shown(Some(&closed), &Words::default()).since, Some(OPENED));
        // Anywhere but a room, there is none.
        let replay = Page {
            presence: Some("watching-replay".into()),
            since: Some(OPENED),
            ..Page::default()
        };
        assert_eq!(shown(Some(&replay), &Words::default()).since, None);
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
                room: None,
                since: None,
            }
        );
    }

    #[test]
    fn the_executable_is_quoted_as_the_platform_reads_a_command() {
        if cfg!(windows) {
            assert_eq!(
                quoted_exe(
                    r"C:\Program Files (x86)\Steam\steamapps\common\Oeee Cafe\oeee-cafe.exe"
                ),
                r#""C:\Program Files (x86)\Steam\steamapps\common\Oeee Cafe\oeee-cafe.exe""#
            );
        } else {
            assert_eq!(
                quoted_exe("/home/a b/oeee-cafe"),
                r#""/home/a b/oeee-cafe""#
            );
            assert_eq!(
                quoted_exe(r#"/tmp/"$(rm -rf ~)"`x`\/oeee-cafe"#),
                r#""/tmp/\"\$(rm -rf ~)\"\`x\`\\/oeee-cafe""#
            );
        }
    }

    #[test]
    fn text_is_cut_where_a_character_ends_and_too_short_is_none() {
        assert_eq!(text(&"가".repeat(50), 128), "가".repeat(42));
        assert_eq!(text("a", 128), "");
        assert_eq!(text("ab", 128), "ab");
    }
}
