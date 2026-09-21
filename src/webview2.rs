//! WebView2 settings Tauri leaves at the browser's defaults.
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
    ICoreWebView2Settings3, ICoreWebView2Settings4,
};
use windows::core::Interface;

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
