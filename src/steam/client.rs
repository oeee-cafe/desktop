//! Steam's API itself, in a build with the `steam` feature: the ticket for
//! signing in and the rich presence, handed to Steam as `super` works them
//! out.

use std::sync::mpsc::{self, Sender};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use steamworks::{AuthTicket, CallbackHandle, Client, TicketForWebApiResponse};

use super::{hex, rich_presence, PagePresence};

/// Names the service the ticket is for; the site accepts no other. Has to
/// match `TICKET_IDENTITY` in oeee-cafe/web's `src/steam.rs`.
const STEAM_TICKET_IDENTITY: &str = "oeee-cafe";

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
