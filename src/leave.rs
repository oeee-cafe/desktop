//! The two calls every app makes around leaving a page, word for word as
//! the site's contract gives them (`scripts` in appContract.json, and
//! app_bridge.jinja, in oeee-cafe/web).
//!
//! Both are guarded to the member, so a page without it -- the loader, or a
//! page of the site from before it -- answers as having nothing to lose, and
//! shows nothing.

/// Whether leaving now would lose something, as the page's own
/// `beforeunload` handlers answer it; evaluated for its answer before the
/// window is closed (close_guard.rs).
pub const WOULD_LOSE_WORK: &str =
    "window.oeeeApp && window.oeeeApp.wouldLoseWork ? window.oeeeApp.wouldLoseWork() : false";

/// The player said Leave in the app's own dialog, and the next page is on
/// its way: the page puts its loading bar up. The promise it returns settles
/// at once on Chromium, and WebView2's load is already going, so it is not
/// waited for (webview2.rs).
#[cfg_attr(not(windows), allow(dead_code))]
pub const LEAVING: &str =
    "window.oeeeApp && window.oeeeApp.leaving ? window.oeeeApp.leaving() : null";
