//! This app against the site's contract with every app
//! (frontend/shared/appContract.json in oeee-cafe/web, copied to
//! src/testdata/ by scripts/sync-app-contract.sh).
//!
//! The site tests its pages against the same file, so a change to what the
//! pages say or answer to shows up as a change to it, and here as a test
//! that fails: every message the page sends, parsed as the app parses it;
//! the user agent, marked as the site reads it; the two calls around
//! leaving, word for word; and every member of `window.oeeeApp` the app
//! calls and every command name its keys send, as the site has them.
//!
//! Nothing here is Windows-only, so it runs wherever `cargo test` does.

use std::collections::BTreeSet;
use std::path::Path;

use serde_json::Value;

use crate::bridge::{self, Message, Page, Place, Theme};
use crate::{chrome, keys, leave};

const CONTRACT: &str = include_str!("testdata/appContract.json");

fn contract() -> Value {
    serde_json::from_str(CONTRACT).expect("appContract.json is JSON")
}

fn strings(value: &Value) -> BTreeSet<String> {
    value
        .as_array()
        .expect("a list")
        .iter()
        .map(|item| item.as_str().expect("a string").to_owned())
        .collect()
}

fn text(value: &Value) -> Option<String> {
    value.as_str().map(str::to_owned)
}

/// A message as the page sends it: the object, as a string of JSON, as the
/// payload of the event bridge.js emits.
fn parse(message: &Value) -> Option<Message> {
    let sent = serde_json::to_string(&message.to_string()).unwrap();
    bridge::parse(&sent)
}

/// The types the app acts on, and what each example must come to.
fn expected(kind: &str, m: &Value) -> Option<Message> {
    Some(match kind {
        "page" => Message::Page(Page {
            signed_in: m["signedIn"].as_bool(),
            presence: text(&m["presence"]),
            community: text(&m["community"]),
            group: text(&m["group"]),
        }),
        "unread" => Message::Unread {
            count: m["count"].as_i64().expect("a count"),
        },
        "theme" => Message::Theme(Theme {
            ground: text(&m["ground"]),
        }),
        "browse" => Message::Browse {
            url: text(&m["url"]).expect("a url"),
        },
        "signIn" => Message::SignIn,
        "prices" => Message::Prices {
            products: m["products"]
                .as_array()
                .expect("products")
                .iter()
                .map(|p| p.as_str().expect("a product").to_owned())
                .collect(),
        },
        "purchase" => Message::Purchase {
            product: text(&m["product"]).expect("a product"),
        },
        "window" => Message::Window {
            action: text(&m["action"]).expect("an action"),
        },
        "caption" => Message::Caption {
            place: (!m["place"].is_null()).then(|| {
                let at = |key: &str| m["place"][key].as_i64().expect("a number") as i32;
                Place {
                    x: at("x"),
                    y: at("y"),
                    width: at("width"),
                    height: at("height"),
                }
            }),
        },
        // `words` is compared field by field, the app keeping only some.
        _ => return None,
    })
}

/// The types the page sends that are some other app's to act on.
const NOT_OURS: [&str; 6] = [
    "haptic", "pressed", "painter", "restore", "share", "download",
];

#[test]
fn every_message_the_page_sends_is_understood() {
    let contract = contract();
    let messages = contract["messages"].as_object().expect("messages");
    for (kind, examples) in messages {
        let examples = examples.as_array().expect("examples");
        assert!(!examples.is_empty(), "no example of {kind}");
        for example in examples {
            assert_eq!(example["type"], kind.as_str(), "{example}");
            let parsed = parse(example);
            if NOT_OURS.contains(&kind.as_str()) {
                assert_eq!(parsed, Some(Message::Other), "{example}");
            } else if kind == "words" {
                let Some(Message::Words(words)) = parsed else {
                    panic!("not understood: {example}");
                };
                assert_eq!(Some(words.leave_title), text(&example["leaveTitle"]));
                assert_eq!(Some(words.leave_body), text(&example["leaveBody"]));
                assert_eq!(Some(words.leave), text(&example["leave"]));
                assert_eq!(Some(words.stay), text(&example["stay"]));
                assert_eq!(Some(words.copy_link), text(&example["copyLink"]));
            } else {
                let want = expected(kind, example)
                    .unwrap_or_else(|| panic!("a type the app has not been told about: {kind}"));
                assert_eq!(parsed, Some(want), "{example}");
            }
        }
    }
    // Every type the app acts on has an example to be held to.
    for kind in [
        "page", "unread", "theme", "words", "browse", "signIn", "prices", "purchase", "window",
        "caption",
    ] {
        assert!(messages.contains_key(kind), "no example of {kind}");
    }
}

#[test]
fn the_user_agent_is_marked_as_the_site_reads_it() {
    let contract = contract();
    let cases = contract["userAgents"].as_array().expect("userAgents");
    let edge = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/140.0.0.0 Safari/537.36 Edg/140.0.0.0";
    for store in [None, Some("steam"), Some("microsoft")] {
        let named = chrome::user_agent(edge, store);
        let added = named.strip_prefix(edge).expect("added at the end");
        let marked: Vec<&Value> = cases
            .iter()
            .filter(|case| {
                case["agent"]
                    .as_str()
                    .is_some_and(|agent| agent.ends_with(added))
            })
            .collect();
        assert!(!marked.is_empty(), "no agent ends {added:?}");
        // Every agent ending as this one does is the Windows app, selling
        // through this store -- none of the site's negative cases among them.
        for case in marked {
            assert_eq!(case["app"], "windows", "{case}");
            assert_eq!(text(&case["store"]).as_deref(), store, "{case}");
        }
    }
}

#[test]
fn leaving_is_asked_in_the_contracts_words() {
    let contract = contract();
    assert_eq!(contract["scripts"]["wouldLoseWork"], leave::WOULD_LOSE_WORK);
    assert_eq!(contract["scripts"]["leaving"], leave::LEAVING);
}

const APP: &str = concat!("window.", "oeeeApp");

/// Every `window.oeeeApp.<path>` in `source`, as the path.
fn members_called(source: &str) -> Vec<String> {
    let mut found = Vec::new();
    let mut rest = source;
    while let Some(at) = rest.find(APP) {
        rest = &rest[at + APP.len()..];
        let path: String = rest
            .chars()
            .take_while(|c| c.is_ascii_alphanumeric() || *c == '_' || *c == '.')
            .collect();
        let path = path.trim_matches('.');
        if !path.is_empty() {
            found.push(path.to_owned());
        }
    }
    found
}

/// Every file of the app's source that could make a script for the page.
fn sources() -> Vec<(String, String)> {
    fn walk(dir: &Path, into: &mut Vec<(String, String)>) {
        for entry in std::fs::read_dir(dir).expect("src") {
            let path = entry.expect("an entry").path();
            if path.is_dir() {
                walk(&path, into);
            } else if path.extension().is_some_and(|e| e == "rs" || e == "js") {
                let source = std::fs::read_to_string(&path).expect("readable");
                into.push((path.display().to_string(), source));
            }
        }
    }
    let mut files = Vec::new();
    walk(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("src"),
        &mut files,
    );
    files
}

#[test]
fn every_member_the_app_calls_is_the_pages() {
    let members = strings(&contract()["members"]);
    let mut called = BTreeSet::new();
    for (file, source) in sources() {
        for path in members_called(&source) {
            // A member, or the object a member is on, as a guard asks
            // `window.oeeeApp.store` before `window.oeeeApp.store.prices`.
            let known = members
                .iter()
                .any(|member| *member == path || member.starts_with(&format!("{path}.")));
            assert!(
                known,
                "{file} calls window.oeeeApp.{path}, which is not in the contract"
            );
            called.insert(path);
        }
    }
    // The scan found what the app is known to call, so it is not passing
    // for having read nothing.
    for member in [
        "wouldLoseWork",
        "leaving",
        "command",
        "caption",
        "signIn.answer",
        "signIn.resume",
        "signIn.unopened",
        "store.prices",
        "store.purchased",
        "store.ticket",
    ] {
        assert!(called.contains(member), "found no call of {member}");
    }
}

#[test]
fn every_command_a_key_sends_is_the_pages() {
    let commands = strings(&contract()["commands"]);
    let mut sent = BTreeSet::new();
    for key in 0..=0xFF {
        for held in 0..8 {
            let held = keys::Modifiers {
                ctrl: held & 1 != 0,
                alt: held & 2 != 0,
                shift: held & 4 != 0,
            };
            if let Some(keys::Action::Command(name)) = keys::action(key, held) {
                sent.insert(name);
            }
        }
    }
    assert!(!sent.is_empty());
    for name in sent {
        assert!(
            commands.contains(name),
            "a key sends {name:?}, which the page does not know"
        );
    }
}

#[test]
fn the_scan_reads_a_path_to_its_end() {
    let source = format!("{APP} && {APP}.store && {APP}.store.prices({{}}); {APP}.caption({{");
    assert_eq!(
        members_called(&source),
        ["store", "store.prices", "caption"]
    );
}
