// No console window behind the app on Windows.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

//! Oeee Cafe for the desktop, on Steam and the Microsoft Store.
//!
//! A window onto the site, not a second copy of it: every page, account and
//! drawing is oeee.cafe's own, so the app needs no release to follow the site.
//! It opens on a page bundled with it (`loader/`), which checks the site can
//! be reached and says so in words when it cannot, rather than leaving the
//! webview's own error page on screen.
//!
//! Each thing the app does beside showing the site is its own module, and
//! `main` only puts them together: where links go (navigation.rs), the
//! window's frame (chrome.rs), what the site tells the app (bridge.rs),
//! Steam (steam.rs), losing a drawing (close_guard.rs), the site going away
//! (offline.rs), and on Windows the keys (keys.rs), the browser's parts the
//! app answers for (webview2.rs) and Snap Layouts (snap.rs).

use std::sync::Arc;

use tauri::{App, Listener, Manager, WebviewUrl, WebviewWindow, WebviewWindowBuilder};
use url::Url;

mod badge;
mod bridge;
mod chrome;
mod close_guard;
// The right-click menu and the keys are WebView2's to hand over (webview2.rs).
#[cfg_attr(not(windows), allow(dead_code))]
mod context_menu;
mod dialogs;
mod downloads;
#[cfg_attr(not(windows), allow(dead_code))]
mod handoff;
mod keys;
mod navigation;
mod offline;
mod site;
#[cfg(windows)]
mod snap;
mod steam;
#[cfg(windows)]
mod webview2;
mod words;

/// The app's one window. caption.js and `capabilities/default.json` name it
/// too, and have to agree.
pub const WINDOW: &str = "main";

/// The site may drag and maximise the window from its toolbar, and tell the
/// app things over the bridge, and do nothing else with it.
fn site_capability(site: &Url) -> tauri::ipc::CapabilityBuilder {
    tauri::ipc::CapabilityBuilder::new("site")
        .remote(site::pattern(site))
        .window(WINDOW)
        .permission("core:window:allow-start-dragging")
        .permission("core:window:allow-internal-toggle-maximize")
        // The Windows caption buttons in the toolbar (caption.js).
        .permission("core:window:allow-minimize")
        .permission("core:window:allow-toggle-maximize")
        .permission("core:window:allow-is-maximized")
        .permission("core:window:allow-close")
        // The bridge (bridge.js) and where the toolbar's maximise button is
        // (snap.rs). Tauri 2's `emit` takes no scope, so this cannot be held
        // to those two event names: a page of the site may emit any event.
        // Nothing listens for any other, and only the site's own pages get
        // this at all.
        .permission("core:event:allow-emit")
}

/// The bundled loader, told which site to open.
fn loader(site: &Url) -> String {
    format!(
        "index.html?site={}",
        url::form_urlencoded::byte_serialize(site.as_str().as_bytes()).collect::<String>()
    )
}

/// Acts on what the site says (bridge.rs): the unread count on the icon, and
/// what the player is doing for their Steam friends.
fn listen_to_the_site(app: &App, window: &WebviewWindow, steam: Option<Arc<steam::Steam>>) {
    let window = window.clone();
    app.listen_any(bridge::EVENT, move |event| match bridge::parse(event.payload()) {
        Some(bridge::Message::Page(page)) => {
            // Nobody signed in has no bell, and so no count to send.
            if page.signed_in == Some(false) {
                badge::show(&window, 0);
            }
            if let Some(steam) = &steam {
                steam.show_presence(Some(&page));
            }
        }
        Some(bridge::Message::Unread { count }) => badge::show(&window, count),
        Some(bridge::Message::Other) | None => {}
    });
}

fn setup(app: &mut App, site: &Url, steam: &Option<Arc<steam::Steam>>) -> tauri::Result<()> {
    app.add_capability(site_capability(site))?;

    let loader = loader(site);
    let builder = WebviewWindowBuilder::new(app, WINDOW, WebviewUrl::App(loader.clone().into()))
        .title("Oeee Cafe")
        .inner_size(1280.0, 860.0)
        .min_inner_size(800.0, 600.0)
        // Edge's address and contact suggestions over form fields.
        .general_autofill_enabled(false)
        // Saved where the player says, and never a replay file
        // (downloads.rs).
        .on_download(|webview, event| downloads::handle(&webview, event));
    let builder = chrome::prepare(builder)
        .initialization_script(bridge::SCRIPT)
        .initialization_script(handoff::SCRIPT)
        .initialization_script(offline::page_script());
    let builder = steam::prepare(builder, steam, site);
    let window = navigation::prepare(builder, app.handle(), site, steam).build()?;

    listen_to_the_site(app, &window, steam.clone());
    handoff::listen(app.handle(), site);
    handoff::listen_for_return(app.handle());
    steam::watch_dlc(app.handle(), steam, site);
    chrome::paint_background(&window);

    #[cfg(windows)]
    {
        webview2::attach(&window)?;
        keys::attach(&window, site)?;
        offline::attach(&window, site, &loader)?;
        snap::attach(&window)?;
    }
    Ok(())
}

fn main() {
    // Before any window, as Steam asks, so its overlay can find them.
    let steam = steam::start();
    let site = site::from_env();

    tauri::Builder::default()
        // First, as the plugin asks: a second launch hands over here and
        // exits, and the window already open comes forward.
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            // Opening oeee-cafe://... while the app is running is a second
            // launch carrying it, on the platforms where a scheme is handed
            // over that way, so this is also how a sign-in comes back.
            handoff::returned(app);
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
        .plugin(tauri_plugin_deep_link::init())
        .plugin(tauri_plugin_opener::init())
        .setup(move |app| Ok(setup(app, &site, &steam)?))
        .on_window_event(|window, event| {
            chrome::on_window_event(window, event);
            close_guard::on_window_event(window, event);
            handoff::on_window_event(window, event);
        })
        .run(tauri::generate_context!())
        .expect("error while running Oeee Cafe");
}
