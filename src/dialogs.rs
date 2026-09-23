//! The questions the window asks, as the system's own dialogs.
//!
//! Closing the window over a drawing always asked this way. The page's own
//! `alert()`, `confirm()` and leaving a page with unsaved work were left to
//! WebView2, which draws them as a browser does -- titled "oeee.cafe says",
//! in the browser's words. webview2.rs hands those here instead. The Steam
//! sign-in failure is asked here directly (steam.rs).

use rfd::{AsyncMessageDialog, MessageButtons, MessageDialogResult, MessageLevel};
use tauri::{AppHandle, Manager};

use crate::words;

/// What the window asks. Only WebView2 hands over the page's own (webview2.rs);
/// elsewhere an alert is only ever the app's.
#[cfg_attr(not(windows), allow(dead_code))]
pub enum Question {
    /// Something to read, and OK: the page's `alert()`, or the app's own.
    Alert(String),
    /// The page's `confirm()`: OK or Cancel.
    Confirm(String),
    /// Leaving something unsaved: Stay or Leave.
    Leave,
}

/// Asks `question` over the window, and passes `answer` whether the player
/// agreed: OK, or Leave. Made on the main thread, as tauri-plugin-dialog makes
/// its dialogs, and awaited off it.
pub fn ask(app: &AppHandle, question: Question, answer: impl FnOnce(bool) + Send + 'static) {
    let window = app.get_webview_window(crate::WINDOW);
    let _ = app.run_on_main_thread(move || {
        let words = words::words();
        let dialog = AsyncMessageDialog::new();
        let dialog = match &question {
            Question::Alert(message) => dialog
                .set_level(MessageLevel::Info)
                .set_title(words::app_name())
                .set_description(message)
                .set_buttons(MessageButtons::Ok),
            Question::Confirm(message) => dialog
                .set_level(MessageLevel::Info)
                .set_title(words::app_name())
                .set_description(message)
                .set_buttons(MessageButtons::OkCancel),
            Question::Leave => dialog
                .set_level(MessageLevel::Warning)
                .set_title(&words.leave_title)
                .set_description(&words.leave_body)
                // Staying is the default, so a reflexive Return keeps the work.
                .set_buttons(MessageButtons::OkCancelCustom(
                    words.stay.clone(),
                    words.leave.clone(),
                )),
        };
        let dialog = match &window {
            Some(window) => dialog.set_parent(window),
            None => dialog,
        };
        let shown = dialog.show();
        std::thread::spawn(move || {
            let result = tauri::async_runtime::block_on(shown);
            answer(agreed(&question, &result, &words.leave));
        });
    });
}

/// Whether `result` is the player agreeing to `question`. Esc and a dialog's
/// close box answer Cancel, which is never agreement: a player dismissing the
/// question has not agreed to lose their drawing.
fn agreed(question: &Question, result: &MessageDialogResult, leave: &str) -> bool {
    match question {
        Question::Alert(_) => true,
        Question::Confirm(_) => {
            matches!(result, MessageDialogResult::Ok | MessageDialogResult::Yes)
        }
        Question::Leave => matches!(result, MessageDialogResult::Custom(label) if label == leave),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_leave_leaves() {
        let words = words::Words::default();
        let leave = words.leave.as_str();
        assert!(agreed(
            &Question::Leave,
            &MessageDialogResult::Custom(leave.into()),
            leave
        ));
        assert!(!agreed(
            &Question::Leave,
            &MessageDialogResult::Custom(words.stay.clone()),
            leave
        ));
        assert!(!agreed(
            &Question::Leave,
            &MessageDialogResult::Cancel,
            leave
        ));
    }

    #[test]
    fn confirm_is_ok_and_nothing_else() {
        let question = Question::Confirm("Delete?".into());
        assert!(agreed(&question, &MessageDialogResult::Ok, ""));
        assert!(!agreed(&question, &MessageDialogResult::Cancel, ""));
    }
}
