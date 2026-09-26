//! When the site stops answering while the app is open.
//!
//! The loader (`loader/`) already says so when the site cannot be reached
//! at start. Later, a page that fails to load would leave whatever the
//! webview or the gateway in front of the site put in its place: WebView2's
//! own error page, or Cloudflare's "Bad gateway", which is not the app's
//! and offers no way back. Instead the window goes back to the loader, in
//! its "can't be reached" state, and trying again opens the page the player
//! was going to.
//!
//! A page is taken to have failed when nothing answered for it, or when the
//! gateway answered in the site's place (`GATEWAY`). An error page of the
//! site's own -- a 404, a 500 -- is the site speaking, and is shown.
//!
//! htmx fetches the site's pages itself, and on an answer like that it puts
//! the gateway's page in the body, or on no answer does nothing at all. The
//! site makes such a page navigation again as an ordinary one, knowing the
//! same statuses as `GATEWAY` (theme_head.jinja in oeee-cafe/web), and that
//! is the one the window sees fail.

use url::Url;

/// The statuses a gateway answers with when the site behind it cannot:
/// nginx's and every proxy's 502, 503 and 504, and Cloudflare's own 52x and
/// 530.
const GATEWAY: &[u16] = &[502, 503, 504, 520, 521, 522, 523, 524, 525, 526, 527, 530];

pub fn gateway_answered(status: u16) -> bool {
    GATEWAY.contains(&status)
}

/// The loader in its "can't be reached" state, set to go to `page` once the
/// site answers again.
pub fn loader_for(loader: &Url, page: &Url) -> Url {
    let mut back = loader.clone();
    let site = loader
        .query_pairs()
        .find(|(key, _)| key == "site")
        .map(|(_, value)| value.into_owned());
    {
        let mut query = back.query_pairs_mut();
        query.clear();
        if let Some(site) = site {
            query.append_pair("site", &site);
        }
        query
            .append_pair("page", page.as_str())
            .append_pair("offline", "1");
    }
    back
}

/// Sends a page of the site that could not be reached back to the loader,
/// which says so and tries it again. `loader` is the loader's path in the
/// app, as the window was opened on.
#[cfg(windows)]
pub fn attach(window: &tauri::WebviewWindow, site: &Url, loader: &str) -> tauri::Result<()> {
    // Where WebView2 serves the bundled loader from; the window has not
    // loaded it yet, so it cannot be asked.
    let loader = Url::parse("http://tauri.localhost/")
        .and_then(|origin| origin.join(loader))
        .expect("the loader's address is a valid URL");
    let (unreachable_window, site) = (window.clone(), site.clone());
    window.with_webview(move |webview| {
        crate::webview2::on_unreachable(&webview, move |page| {
            let Ok(page) = Url::parse(&page) else {
                return;
            };
            if crate::site::is_site(&page, &site) {
                let _ = unreachable_window.navigate(loader_for(&loader, &page));
            }
        });
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn url(value: &str) -> Url {
        Url::parse(value).unwrap()
    }

    #[test]
    fn a_gateway_speaking_for_the_site_is_the_site_unreachable() {
        assert!(gateway_answered(502));
        assert!(gateway_answered(522));
        assert!(!gateway_answered(500));
        assert!(!gateway_answered(404));
        assert!(!gateway_answered(200));
    }

    #[test]
    fn the_loader_goes_back_to_the_page() {
        let loader = url("http://tauri.localhost/index.html?site=https%3A%2F%2Foeee.cafe%2F");
        let back = loader_for(&loader, &url("https://oeee.cafe/communities?page=2"));
        let query: Vec<(String, String)> = back.query_pairs().into_owned().collect();
        assert_eq!(back.path(), "/index.html");
        assert_eq!(
            query,
            vec![
                ("site".into(), "https://oeee.cafe/".into()),
                ("page".into(), "https://oeee.cafe/communities?page=2".into()),
                ("offline".into(), "1".into()),
            ]
        );
    }
}
