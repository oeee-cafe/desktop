//! Where a link goes: the window, the player's browser, Steam, or nowhere.
//!
//! The site and the bundled loader stay in the app's window; anything else is
//! a link off the site, and goes to the browser the player already uses,
//! where it has their sign-ins and an address bar. A replay file goes
//! nowhere (downloads.rs), and the site's "Sign in with Steam" link is the
//! app's to carry out (steam.rs).

use std::sync::Arc;

use tauri::webview::NewWindowResponse;
use tauri::{AppHandle, Manager, Runtime, WebviewWindowBuilder};
use tauri_plugin_opener::OpenerExt;
use url::Url;

use crate::{downloads, handoff, site, steam, WINDOW};

/// Whether a navigation stays in the app's window.
fn stays_in_app(url: &Url, site: &Url) -> bool {
    match url.scheme() {
        // The bundled loader: tauri://localhost on Linux,
        // http://tauri.localhost on Windows.
        "tauri" | "about" | "data" | "blob" => true,
        "http" | "https" if url.host_str() == Some("tauri.localhost") => true,
        "http" | "https" => site::is_site(url, site),
        _ => false,
    }
}

/// Hand a link off the site to the player's browser.
fn open_in_browser<R: Runtime>(app: &AppHandle<R>, url: &Url) {
    if let Err(error) = app.opener().open_url(url.as_str(), None::<&str>) {
        eprintln!("could not open {url} in the browser: {error}");
    }
}

/// Decides every navigation the window makes, and every new window a page
/// asks for.
pub fn prepare<'a, M: Manager<tauri::Wry>>(
    builder: WebviewWindowBuilder<'a, tauri::Wry, M>,
    app: &AppHandle,
    site: &Url,
    steam: &Option<Arc<steam::Steam>>,
) -> WebviewWindowBuilder<'a, tauri::Wry, M> {
    let navigation = (app.clone(), site.clone(), steam.clone());
    let new_window = (app.clone(), site.clone());
    builder
        .on_navigation(move |url| {
            let (app, site, steam) = &navigation;
            // Not in the window and not in the browser, which would
            // download it.
            if downloads::is_replay(url) {
                return false;
            }
            if let (Some(steam), Some(next)) = (steam, steam::sign_in_link(url, site)) {
                steam::sign_in(app, steam.clone(), next);
                return false;
            }
            // Apple's and Google's, which neither this window nor the
            // browser can finish on its own: the sign-in goes out to the
            // browser and the answer comes back through a handoff.
            if let Some((provider, next)) = handoff::sign_in_link(url, site) {
                handoff::sign_in(app, provider, next);
                return false;
            }
            if stays_in_app(url, site) {
                return true;
            }
            open_in_browser(app, url);
            false
        })
        // `target="_blank"` and `window.open`. A second window of the site
        // would be a second app with nothing to tell it apart, so a page of
        // the site replaces the current one instead.
        .on_new_window(move |url, _features| {
            let (app, site) = &new_window;
            if downloads::is_replay(&url) {
                return NewWindowResponse::Deny;
            }
            if stays_in_app(&url, site) {
                if let Some(window) = app.get_webview_window(WINDOW) {
                    let _ = window.navigate(url);
                }
            } else {
                open_in_browser(app, &url);
            }
            NewWindowResponse::Deny
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn url(value: &str) -> Url {
        Url::parse(value).unwrap()
    }

    #[test]
    fn the_site_and_the_loader_stay_in_the_app() {
        let site = site::for_tests();
        assert!(stays_in_app(&url("https://oeee.cafe/communities"), &site));
        assert!(stays_in_app(&url("tauri://localhost/index.html"), &site));
        assert!(stays_in_app(
            &url("http://tauri.localhost/index.html"),
            &site
        ));
    }

    #[test]
    fn everything_else_goes_to_the_browser() {
        let site = site::for_tests();
        assert!(!stays_in_app(&url("https://example.com/"), &site));
        assert!(!stays_in_app(&url("http://oeee.cafe/"), &site));
        assert!(!stays_in_app(&url("https://oeee.cafe.example.com/"), &site));
        assert!(!stays_in_app(&url("mailto:someone@example.com"), &site));
    }
}
