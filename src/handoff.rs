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
/// Only where [`may_open`] says: the page is the site's, but the window is the
/// app's, and a page is not given the run of whatever the browser will open.
pub fn browse(app: &AppHandle, site: &Url, url: &str) {
    let opened = match Url::parse(url) {
        Ok(url) if may_open(&url, site) => app
            .opener()
            .open_url(url.as_str(), None::<&str>)
            .map_err(|error| eprintln!("could not open the sign-in in the browser: {error}"))
            .is_ok(),
        _ => {
            eprintln!("a page asked for a browser somewhere it may not open: {url}");
            false
        }
    };
    if !opened {
        if let Some(window) = app.get_webview_window(WINDOW) {
            let _ = window.eval(
                "window.oeeeApp && window.oeeeApp.signIn && window.oeeeApp.signIn.unopened();",
            );
        }
    }
}

/// Google's sign-in page, which the site sends a Google sign-in straight to
/// (`handoff::AtProvider` in oeee-cafe/web) rather than through its own page
/// first, saving the browser the stop.
const GOOGLE_SIGN_IN: (&str, &str) = ("accounts.google.com", "/o/oauth2/v2/auth");

/// Whether the browser may be opened at `url`: the site's own pages, or
/// Google's sign-in page over https -- by that path exactly, and with nobody's
/// name or password in front of the host.
fn may_open(url: &Url, site: &Url) -> bool {
    if site::is_site(url, site) {
        return true;
    }
    let (host, path) = GOOGLE_SIGN_IN;
    url.scheme() == "https"
        && url.host_str() == Some(host)
        && url.port().is_none()
        && url.path() == path
        && url.username().is_empty()
        && url.password().is_none()
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
    let _ = window.eval(resume_script(None));
}

/// Tells the page to ask the site now, with the code the browser came back
/// with when it came back with one, as a JavaScript string.
fn resume_script(code: Option<&str>) -> String {
    let code = code
        .and_then(|code| serde_json::to_string(code).ok())
        .unwrap_or_default();
    format!("window.oeeeApp && window.oeeeApp.signIn && window.oeeeApp.signIn.resume({code});")
}

/// The scheme the browser is sent to once the provider has answered.
///
/// Registered so the browser can knock: the page in the window is asking the
/// site every couple of seconds anyway, but nobody should have to go looking
/// for the window. The knock, `oeee-cafe://handoff/done?code=...`, carries a
/// code the site gave the browser that finished the sign-in, which the page
/// hands in beside the id and secret it is holding ([`returned`]): the
/// sign-in goes only to the window that started it *and* heard from the
/// browser that finished it, so a handoff started anywhere else and pressed
/// on somebody to finish gets nothing. An app that took the scheme for
/// itself would learn the code, and without the window's secret could do
/// nothing with it.
#[cfg(windows)]
pub const RETURN_SCHEME: &str = "oeee-cafe";

/// Tells Windows the app answers [`RETURN_SCHEME`].
///
/// The knock itself comes back as a second launch carrying the URL, which
/// single-instance hands to the window already open and which calls
/// [`returned`] (main.rs). Nothing listens on the deep-link plugin: it only
/// hears a URL at its own start, before anything could be listening, or when
/// single-instance forwards one, which it does only with its `deep-link`
/// feature, not enabled here. On macOS the system would deliver the URL to
/// the running app instead, but only to a bundled one declaring the scheme,
/// and the developer's build there is neither.
///
/// The two Windows builds want the scheme registered differently. Steam
/// ships loose files into a depot folder and installs no package, so there
/// is nothing to declare it for us and this is what registers it. The Store
/// build is an MSIX, which takes its protocols from its manifest and only
/// from there (msstore/AppxManifest.xml); inside the package this registry
/// write is virtualised and does nothing, which is harmless.
///
/// A failure is not worth stopping for either way: all it costs is the
/// knock, and the page still asks the site every couple of seconds.
#[cfg(windows)]
pub fn register_return(app: &AppHandle) {
    use tauri_plugin_deep_link::DeepLinkExt;
    if let Err(error) = app.deep_link().register(RETURN_SCHEME) {
        eprintln!("could not register {RETURN_SCHEME}://: {error}");
    }
}

/// Brings the window forward on a second launch -- the browser knocking, or
/// anything else that opens `oeee-cafe://`, Discord's `oeee-cafe://discord`
/// among them -- and has the page ask the site at once rather than waiting
/// for its own clock: with the code, when `args`, the second launch's as
/// single-instance hands them over on Windows and Linux alike, are the
/// browser's knock.
///
/// The page is told before the window comes forward, which tells it again
/// without the code (`on_window_event`), so that the ask with the code is
/// the first.
///
/// A first launch carrying the knock -- the app closed while the browser
/// signed in, which the deep-link plugin hands over at its start -- is a new
/// window holding no handoff, with nothing to resume, and is let be.
pub fn returned(app: &AppHandle, args: &[String]) {
    let Some(window) = app.get_webview_window(WINDOW) else {
        return;
    };
    let _ = window.eval(resume_script(code_in(args).as_deref()));
    let _ = window.unminimize();
    let _ = window.show();
    let _ = window.set_focus();
}

/// The code in the browser's knock among `args`: `oeee-cafe://handoff/done`
/// with one `code`, of 64 lowercase hex digits, as the site makes them, and
/// nothing else. Anything can open the scheme with anything in it, and what
/// is not a code is not handed to the page.
fn code_in(args: &[String]) -> Option<String> {
    args.iter().find_map(|arg| {
        let url = Url::parse(arg).ok()?;
        if url.scheme() != "oeee-cafe" || url.host_str() != Some("handoff") || url.path() != "/done"
        {
            return None;
        }
        let mut codes = url.query_pairs().filter(|(name, _)| name == "code");
        let code = codes.next()?.1;
        let only = codes.next().is_none();
        let well_formed =
            code.len() == 64 && code.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'));
        (only && well_formed).then(|| code.into_owned())
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn opens(value: &str) -> bool {
        may_open(&Url::parse(value).unwrap(), &site::for_tests())
    }

    #[test]
    fn opens_the_site_and_googles_sign_in_page() {
        assert!(opens("https://oeee.cafe/auth/apple?handoff=I"));
        assert!(opens(
            "https://accounts.google.com/o/oauth2/v2/auth?client_id=C&state=S&nonce=N"
        ));
    }

    const CODE: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

    fn code(args: &[&str]) -> Option<String> {
        code_in(&args.iter().map(|a| a.to_string()).collect::<Vec<_>>())
    }

    #[test]
    fn the_browsers_knock_carries_the_code_to_the_page() {
        let knock = format!("oeee-cafe://handoff/done?code={CODE}");
        assert_eq!(
            code(&[r"C:\Games\Oeee Cafe\oeee-cafe.exe", &knock]),
            Some(CODE.into())
        );
        assert_eq!(code(&[&knock]), Some(CODE.into()));
        assert_eq!(
            resume_script(Some(CODE)),
            format!(
                r#"window.oeeeApp && window.oeeeApp.signIn && window.oeeeApp.signIn.resume("{CODE}");"#
            )
        );
        assert_eq!(
            resume_script(None),
            "window.oeeeApp && window.oeeeApp.signIn && window.oeeeApp.signIn.resume();"
        );
    }

    #[test]
    fn anything_but_a_code_in_the_knock_is_no_code() {
        let upper = CODE.to_uppercase();
        let short = &CODE[..63];
        let long = format!("{CODE}0");
        let breaking_out = format!("{}%22);alert(1);//", &CODE[..48]);
        for knock in [
            format!("oeee-cafe://handoff/done?code={upper}"),
            format!("oeee-cafe://handoff/done?code={short}"),
            format!("oeee-cafe://handoff/done?code={long}"),
            format!("oeee-cafe://handoff/done?code={breaking_out}"),
            format!("oeee-cafe://handoff/done?code={CODE}&code={CODE}"),
            format!("oeee-cafe://handoff/done/?code={CODE}"),
            format!("oeee-cafe://handoff/start?code={CODE}"),
            format!("oeee-cafe://discord?code={CODE}"),
            format!("https://handoff/done?code={CODE}"),
            format!("other://handoff/done?code={CODE}"),
            "oeee-cafe://handoff/done".to_string(),
            "oeee-cafe://handoff/done?code=".to_string(),
            "oeee-cafe://discord".to_string(),
            "--flag".to_string(),
        ] {
            assert_eq!(code(&["oeee-cafe.exe", &knock]), None, "{knock}");
        }
    }

    #[test]
    fn opens_nothing_else_of_google_or_anyone() {
        assert!(!opens(
            "http://accounts.google.com/o/oauth2/v2/auth?state=S"
        ));
        assert!(!opens("https://accounts.google.com/signin/v2/identifier"));
        assert!(!opens("https://accounts.google.com:8443/o/oauth2/v2/auth"));
        assert!(!opens(
            "https://someone:secret@accounts.google.com/o/oauth2/v2/auth"
        ));
        assert!(!opens(
            "https://accounts.google.com.example/o/oauth2/v2/auth"
        ));
        assert!(!opens("https://example.com/"));
        assert!(!opens("file:///C:/Windows/System32/calc.exe"));
    }
}
