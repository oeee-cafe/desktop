//! Snap Layouts on the maximise button the toolbar draws (caption.js).
//!
//! Windows 11 offers its layouts to a pointer resting on a window's maximise
//! button, and it knows the button by asking the window under the pointer
//! what is there (`WM_NCHITTEST`) and hearing `HTMAXBUTTON`. The toolbar's
//! button is the page's, and the page cannot answer, so a small window of
//! the app's own sits over it, draws nothing, and answers for it -- as
//! Windows Terminal does for the buttons in its own title bar.
//!
//! The page says where its button is (an `oeee-caption-maximize` event, in
//! the window's pixels), and the stand-in does the button's work while the
//! pointer is on it: tells the page to show the button hot or pressed, and
//! maximises or restores the window on a click, the way the system's own
//! button does.

use std::cell::Cell;
use std::sync::atomic::{AtomicIsize, Ordering};
use std::sync::OnceLock;

use serde::Deserialize;
use tauri::{Listener, Manager, WebviewWindow};
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

/// The event the page says where its maximise button is in.
pub const EVENT: &str = "oeee-caption-maximize";

/// The button's place in the window, in physical pixels from the top left of
/// the window's client area -- which is where the webview starts.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq)]
pub struct Place {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
}

/// The largest the stand-in may be on a side, so a page cannot spread it
/// over the rest of itself: a caption button is 46 by 52 at 100%, and this
/// is room for it at 400%.
const MOST: i32 = 256;

/// Where the page says its button is: `null` or anything unreadable as
/// nowhere, and a size past a button's as no more than one.
pub fn parse(payload: &str) -> Option<Place> {
    let place: Place = serde_json::from_str::<Option<Place>>(payload).ok()??;
    if place.width <= 0 || place.height <= 0 {
        return None;
    }
    Some(Place {
        x: place.x.max(0),
        y: place.y.max(0),
        width: place.width.min(MOST),
        height: place.height.min(MOST),
    })
}

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

/// Makes the stand-in, hidden until the page says where its button is, and
/// keeps it over the button wherever the page says it is.
pub fn attach(window: &WebviewWindow) -> tauri::Result<()> {
    make_stand_in(window)?;
    let app = window.app_handle().clone();
    window.app_handle().listen_any(EVENT, move |event| {
        let place = parse(event.payload());
        let _ = app.run_on_main_thread(move || self::place(place));
    });
    Ok(())
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
            "window.__oeeeCaption && window.__oeeeCaption.maximizeState({state:?})"
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_page_says_where_its_button_is() {
        assert_eq!(
            parse(r#"{"x":1782,"y":0,"width":69,"height":78}"#),
            Some(Place { x: 1782, y: 0, width: 69, height: 78 })
        );
    }

    #[test]
    fn no_button_is_nowhere() {
        assert_eq!(parse("null"), None);
        assert_eq!(parse("not json"), None);
        assert_eq!(parse(r#"{"x":0,"y":0,"width":0,"height":52}"#), None);
    }

    #[test]
    fn the_stand_in_is_never_bigger_than_a_button() {
        assert_eq!(
            parse(r#"{"x":-5,"y":-5,"width":5000,"height":5000}"#),
            Some(Place { x: 0, y: 0, width: MOST, height: MOST })
        );
    }

    #[test]
    fn the_toolbar_says_where_its_button_is() {
        assert!(crate::chrome::WINDOWS_CAPTION.contains(EVENT));
    }
}
