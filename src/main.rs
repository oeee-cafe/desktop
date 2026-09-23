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
//! (offline.rs), the store the build sells through (store.rs; steam.rs and
//! microsoft.rs), and on Windows the keys (keys.rs), the browser's parts the
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
// The keys are WebView2's to hand over (webview2.rs).
#[cfg_attr(not(windows), allow(dead_code))]
mod keys;
mod leave;
mod microsoft;
mod navigation;
// What a page that failed is sent back to is WebView2's to report (webview2.rs).
#[cfg_attr(not(windows), allow(dead_code))]
mod offline;
mod site;
#[cfg(windows)]
mod snap;
mod steam;
mod store;
#[cfg(windows)]
mod webview2;
mod words;

/// The app's one window. The loader and `capabilities/default.json` name it
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
        // The bridge (bridge.js) -- by which the toolbar's window controls
        // ask for the window, rather than being let at it themselves
        // (chrome.rs) -- and the page's answer to the Microsoft Store build's
        // request for a ticket (microsoft.rs). Tauri 2's `emit` takes no
        // scope, so this cannot be held to those event names: a page of the
        // site may emit any event. Nothing listens for any other, and only
        // the site's own pages get this at all.
        .permission("core:event:allow-emit")
}

/// The bundled loader, told which site to open.
fn loader(site: &Url) -> String {
    format!(
        "index.html?site={}",
        url::form_urlencoded::byte_serialize(site.as_str().as_bytes()).collect::<String>()
    )
}

/// Acts on what the site says (bridge.rs): the unread count on the icon,
/// what the player is doing for their Steam friends, the ground behind the
/// page, the words for the app's own dialogs, a sign-in to open in the
/// browser, Steam's sign-in, and the store -- Steam's or the Microsoft
/// Store's, whichever the build sells through.
fn listen_to_the_site(
    app: &App,
    window: &WebviewWindow,
    site: &Url,
    steam: Option<Arc<steam::Steam>>,
    microsoft: Option<Arc<microsoft::Microsoft>>,
) {
    let (window, site) = (window.clone(), site.clone());
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
        Some(bridge::Message::Theme(theme)) => {
            if let Some(ground) = theme.ground {
                chrome::paint_ground(&window, &ground);
            }
        }
        Some(bridge::Message::Words(said)) => words::heard(said),
        Some(bridge::Message::Window { action }) => chrome::window_asked(&window, &action),
        Some(bridge::Message::Caption { place }) => chrome::caption_placed(&window, place),
        Some(bridge::Message::Browse { url }) => handoff::browse(window.app_handle(), &site, &url),
        // The page asks this app only for Steam's sign-in, and only where
        // the user agent named Steam as the store (steam::store). Answered
        // with nothing where there is no Steam, rather than left waiting.
        Some(bridge::Message::SignIn) => steam::answer_sign_in(window.app_handle(), steam.clone()),
        // Likewise only a page whose user agent named a store asks what
        // anything costs or to buy it, and a build sells through one store
        // at most.
        Some(bridge::Message::Prices { products }) => {
            if let Some(steam) = &steam {
                steam::answer_prices(window.app_handle(), steam, products);
            } else if let Some(microsoft) = &microsoft {
                microsoft.answer_prices(products);
            }
        }
        Some(bridge::Message::Purchase { product }) => {
            if let Some(steam) = &steam {
                steam::open_store(steam, &product);
            } else if let Some(microsoft) = &microsoft {
                microsoft.sell(product);
            }
        }
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
    let builder = chrome::prepare(builder).initialization_script(bridge::SCRIPT);
    let builder = steam::prepare(builder, steam, site);
    let window = navigation::prepare(builder, app.handle(), site).build()?;

    // The Microsoft Store's build sells through the Store when it is the
    // Store's package; everywhere else there is none.
    let microsoft = microsoft::start(&window);
    listen_to_the_site(app, &window, site, steam.clone(), microsoft.clone());
    handoff::listen_for_return(app.handle());
    steam::watch_dlc(app.handle(), steam);
    chrome::paint_background(&window);

    #[cfg(windows)]
    {
        let store = steam::store(steam).or(microsoft::store(&microsoft));
        webview2::attach(&window, store)?;
        keys::attach(&window)?;
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
