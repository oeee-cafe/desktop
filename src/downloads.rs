//! What the site hands the player to keep.
//!
//! The site's own downloads are pictures -- a drawing or a collaborative
//! room exported as a PNG -- and each is saved where the player says, in the
//! system's Save dialog, as a desktop program saves: not dropped silently
//! in Downloads, and without the browser's download bubble.
//!
//! A `.pch` is not handed over at all: not as a download, and not as a link
//! the player's browser would download instead. A replay is watched on the
//! site, whose player fetches the file itself, which is untouched by this.

use std::path::{Path, PathBuf};

use tauri::webview::DownloadEvent;
use tauri::{Runtime, Webview};
use url::Url;

/// Whether `url` is a NEO replay file.
pub fn is_replay(url: &Url) -> bool {
    has_replay_extension(url.path())
}

fn has_replay_extension(name: &str) -> bool {
    Path::new(name)
        .extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case("pch"))
}

/// Answers a download: `false` refuses it, and `true` lets it go to the
/// `destination` the player picked.
pub fn handle<R: Runtime>(webview: &Webview<R>, event: DownloadEvent<'_>) -> bool {
    let DownloadEvent::Requested { url, destination } = event else {
        return true;
    };
    // The file's name is the page's `download` attribute or the server's
    // header, which is where a replay says what it is when its address
    // does not.
    if is_replay(&url) || has_replay_extension(&destination.to_string_lossy()) {
        return false;
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    fn url(value: &str) -> Url {
        Url::parse(value).unwrap()
    }

    #[test]
    fn a_replay_is_known_by_its_extension() {
        assert!(is_replay(&url(
            "https://r2.example/replay/30/30ca3f590dda85e21dbc94250199a692.pch"
        )));
        assert!(is_replay(&url("https://oeee.cafe/replay/a/B.PCH?x=1")));
        assert!(has_replay_extension(r"C:\Users\p\Downloads\drawing.pch"));
    }

    #[test]
    fn a_picture_is_not_a_replay() {
        assert!(!is_replay(&url("https://oeee.cafe/@oeee/post.png")));
        assert!(!is_replay(&url("https://oeee.cafe/pch")));
        assert!(!is_replay(&url("https://oeee.cafe/replay/pch/")));
        assert!(!has_replay_extension(r"C:\Users\p\Downloads\collaboration.png"));
    }
}
