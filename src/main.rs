// No console window behind the app on Windows.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

//! Oeee Cafe for Steam.
//!
//! A window onto the site, not a second copy of it: every page, account and
//! drawing is oeee.cafe's own, so the app needs no release to follow the site.
//! It opens on a page bundled with it (`loader/`), which checks the site can
//! be reached and says so in words when it cannot, rather than leaving the
//! webview's own error page on screen.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

use tauri::webview::NewWindowResponse;
use tauri::{AppHandle, Manager, RunEvent, WebviewUrl, WebviewWindowBuilder, WindowEvent};
use tauri_plugin_opener::OpenerExt;
use url::Url;

#[cfg(target_os = "macos")]
mod macos;
#[cfg(windows)]
mod webview2;
mod words;

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

/// The browser's own right-click menu -- Back, Reload, Save as, Print -- is the
/// plainest sign that a window is a browser, so it is kept to where it earns
/// its place: text fields (Cut, Copy, Paste, spelling), a selection (Copy)
/// and images (Save, Copy). Anywhere else a right click does nothing, unless
/// the page has its own use for it, as the painter does; this listens last
/// and steps aside when the page has already answered.
const QUIET_CONTEXT_MENU: &str = r#"window.addEventListener("contextmenu", function (event) {
  if (event.defaultPrevented) return;
  var target = event.target;
  if (target && target.closest) {
    if (target.closest("input, textarea, select, [contenteditable]")) return;
    if (target.closest("img")) return;
  }
  if (window.getSelection && String(window.getSelection()) !== "") return;
  event.preventDefault();
});"#;

/// What the window shows before a page has painted, matched to the site's own
/// background in each theme so a load does not flash white in dark mode.
fn background(theme: tauri::Theme) -> tauri::window::Color {
    match theme {
        tauri::Theme::Dark => tauri::window::Color(0x2c, 0x2c, 0x2c, 0xff),
        _ => tauri::window::Color(0xff, 0xff, 0xff, 0xff),
    }
}

/// Whether the page would stop a browser from leaving it: the page's own
/// `beforeunload` handlers, asked the way a browser asks them.
///
/// Closing a window unloads its page without asking it on any platform, so the
/// app asks first. Every handler runs for real, so one that did more than
/// answer -- sent a "goodbye" to a server, say -- would do it here too, though
/// the player may yet stay. The site's only handler, the painter's, just
/// answers.
const WOULD_LOSE_WORK: &str = r#"(function () {
  var event;
  try {
    event = document.createEvent("BeforeUnloadEvent");
    event.initEvent("beforeunload", false, true);
  } catch (_) {
    event = new Event("beforeunload", { cancelable: true });
  }
  window.dispatchEvent(event);
  return event.defaultPrevented ||
    (typeof event.returnValue === "string" && event.returnValue !== "");
})()"#;

/// Set while the question is on screen, so a second click on the close
/// button does not stack a second one behind it.
static ASKING: AtomicBool = AtomicBool::new(false);

/// Run `then` once the page has agreed to go, asking the player first if it
/// holds something unsaved.
pub(crate) fn after_leaving(app: &AppHandle, then: impl FnOnce(&AppHandle) + Send + 'static) {
    let Some(window) = app.get_webview_window("main") else {
        return then(app);
    };
    if ASKING.swap(true, Ordering::SeqCst) {
        return;
    }

    let then = Mutex::new(Some(then));
    let evaluated = window.eval_with_callback(WOULD_LOSE_WORK, {
        let app = app.clone();
        move |answer| {
            let Some(then) = then.lock().unwrap().take() else {
                return;
            };
            if answer != "true" {
                ASKING.store(false, Ordering::SeqCst);
                return then(&app);
            }
            let continuation = app.clone();
            ask_to_leave(&app, move |leave| {
                ASKING.store(false, Ordering::SeqCst);
                if leave {
                    then(&continuation);
                }
            });
        }
    });
    if evaluated.is_err() {
        // A page that cannot be asked cannot answer; do not keep the player.
        ASKING.store(false, Ordering::SeqCst);
        let _ = window.destroy();
    }
}

/// Ask the player whether to leave anyway, and pass `answer` their choice.
#[cfg(target_os = "macos")]
fn ask_to_leave(app: &AppHandle, answer: impl FnOnce(bool) + Send + 'static) {
    // The same alert the page's own `beforeunload` gets, so leaving by the
    // close button and leaving by a link ask in the same words.
    let _ = app.run_on_main_thread(move || answer(macos::confirm_leaving()));
}

/// Ask the player whether to leave anyway, and pass `answer` their choice.
#[cfg(not(target_os = "macos"))]
fn ask_to_leave(app: &AppHandle, answer: impl FnOnce(bool) + Send + 'static) {
    use rfd::{AsyncMessageDialog, MessageButtons, MessageDialogResult, MessageLevel};

    let window = app.get_webview_window("main");
    let _ = app.run_on_main_thread(move || {
        let words = words::words();
        let mut dialog = AsyncMessageDialog::new()
            .set_level(MessageLevel::Warning)
            .set_title(words.leave_title)
            .set_description(words.leave_body)
            // Staying is the default, so a reflexive Return keeps the work.
            .set_buttons(MessageButtons::OkCancelCustom(
                words.stay.into(),
                words.leave.into(),
            ));
        if let Some(window) = &window {
            dialog = dialog.set_parent(window);
        }
        let shown = dialog.show();
        // As tauri-plugin-dialog does it: made on the main thread, awaited
        // off it.
        std::thread::spawn(move || {
            let leave = match tauri::async_runtime::block_on(shown) {
                MessageDialogResult::Custom(label) => label == words.leave,
                // GTK answers with the slot rather than the label.
                MessageDialogResult::Cancel => true,
                _ => false,
            };
            answer(leave);
        });
    });
}

fn main() {
    let site = site();

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .setup(move |app| {
            let loader = format!(
                "index.html?site={}",
                url::form_urlencoded::byte_serialize(site.as_str().as_bytes()).collect::<String>()
            );
            let navigation = (app.handle().clone(), site.clone());
            let new_window = (app.handle().clone(), site.clone());

            let window = WebviewWindowBuilder::new(app, "main", WebviewUrl::App(loader.into()))
                .title("Oeee Cafe")
                .inner_size(1280.0, 860.0)
                .min_inner_size(800.0, 600.0)
                .initialization_script(QUIET_CONTEXT_MENU)
                // Edge's address and contact suggestions over form fields.
                .general_autofill_enabled(false)
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

            if let Ok(theme) = window.theme() {
                let _ = window.set_background_color(Some(background(theme)));
            }

            #[cfg(windows)]
            window.with_webview(|webview| webview2::quiet_the_browser(&webview))?;

            #[cfg(target_os = "macos")]
            window.with_webview(|webview| unsafe {
                macos::install_dialogs(&*webview.inner().cast::<objc2_web_kit::WKWebView>());
            })?;
            #[cfg(target_os = "macos")]
            unsafe {
                macos::guard_quit(app.handle());
            }
            Ok(())
        })
        .on_window_event(|window, event| {
            if let WindowEvent::ThemeChanged(theme) = event {
                let _ = window.set_background_color(Some(background(*theme)));
            }
            if let WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                let label = window.label().to_owned();
                after_leaving(window.app_handle(), move |app| {
                    if let Some(window) = app.get_webview_window(&label) {
                        let _ = window.destroy();
                    }
                });
            }
        })
        .build(tauri::generate_context!())
        .expect("error while building Oeee Cafe")
        .run(|app, event| {
            // Quitting (Cmd+Q, the Dock) rather than closing: no code yet
            // means the player asked for it, and the page has not been asked.
            if let RunEvent::ExitRequested {
                code: None, api, ..
            } = event
            {
                api.prevent_exit();
                after_leaving(app, |app| app.exit(0));
            }
        });
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
        assert!(stays_in_app(
            &url("http://tauri.localhost/index.html"),
            &site
        ));
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
