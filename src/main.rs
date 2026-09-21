// No console window behind the app on Windows.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

//! Oeee Cafe for Steam.
//!
//! A window onto the site, not a second copy of it: every page, account and
//! drawing is oeee.cafe's own, so the app needs no release to follow the site.
//! It opens on a page bundled with it (`loader/`), which checks the site can
//! be reached and says so in words when it cannot, rather than leaving the
//! webview's own error page on screen.

use tauri::webview::NewWindowResponse;
use tauri::{AppHandle, Manager, WebviewUrl, WebviewWindowBuilder};
use tauri_plugin_opener::OpenerExt;
use url::Url;

const SITE: &str = "https://oeee.cafe/";

/// The site to open. `OEEE_CAFE_URL` points a development build somewhere
/// else, such as `https://oeee.test/`.
fn site() -> Url {
    std::env::var("OEEE_CAFE_URL")
        .ok()
        .and_then(|value| Url::parse(&value).ok())
        .unwrap_or_else(|| Url::parse(SITE).expect("SITE is a valid URL"))
}

/// Whether a navigation stays in the app's window.
///
/// The site and the bundled loader do; anything else is a link off the site,
/// and goes to the browser the player already uses, where it has their
/// sign-ins and an address bar.
fn stays_in_app(url: &Url, site: &Url) -> bool {
    match url.scheme() {
        // The bundled loader: tauri://localhost on macOS and Linux,
        // http://tauri.localhost on Windows.
        "tauri" | "about" | "data" | "blob" => true,
        "http" | "https" if url.host_str() == Some("tauri.localhost") => true,
        "http" | "https" => url.origin() == site.origin(),
        _ => false,
    }
}

/// Hand a link off the site to the player's browser.
fn open_in_browser(app: &AppHandle, url: &Url) {
    if let Err(error) = app.opener().open_url(url.as_str(), None::<&str>) {
        eprintln!("could not open {url} in the browser: {error}");
    }
}

fn main() {
    let site = site();

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .setup(move |app| {
            let loader = format!(
                "index.html?site={}",
                url::form_urlencoded::byte_serialize(site.as_str().as_bytes())
                    .collect::<String>()
            );
            let navigation = (app.handle().clone(), site.clone());
            let new_window = (app.handle().clone(), site.clone());

            WebviewWindowBuilder::new(app, "main", WebviewUrl::App(loader.into()))
                .title("Oeee Cafe")
                .inner_size(1280.0, 860.0)
                .min_inner_size(800.0, 600.0)
                .on_navigation(move |url| {
                    let (app, site) = &navigation;
                    if stays_in_app(url, site) {
                        return true;
                    }
                    open_in_browser(app, url);
                    false
                })
                // `target="_blank"` and `window.open`. A second window of the
                // site would be a second app with nothing to tell it apart,
                // so a page of the site replaces the current one instead.
                .on_new_window(move |url, _features| {
                    let (app, site) = &new_window;
                    if stays_in_app(&url, site) {
                        if let Some(window) = app.get_webview_window("main") {
                            let _ = window.navigate(url);
                        }
                    } else {
                        open_in_browser(app, &url);
                    }
                    NewWindowResponse::Deny
                })
                .build()?;
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running Oeee Cafe");
}

#[cfg(test)]
mod tests {
    use super::*;

    fn url(value: &str) -> Url {
        Url::parse(value).unwrap()
    }

    #[test]
    fn the_site_and_the_loader_stay_in_the_app() {
        let site = url(SITE);
        assert!(stays_in_app(&url("https://oeee.cafe/communities"), &site));
        assert!(stays_in_app(&url("tauri://localhost/index.html"), &site));
        assert!(stays_in_app(&url("http://tauri.localhost/index.html"), &site));
    }

    #[test]
    fn everything_else_goes_to_the_browser() {
        let site = url(SITE);
        assert!(!stays_in_app(&url("https://example.com/"), &site));
        assert!(!stays_in_app(&url("http://oeee.cafe/"), &site));
        assert!(!stays_in_app(&url("https://oeee.cafe.example.com/"), &site));
        assert!(!stays_in_app(&url("mailto:someone@example.com"), &site));
    }
}
