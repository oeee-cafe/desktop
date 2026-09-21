//! `alert()`, `confirm()`, `beforeunload` and quitting on macOS.
//!
//! WKWebView shows none of them by itself. It asks its UI delegate, and wry's
//! delegate answers only for file pickers and camera permission, so on a Mac
//! `alert()` showed nothing, `confirm()` answered "no" without asking -- which
//! cancelled every `hx-confirm` on the site, so nothing could be deleted -- and
//! the painter's guard against leaving an unsaved drawing never appeared.
//!
//! This teaches wry's delegate class the missing methods rather than replacing
//! the delegate, which would take the file picker with it. WebKit reads which
//! methods a delegate has once, when the delegate is set, so the delegate is
//! set again afterwards.
//!
//! The `beforeunload` panel is `WKUIDelegatePrivate`, the method Safari itself
//! implements. That is fine for Steam; it would not pass the Mac App Store.

use std::ffi::CStr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::OnceLock;

use block2::Block;
use objc2::ffi::class_addMethod;
use objc2::runtime::{AnyClass, AnyObject, Bool, Imp, ProtocolObject, Sel};
use objc2::{sel, MainThreadMarker};
use objc2_app_kit::{
    NSAlert, NSAlertFirstButtonReturn, NSApplication, NSApplicationTerminateReply,
};
use objc2_foundation::NSString;
use objc2_web_kit::WKWebView;
use tauri::AppHandle;

use crate::words::words;

/// Give `webview`'s UI delegate the dialogs it is missing.
///
/// # Safety
///
/// `webview` is a live `WKWebView`, and this runs on the main thread.
pub unsafe fn install_dialogs(webview: &WKWebView) {
    let Some(delegate) = webview.UIDelegate() else {
        return;
    };
    let object: &AnyObject = ProtocolObject::as_ref(&*delegate);
    let class: *const AnyClass = object.class();

    add(
        class,
        sel!(webView:runJavaScriptAlertPanelWithMessage:initiatedByFrame:completionHandler:),
        alert as *const (),
        c"v@:@@@@?",
    );
    add(
        class,
        sel!(webView:runJavaScriptConfirmPanelWithMessage:initiatedByFrame:completionHandler:),
        confirm as *const (),
        c"v@:@@@@?",
    );
    add(
        class,
        sel!(_webView:runBeforeUnloadConfirmPanelWithMessage:initiatedByFrame:completionHandler:),
        before_unload as *const (),
        c"v@:@@@@?",
    );

    webview.setUIDelegate(Some(&delegate));
}

unsafe fn add(class: *const AnyClass, name: Sel, imp: *const (), types: &CStr) {
    // Adds nothing if the class already answers, as a later wry may.
    class_addMethod(
        class as *mut AnyClass,
        name,
        std::mem::transmute::<*const (), Imp>(imp),
        types.as_ptr(),
    );
}

/// Show a modal alert and return the index of the button that was pressed.
///
/// The first button is the default, answering Return.
pub fn ask(title: &str, body: Option<&str>, buttons: &[&str]) -> usize {
    let mtm = MainThreadMarker::new().expect("dialogs are shown on the main thread");
    let alert = NSAlert::new(mtm);
    alert.setMessageText(&NSString::from_str(title));
    if let Some(body) = body {
        alert.setInformativeText(&NSString::from_str(body));
    }
    for button in buttons {
        alert.addButtonWithTitle(&NSString::from_str(button));
    }
    let response = alert.runModal();
    usize::try_from(response - NSAlertFirstButtonReturn).unwrap_or(0)
}

unsafe fn message(message: *mut NSString) -> String {
    message.as_ref().map(|m| m.to_string()).unwrap_or_default()
}

extern "C-unwind" fn alert(
    _this: *mut AnyObject,
    _cmd: Sel,
    _webview: *mut AnyObject,
    text: *mut NSString,
    _frame: *mut AnyObject,
    completion: *mut Block<dyn Fn()>,
) {
    let text = unsafe { message(text) };
    ask(&text, None, &[words().ok]);
    unsafe { (*completion).call(()) };
}

extern "C-unwind" fn confirm(
    _this: *mut AnyObject,
    _cmd: Sel,
    _webview: *mut AnyObject,
    text: *mut NSString,
    _frame: *mut AnyObject,
    completion: *mut Block<dyn Fn(Bool)>,
) {
    let text = unsafe { message(text) };
    let words = words();
    let accepted = ask(&text, None, &[words.ok, words.cancel]) == 0;
    unsafe { (*completion).call((Bool::new(accepted),)) };
}

extern "C-unwind" fn before_unload(
    _this: *mut AnyObject,
    _cmd: Sel,
    _webview: *mut AnyObject,
    // What the page asked to say. Browsers stopped showing it years ago,
    // because pages used it to plead, and this does not show it either.
    _text: *mut NSString,
    _frame: *mut AnyObject,
    completion: *mut Block<dyn Fn(Bool)>,
) {
    let leave = confirm_leaving();
    unsafe { (*completion).call((Bool::new(leave),)) };
}

/// Whether the player means to leave a page that has asked them not to.
/// Staying is the default, so a reflexive Return keeps the drawing.
pub fn confirm_leaving() -> bool {
    let words = words();
    ask(
        words.leave_title,
        Some(words.leave_body),
        &[words.stay, words.leave],
    ) == 1
}

static APP: OnceLock<AppHandle> = OnceLock::new();

/// Set once the page has agreed to go, so the quit it then asks for is not
/// questioned a second time.
static QUIT_AGREED: AtomicBool = AtomicBool::new(false);

/// Ask the page before quitting, however the quit was asked for.
///
/// ⌘Q, the app menu, the Dock and logging out all send `terminate:`, and tao
/// answers only `applicationWillTerminate:`, by which point it is too late to
/// say no -- so none of them reach Tauri's `ExitRequested`, and each ended
/// the app with an unsaved drawing in it. This gives tao's app delegate the
/// question NSApplication asks first.
///
/// # Safety
///
/// Runs on the main thread, after tao has set the app delegate.
pub unsafe fn guard_quit(app: &AppHandle) {
    let _ = APP.set(app.clone());
    let mtm = MainThreadMarker::new().expect("setup runs on the main thread");
    let Some(delegate) = NSApplication::sharedApplication(mtm).delegate() else {
        return;
    };
    let object: &AnyObject = ProtocolObject::as_ref(&*delegate);
    add(
        object.class(),
        sel!(applicationShouldTerminate:),
        should_terminate as *const (),
        // Returns NSApplicationTerminateReply, an NSUInteger.
        c"Q@:@",
    );
}

extern "C-unwind" fn should_terminate(
    _this: *mut AnyObject,
    _cmd: Sel,
    _sender: *mut AnyObject,
) -> NSApplicationTerminateReply {
    if QUIT_AGREED.load(Ordering::SeqCst) {
        return NSApplicationTerminateReply::TerminateNow;
    }
    let Some(app) = APP.get() else {
        return NSApplicationTerminateReply::TerminateNow;
    };
    // The page answers asynchronously, so refuse this quit and ask for a new
    // one once it has.
    crate::after_leaving(app, |app| {
        QUIT_AGREED.store(true, Ordering::SeqCst);
        app.exit(0);
    });
    NSApplicationTerminateReply::TerminateCancel
}
