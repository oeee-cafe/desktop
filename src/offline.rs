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
//! the gateway's page in the body, or on no answer does nothing at all; so a
//! page navigation htmx cannot complete is made again as an ordinary one
//! (`PAGE_SCRIPT`), which the window sees fail.

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
        query.append_pair("page", page.as_str()).append_pair("offline", "1");
    }
    back
}

/// Runs in every page of the site: a page navigation htmx could not complete
/// is made again as an ordinary one.
pub fn page_script() -> String {
    let statuses = GATEWAY
        .iter()
        .map(u16::to_string)
        .collect::<Vec<_>>()
        .join(", ");
    format!(
        r#"(function () {{
  var GATEWAY = [{statuses}];
  // A page, as htmx fetches one: the whole body, by GET -- a boosted link
  // or search. Anything smaller is a part of the page, and its failure is
  // the page's to show.
  function page(ctx) {{
    return !!ctx && ctx.target === document.body && !!ctx.request &&
      String(ctx.request.method).toUpperCase() === "GET";
  }}
  function again(ctx) {{
    location.assign(ctx.request.action);
  }}
  document.addEventListener("htmx:before:swap", function (event) {{
    var ctx = event.detail && event.detail.ctx;
    if (!page(ctx) || !ctx.response || GATEWAY.indexOf(ctx.response.status) < 0) return;
    event.preventDefault();
    again(ctx);
  }});
  document.addEventListener("htmx:error", function (event) {{
    var ctx = event.detail && event.detail.ctx;
    if (!page(ctx) || ctx.response) return;
    again(ctx);
  }});
}})();"#
    )
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

    #[test]
    fn the_page_script_knows_the_same_statuses() {
        assert!(page_script().contains("var GATEWAY = [502, 503, 504, 520,"));
    }
}
