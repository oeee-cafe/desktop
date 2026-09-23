//! Closing the window over a drawing asks first.
//!
//! Closing a window unloads its page without asking it on any platform, so
//! the app asks first: the page asks its own `beforeunload` handlers the way
//! a browser asks them (`window.oeeeApp.wouldLoseWork()`, app_bridge.jinja in
//! oeee-cafe/web), and if one would keep a browser on the page, the player
//! is asked whether to leave anyway. Every handler runs for real, so one that
//! did more than answer -- sent a "goodbye" to a server, say -- would do it
//! here too, though the player may yet stay. The site's only handler, the
//! painter's, just answers.
//!
//! The site also says whether a page is painting (`painting` in its `page`
//! message, bridge.rs), but the handlers are what the page itself decides by,
//! and asking them keeps the window and a browser in agreement.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

use tauri::{AppHandle, Manager, Window, WindowEvent};

use crate::{dialogs, WINDOW};

/// Whether leaving now would lose something, as the page answers it; a page
/// that is not the site's -- the loader -- has nothing to lose. Evaluated for
/// its answer.
const WOULD_LOSE_WORK: &str = "window.oeeeApp ? window.oeeeApp.wouldLoseWork() : false";

/// Set while the question is on screen, so a second click on the close
/// button does not stack a second one behind it.
static ASKING: AtomicBool = AtomicBool::new(false);

/// Holds a close back until the page has agreed to go.
pub fn on_window_event(window: &Window, event: &WindowEvent) {
    if let WindowEvent::CloseRequested { api, .. } = event {
        api.prevent_close();
        let label = window.label().to_owned();
        after_leaving(window.app_handle(), move |app| {
            if let Some(window) = app.get_webview_window(&label) {
                let _ = window.destroy();
            }
        });
    }
}

/// Run `then` once the page has agreed to go, asking the player first if it
/// holds something unsaved.
fn after_leaving(app: &AppHandle, then: impl FnOnce(&AppHandle) + Send + 'static) {
    let Some(window) = app.get_webview_window(WINDOW) else {
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
            dialogs::ask(&app, dialogs::Question::Leave, move |leave| {
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
