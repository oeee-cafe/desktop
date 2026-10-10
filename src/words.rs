//! The few sentences the app says itself.
//!
//! What it says over a page -- the question before leaving a drawing, Copy
//! link in the right-click menu -- is in the page's
//! language, not the system's: the site sends it in a `words` message
//! (bridge.rs; the app-* messages in locales/*.ftl in oeee-cafe/web) once a
//! page, so a reader who chose Korean on the site is asked in Korean on an
//! English Windows, as the page around the dialog already speaks to them.
//! It used to follow the system language, from copies of the site's words
//! kept here in each of its languages, which a reader's choice on the site
//! never reached and a change of wording on the site never updated. English
//! stays for before the first page has said anything.

use std::sync::RwLock;

use serde::Deserialize;

/// What the app says over a page, as the site words it. Any the site leaves
/// out are the English ones.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(default, rename_all = "camelCase")]
pub struct Words {
    pub leave_title: String,
    pub leave_body: String,
    pub leave: String,
    pub stay: String,
    /// The right-click menu's item for a link (webview2.rs).
    #[cfg_attr(not(windows), allow(dead_code))]
    pub copy_link: String,
    /// What Discord shows the player doing (discord.rs): one for each
    /// activity presence_meta.jinja names, and browsing for any other page.
    #[cfg_attr(not(feature = "discord"), allow(dead_code))]
    pub presence_browsing: String,
    #[cfg_attr(not(feature = "discord"), allow(dead_code))]
    pub presence_drawing: String,
    #[cfg_attr(not(feature = "discord"), allow(dead_code))]
    pub presence_relaying: String,
    #[cfg_attr(not(feature = "discord"), allow(dead_code))]
    pub presence_collaborating: String,
    #[cfg_attr(not(feature = "discord"), allow(dead_code))]
    pub presence_drawing_banner: String,
    #[cfg_attr(not(feature = "discord"), allow(dead_code))]
    pub presence_watching_replay: String,
}

impl Default for Words {
    fn default() -> Self {
        Words {
            leave_title: "Leave this page?".into(),
            leave_body: "Anything you have not saved will be lost.".into(),
            leave: "Leave".into(),
            stay: "Stay".into(),
            copy_link: "Copy link".into(),
            presence_browsing: "Browsing Oeee Cafe".into(),
            presence_drawing: "Drawing".into(),
            presence_relaying: "Drawing a relay".into(),
            presence_collaborating: "Drawing together".into(),
            presence_drawing_banner: "Drawing a banner".into(),
            presence_watching_replay: "Watching a replay".into(),
        }
    }
}

/// The last words a page sent, or none before the first.
static HEARD: RwLock<Option<Words>> = RwLock::new(None);

/// Keeps the words a page sent, for every dialog and menu after.
pub fn heard(words: Words) {
    *HEARD
        .write()
        .unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(words);
}

/// The words to say now: the page's, or English before it has said any.
pub fn words() -> Words {
    HEARD
        .read()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .clone()
        .unwrap_or_default()
}
