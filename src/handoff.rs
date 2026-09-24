//! Signing in with Apple or Google, which this window cannot do itself.
//!
//! Google refuses its own sign-in pages inside an embedded web view, and
//! Apple ships no sheet to open on Windows. Opening either in the player's
//! browser signs them in *there*, where the cookies are not this window's, so
//! the site hands the sign-in out and takes it back (src/handoff.rs in
//! oeee-cafe/web): the page asks the site to start a handoff, the app opens
//! the browser at the URL it is given, and the page asks the site until the
//! browser has finished, at which point the site signs this window in.
//!
//! The page's half is the site's own (app_sign_in.jinja in oeee-cafe/web),
//! shared with the other apps, and it starts itself: the page takes the
//! press on the sign-in buttons, so the app never sees the link. The app's
//! whole part is opening a browser when the page asks (`browse` on the
//! bridge, bridge.rs), and saying when it could not or when the window comes
//! back. Every request is the page's, so each carries the page's cookie and
//! origin; the app never holds the session and never sees the secret that
//! claims the handoff.

use tauri::{AppHandle, Manager};
use tauri_plugin_opener::OpenerExt;
use url::Url;

use crate::{site, WINDOW};

/// Opens the browser where the page asks, once it has a handoff to carry
/// (`browse` on the bridge), or tells the page it could not.
///
/// Only at the site's own URLs: the page is the site's, but the window is the
/// app's, and a page is not given the run of whatever the browser will open.
pub fn browse(app: &AppHandle, site: &Url, url: &str) {
    let opened = match Url::parse(url) {
        Ok(url) if site::is_site(&url, site) => app
            .opener()
            .open_url(url.as_str(), None::<&str>)
            .map_err(|error| eprintln!("could not open the sign-in in the browser: {error}"))
            .is_ok(),
        _ => {
            eprintln!("a page asked for a browser somewhere that is not the site: {url}");
            false
        }
    };
    if !opened {
        if let Some(window) = app.get_webview_window(WINDOW) {
            let _ = window.eval("window.oeeeApp && window.oeeeApp.signIn && window.oeeeApp.signIn.unopened();");
        }
    }
}

/// The window coming back to the front is somebody returning from the
/// browser, most likely having just signed in: the page asks the site then
/// and there rather than waiting for the next turn of its own clock.
pub fn on_window_event<R: tauri::Runtime>(window: &tauri::Window<R>, event: &tauri::WindowEvent) {
    if !matches!(event, tauri::WindowEvent::Focused(true)) {
        return;
    }
    if let Some(window) = window.get_webview_window(WINDOW) {
        ask_now(&window);
    }
}

fn ask_now<R: tauri::Runtime>(window: &tauri::WebviewWindow<R>) {
    let _ = window.eval("window.oeeeApp && window.oeeeApp.signIn && window.oeeeApp.signIn.resume();");
}

/// The scheme the browser is sent to once the provider has answered.
///
/// Registered so the browser can knock: the page in the window is asking the
/// site every couple of seconds anyway, and would get there on its own the
/// moment somebody switched back, but nobody should have to go looking for
/// the window. It carries nothing -- the window is already holding the id
/// and the secret that claim the sign-in -- so an app that took the scheme
/// for itself would learn nothing by it.
pub const RETURN_SCHEME: &str = "oeee-cafe";

/// Hears the browser knock while the app is already running.
///
/// Where a scheme arrives as a second launch instead, single-instance hands
/// it over and calls [`returned`] the same way (main.rs); both are wired
/// because which one happens is the platform's business, not ours.
pub fn listen_for_return(app: &AppHandle) {
    use tauri_plugin_deep_link::DeepLinkExt;
    // Windows wants the scheme registered with the system, and the two
    // Windows builds want it registered differently.
    //
    // Steam ships loose files into a depot folder and installs no package, so
    // there is nothing to declare it for us and this is what registers it.
    // The Store build is an MSIX, which takes its protocols from its manifest
    // and only from there (msstore/AppxManifest.xml); inside the package this
    // registry write is virtualised and does nothing, which is harmless.
    //
    // A failure is not worth stopping for either way: all it costs is the
    // knock, and the page still asks the site every couple of seconds.
    #[cfg(windows)]
    if let Err(error) = app.deep_link().register(RETURN_SCHEME) {
        eprintln!("could not register {RETURN_SCHEME}://: {error}");
    }
    let app = app.clone();
    app.clone().deep_link().on_open_url(move |_event| {
        returned(&app);
    });
}

/// Brings the window forward when the browser knocks, and asks the site at
/// once rather than waiting for the page's own clock.
pub fn returned(app: &AppHandle) {
    let Some(window) = app.get_webview_window(WINDOW) else {
        return;
    };
    let _ = window.unminimize();
    let _ = window.show();
    let _ = window.set_focus();
    ask_now(&window);
}
