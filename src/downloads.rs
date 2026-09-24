//! What the site hands the player to keep.
//!
//! The site's own downloads are pictures -- a drawing or a collaborative
//! room exported as a PNG -- and each is saved where the player says, in the
//! system's Save dialog, as a desktop program saves: not dropped silently
//! in Downloads, and without the browser's download bubble.

use std::path::{Path, PathBuf};

use tauri::webview::DownloadEvent;
use tauri::{Runtime, Webview};

/// Answers a download: `false` refuses it, and `true` lets it go to the
/// `destination` the player picked.
pub fn handle<R: Runtime>(webview: &Webview<R>, event: DownloadEvent<'_>) -> bool {
    let DownloadEvent::Requested { destination, .. } = event else {
        return true;
    };
    match ask_where(webview, destination) {
        Some(chosen) => {
            *destination = chosen;
            true
        }
        None => false,
    }
}

/// The system's Save dialog, over the window, starting where WebView2 would
/// have put the file and with the name it would have given it.
fn ask_where<R: Runtime>(webview: &Webview<R>, suggested: &Path) -> Option<PathBuf> {
    let mut dialog = rfd::FileDialog::new();
    if let Some(folder) = suggested.parent().filter(|folder| folder.is_dir()) {
        dialog = dialog.set_directory(folder);
    }
    if let Some(name) = suggested.file_name() {
        dialog = dialog.set_file_name(name.to_string_lossy());
    }
    // Keeps the name's extension when the player types a name without one.
    if let Some(extension) = suggested.extension().map(|e| e.to_string_lossy().into_owned()) {
        dialog = dialog.add_filter(extension.to_uppercase(), &[extension]);
    }
    let window = webview.window();
    dialog.set_parent(&window).save_file()
}
