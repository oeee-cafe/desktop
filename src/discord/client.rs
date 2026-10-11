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
    authorized: extern "C" fn(*mut c_void, u64, OeeeText, OeeeText, OeeeText),
    token: extern "C" fn(*mut c_void, u64, bool),
    status: extern "C" fn(*mut c_void, i32, i32, i32),
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
    fn oeee_discord_authorize(generation: u64);
    fn oeee_discord_update_token(generation: u64, access_token: OeeeText);
    fn oeee_discord_connect();
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

/// How long before an activity Discord did take is offered again all the
/// same, while it is the Discord app beside this one showing it rather than
/// the player's connected account. Nothing says when that app is quit and
/// started again, which forgets the activity, so the app asks now and then:
/// taken, nothing changes; not, it is offered every [`RETRY`] until it is.
const REOFFER: Duration = Duration::from_secs(2 * 60);

/// How long connecting may take -- Discord's consent screen open, the site
/// trading the code, the SDK connecting -- before the page is told it is
/// not happening. A consent screen closed without an answer is never
/// answered at all.
const CONNECTING: Duration = Duration::from_secs(5 * 60);

/// How long after the site could not be asked for tokens it is asked again,
/// doubling each time it still cannot be, up to [`RETRY_TOKENS_MOST`].
const RETRY_TOKENS: Duration = Duration::from_secs(30);
const RETRY_TOKENS_MOST: Duration = Duration::from_secs(60 * 60);

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

/// The gateway's close code for an access token it would not take: run out,
/// most likely, while the computer slept through the SDK's warning.
const AUTHENTICATION_FAILED: i32 = 4004;

/// What the SDK's thread is told, by the rest of the app, by the threads
/// asking the site, and by the SDK's own callbacks.
///
/// What answers something the thread asked carries the generation it was
/// asked in ([`Thread::generation`]), and is only listened to in the same
/// one.
enum Told {
    Show(Shown),
    Asked(Asked),
    /// The consent screen's code, its redirect and the verifier, or none.
    Authorized(u64, Option<(String, String, String)>),
    Tokens(u64, Result<Tokens, Refused>),
    /// Whether the SDK took the access token it was given.
    TokenTaken(u64, bool),
    /// The SDK's Client::Status, and when it ended, its Client::Error and
    /// the gateway's close code.
    Status(i32, i32, i32),
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
                generation: 0,
                asking: false,
                ask_again: None,
                ask_again_after: RETRY_TOKENS,
                refreshed_for_failure: false,
                connecting_since: None,
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
    /// The account's refresh token while it is connected, or kept and not
    /// yet traded, kept for refreshing and for letting it go.
    refresh_token: Option<String>,
    /// Counted up whenever the player starts connecting or disconnects, so
    /// that an answer to something asked before -- tokens the site took its
    /// time over, a consent screen left open -- is not taken for one to
    /// now, and does not connect an account the player let go.
    generation: u64,
    /// The site is being asked for tokens. A refresh token works once, and
    /// asking with it twice at once would have the second ask refused and
    /// the account forgotten for it.
    asking: bool,
    /// When to ask the site again for the tokens it could not be asked for,
    /// and how long to wait the time after that.
    ask_again: Option<Instant>,
    ask_again_after: Duration,
    /// The gateway turned the access token away, and a new one was asked
    /// for: once, until the account is connected again.
    refreshed_for_failure: bool,
    /// Since when the page has been told the account is connecting.
    connecting_since: Option<Instant>,
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
            token: token_taken,
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
        self.refresh_token = linking::kept(&self.site);
        self.refresh();

        loop {
            match told.recv_timeout(TICK) {
                Ok(told) => self.heard(told),
                Err(RecvTimeoutError::Timeout) => {}
                Err(RecvTimeoutError::Disconnected) => return,
            }
            unsafe { oeee_discord_tick() };
            // Discord started after the app, or went and came back.
            if let Some((shown, start)) = &self.current {
                let due = if unsafe { (*self.context).taken } {
                    self.status != READY && self.offered.elapsed() >= REOFFER
                } else {
                    self.offered.elapsed() >= RETRY
                };
                if due {
                    let (shown, start) = (shown.clone(), *start);
                    self.show(&shown, start);
                }
            }
            if self.people_changed.is_some_and(|at| at.elapsed() >= SETTLE) {
                self.people_changed = None;
                self.send_friends();
            }
            if self.ask_again.is_some_and(|at| Instant::now() >= at) {
                self.ask_again = None;
                self.refresh();
            }
            if self
                .connecting_since
                .is_some_and(|at| at.elapsed() >= CONNECTING)
            {
                self.stop_connecting();
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
                    self.generation += 1;
                    self.ask_again = None;
                    if self.refresh_token.is_some() {
                        // Kept, and the site could not be asked, or the
                        // gateway went away: no need for the consent
                        // screen again.
                        self.refresh();
                    } else {
                        self.set(|linked| linked.connecting = true);
                        unsafe { oeee_discord_authorize(self.generation) };
                    }
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
            // The player disconnected, or gave up waiting, while the consent
            // screen was open: the code is never traded.
            Told::Authorized(generation, _) if generation != self.generation => {}
            Told::Authorized(_, Some((code, redirect, verifier))) => {
                let (site, tell, generation) =
                    (self.site.clone(), self.tell.clone(), self.generation);
                self.asking = true;
                std::thread::spawn(move || {
                    let tokens = linking::trade(&site, &code, &redirect, &verifier);
                    let _ = tell.send(Told::Tokens(generation, tokens));
                });
            }
            Told::Authorized(_, None) => self.set(|linked| linked.connecting = false),
            // Asked for before the player disconnected: the account is let
            // go, these tokens with it, rather than left with Discord
            // among the player's authorized apps and nowhere here.
            Told::Tokens(generation, tokens) if generation != self.generation => {
                if let Ok(Tokens { refresh_token, .. }) = tokens {
                    self.revoke(refresh_token);
                }
            }
            Told::Tokens(_, Ok(tokens)) => {
                self.asking = false;
                self.ask_again_after = RETRY_TOKENS;
                // The refresh token traded for these works no more.
                linking::keep(&self.site, &tokens.refresh_token);
                self.refresh_token = Some(tokens.refresh_token).filter(|t| !t.is_empty());
                self.letting_go = false;
                unsafe {
                    oeee_discord_update_token(self.generation, OeeeText::of(&tokens.access_token))
                };
            }
            Told::Tokens(_, Err(Refused::No)) => {
                // The account was let go from Discord's side, or the code
                // was stale: nothing kept is any use.
                self.asking = false;
                self.forget_account();
            }
            Told::Tokens(_, Err(Refused::Unreachable(error))) => {
                // Kept, and asked for again in a while; for now, not
                // connected, unless it still is on the token it has.
                eprintln!("Discord: the site could not be asked for tokens: {error}");
                self.asking = false;
                if self.refresh_token.is_some() {
                    self.ask_again = Some(Instant::now() + self.ask_again_after);
                    self.ask_again_after = (self.ask_again_after * 2).min(RETRY_TOKENS_MOST);
                }
                self.set(|linked| linked.connecting = false);
            }
            Told::TokenTaken(generation, _) if generation != self.generation => {}
            // Connected already, the SDK goes on with the new token; else
            // it connects by it.
            Told::TokenTaken(_, true) => {
                if self.status == DISCONNECTED {
                    unsafe { oeee_discord_connect() };
                }
            }
            Told::TokenTaken(_, false) => self.forget_account(),
            Told::Status(status, error, detail) => self.status_changed(status, error, detail),
            Told::People => self.people_changed = Some(Instant::now()),
            Told::Invited(user, sent) => self.eval(&invited_script(user, sent)),
            // Kept until the new one comes, so that the account can still
            // be let go, or asked for again, should the site not answer.
            Told::Expiring => self.refresh(),
        }
    }

    fn status_changed(&mut self, status: i32, error: i32, detail: i32) {
        self.status = status;
        match status {
            READY => {
                self.refreshed_for_failure = false;
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
            DISCONNECTED if self.letting_go => {
                self.letting_go = false;
                self.set(|linked| *linked = Linked::default());
            }
            // The gateway turned the access token away: a new one, once,
            // rather than the player having to connect again.
            DISCONNECTED
                if detail == AUTHENTICATION_FAILED
                    && !self.refreshed_for_failure
                    && self.refresh_token.is_some() =>
            {
                self.refreshed_for_failure = true;
                self.set(|linked| linked.connected = false);
                self.refresh();
            }
            // Not connected, whatever the page was told before -- the
            // connection failed or was dropped, which shim.cpp says why of
            // -- and nothing on its way unless the site is being asked.
            DISCONNECTED => {
                if error != 0 {
                    eprintln!("Discord: disconnected ({error}, {detail})");
                }
                let asking = self.asking;
                self.set(|linked| {
                    linked.connected = false;
                    linked.connecting = asking;
                });
            }
            // Connecting or reconnecting, which the SDK does by itself.
            _ => {}
        }
        // The activity goes on by the Discord app beside this one, which
        // may not have it: it was the account's.
        if status == DISCONNECTED {
            if let Some((shown, start)) = self.current.clone() {
                self.show(&shown, start);
            }
        }
    }

    fn let_go(&mut self) {
        self.generation += 1;
        self.asking = false;
        self.ask_again = None;
        self.ask_again_after = RETRY_TOKENS;
        linking::forget(&self.site);
        if let Some(refresh_token) = self.refresh_token.take() {
            self.revoke(refresh_token);
        }
        self.friends_wanted = None;
        if self.status == DISCONNECTED {
            self.set(|linked| *linked = Linked::default());
        } else {
            self.letting_go = true;
            unsafe { oeee_discord_disconnect() };
        }
    }

    /// The account is no good -- Discord said no to its refresh token, or
    /// the SDK would not take its access token -- and is forgotten, and let
    /// go of if it is connected yet.
    fn forget_account(&mut self) {
        linking::forget(&self.site);
        self.refresh_token = None;
        self.ask_again = None;
        if self.status == DISCONNECTED {
            self.set(|linked| *linked = Linked::default());
        } else {
            self.letting_go = true;
            unsafe { oeee_discord_disconnect() };
        }
    }

    /// Hands `refresh_token` back to Discord, on a thread of its own.
    fn revoke(&self, refresh_token: String) {
        if refresh_token.is_empty() {
            return;
        }
        let site = self.site.clone();
        std::thread::spawn(move || linking::revoke(&site, &refresh_token));
    }

    /// Asks the site for new tokens for the refresh token kept, unless it
    /// is being asked already: the account's page says it is connecting,
    /// unless it is connected on the token it has.
    fn refresh(&mut self) {
        let Some(refresh_token) = self.refresh_token.clone() else {
            return;
        };
        if self.asking {
            return;
        }
        self.asking = true;
        self.ask_again = None;
        if !self.linked.lock().unwrap().connected {
            self.set(|linked| linked.connecting = true);
        }
        let (site, tell, generation) = (self.site.clone(), self.tell.clone(), self.generation);
        std::thread::spawn(move || {
            let tokens = linking::refresh(&site, &refresh_token);
            let _ = tell.send(Told::Tokens(generation, tokens));
        });
    }

    /// Connecting took too long: whatever is still on its way is no longer
    /// waited for, and the page offers to connect again.
    fn stop_connecting(&mut self) {
        eprintln!("Discord: connecting took too long");
        self.generation += 1;
        self.asking = false;
        if self.status != DISCONNECTED && self.status != READY {
            unsafe { oeee_discord_disconnect() };
        }
        self.set(|linked| linked.connecting = false);
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
    fn set(&mut self, change: impl FnOnce(&mut Linked)) {
        let (script, connecting) = {
            let mut linked = self.linked.lock().unwrap();
            change(&mut linked);
            (state_script(&linked), linked.connecting)
        };
        self.connecting_since = match self.connecting_since {
            Some(since) if connecting => Some(since),
            None if connecting => Some(Instant::now()),
            _ => None,
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
    generation: u64,
    code: OeeeText,
    redirect: OeeeText,
    verifier: OeeeText,
) {
    let (code, redirect, verifier) = unsafe { (code.read(), redirect.read(), verifier.read()) };
    let answer = (!code.is_empty()).then_some((code, redirect, verifier));
    tell(context, Told::Authorized(generation, answer));
}

extern "C" fn token_taken(context: *mut c_void, generation: u64, taken: bool) {
    tell(context, Told::TokenTaken(generation, taken));
}

extern "C" fn status_changed(context: *mut c_void, status: i32, error: i32, detail: i32) {
    tell(context, Told::Status(status, error, detail));
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
