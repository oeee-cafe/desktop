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
//! The page's half is the site's own (`window.oeeeSignIn`, app_sign_in.jinja
//! in oeee-cafe/web), shared with the other apps. The app's whole part is
//! stopping the link, opening a browser when the page asks (`browse` on the
//! bridge, bridge.rs), and saying when it could not or when the window comes
//! back. Every request is the page's, so each carries the page's cookie and
//! origin; the app never holds the session and never sees the secret that
//! claims the handoff.
//!
//! Steam's sign-in next door is the shape this follows: the link is stopped
//! and the app carries it out, rather than being followed.

use tauri::{AppHandle, Manager};
use tauri_plugin_opener::OpenerExt;
use url::Url;

use crate::{site, WINDOW};

/// The providers signed in this way, by the path the site links to.
const PROVIDERS: [(&str, &str); 2] = [("/auth/apple", "apple"), ("/auth/google", "google")];

/// Whether `url` is one of the site's links to sign in with a provider the
/// browser has to carry, and if so which, and where it goes afterwards.
pub fn sign_in_link(url: &Url, site: &Url) -> Option<(&'static str, Option<String>)> {
    if !site::is_site(url, site) {
        return None;
    }
    let (_, provider) = PROVIDERS.iter().find(|(path, _)| *path == url.path())?;
    let next = url
        .query_pairs()
        .find(|(key, _)| key == "next")
        .map(|(_, value)| value.into_owned());
    Some((provider, next))
}

/// Stops the link and hands the sign-in to the page, to carry through the
/// browser.
pub fn sign_in(app: &AppHandle, provider: &str, next: Option<String>) {
    let Some(window) = app.get_webview_window(WINDOW) else {
        return;
    };
    let next = match next {
        Some(next) => serde_json::to_string(&next).unwrap_or_else(|_| "null".to_owned()),
        None => "null".to_owned(),
    };
    let provider = serde_json::to_string(provider).unwrap_or_else(|_| "\"\"".to_owned());
    let _ = window.eval(format!(
        "window.oeeeSignIn && window.oeeeSignIn.browser({provider}, {next});"
    ));
}

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
            let _ = window.eval("window.oeeeSignIn && window.oeeeSignIn.unopened();");
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
    let _ = window.eval("window.oeeeSignIn && window.oeeeSignIn.resume();");
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
    // Windows and Linux want the scheme registered with the system, and the
    // two Windows builds want it registered differently.
    //
    // Steam ships loose files into a depot folder and installs no package, so
    // there is nothing to declare it for us and this is what registers it.
    // The Store build is an MSIX, which takes its protocols from its manifest
    // and only from there (msstore/AppxManifest.xml); inside the package this
    // registry write is virtualised and does nothing, which is harmless.
    //
    // A failure is not worth stopping for either way: all it costs is the
    // knock, and the page still asks the site every couple of seconds.
    #[cfg(any(windows, target_os = "linux"))]
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

#[cfg(test)]
mod tests {
    use super::*;

    fn site() -> Url {
        Url::parse("https://oeee.cafe/").unwrap()
    }

    #[test]
    fn the_sites_provider_links_are_the_ones_carried() {
        let apple = Url::parse("https://oeee.cafe/auth/apple").unwrap();
        assert_eq!(sign_in_link(&apple, &site()), Some(("apple", None)));

        let google = Url::parse("https://oeee.cafe/auth/google?next=%2Faccount").unwrap();
        assert_eq!(
            sign_in_link(&google, &site()),
            Some(("google", Some("/account".to_owned())))
        );
    }

    #[test]
    fn nothing_else_is() {
        for away in [
            // Another page of the site.
            "https://oeee.cafe/login",
            // Steam's, which the app signs in for itself.
            "https://oeee.cafe/auth/steam/app",
            // Where the browser comes back to, which is the browser's business.
            "https://oeee.cafe/auth/apple/callback",
            // Somebody else wearing the path.
            "https://evil.test/auth/apple",
        ] {
            let url = Url::parse(away).unwrap();
            assert_eq!(sign_in_link(&url, &site()), None, "{away}");
        }
    }
}
