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
use tauri::{AppHandle, Listener, Manager, WebviewUrl, WebviewWindowBuilder, WindowEvent};
use tauri_plugin_opener::OpenerExt;
use url::Url;

mod badge;
mod chrome;
mod downloads;
mod offline;
#[cfg(windows)]
mod snap;
mod steam;
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
        // The bundled loader: tauri://localhost on Linux,
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

/// What the window shows before a page has painted: the site's ground, which
/// is NEO's -- lavender, or its night blue -- so a load does not flash.
fn background(theme: tauri::Theme) -> tauri::window::Color {
    match theme {
        tauri::Theme::Dark => tauri::window::Color(0x17, 0x17, 0x2b, 0xff),
        _ => tauri::window::Color(0xcc, 0xcc, 0xff, 0xff),
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
fn after_leaving(app: &AppHandle, then: impl FnOnce(&AppHandle) + Send + 'static) {
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
            // Only the Leave button leaves. Esc and the dialog's close box
            // answer Cancel, and a player dismissing the question has not
            // agreed to lose their drawing.
            let leave = matches!(
                tauri::async_runtime::block_on(shown),
                MessageDialogResult::Custom(label) if label == words.leave
            );
            answer(leave);
        });
    });
}

/// Gets a ticket from Steam and posts it to the site from the page showing,
/// or tells the player it could not. Off the main thread: Steam can take a
/// moment to answer.
fn sign_in_with_steam(app: &AppHandle, steam: std::sync::Arc<steam::Steam>, next: Option<String>) {
    let app = app.clone();
    std::thread::spawn(move || {
        let Some(window) = app.get_webview_window("main") else {
            return;
        };
        let script = match steam.web_api_ticket() {
            Ok(ticket) => steam::post_ticket_script(&ticket, next.as_deref()),
            Err(error) => {
                eprintln!("no Steam ticket: {error}");
                steam::failed_script(words::words().steam_sign_in_failed)
            }
        };
        let _ = window.eval(script);
    });
}

/// Gets a fresh ticket and posts it to the site in the background, from the
/// page showing, so the site asks Steam again what the player owns. Off the
/// thread Steam called from: the ticket arrives on that thread.
fn refresh_steam_standing(app: &AppHandle, steam: &std::sync::Arc<steam::Steam>, site: &Url) {
    let (app, steam, site) = (app.clone(), steam.clone(), site.clone());
    std::thread::spawn(move || {
        let Some(window) = app.get_webview_window("main") else {
            return;
        };
        if !window.url().is_ok_and(|page| steam::on_the_site(&page, &site)) {
            // The loader or its "can't be reached" page: the daily recheck
            // on the site will notice instead.
            return;
        }
        match steam.web_api_ticket() {
            Ok(ticket) => {
                let _ = window.eval(steam::refresh_script(&ticket));
            }
            Err(error) => eprintln!("no Steam ticket to refresh with: {error}"),
        }
    });
}

fn main() {
    // Before any window, as Steam asks, so its overlay can find them.
    let steam = steam::start();
    let site = site();

    tauri::Builder::default()
        // First, as the plugin asks: a second launch hands over here and
        // exits, and the window already open comes forward.
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.unminimize();
                let _ = window.show();
                let _ = window.set_focus();
            }
        }))
        // Where the window was, how big, and whether it was maximised or full
        // screen -- and not its decorations or visibility, which are the
        // app's to set on each platform.
        .plugin(
            tauri_plugin_window_state::Builder::default()
                .with_state_flags(
                    tauri_plugin_window_state::StateFlags::SIZE
                        | tauri_plugin_window_state::StateFlags::POSITION
                        | tauri_plugin_window_state::StateFlags::MAXIMIZED
                        | tauri_plugin_window_state::StateFlags::FULLSCREEN,
                )
                .build(),
        )
        .plugin(tauri_plugin_opener::init())
        .setup(move |app| {
            let loader = format!(
                "index.html?site={}",
                url::form_urlencoded::byte_serialize(site.as_str().as_bytes()).collect::<String>()
            );
            // The site may drag and maximise the window from its toolbar, and
            // do nothing else with it.
            app.add_capability(
                tauri::ipc::CapabilityBuilder::new("site")
                    .remote(chrome::site_pattern(&site))
                    .window("main")
                    .permission("core:window:allow-start-dragging")
                    .permission("core:window:allow-internal-toggle-maximize")
                    // The Windows caption buttons in the toolbar (caption.js).
                    .permission("core:window:allow-minimize")
                    .permission("core:window:allow-toggle-maximize")
                    .permission("core:window:allow-is-maximized")
                    .permission("core:window:allow-close")
                    // The unread count for the app's icon (badge.rs).
                    .permission("core:event:allow-emit"),
            )?;

            let navigation = (app.handle().clone(), site.clone(), steam.clone());
            let new_window = (app.handle().clone(), site.clone());
            let page_load = steam.clone();

            let builder = WebviewWindowBuilder::new(app, "main", WebviewUrl::App(loader.clone().into()))
                .title("Oeee Cafe")
                .inner_size(1280.0, 860.0)
                .min_inner_size(800.0, 600.0)
                .initialization_script(QUIET_CONTEXT_MENU)
                .initialization_script(chrome::script(std::env::consts::OS))
                .initialization_script(offline::page_script());
            let builder = match steam {
                Some(_) => builder.initialization_script(steam::MARK_PAGE),
                None => builder,
            };
            // On Windows the title bar goes altogether: the toolbar is the
            // title bar, and draws the window's controls at its right end
            // (caption.js). The window keeps its shadow, and with it the
            // system's resize edges.
            #[cfg(windows)]
            let builder = builder.decorations(false).shadow(true);
            let window = builder
                // Edge's address and contact suggestions over form fields.
                .general_autofill_enabled(false)
                // What the player is doing, for their Steam friends: read
                // off each page as it finishes loading (steam.rs).
                .on_page_load(move |window, payload| {
                    let Some(steam) = page_load.clone() else {
                        return;
                    };
                    if payload.event() != tauri::webview::PageLoadEvent::Finished {
                        return;
                    }
                    let _ = window.eval_with_callback(steam::READ_PRESENCE, move |answer| {
                        steam.show_presence(&answer);
                    });
                })
                // Saved where the player says, and never a replay file
                // (downloads.rs).
                .on_download(|webview, event| downloads::handle(&webview, event))
                .on_navigation(move |url| {
                    let (app, site, steam) = &navigation;
                    // Not in the window and not in the browser, which would
                    // download it.
                    if downloads::is_replay(url) {
                        return false;
                    }
                    if let (Some(steam), Some(next)) = (steam, steam::sign_in_link(url, site)) {
                        sign_in_with_steam(app, steam.clone(), next);
                        return false;
                    }
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
                    if downloads::is_replay(&url) {
                        return NewWindowResponse::Deny;
                    }
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

            // A DLC bought while the app is open -- the Supporter Pack -- is
            // told to the site at once, rather than at its daily recheck.
            if let Some(steam) = &steam {
                let (app, site) = (app.handle().clone(), site.clone());
                let weak = std::sync::Arc::downgrade(steam);
                steam.on_dlc_installed(move |_app_id| {
                    if let Some(steam) = weak.upgrade() {
                        refresh_steam_standing(&app, &steam, &site);
                    }
                });
            }

            // The site's unread count, on the app's icon.
            let badge_window = window.clone();
            app.listen_any(badge::EVENT, move |event| {
                badge::show(&badge_window, badge::parse(event.payload()));
            });

            if let Ok(theme) = window.theme() {
                let _ = window.set_background_color(Some(background(theme)));
            }

            #[cfg(windows)]
            window.with_webview(|webview| webview2::quiet_the_browser(&webview))?;

            // A page of the site that could not be reached goes back to the
            // loader, which says so and tries it again (offline.rs).
            #[cfg(windows)]
            {
                // Where WebView2 serves the bundled loader from; the window
                // has not loaded it yet, so it cannot be asked.
                let loader = Url::parse("http://tauri.localhost/")
                    .and_then(|origin| origin.join(&loader))
                    .expect("the loader's address is a valid URL");
                let (unreachable_window, site) = (window.clone(), site.clone());
                window.with_webview(move |webview| {
                    webview2::on_unreachable(&webview, move |page| {
                        let Ok(page) = Url::parse(&page) else {
                            return;
                        };
                        if page.origin() == site.origin() {
                            let _ = unreachable_window.navigate(offline::loader_for(&loader, &page));
                        }
                    });
                })?;
            }

            // Snap Layouts on the toolbar's maximise button (snap.rs), kept
            // over the button wherever the page says it is.
            #[cfg(windows)]
            {
                snap::attach(&window)?;
                let handle = app.handle().clone();
                app.listen_any(snap::EVENT, move |event| {
                    let place = snap::parse(event.payload());
                    let _ = handle.run_on_main_thread(move || snap::place(place));
                });
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
