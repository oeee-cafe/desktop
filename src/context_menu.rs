//! What the right-click menu keeps of WebView2's own.
//!
//! The browser's menu -- Back, Reload, Save as, Print, Create QR code, Inspect
//! -- is the plainest sign that a window is a browser. What earns its place is
//! what any program's menu has: Cut, Copy and Paste in a text field, with its
//! spelling suggestions and emoji; Copy on a selection; Save and Copy on a
//! picture, which is most of what the site is. A link gets the app's own Copy
//! link (webview2.rs). Anything else goes, and a menu with nothing left in it
//! is not shown at all.
//!
//! WebView2 names its items by their English label in lower camel case
//! ("Save image as" is `saveImageAs`) and every spelling suggestion
//! `spellCheck`, so an item a later WebView2 adds is left out until it is
//! named here.

/// An item of WebView2's menu, as far as deciding goes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Item<'a> {
    Separator,
    Named(&'a str),
}

fn wanted(name: &str) -> bool {
    matches!(
        name,
        "cut"
            | "copy"
            | "paste"
            | "pasteAsPlainText"
            | "pasteAndMatchStyle"
            | "undo"
            | "redo"
            | "selectAll"
            | "spellCheck"
            | "emoji"
            | "saveImageAs"
            | "copyImage"
    ) || (cfg!(debug_assertions) && name == "inspectElement")
}

/// Which of `items`, by index, stay: the wanted ones, with a separator only
/// between two groups that both kept something.
pub fn kept(items: &[Item]) -> Vec<usize> {
    let mut keep = Vec::new();
    let mut pending_separator = None;
    for (index, item) in items.iter().enumerate() {
        match item {
            Item::Separator => {
                if !keep.is_empty() {
                    pending_separator = Some(index);
                }
            }
            Item::Named(name) if wanted(name) => {
                if let Some(separator) = pending_separator.take() {
                    keep.push(separator);
                }
                keep.push(index);
            }
            Item::Named(_) => {}
        }
    }
    keep
}

/// The link a Copy link item copies: the site's pages and other sites', not
/// a script or a `mailto:`.
pub fn copyable_link(link: &str) -> Option<&str> {
    let link = link.trim();
    (link.starts_with("https://") || link.starts_with("http://")).then_some(link)
}

#[cfg(test)]
mod tests {
    use super::*;
    use Item::{Named, Separator};

    #[test]
    fn the_browser_goes() {
        let page = [
            Named("back"),
            Named("forward"),
            Named("reload"),
            Separator,
            Named("saveAs"),
            Named("print"),
            Named("createQrCode"),
            Separator,
            Named("inspectElement"),
        ];
        let kept = kept(&page);
        if cfg!(debug_assertions) {
            assert_eq!(kept, vec![8]);
        } else {
            assert!(kept.is_empty());
        }
    }

    #[test]
    fn a_picture_keeps_save_and_copy() {
        let image = [
            Named("openImageInNewWindow"),
            Named("saveImageAs"),
            Named("copyImage"),
            Named("copyImageLink"),
            Separator,
            Named("createQrCode"),
        ];
        assert_eq!(kept(&image), vec![1, 2]);
    }

    #[test]
    fn a_field_keeps_editing_and_spelling() {
        let field = [
            Named("spellCheck"),
            Named("spellCheck"),
            Separator,
            Named("undo"),
            Named("redo"),
            Separator,
            Named("cut"),
            Named("copy"),
            Named("paste"),
            Named("writingDirection"),
            Separator,
            Named("selectAll"),
        ];
        assert_eq!(kept(&field), vec![0, 1, 2, 3, 4, 5, 6, 7, 8, 10, 11]);
    }

    #[test]
    fn separators_never_lead_trail_or_double() {
        let menu = [
            Separator,
            Named("print"),
            Separator,
            Named("copy"),
            Separator,
            Separator,
            Named("reload"),
        ];
        assert_eq!(kept(&menu), vec![3]);
    }

    #[test]
    fn only_web_links_are_copied() {
        assert_eq!(
            copyable_link("https://oeee.cafe/@someone"),
            Some("https://oeee.cafe/@someone")
        );
        assert_eq!(copyable_link("javascript:void(0)"), None);
        assert_eq!(copyable_link("mailto:a@example.com"), None);
    }
}
