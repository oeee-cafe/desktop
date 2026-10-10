//! Discord's Social SDK itself, in a build with the `discord` feature,
//! through the few calls shim.cpp makes of it: the activity `super` works
//! out, and a friend's join secret back.
//!
//! The SDK is had on one thread of its own, which starts it, runs its
//! callbacks and is the only one to call it; everything else sends that
//! thread what to show.

use std::ffi::{c_char, c_void};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use tauri::{AppHandle, Manager};
use url::Url;

use super::{room_path, shown, Shown, APPLICATION_ID, LAUNCH};
use crate::bridge::Page;
use crate::words;
use crate::{site, WINDOW};

#[repr(C)]
struct OeeeText {
    ptr: *const c_char,
    len: usize,
}

impl OeeeText {
    fn of(text: &str) -> Self {
        OeeeText {
            ptr: text.as_ptr().cast(),
            len: text.len(),
        }
    }
}

#[repr(C)]
struct OeeeActivity {
    details: OeeeText,
    state: OeeeText,
    start: u64,
    party: OeeeText,
    join: OeeeText,
    size: i32,
    seats: i32,
}

type Joined = extern "C" fn(context: *mut c_void, secret: *const c_char, len: usize);
type ShownBack = extern "C" fn(context: *mut c_void, shown: bool);

extern "C" {
    fn oeee_discord_start(
        app: u64,
        steam_app: u32,
        launch: OeeeText,
        context: *mut c_void,
        joined: Joined,
        shown: ShownBack,
    );
    fn oeee_discord_show(activity: *const OeeeActivity);
    fn oeee_discord_tick();
}

/// How often the SDK's callbacks are run. Nothing here is urgent.
const TICK: Duration = Duration::from_millis(100);

/// How long before an activity Discord did not take -- because it was not
/// running yet, most likely -- is offered again.
const RETRY: Duration = Duration::from_secs(15);

pub struct Discord {
    tell: Mutex<Sender<Shown>>,
    joining: Arc<Joining>,
}

/// A friend's way into a room, and where to take them.
struct Joining {
    app: AppHandle,
    site: Url,
    /// A room the window could not be sent to yet, because it was not
    /// showing the site -- or not there at all -- at a launch Discord made
    /// to join it.
    waiting: Mutex<Option<String>>,
}

/// What the SDK's thread hands back to itself through shim.cpp.
struct Context {
    joining: Arc<Joining>,
    /// Whether Discord took the last activity offered.
    taken: bool,
}

/// Starts Discord's SDK on a thread of its own. `steam_app` is the app's
/// own id when Steam started it, which Discord then launches it through.
pub fn start(app: &AppHandle, site: &Url, steam_app: Option<u32>) -> Option<Arc<Discord>> {
    let joining = Arc::new(Joining {
        app: app.clone(),
        site: site.clone(),
        waiting: Mutex::default(),
    });
    let (tell, told) = mpsc::channel();
    let context = Context {
        joining: joining.clone(),
        taken: false,
    };
    std::thread::Builder::new()
        .name("discord".into())
        .spawn(move || run(context, steam_app.unwrap_or(0), told))
        .map_err(|error| eprintln!("running without Discord: {error}"))
        .ok()?;
    Some(Arc::new(Discord {
        tell: Mutex::new(tell),
        joining,
    }))
}

fn run(context: Context, steam_app: u32, told: Receiver<Shown>) {
    // Lives as long as the thread, which is as long as the app: shim.cpp
    // holds it for the SDK's callbacks.
    let context: *mut Context = Box::into_raw(Box::new(context));
    unsafe {
        oeee_discord_start(
            APPLICATION_ID,
            steam_app,
            OeeeText::of(LAUNCH),
            context.cast(),
            joined,
            shown_back,
        );
    }

    let mut current: Option<(Shown, u64)> = None;
    let mut offered = Instant::now();
    loop {
        match told.recv_timeout(TICK) {
            // The page says it all again whenever anything about it changes,
            // its colours included, and Discord takes only a few updates a
            // minute.
            Ok(next) if current.as_ref().is_some_and(|(was, _)| *was == next) => {}
            Ok(next) => {
                // The clock starts again when what they are doing changes,
                // and not when only the count in the room does.
                let start = match &current {
                    Some((was, start)) if same_doing(was, &next) => *start,
                    _ => now(),
                };
                show(context, &next, start);
                current = Some((next, start));
                offered = Instant::now();
            }
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => return,
        }
        unsafe { oeee_discord_tick() };
        // Discord started after the app, or went and came back.
        if let Some((shown, start)) = &current {
            if !unsafe { (*context).taken } && offered.elapsed() >= RETRY {
                show(context, shown, *start);
                offered = Instant::now();
            }
        }
    }
}

fn show(context: *mut Context, shown: &Shown, start: u64) {
    let state = shown.state.as_deref().unwrap_or("");
    let room = shown.room.as_ref();
    let activity = OeeeActivity {
        details: OeeeText::of(&shown.details),
        state: OeeeText::of(state),
        start,
        party: OeeeText::of(room.map_or("", |r| r.party.as_str())),
        join: OeeeText::of(room.map_or("", |r| r.join.as_str())),
        size: room.map_or(0, |r| r.size),
        seats: room.map_or(0, |r| r.seats),
    };
    unsafe {
        // Not taken until Discord says so.
        (*context).taken = false;
        oeee_discord_show(&activity);
    }
}

fn same_doing(a: &Shown, b: &Shown) -> bool {
    a.details == b.details
        && a.state == b.state
        && a.room.as_ref().map(|r| &r.party) == b.room.as_ref().map(|r| &r.party)
}

/// Milliseconds since the epoch, as Discord counts an activity's start.
fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as u64)
}

extern "C" fn shown_back(context: *mut c_void, shown: bool) {
    let context = context.cast::<Context>();
    unsafe { (*context).taken = shown };
}

/// A friend accepted the player's invitation, or the player's own request
/// to join was let in: opens the room the secret names, and nothing else.
extern "C" fn joined(context: *mut c_void, secret: *const c_char, len: usize) {
    let context = context.cast::<Context>();
    let secret = unsafe { std::slice::from_raw_parts(secret.cast::<u8>(), len) };
    let Some(path) = std::str::from_utf8(secret).ok().and_then(room_path) else {
        eprintln!("Discord gave a join secret that is not a room");
        return;
    };
    let joining = unsafe { (*context).joining.clone() };
    joining.join(path);
}

impl Joining {
    fn join(&self, path: String) {
        let window = self.app.get_webview_window(WINDOW);
        if let Some(window) = &window {
            let _ = window.unminimize();
            let _ = window.show();
            let _ = window.set_focus();
        }
        let on_site = window
            .and_then(|window| window.url().ok())
            .is_some_and(|page| site::is_site(&page, &self.site));
        if on_site {
            self.go(&path);
        } else {
            *self.waiting.lock().unwrap() = Some(path);
        }
    }

    /// As a link in the page goes, so a drawing in progress is asked about
    /// first.
    fn go(&self, path: &str) {
        let (Some(window), Ok(url)) = (self.app.get_webview_window(WINDOW), self.site.join(path))
        else {
            return;
        };
        let url = serde_json::to_string(url.as_str()).unwrap_or_default();
        let _ = window.eval(format!("location.assign({url});"));
    }
}

impl Discord {
    /// Tells Discord what the page says the player is doing, or that they
    /// are browsing when there is no page of the site to say.
    pub fn show_presence(&self, page: Option<&Page>) {
        let _ = self.tell.lock().unwrap().send(shown(page, &words::words()));
    }

    /// A page of the site is showing and speaking: the room a friend was
    /// asked into before it was can be opened now.
    pub fn page_shown(&self) {
        let waiting = self.joining.waiting.lock().unwrap().take();
        if let Some(path) = waiting {
            self.joining.go(&path);
        }
    }
}
