//! WebView2 settings Tauri leaves at the browser's defaults, and the page
//! loads that failed, which Tauri does not report (offline.rs).
//!
//! Tauri already turns off the status bar, zoom and swipe navigation. It
//! leaves on the browser's own keys -- F5, Ctrl+R, Ctrl+F's find bar, Ctrl+P's
//! print preview -- and Edge's offer to save a password, each of which is the
//! browser speaking rather than the app. Editing keys (copy, paste, undo) are
//! not browser accelerators and keep working.
//!
//! The default context menu is handled by the page script in `main.rs`
//! instead: WebView2's switch for it takes Cut, Copy and Paste out of text
//! fields as well.

use webview2_com::Microsoft::Web::WebView2::Win32::{
    ICoreWebView2NavigationCompletedEventArgs2, ICoreWebView2Settings3,
    ICoreWebView2Settings4, COREWEBVIEW2_WEB_ERROR_STATUS,
    COREWEBVIEW2_WEB_ERROR_STATUS_CANNOT_CONNECT, COREWEBVIEW2_WEB_ERROR_STATUS_CONNECTION_ABORTED,
    COREWEBVIEW2_WEB_ERROR_STATUS_CONNECTION_RESET, COREWEBVIEW2_WEB_ERROR_STATUS_DISCONNECTED,
    COREWEBVIEW2_WEB_ERROR_STATUS_HOST_NAME_NOT_RESOLVED,
    COREWEBVIEW2_WEB_ERROR_STATUS_SERVER_UNREACHABLE, COREWEBVIEW2_WEB_ERROR_STATUS_TIMEOUT,
};
use webview2_com::NavigationCompletedEventHandler;
use windows::core::{Interface, PWSTR};
use windows::Win32::System::Com::CoTaskMemFree;

use crate::offline;

pub fn quiet_the_browser(webview: &tauri::webview::PlatformWebview) {
    unsafe {
        let Ok(core) = webview.controller().CoreWebView2() else {
            return;
        };
        let Ok(settings) = core.Settings() else {
            return;
        };
        if let Ok(settings) = settings.cast::<ICoreWebView2Settings3>() {
            let _ = settings.SetAreBrowserAcceleratorKeysEnabled(false);
        }
        if let Ok(settings) = settings.cast::<ICoreWebView2Settings4>() {
            let _ = settings.SetIsPasswordAutosaveEnabled(false);
        }
    }
}

/// Calls `unreachable` with the address of each page the window could not
/// show because the site could not be reached (offline.rs): nothing
/// answered, or a gateway answered in the site's place.
pub fn on_unreachable(
    webview: &tauri::webview::PlatformWebview,
    unreachable: impl Fn(String) + 'static,
) {
    unsafe {
        let Ok(core) = webview.controller().CoreWebView2() else {
            return;
        };
        let handler = NavigationCompletedEventHandler::create(Box::new(move |sender, args| {
            let (Some(sender), Some(args)) = (sender, args) else {
                return Ok(());
            };
            let mut success = Default::default();
            args.IsSuccess(&mut success)?;
            let mut error = COREWEBVIEW2_WEB_ERROR_STATUS::default();
            args.WebErrorStatus(&mut error)?;
            let mut status = 0;
            if let Ok(args) = args.cast::<ICoreWebView2NavigationCompletedEventArgs2>() {
                let _ = args.HttpStatusCode(&mut status);
            }
            if !failed(success.as_bool(), error, status) {
                return Ok(());
            }
            let mut source = PWSTR::null();
            sender.Source(&mut source)?;
            let page = source.to_string().unwrap_or_default();
            CoTaskMemFree(Some(source.0 as _));
            unreachable(page);
            Ok(())
        }));
        let mut token = Default::default();
        let _ = core.add_NavigationCompleted(&handler, &mut token);
    }
}

/// Whether a page that finished like this is the site unreachable, rather
/// than the site answering or the navigation being called off.
fn failed(success: bool, error: COREWEBVIEW2_WEB_ERROR_STATUS, status: i32) -> bool {
    if u16::try_from(status).is_ok_and(offline::gateway_answered) {
        return true;
    }
    !success
        && [
            COREWEBVIEW2_WEB_ERROR_STATUS_CANNOT_CONNECT,
            COREWEBVIEW2_WEB_ERROR_STATUS_CONNECTION_ABORTED,
            COREWEBVIEW2_WEB_ERROR_STATUS_CONNECTION_RESET,
            COREWEBVIEW2_WEB_ERROR_STATUS_DISCONNECTED,
            COREWEBVIEW2_WEB_ERROR_STATUS_HOST_NAME_NOT_RESOLVED,
            COREWEBVIEW2_WEB_ERROR_STATUS_SERVER_UNREACHABLE,
            COREWEBVIEW2_WEB_ERROR_STATUS_TIMEOUT,
        ]
        .contains(&error)
}

#[cfg(test)]
mod tests {
    use super::*;
    use webview2_com::Microsoft::Web::WebView2::Win32::{
        COREWEBVIEW2_WEB_ERROR_STATUS_OPERATION_CANCELED, COREWEBVIEW2_WEB_ERROR_STATUS_UNKNOWN,
    };

    #[test]
    fn no_answer_is_unreachable() {
        assert!(failed(false, COREWEBVIEW2_WEB_ERROR_STATUS_CANNOT_CONNECT, 0));
        assert!(failed(false, COREWEBVIEW2_WEB_ERROR_STATUS_TIMEOUT, 0));
    }

    #[test]
    fn a_gateway_answering_is_unreachable() {
        assert!(failed(false, COREWEBVIEW2_WEB_ERROR_STATUS_UNKNOWN, 502));
        assert!(failed(true, COREWEBVIEW2_WEB_ERROR_STATUS_UNKNOWN, 522));
    }

    #[test]
    fn the_site_answering_or_a_stopped_navigation_is_not() {
        assert!(!failed(true, COREWEBVIEW2_WEB_ERROR_STATUS_UNKNOWN, 200));
        assert!(!failed(false, COREWEBVIEW2_WEB_ERROR_STATUS_UNKNOWN, 404));
        assert!(!failed(false, COREWEBVIEW2_WEB_ERROR_STATUS_UNKNOWN, 500));
        // A link handed to the browser, or a page the player chose to stay on.
        assert!(!failed(false, COREWEBVIEW2_WEB_ERROR_STATUS_OPERATION_CANCELED, 0));
    }
}
