//! The question the window asks before a drawing is lost, as the system's
//! own dialog.
//!
//! It is asked when the window is closed over a drawing (close_guard.rs), and
//! when the page asks before it is left (its `beforeunload`), which WebView2
//! would otherwise draw as a browser does -- titled "oeee.cafe says", in the
//! browser's words; webview2.rs hands that one here. The site asks every
//! other question in its own dialog (confirm_dialog.jinja in oeee-cafe/web)
//! and calls neither `alert()` nor `confirm()`, so nothing else is asked here.

use rfd::{AsyncMessageDialog, MessageButtons, MessageDialogResult, MessageLevel};
use tauri::{AppHandle, Manager};

use crate::words;

/// Asks whether to leave something unsaved, over the window, and passes
/// `answer` whether the player chose Leave. Made on the main thread, as
/// tauri-plugin-dialog makes its dialogs, and awaited off it.
pub fn ask_to_leave(app: &AppHandle, answer: impl FnOnce(bool) + Send + 'static) {
    let window = app.get_webview_window(crate::WINDOW);
    let _ = app.run_on_main_thread(move || {
        let words = words::words();
        let dialog = AsyncMessageDialog::new()
            .set_level(MessageLevel::Warning)
            .set_title(&words.leave_title)
            .set_description(&words.leave_body)
            // Staying is the default, so a reflexive Return keeps the work.
            .set_buttons(MessageButtons::OkCancelCustom(
                words.stay.clone(),
                words.leave.clone(),
            ));
        let dialog = match &window {
            Some(window) => dialog.set_parent(window),
            None => dialog,
        };
        let shown = dialog.show();
        std::thread::spawn(move || {
            let result = tauri::async_runtime::block_on(shown);
            answer(leaves(&result, &words.leave));
        });
    });
}

/// Whether `result` is the player choosing Leave. Esc and a dialog's close box
/// answer Cancel, which is never agreement: a player dismissing the question
/// has not agreed to lose their drawing.
fn leaves(result: &MessageDialogResult, leave: &str) -> bool {
    matches!(result, MessageDialogResult::Custom(label) if label == leave)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_leave_leaves() {
        let words = words::Words::default();
        let leave = words.leave.as_str();
        assert!(leaves(&MessageDialogResult::Custom(leave.into()), leave));
        assert!(!leaves(
            &MessageDialogResult::Custom(words.stay.clone()),
            leave
        ));
        assert!(!leaves(&MessageDialogResult::Cancel, leave));
    }
}
