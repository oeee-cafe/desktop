//! Snap Layouts on the maximise button the toolbar draws (app_caption.jinja
//! in oeee-cafe/web).
//!
//! Windows 11 offers its layouts to a pointer resting on a window's maximise
//! button, and it knows the button by asking the window under the pointer
//! what is there (`WM_NCHITTEST`) and hearing `HTMAXBUTTON`. The toolbar's
//! button is the page's, and the page cannot answer, so a small window of
//! the app's own sits over it, draws nothing, and answers for it -- as
//! Windows Terminal does for the buttons in its own title bar.
//!
//! The page says where its button is (a `caption` message on the bridge, in
//! the window's pixels; chrome.rs), and the stand-in does the button's work
//! while the pointer is on it: tells the page to show the button hot or
//! pressed (`oeeeApp.caption`), and
//! maximises or restores the window on a click, the way the system's own
//! button does.
//!
//! It goes with each document the window leaves. A page of the site says
//! where its button is as it loads, but nothing says when the button has
//! gone: the loader, where offline.rs sends a page the site did not answer
//! for, draws caption buttons of its own in the same corner and says
//! nothing, and neither does a page of the site without the toolbar. So
//! the stand-in is taken away when a new document starts (WebView2's
//! `ContentLoading`), which is before any of that document's scripts run,
//! and a page that has a button puts it back. A navigation that is refused
//! or cancelled -- a link handed to the browser, a Stay -- starts no
//! document and leaves it where it is, and so does htmx's swap of a page,
//! which is the same document.

use std::cell::Cell;
use std::sync::atomic::{AtomicIsize, Ordering};
use std::sync::OnceLock;

use crate::bridge::Place;

use tauri::WebviewWindow;
use webview2_com::ContentLoadingEventHandler;
use windows::core::w;
use windows::Win32::Foundation::{HINSTANCE, HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    TrackMouseEvent, TME_LEAVE, TME_NONCLIENT, TRACKMOUSEEVENT,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, GetParent, IsZoomed, LoadCursorW, PostMessageW,
    RegisterClassExW, SetWindowPos, ShowWindow, HTMAXBUTTON, HWND_TOP, IDC_ARROW, SC_MAXIMIZE,
    SC_RESTORE, SWP_NOACTIVATE, SWP_SHOWWINDOW, SW_HIDE, WM_NCHITTEST, WM_NCLBUTTONDBLCLK,
    WM_NCLBUTTONDOWN, WM_NCLBUTTONUP, WM_NCMOUSELEAVE, WM_NCMOUSEMOVE, WM_SYSCOMMAND, WNDCLASSEXW,
    WS_CHILD, WS_CLIPSIBLINGS, WS_EX_NOREDIRECTIONBITMAP,
};

/// The stand-in window, once made.
static STAND_IN: AtomicIsize = AtomicIsize::new(0);
/// The window whose page draws the button, for telling it how to show it.
static WINDOW: OnceLock<WebviewWindow> = OnceLock::new();

thread_local! {
    /// Whether the pointer is on the stand-in, and whether it was pressed
    /// there -- only ever touched on the window's thread.
    static HOT: Cell<bool> = const { Cell::new(false) };
    static PRESSED: Cell<bool> = const { Cell::new(false) };
}

/// Makes the stand-in, hidden until the page says where its button is
/// (`place`), and hidden again whenever the window starts on another
/// document.
pub fn attach(window: &WebviewWindow) -> tauri::Result<()> {
    make_stand_in(window)?;
    window.with_webview(|webview| unsafe {
        let Ok(core) = webview.controller().CoreWebView2() else {
            return;
        };
        // On the window's thread, as `place` has to be.
        let handler = ContentLoadingEventHandler::create(Box::new(|_, _| {
            place(None);
            Ok(())
        }));
        let mut token = Default::default();
        let _ = core.add_ContentLoading(&handler, &mut token);
    })
}

fn make_stand_in(window: &WebviewWindow) -> tauri::Result<()> {
    let parent = window.hwnd()?;
    let _ = WINDOW.set(window.clone());
    unsafe {
        let instance = GetModuleHandleW(None).map(|h| HINSTANCE(h.0)).ok();
        let class = WNDCLASSEXW {
            cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
            lpfnWndProc: Some(stand_in_proc),
            hInstance: instance.unwrap_or_default(),
            hCursor: LoadCursorW(None, IDC_ARROW).unwrap_or_default(),
            lpszClassName: w!("OEEE_CAPTION_MAXIMIZE"),
            ..Default::default()
        };
        RegisterClassExW(&class);
        // Nothing of its own to show: the page's button shows through.
        let stand_in = CreateWindowExW(
            WS_EX_NOREDIRECTIONBITMAP,
            w!("OEEE_CAPTION_MAXIMIZE"),
            w!(""),
            WS_CHILD | WS_CLIPSIBLINGS,
            0,
            0,
            0,
            0,
            Some(parent),
            None,
            instance,
            None,
        )
        .map_err(|error| tauri::Error::Anyhow(error.into()))?;
        STAND_IN.store(stand_in.0 as isize, Ordering::SeqCst);
    }
    Ok(())
}

/// Puts the stand-in over the page's button, above the webview, or takes it
/// away when the page has none. On the window's thread.
pub fn place(place: Option<Place>) {
    let stand_in = HWND(STAND_IN.load(Ordering::SeqCst) as _);
    if stand_in.is_invalid() {
        return;
    }
    unsafe {
        match place {
            Some(p) => {
                let _ = SetWindowPos(
                    stand_in,
                    Some(HWND_TOP),
                    p.x,
                    p.y,
                    p.width,
                    p.height,
                    SWP_NOACTIVATE | SWP_SHOWWINDOW,
                );
            }
            None => {
                let _ = ShowWindow(stand_in, SW_HIDE);
                leave();
            }
        }
    }
}

/// Tells the page how to show its button: `hot`, `pressed`, or as it is.
fn show(state: &str) {
    if let Some(window) = WINDOW.get() {
        let _ = window.eval(format!(
            "window.oeeeApp && window.oeeeApp.caption && window.oeeeApp.caption({{pointer: {state:?}}})"
        ));
    }
}

fn leave() {
    HOT.set(false);
    PRESSED.set(false);
    show("");
}

unsafe extern "system" fn stand_in_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match msg {
        // What Windows asks before offering its layouts.
        WM_NCHITTEST => LRESULT(HTMAXBUTTON as isize),
        WM_NCMOUSEMOVE => {
            if !HOT.replace(true) {
                // Told when the pointer goes, or the layouts open over it.
                let mut track = TRACKMOUSEEVENT {
                    cbSize: std::mem::size_of::<TRACKMOUSEEVENT>() as u32,
                    dwFlags: TME_LEAVE | TME_NONCLIENT,
                    hwndTrack: hwnd,
                    dwHoverTime: 0,
                };
                let _ = TrackMouseEvent(&mut track);
                show(if PRESSED.get() { "pressed" } else { "hot" });
            }
            LRESULT(0)
        }
        WM_NCMOUSELEAVE => {
            leave();
            LRESULT(0)
        }
        // Handled here, not by the system: its own tracking of a maximise
        // button is for a title bar this window does not have.
        WM_NCLBUTTONDOWN | WM_NCLBUTTONDBLCLK => {
            PRESSED.set(true);
            show("pressed");
            LRESULT(0)
        }
        WM_NCLBUTTONUP => {
            if PRESSED.replace(false) {
                show("hot");
                if let Ok(parent) = GetParent(hwnd) {
                    // As the system's button asks it, so the window's own
                    // maximising -- to the work area, undecorated -- runs.
                    let command = if IsZoomed(parent).as_bool() { SC_RESTORE } else { SC_MAXIMIZE };
                    let _ = PostMessageW(
                        Some(parent),
                        WM_SYSCOMMAND,
                        WPARAM(command as usize),
                        LPARAM(0),
                    );
                }
            }
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}
