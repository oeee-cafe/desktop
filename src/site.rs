//! Which site the app is a window onto, and what counts as being on it.

use url::Url;

const SITE: &str = "https://oeee.cafe/";

/// The site to open. `OEEE_CAFE_URL` points a development build somewhere
/// else, such as `https://oeee.test/`.
pub fn from_env() -> Url {
    std::env::var("OEEE_CAFE_URL")
        .ok()
        .and_then(|value| Url::parse(&value).ok())
        .unwrap_or_else(default)
}

fn default() -> Url {
    Url::parse(SITE).expect("SITE is a valid URL")
}

/// Whether `page` is one of the site's: the same scheme, host and port, and
/// so not the loader, its "can't be reached" page, or anywhere else. Every
/// decision that trusts a page -- that it may be posted to from, that its
/// failure is the site's -- is made by this one comparison.
pub fn is_site(page: &Url, site: &Url) -> bool {
    page.origin() == site.origin()
}

/// The address pattern a capability gives the site's pages by (main.rs):
/// the site and nothing else -- its origin, as [`is_site`] compares it.
pub fn pattern(site: &Url) -> String {
    format!("{}/*", site.origin().ascii_serialization())
}

#[cfg(test)]
pub fn for_tests() -> Url {
    default()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn url(value: &str) -> Url {
        Url::parse(value).unwrap()
    }

    #[test]
    fn the_pattern_covers_the_site_and_nothing_else() {
        assert_eq!(pattern(&url("https://oeee.cafe/")), "https://oeee.cafe/*");
        assert_eq!(
            pattern(&url("http://127.0.0.1:8765/")),
            "http://127.0.0.1:8765/*"
        );
    }

    #[test]
    fn only_the_sites_own_pages_are_the_site() {
        let site = for_tests();
        assert!(is_site(&url("https://oeee.cafe/draw"), &site));
        assert!(!is_site(&url("tauri://localhost/index.html"), &site));
        assert!(!is_site(&url("http://tauri.localhost/index.html"), &site));
        assert!(!is_site(&url("http://oeee.cafe/draw"), &site));
    }
}
