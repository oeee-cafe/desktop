//! The Microsoft Store, in its build: the Supporter Pack sold as a durable
//! add-on, the way every app's page sells (app_store.jinja in oeee-cafe/web).
//!
//! The Store's build names the Microsoft Store in its user agent,
//! ` store/microsoft` (chrome.rs), when it is running as the Store's package
//! -- which is when the Store can sell anything to it -- and the site marks
//! the root `data-store="microsoft"` from that before the page paints. Run
//! unpackaged, from `cargo run --no-default-features`, it names no store and
//! the page offers nothing, as the Steam build does without Steam.
//!
//! The page names each add-on by its Store ID (Partner Center, the add-on's
//! Product identity). `prices` is answered with each one's price as the Store
//! formats it for the player. `purchase` shows the Store's own purchase
//! dialog over the window; once the player owns the add-on -- just now, or
//! already -- the app hands the page proof of it, and the proof is where the
//! site has to take part. The Store says who owns what only to a server that
//! can name the customer, by a Microsoft Store ID key, and a key is made on
//! the player's machine from a ticket the site's server made:
//!
//!   1. the app asks the page for `oeeeApp.store.ticket()`, which asks the
//!      site and resolves to `{ticket, user}` -- an Azure AD access token for
//!      the Store's collections service, and the site's own id for the
//!      player -- or to null when it cannot give one;
//!   2. the app hands those to the Store (`GetCustomerPurchaseIdAsync`),
//!      which answers with a key;
//!   3. the app hands the key to the page with `oeeeApp.store.purchased`,
//!      and the site asks the Store's collections service what that
//!      customer owns.
//!
//! Tauri's `eval` cannot bring a value back out of the page, let alone wait
//! for a Promise, so step 1 is a script the app evaluates that awaits the
//! Promise itself and emits the answer as an `oeee-store-ticket` event,
//! carrying the number the app asked with (`ticket_script`). It is the
//! app's own event, not a message of the site's bridge, so the site's
//! contract gains only `ticket()`; the site's pages may emit events at all
//! only by `core:event:allow-emit` (main.rs), which the bridge needs
//! already. An answer the app is not waiting for is dropped.

// Outside the Microsoft Store's build -- on the host, and in the Steam
// build -- what reads answers and writes scripts is still built and
// tested, and nothing calls it.
#![cfg_attr(not(all(windows, not(feature = "steam"))), allow(dead_code))]

use std::sync::Arc;
use std::time::Duration;

use serde::Deserialize;

use crate::store::quoted;

#[cfg(all(windows, not(feature = "steam")))]
mod client;
#[cfg(all(windows, not(feature = "steam")))]
pub use client::{start, Microsoft};

/// The Microsoft Store, anywhere but its own build: there is never one.
#[cfg(not(all(windows, not(feature = "steam"))))]
pub enum Microsoft {}

#[cfg(not(all(windows, not(feature = "steam"))))]
impl Microsoft {
    pub fn answer_prices(self: &Arc<Self>, _products: Vec<String>) {
        match **self {}
    }

    pub fn sell(self: &Arc<Self>, _product: String) {
        match **self {}
    }
}

#[cfg(not(all(windows, not(feature = "steam"))))]
pub fn start(_window: &tauri::WebviewWindow) -> Option<Arc<Microsoft>> {
    None
}

/// The store to name in the user agent (chrome.rs): the Microsoft Store,
/// when this is its package and so it can sell.
#[cfg_attr(not(windows), allow(dead_code))]
pub fn store(microsoft: &Option<Arc<Microsoft>>) -> Option<&'static str> {
    microsoft.as_ref().map(|_| "microsoft")
}

/// The event `ticket_script` answers in.
pub const TICKET_EVENT: &str = "oeee-store-ticket";

/// How long the page gets to give a ticket. It asks the site for one, so
/// it answers as quickly as a page loads; longer than this, and the site is
/// not answering.
const TICKET_TIMEOUT: Duration = Duration::from_secs(30);

/// What `oeeeApp.store.ticket()` resolves to: a token the site made for the
/// Store's collections service, and the site's own name for the player,
/// which the Store writes into the key so the site can tell it was made for
/// them.
#[derive(Debug, Deserialize, PartialEq, Eq)]
pub struct Ticket {
    pub ticket: String,
    pub user: String,
}

#[derive(Deserialize)]
struct TicketReply {
    id: u64,
    #[serde(default)]
    answer: serde_json::Value,
}

/// Asks the page for a ticket and emits whatever it answers, as
/// `{id, answer}`, in `TICKET_EVENT`: the ticket, or null when the page has
/// no `ticket()` -- an older page, or the loader -- or it failed. The call
/// goes by `__TAURI_INTERNALS__`, as bridge.js's does, because that is the
/// only way out of the page.
fn ticket_script(id: u64) -> String {
    format!(
        r#"(function () {{
  var reply = function (answer) {{
    if (!window.__TAURI_INTERNALS__) return;
    window.__TAURI_INTERNALS__
      .invoke("plugin:event|emit", {{ event: {event}, payload: {{ id: {id}, answer: answer || null }} }})
      .catch(function () {{}});
  }};
  var store = window.oeeeApp && window.oeeeApp.store;
  if (!store || typeof store.ticket !== "function") return reply(null);
  Promise.resolve()
    .then(function () {{ return store.ticket(); }})
    .then(reply, function () {{ reply(null); }});
}})();"#,
        event = quoted(&TICKET_EVENT),
    )
}

/// The request an answer is for, and the ticket in it, if the page gave one
/// that is whole: an answer the page got wrong is no ticket, rather than no
/// answer, so the app is not left waiting for one that will not come.
fn ticket_reply(payload: &str) -> Option<(u64, Option<Ticket>)> {
    let TicketReply { id, answer } = serde_json::from_str(payload).ok()?;
    let ticket = serde_json::from_value::<Ticket>(answer)
        .ok()
        .filter(|t| !t.ticket.is_empty() && !t.user.is_empty());
    Some((id, ticket))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_ticket_is_asked_for_and_answered_in_the_apps_own_event() {
        let script = ticket_script(7);
        assert!(script.contains(r#"event: "oeee-store-ticket", payload: { id: 7, answer: answer || null }"#));
        assert!(script.contains("store.ticket()"));
        assert!(script.contains(r#"typeof store.ticket !== "function") return reply(null)"#));
    }

    #[test]
    fn a_ticket_is_a_token_and_a_user() {
        assert_eq!(
            ticket_reply(r#"{"id":7,"answer":{"ticket":"eyJ0","user":"0f3c","expires":3600}}"#),
            Some((
                7,
                Some(Ticket {
                    ticket: "eyJ0".into(),
                    user: "0f3c".into()
                })
            ))
        );
    }

    #[test]
    fn no_ticket_or_half_of_one_is_none_but_still_an_answer() {
        for payload in [
            r#"{"id":7,"answer":null}"#,
            r#"{"id":7}"#,
            r#"{"id":7,"answer":{"ticket":"eyJ0"}}"#,
            r#"{"id":7,"answer":{"ticket":"","user":"0f3c"}}"#,
            r#"{"id":7,"answer":"eyJ0"}"#,
        ] {
            assert_eq!(ticket_reply(payload), Some((7, None)), "{payload}");
        }
        assert_eq!(ticket_reply(r#"{"answer":null}"#), None);
        assert_eq!(ticket_reply("not json"), None);
    }
}
