//! WebView2 settings Tauri leaves at the browser's defaults, the page loads
//! that failed, which Tauri does not report (offline.rs), and the parts of
//! the browser the app answers for itself.
//!
//! Tauri already turns off the status bar, zoom and swipe navigation. It
//! leaves on the browser's own keys -- F5, Ctrl+R, Ctrl+F's find bar, Ctrl+P's
//! print preview -- and Edge's offer to save a password, each of which is the
//! browser speaking rather than the app. Editing keys (copy, paste, undo) are
//! not browser accelerators and keep working, and the ones a program answers
//! to -- back, forward, reload, close -- are the app's (`on_keys`, keys.rs).
//!
//! The page's `alert()`, `confirm()` and leaving a page with unsaved work are
//! asked as the system's dialogs (`on_script_dialogs`, dialogs.rs), and the
//! right-click menu keeps only what a program's would have
//! (`on_context_menu`, context_menu.rs). WebView2's switch for that menu would
//! take Cut, Copy and Paste out of text fields as well, so it is trimmed
//! rather than turned off.

use webview2_com::Microsoft::Web::WebView2::Win32::{
    ICoreWebView2ContextMenuItemCollection, ICoreWebView2Environment9,
    ICoreWebView2NavigationCompletedEventArgs2, ICoreWebView2ScriptDialogOpeningEventArgs,
    ICoreWebView2Settings3, ICoreWebView2Settings4, ICoreWebView2_11, ICoreWebView2_2,
    COREWEBVIEW2_CONTEXT_MENU_ITEM_KIND, COREWEBVIEW2_CONTEXT_MENU_ITEM_KIND_COMMAND,
    COREWEBVIEW2_CONTEXT_MENU_ITEM_KIND_SEPARATOR, COREWEBVIEW2_KEY_EVENT_KIND,
    COREWEBVIEW2_KEY_EVENT_KIND_KEY_DOWN, COREWEBVIEW2_KEY_EVENT_KIND_SYSTEM_KEY_DOWN,
    COREWEBVIEW2_PHYSICAL_KEY_STATUS, COREWEBVIEW2_SCRIPT_DIALOG_KIND,
    COREWEBVIEW2_SCRIPT_DIALOG_KIND_ALERT, COREWEBVIEW2_SCRIPT_DIALOG_KIND_BEFOREUNLOAD,
    COREWEBVIEW2_SCRIPT_DIALOG_KIND_CONFIRM, COREWEBVIEW2_WEB_ERROR_STATUS,
    COREWEBVIEW2_WEB_ERROR_STATUS_CANNOT_CONNECT, COREWEBVIEW2_WEB_ERROR_STATUS_CONNECTION_ABORTED,
    COREWEBVIEW2_WEB_ERROR_STATUS_CONNECTION_RESET, COREWEBVIEW2_WEB_ERROR_STATUS_DISCONNECTED,
    COREWEBVIEW2_WEB_ERROR_STATUS_HOST_NAME_NOT_RESOLVED,
    COREWEBVIEW2_WEB_ERROR_STATUS_SERVER_UNREACHABLE, COREWEBVIEW2_WEB_ERROR_STATUS_TIMEOUT,
};
use webview2_com::{
    AcceleratorKeyPressedEventHandler, ContextMenuRequestedEventHandler,
    CustomItemSelectedEventHandler, NavigationCompletedEventHandler,
    ScriptDialogOpeningEventHandler,
};
use windows::core::{Interface, HSTRING, PWSTR};
use windows::Win32::System::Com::{CoTaskMemFree, IStream};
use windows::Win32::UI::Input::KeyboardAndMouse::{GetKeyState, VIRTUAL_KEY, VK_CONTROL, VK_MENU, VK_SHIFT};

use crate::context_menu::{self, Item};
use crate::dialogs::{self, Question};
use crate::keys::{self, Modifiers};
use crate::offline;
use crate::words::Words;

/// A string WebView2 hands over, which the caller frees.
unsafe fn take_string(get: impl FnOnce(*mut PWSTR) -> windows::core::Result<()>) -> windows::core::Result<String> {
    let mut value = PWSTR::null();
    get(&mut value)?;
    let text = value.to_string().unwrap_or_default();
    CoTaskMemFree(Some(value.0 as _));
    Ok(text)
}

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

/// Calls `act` with each of the window's keys pressed (keys.rs), and keeps it
/// from the page. A key held down acts once.
pub fn on_keys(webview: &tauri::webview::PlatformWebview, act: impl Fn(keys::Action) + 'static) {
    unsafe {
        let controller = webview.controller();
        let handler = AcceleratorKeyPressedEventHandler::create(Box::new(move |_, args| {
            let Some(args) = args else {
                return Ok(());
            };
            let mut kind = COREWEBVIEW2_KEY_EVENT_KIND::default();
            args.KeyEventKind(&mut kind)?;
            if kind != COREWEBVIEW2_KEY_EVENT_KIND_KEY_DOWN && kind != COREWEBVIEW2_KEY_EVENT_KIND_SYSTEM_KEY_DOWN {
                return Ok(());
            }
            let mut key = 0;
            args.VirtualKey(&mut key)?;
            let held = Modifiers {
                ctrl: is_down(VK_CONTROL),
                alt: is_down(VK_MENU),
                shift: is_down(VK_SHIFT),
            };
            let Some(action) = keys::action(key, held) else {
                return Ok(());
            };
            args.SetHandled(true)?;
            let mut status = COREWEBVIEW2_PHYSICAL_KEY_STATUS::default();
            args.PhysicalKeyStatus(&mut status)?;
            if !status.WasKeyDown.as_bool() {
                act(action);
            }
            Ok(())
        }));
        let mut token = Default::default();
        let _ = controller.add_AcceleratorKeyPressed(&handler, &mut token);
    }
}

fn is_down(key: VIRTUAL_KEY) -> bool {
    // The high bit: held now, as the key being handled was pressed.
    unsafe { GetKeyState(key.0 as i32) < 0 }
}

/// WebView2's objects belong to the window's thread. A dialog's answer comes
/// back on another, and is carried to the window's thread in this before
/// anything touches them.
struct OnWindowThread<T>(T);

// SAFETY: only ever opened inside `run_on_main_thread`, the thread the
// objects were made on.
unsafe impl<T> Send for OnWindowThread<T> {}

impl<T> OnWindowThread<T> {
    fn into_inner(self) -> T {
        self.0
    }
}

/// The page's `alert()`, `confirm()` and its asking before it is left, as the
/// system's dialogs (dialogs.rs) instead of WebView2's "oeee.cafe says". The
/// page waits, as it would for the browser's, until the player answers.
/// `prompt()`, which the site never asks, answers as cancelled.
pub fn on_script_dialogs(webview: &tauri::webview::PlatformWebview, app: tauri::AppHandle) {
    unsafe {
        let Ok(core) = webview.controller().CoreWebView2() else {
            return;
        };
        let Ok(settings) = core.Settings() else {
            return;
        };
        if settings.SetAreDefaultScriptDialogsEnabled(false).is_err() {
            return;
        }
        let handler = ScriptDialogOpeningEventHandler::create(Box::new(move |_, args| {
            let Some(args) = args else {
                return Ok(());
            };
            let mut kind = COREWEBVIEW2_SCRIPT_DIALOG_KIND::default();
            args.Kind(&mut kind)?;
            let question = match kind {
                COREWEBVIEW2_SCRIPT_DIALOG_KIND_ALERT => Question::Alert(message(&args)?),
                COREWEBVIEW2_SCRIPT_DIALOG_KIND_CONFIRM => Question::Confirm(message(&args)?),
                COREWEBVIEW2_SCRIPT_DIALOG_KIND_BEFOREUNLOAD => Question::Leave,
                _ => return Ok(()),
            };
            let deferral = args.GetDeferral()?;
            let pending = OnWindowThread((args, deferral));
            let window_thread = app.clone();
            dialogs::ask(&app, question, move |agreed| {
                let _ = window_thread.run_on_main_thread(move || {
                    let (args, deferral) = pending.into_inner();
                    if agreed {
                        let _ = args.Accept();
                    }
                    let _ = deferral.Complete();
                });
            });
            Ok(())
        }));
        let mut token = Default::default();
        let _ = core.add_ScriptDialogOpening(&handler, &mut token);
    }
}

unsafe fn message(args: &ICoreWebView2ScriptDialogOpeningEventArgs) -> windows::core::Result<String> {
    take_string(|value| args.Message(value))
}

/// Trims WebView2's right-click menu to what a program's would have
/// (context_menu.rs), and puts Copy link first on a link.
pub fn on_context_menu(webview: &tauri::webview::PlatformWebview, words: &'static Words) {
    unsafe {
        let Ok(core) = webview.controller().CoreWebView2() else {
            return;
        };
        let (Ok(core), Some(environment)) = (
            core.cast::<ICoreWebView2_11>(),
            core.cast::<ICoreWebView2_2>()
                .and_then(|core| core.Environment())
                .and_then(|environment| environment.cast::<ICoreWebView2Environment9>())
                .ok(),
        ) else {
            return;
        };
        let handler = ContextMenuRequestedEventHandler::create(Box::new(move |_, args| {
            let Some(args) = args else {
                return Ok(());
            };
            let items = args.MenuItems()?;
            trim(&items)?;

            let target = args.ContextMenuTarget()?;
            let mut has_link = Default::default();
            target.HasLinkUri(&mut has_link)?;
            if has_link.as_bool() {
                let link = take_string(|value| target.LinkUri(value))?;
                if let Some(link) = context_menu::copyable_link(&link).map(str::to_owned) {
                    let copy = environment.CreateContextMenuItem(
                        &HSTRING::from(words.copy_link),
                        None::<&IStream>,
                        COREWEBVIEW2_CONTEXT_MENU_ITEM_KIND_COMMAND,
                    )?;
                    let selected = CustomItemSelectedEventHandler::create(Box::new(move |_, _| {
                        if let Err(error) = copy_text(&link) {
                            eprintln!("could not copy the link: {error}");
                        }
                        Ok(())
                    }));
                    let mut token = Default::default();
                    copy.add_CustomItemSelected(&selected, &mut token)?;
                    if count(&items)? > 0 {
                        let separator = environment.CreateContextMenuItem(
                            &HSTRING::new(),
                            None::<&IStream>,
                            COREWEBVIEW2_CONTEXT_MENU_ITEM_KIND_SEPARATOR,
                        )?;
                        items.InsertValueAtIndex(0, &separator)?;
                    }
                    items.InsertValueAtIndex(0, &copy)?;
                }
            }

            if count(&items)? == 0 {
                args.SetHandled(true)?;
            }
            Ok(())
        }));
        let mut token = Default::default();
        let _ = core.add_ContextMenuRequested(&handler, &mut token);
    }
}

unsafe fn count(items: &ICoreWebView2ContextMenuItemCollection) -> windows::core::Result<u32> {
    let mut count = 0;
    items.Count(&mut count)?;
    Ok(count)
}

/// Removes from `items` everything context_menu.rs does not keep.
unsafe fn trim(items: &ICoreWebView2ContextMenuItemCollection) -> windows::core::Result<()> {
    let mut names = Vec::new();
    for index in 0..count(items)? {
        let item = items.GetValueAtIndex(index)?;
        let mut kind = COREWEBVIEW2_CONTEXT_MENU_ITEM_KIND::default();
        item.Kind(&mut kind)?;
        names.push(if kind == COREWEBVIEW2_CONTEXT_MENU_ITEM_KIND_SEPARATOR {
            None
        } else {
            Some(take_string(|value| item.Name(value))?)
        });
    }
    let described: Vec<Item> = names
        .iter()
        .map(|name| name.as_deref().map_or(Item::Separator, Item::Named))
        .collect();
    let kept = context_menu::kept(&described);
    // From the end, so the indices still to come stay where they were.
    for index in (0..names.len()).rev() {
        if !kept.contains(&index) {
            items.RemoveValueAtIndex(index as u32)?;
        }
    }
    Ok(())
}

/// Puts `text` on the clipboard, as the system's own Copy does.
fn copy_text(text: &str) -> windows::core::Result<()> {
    use windows::Win32::Foundation::{GlobalFree, HANDLE, HGLOBAL};
    use windows::Win32::System::DataExchange::{CloseClipboard, EmptyClipboard, OpenClipboard, SetClipboardData};
    use windows::Win32::System::Memory::{GlobalAlloc, GlobalLock, GlobalUnlock, GMEM_MOVEABLE};
    use windows::Win32::System::Ole::CF_UNICODETEXT;

    let wide: Vec<u16> = text.encode_utf16().chain(std::iter::once(0)).collect();
    unsafe {
        OpenClipboard(None)?;
        let copied = (|| {
            EmptyClipboard()?;
            let memory: HGLOBAL = GlobalAlloc(GMEM_MOVEABLE, wide.len() * 2)?;
            let target = GlobalLock(memory) as *mut u16;
            if target.is_null() {
                let _ = GlobalFree(Some(memory));
                return Err(windows::core::Error::from_win32());
            }
            std::ptr::copy_nonoverlapping(wide.as_ptr(), target, wide.len());
            let _ = GlobalUnlock(memory);
            // The clipboard owns the memory once it has it.
            if let Err(error) = SetClipboardData(CF_UNICODETEXT.0 as u32, Some(HANDLE(memory.0))) {
                let _ = GlobalFree(Some(memory));
                return Err(error);
            }
            Ok(())
        })();
        let _ = CloseClipboard();
        copied
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
