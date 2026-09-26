//! Steam's API itself, in a build with the `steam` feature: the ticket the
//! page asks for, the rich presence, handed to Steam as `super` works them out,
//! the overlay's store page for a DLC, word of one installed, and the
//! player's country for its price.

use std::sync::mpsc::{self, Sender};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use std::ffi::c_void;

use steamworks::{
    sys, AppId, AuthTicket, Callback, CallbackHandle, Client, OverlayToStoreFlag,
    TicketForWebApiResponse,
};

use super::{hex, rich_presence};
use crate::bridge::Page;

/// Names the service the ticket is for; the site accepts no other. Has to
/// match `TICKET_IDENTITY` in oeee-cafe/web's `src/steam.rs`.
const STEAM_TICKET_IDENTITY: &str = "oeee-cafe";

/// How long Steam gets to hand over a ticket. It usually answers within a
/// second; a player waiting longer than this is better told it failed.
const TICKET_TIMEOUT: Duration = Duration::from_secs(15);

type Waiting = Arc<Mutex<Vec<(AuthTicket, Sender<Result<Vec<u8>, String>>)>>>;
type DlcListener = Arc<Mutex<Option<Box<dyn Fn(u32) + Send>>>>;

/// `DlcInstalled_t`, which the crate does not wrap: the player has come to
/// own a DLC and it is installed. The Supporter Pack has no content, so it
/// is installed the moment it is owned.
struct DlcInstalled {
    app_id: u32,
}

unsafe impl Callback for DlcInstalled {
    const ID: i32 = sys::DlcInstalled_t_k_iCallback as _;

    unsafe fn from_raw(raw: *mut c_void) -> Self {
        let raw = raw.cast::<sys::DlcInstalled_t>().read_unaligned();
        DlcInstalled {
            app_id: raw.m_nAppID,
        }
    }
}

pub struct Steam {
    client: Client,
    waiting: Waiting,
    dlc_listener: DlcListener,
    _ticket_callback: CallbackHandle,
    _dlc_callback: CallbackHandle,
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
                let Some(at) = waiting
                    .iter()
                    .position(|(h, _)| *h == response.ticket_handle)
                else {
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

    let dlc_listener: DlcListener = Arc::default();
    let dlc_callback = client.register_callback({
        let dlc_listener = dlc_listener.clone();
        move |installed: DlcInstalled| {
            if let Some(then) = dlc_listener.lock().unwrap().as_ref() {
                then(installed.app_id);
            }
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
        dlc_listener,
        _ticket_callback: ticket_callback,
        _dlc_callback: dlc_callback,
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
    /// Tells Steam what the page says the player is doing, or that they are
    /// browsing when there is no page of the site to say.
    pub fn show_presence(&self, page: Option<&Page>) {
        let friends = self.client.friends();
        for (key, value) in rich_presence(page) {
            friends.set_rich_presence(key, value.as_deref());
        }
    }
}

impl Steam {
    /// Calls `then` with a DLC's app id whenever Steam says one has been
    /// installed. On the thread that pumps Steam's callbacks, so `then` must
    /// not wait on Steam itself -- a ticket, say -- without moving off it.
    pub fn on_dlc_installed(&self, then: impl Fn(u32) + Send + 'static) {
        *self.dlc_listener.lock().unwrap() = Some(Box::new(then));
    }
}

impl Steam {
    /// The player's country as two letters, from where Steam sees them
    /// connect, for the currency the store's prices are asked in.
    pub fn country(&self) -> String {
        self.client.utils().ip_country()
    }
}

impl Steam {
    /// Shows a DLC's store page in the overlay. Steam does the selling there;
    /// the app hears only that the DLC was installed, if it was.
    pub fn show_store(&self, app_id: u32) {
        self.client
            .friends()
            .activate_game_overlay_to_store(AppId(app_id), OverlayToStoreFlag::None);
    }
}
