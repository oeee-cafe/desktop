//! Discord's Social SDK itself, in a build with the `discord` feature,
//! through the few calls shim.cpp makes of it: the activity `super` works
//! out, a friend's join secret back, and -- once the player has connected
//! their account (linking.rs) -- their friends and the invitations sent them.
//!
//! The SDK is had on one thread of its own, which starts it, runs its
//! callbacks and is the only one to call it; everything else sends that
//! thread what to do, and what takes time -- asking the site for tokens --
//! happens on a thread of its own that sends its answer back the same way.

use std::ffi::{c_char, c_void};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use tauri::Manager;
use url::Url;

use super::linking::{self, Refused, Tokens};
use super::{
    friends_script, invited_script, launch_command, shown, state_script, Asked, Friend, Linked,
    Shown, APPLICATION_ID,
};
use crate::bridge::Page;
use crate::joining::{room_path, Joining};
use crate::words;
use crate::WINDOW;

#[repr(C)]
#[derive(Clone, Copy)]
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

    /// The text shim.cpp handed over, for as long as the call lasts.
    unsafe fn read(&self) -> String {
        if self.ptr.is_null() || self.len == 0 {
            return String::new();
        }
        let bytes = std::slice::from_raw_parts(self.ptr.cast::<u8>(), self.len);
        String::from_utf8_lossy(bytes).into_owned()
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

#[repr(C)]
struct OeeeCallbacks {
    joined: extern "C" fn(*mut c_void, OeeeText),
    shown: extern "C" fn(*mut c_void, bool),
    authorized: extern "C" fn(*mut c_void, OeeeText, OeeeText, OeeeText),
    status: extern "C" fn(*mut c_void, i32),
    people: extern "C" fn(*mut c_void),
    invited: extern "C" fn(*mut c_void, u64, bool),
    expiring: extern "C" fn(*mut c_void),
}

type Person = extern "C" fn(data: *mut c_void, id: u64, name: OeeeText, status: i32, group: i32);

extern "C" {
    fn oeee_discord_start(
        app: u64,
        steam_app: u32,
        launch: OeeeText,
        context: *mut c_void,
        callbacks: *const OeeeCallbacks,
    );
    fn oeee_discord_show(activity: *const OeeeActivity);
    fn oeee_discord_authorize();
    fn oeee_discord_connect(access_token: OeeeText);
    fn oeee_discord_disconnect();
    fn oeee_discord_me(each: Person, data: *mut c_void);
    fn oeee_discord_friends(each: Person, data: *mut c_void);
    fn oeee_discord_invite(user: u64);
    fn oeee_discord_tick();
}

/// How often the SDK's callbacks are run. Nothing here is urgent.
const TICK: Duration = Duration::from_millis(100);

/// How long before an activity Discord did not take -- because it was not
/// running yet, most likely -- is offered again.
const RETRY: Duration = Duration::from_secs(15);

/// How long after the page last asked for friends it is still sent them as
/// they change: about as long as a picker stays open.
const FRIENDS_WANTED: Duration = Duration::from_secs(5 * 60);

/// How long a change to the friends waits for the next, so a burst of them
/// -- as there is on connecting -- is sent once.
const SETTLE: Duration = Duration::from_millis(500);

/// The SDK's Client::Status that means connected and safe to use.
const READY: i32 = 3;
/// Its Client::Status for not connected at all.
const DISCONNECTED: i32 = 0;

/// What the SDK's thread is told, by the rest of the app, by the threads
/// asking the site, and by the SDK's own callbacks.
enum Told {
    Show(Shown),
    Asked(Asked),
    /// The consent screen's code, its redirect and the verifier, or none.
    Authorized(Option<(String, String, String)>),
    Tokens(Result<Tokens, Refused>),
    Status(i32),
    People,
    Invited(u64, bool),
    Expiring,
}

pub struct Discord {
    tell: Mutex<Sender<Told>>,
    joining: Arc<Joining>,
    linked: Arc<Mutex<Linked>>,
}

/// What the SDK's thread hands back to itself through shim.cpp.
struct Context {
    joining: Arc<Joining>,
    tell: Sender<Told>,
    /// Whether Discord took the last activity offered.
    taken: bool,
}

/// Starts Discord's SDK on a thread of its own. `steam_app` is the app's
/// own id when Steam started it, which Discord then launches it through.
/// `joining` takes a friend into the room their join secret names.
pub fn start(site: &Url, steam_app: Option<u32>, joining: Arc<Joining>) -> Option<Arc<Discord>> {
    let linked = Arc::new(Mutex::new(Linked::default()));
    let (tell, told) = mpsc::channel();
    let context = Context {
        joining: joining.clone(),
        tell: tell.clone(),
        taken: false,
    };
    let (to_self, kept_linked, site_for_thread) = (tell.clone(), linked.clone(), site.clone());
    std::thread::Builder::new()
        .name("discord".into())
        .spawn(move || {
            let thread = Thread {
                context: std::ptr::null_mut(),
                tell: to_self,
                linked: kept_linked,
                site: site_for_thread,
                current: None,
                offered: Instant::now(),
                status: DISCONNECTED,
                refresh_token: None,
                letting_go: false,
                friends_wanted: None,
                people_changed: None,
            };
            thread.run(context, steam_app.unwrap_or(0), told)
        })
        .map_err(|error| eprintln!("running without Discord: {error}"))
        .ok()?;
    Some(Arc::new(Discord {
        tell: Mutex::new(tell),
        joining,
        linked,
    }))
}

/// Everything the SDK's thread keeps.
struct Thread {
    context: *mut Context,
    tell: Sender<Told>,
    /// What the page is told, which `Discord::page_shown` reads too.
    linked: Arc<Mutex<Linked>>,
    site: Url,
    current: Option<(Shown, u64)>,
    offered: Instant,
    /// The SDK's Client::Status, as it last said.
    status: i32,
    /// The account's refresh token while it is connected, kept for
    /// refreshing and for letting it go.
    refresh_token: Option<String>,
    /// The player asked to disconnect, so the SDK's going quiet is that.
    letting_go: bool,
    friends_wanted: Option<Instant>,
    people_changed: Option<Instant>,
}

impl Thread {
    fn run(mut self, context: Context, steam_app: u32, told: Receiver<Told>) {
        // Lives as long as the thread, which is as long as the app: shim.cpp
        // holds it for the SDK's callbacks.
        self.context = Box::into_raw(Box::new(context));
        let launch = launch_command();
        let callbacks = OeeeCallbacks {
            joined,
            shown: shown_back,
            authorized,
            status: status_changed,
            people,
            invited,
            expiring,
        };
        unsafe {
            oeee_discord_start(
                APPLICATION_ID,
                steam_app,
                OeeeText::of(&launch),
                self.context.cast(),
                &callbacks,
            );
        }

        // An account connected before, kept on this computer.
        if let Some(refresh_token) = linking::kept() {
            self.set(|linked| linked.connecting = true);
            self.refresh(refresh_token);
        }

        loop {
            match told.recv_timeout(TICK) {
                Ok(told) => self.heard(told),
                Err(RecvTimeoutError::Timeout) => {}
                Err(RecvTimeoutError::Disconnected) => return,
            }
            unsafe { oeee_discord_tick() };
            // Discord started after the app, or went and came back.
            if let Some((shown, start)) = &self.current {
                if !unsafe { (*self.context).taken } && self.offered.elapsed() >= RETRY {
                    let (shown, start) = (shown.clone(), *start);
                    self.show(&shown, start);
                }
            }
            if self.people_changed.is_some_and(|at| at.elapsed() >= SETTLE) {
                self.people_changed = None;
                self.send_friends();
            }
        }
    }

    fn heard(&mut self, told: Told) {
        match told {
            // The page says it all again whenever anything about it changes,
            // its colours included, and Discord takes only a few updates a
            // minute.
            Told::Show(next) if self.current.as_ref().is_some_and(|(was, _)| *was == next) => {}
            Told::Show(next) => {
                // The clock starts again when what they are doing changes,
                // and not when only the count in the room does.
                let start = match &self.current {
                    Some((was, start)) if same_doing(was, &next) => *start,
                    _ => now(),
                };
                self.show(&next, start);
                self.current = Some((next, start));
            }
            Told::Asked(Asked::Connect) => {
                let linked = self.linked.lock().unwrap().clone();
                if !linked.connected && !linked.connecting {
                    self.set(|linked| linked.connecting = true);
                    unsafe { oeee_discord_authorize() };
                }
            }
            Told::Asked(Asked::Disconnect) => self.let_go(),
            Told::Asked(Asked::Friends) => {
                self.friends_wanted = Some(Instant::now());
                self.send_friends();
            }
            Told::Asked(Asked::Invite(user)) => {
                if self.status == READY
                    && self.current.as_ref().is_some_and(|(s, _)| s.room.is_some())
                {
                    unsafe { oeee_discord_invite(user) };
                } else {
                    self.eval(&invited_script(user, false));
                }
            }
            Told::Authorized(Some((code, redirect, verifier))) => {
                let (site, tell) = (self.site.clone(), self.tell.clone());
                std::thread::spawn(move || {
                    let _ = tell.send(Told::Tokens(linking::trade(
                        &site, &code, &redirect, &verifier,
                    )));
                });
            }
            Told::Authorized(None) => self.set(|linked| linked.connecting = false),
            Told::Tokens(Ok(tokens)) => {
                linking::keep(&tokens.refresh_token);
                self.refresh_token = Some(tokens.refresh_token);
                self.letting_go = false;
                unsafe { oeee_discord_connect(OeeeText::of(&tokens.access_token)) };
            }
            Told::Tokens(Err(Refused::No)) => {
                // The account was let go from Discord's side, or the code
                // was stale: nothing kept is any use.
                linking::forget();
                self.refresh_token = None;
                self.set(|linked| *linked = Linked::default());
            }
            Told::Tokens(Err(Refused::Unreachable(error))) => {
                // Kept for the next start; for now, not connected.
                eprintln!("Discord: the site could not be asked for tokens: {error}");
                self.set(|linked| linked.connecting = false);
            }
            Told::Status(status) => self.status_changed(status),
            Told::People => self.people_changed = Some(Instant::now()),
            Told::Invited(user, sent) => self.eval(&invited_script(user, sent)),
            Told::Expiring => {
                if let Some(refresh_token) = self.refresh_token.take() {
                    self.refresh(refresh_token);
                }
            }
        }
    }

    fn status_changed(&mut self, status: i32) {
        self.status = status;
        match status {
            READY => {
                let name = me();
                self.set(|linked| {
                    *linked = Linked {
                        connected: true,
                        connecting: false,
                        name,
                    }
                });
                // Connecting clears what the Discord app beside this one was
                // showing: say it again, now by the account.
                if let Some((shown, start)) = self.current.clone() {
                    self.show(&shown, start);
                }
                self.send_friends();
            }
            // A token Discord would not take.
            -1 => {
                linking::forget();
                self.refresh_token = None;
                self.set(|linked| *linked = Linked::default());
            }
            DISCONNECTED if self.letting_go => {
                self.letting_go = false;
                self.set(|linked| *linked = Linked::default());
                // The activity goes on by the Discord app beside this one.
                if let Some((shown, start)) = self.current.clone() {
                    self.show(&shown, start);
                }
            }
            // Connecting or reconnecting, which the SDK does by itself.
            _ => {}
        }
    }

    fn let_go(&mut self) {
        linking::forget();
        if let Some(refresh_token) = self.refresh_token.take() {
            let site = self.site.clone();
            std::thread::spawn(move || linking::revoke(&site, &refresh_token));
        }
        self.friends_wanted = None;
        if self.status == DISCONNECTED {
            self.set(|linked| *linked = Linked::default());
        } else {
            self.letting_go = true;
            unsafe { oeee_discord_disconnect() };
        }
    }

    fn refresh(&self, refresh_token: String) {
        let (site, tell) = (self.site.clone(), self.tell.clone());
        std::thread::spawn(move || {
            let _ = tell.send(Told::Tokens(linking::refresh(&site, &refresh_token)));
        });
    }

    fn show(&mut self, shown: &Shown, start: u64) {
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
            (*self.context).taken = false;
            oeee_discord_show(&activity);
        }
        self.offered = Instant::now();
    }

    /// The friends, to a page that asked for them lately.
    fn send_friends(&self) {
        let wanted = self
            .friends_wanted
            .is_some_and(|at| at.elapsed() < FRIENDS_WANTED);
        if !wanted {
            return;
        }
        let friends = if self.status == READY {
            friends()
        } else {
            Vec::new()
        };
        self.eval(&friends_script(&friends));
    }

    /// Changes what the page is told, and tells it.
    fn set(&self, change: impl FnOnce(&mut Linked)) {
        let script = {
            let mut linked = self.linked.lock().unwrap();
            change(&mut linked);
            state_script(&linked)
        };
        self.eval(&script);
    }

    fn eval(&self, script: &str) {
        let joining = unsafe { &(*self.context).joining };
        if let Some(window) = joining.app().get_webview_window(WINDOW) {
            let _ = window.eval(script);
        }
    }
}

/// The player, as the connected account names them.
fn me() -> Option<String> {
    let mut found: Vec<Friend> = Vec::new();
    unsafe { oeee_discord_me(person, (&mut found as *mut Vec<Friend>).cast()) };
    found
        .pop()
        .map(|me| me.name)
        .filter(|name| !name.is_empty())
}

/// The player's Discord friends, in the SDK's order: playing, online, the
/// rest.
fn friends() -> Vec<Friend> {
    let mut found: Vec<Friend> = Vec::new();
    unsafe { oeee_discord_friends(person, (&mut found as *mut Vec<Friend>).cast()) };
    found
}

extern "C" fn person(data: *mut c_void, id: u64, name: OeeeText, status: i32, group: i32) {
    let found = unsafe { &mut *data.cast::<Vec<Friend>>() };
    let name = unsafe { name.read() };
    found.push(Friend::from_sdk(id, &name, status, group));
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

fn tell(context: *mut c_void, told: Told) {
    let context = context.cast::<Context>();
    let _ = unsafe { (*context).tell.send(told) };
}

extern "C" fn shown_back(context: *mut c_void, shown: bool) {
    let context = context.cast::<Context>();
    unsafe { (*context).taken = shown };
}

extern "C" fn authorized(
    context: *mut c_void,
    code: OeeeText,
    redirect: OeeeText,
    verifier: OeeeText,
) {
    let (code, redirect, verifier) = unsafe { (code.read(), redirect.read(), verifier.read()) };
    let answer = (!code.is_empty()).then_some((code, redirect, verifier));
    tell(context, Told::Authorized(answer));
}

extern "C" fn status_changed(context: *mut c_void, status: i32) {
    tell(context, Told::Status(status));
}

extern "C" fn people(context: *mut c_void) {
    tell(context, Told::People);
}

extern "C" fn invited(context: *mut c_void, user: u64, sent: bool) {
    tell(context, Told::Invited(user, sent));
}

extern "C" fn expiring(context: *mut c_void) {
    tell(context, Told::Expiring);
}

/// A friend accepted the player's invitation, or the player's own request
/// to join was let in: opens the room the secret names, and nothing else.
extern "C" fn joined(context: *mut c_void, secret: OeeeText) {
    let context = context.cast::<Context>();
    let secret = unsafe { secret.read() };
    let Some(path) = room_path(&secret) else {
        eprintln!("Discord gave a join secret that is not a room");
        return;
    };
    let joining = unsafe { (*context).joining.clone() };
    joining.join(path);
}

impl Discord {
    /// Tells Discord what the page says the player is doing, or that they
    /// are browsing when there is no page of the site to say.
    pub fn show_presence(&self, page: Option<&Page>) {
        let _ = self
            .tell
            .lock()
            .unwrap()
            .send(Told::Show(shown(page, &words::words())));
    }

    /// A page of the site is showing and speaking: it is told where the
    /// player's account stands. A room a friend was asked into before it was
    /// is opened by main.rs, for Steam's friends as well.
    pub fn page_shown(&self) {
        let script = state_script(&self.linked.lock().unwrap());
        if let Some(window) = self.joining.app().get_webview_window(WINDOW) {
            let _ = window.eval(&script);
        }
    }

    /// Something the page asked of the player's Discord account.
    pub fn asked(&self, asked: Asked) {
        let _ = self.tell.lock().unwrap().send(Told::Asked(asked));
    }
}
