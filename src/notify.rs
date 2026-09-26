//! A new notification as Windows's own: the `notify` message (bridge.rs).
//!
//! The phones and the Mac are sent a push for every notification; Windows is
//! sent none, so the site hands the app the same words over the bridge
//! whenever it hears one live (live.jinja in oeee-cafe/web), and shows them
//! as a toast of its own too, as it does in every app.
//!
//! Shown as a toast. Pressed, it brings the window forward and goes to the
//! notification's page the way a link in the page would -- so a page holding
//! a drawing asks first (leave.rs) -- which is only while the app is running,
//! the one time the site can say anything to it; no activator is registered
//! for a toast pressed after the app has closed, and it then does nothing.
//!
//! A toast needs an app user model id to be shown under. The Microsoft
//! Store's build has its package's; Steam's is not packaged, so it registers
//! one for the user (HKCU\Software\Classes\AppUserModelId), Windows's own way
//! for an unpackaged app, with the name the Action Center shows it under.

use url::Url;

/// The id the Steam build's toasts are shown under: the app's identifier
/// (tauri.conf.json).
#[cfg_attr(not(windows), allow(dead_code))]
const APP_ID: &str = "cafe.oeee";

/// Where a notification may take the window: a path on the site, and
/// nowhere else, whatever the message says.
pub fn destination(site: &Url, path: &str) -> Option<Url> {
    if !path.starts_with('/') || path.starts_with("//") || path.contains('\\') {
        return None;
    }
    let url = site.join(path).ok()?;
    (url.origin() == site.origin()).then_some(url)
}

/// The toast's XML, the words escaped.
#[cfg_attr(not(windows), allow(dead_code))]
fn toast_xml(title: &str, body: &str) -> String {
    format!(
        concat!(
            r#"<toast><visual><binding template="ToastGeneric">"#,
            "<text>{}</text><text>{}</text>",
            "</binding></visual></toast>"
        ),
        escape(title),
        escape(body)
    )
}

#[cfg_attr(not(windows), allow(dead_code))]
fn escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

/// Shows the notification, in the background: nothing here may hold up the
/// bridge, and a toast that cannot be shown is only logged.
#[cfg_attr(not(windows), allow(unused_variables))]
pub fn show(window: &tauri::WebviewWindow, site: &Url, title: String, body: String, path: String) {
    let Some(url) = destination(site, &path) else {
        eprintln!("a notification pointed somewhere it may not go: {path}");
        return;
    };
    #[cfg(windows)]
    {
        let window = window.clone();
        std::thread::spawn(move || {
            if let Err(error) = windows_toast::show(window, &title, &body, url) {
                eprintln!("could not show a notification: {error}");
            }
        });
    }
}

#[cfg(windows)]
mod windows_toast {
    use tauri::WebviewWindow;
    use url::Url;
    use windows::core::HSTRING;
    use windows::Data::Xml::Dom::XmlDocument;
    use windows::Foundation::TypedEventHandler;
    use windows::Win32::Foundation::APPMODEL_ERROR_NO_PACKAGE;
    use windows::Win32::Storage::Packaging::Appx::GetCurrentPackageFullName;
    use windows::Win32::System::Registry::{RegSetKeyValueW, HKEY_CURRENT_USER, REG_SZ};
    use windows::UI::Notifications::{ToastNotification, ToastNotificationManager};

    use super::{toast_xml, APP_ID};

    fn packaged() -> bool {
        let mut length = 0u32;
        unsafe { GetCurrentPackageFullName(&mut length, None) != APPMODEL_ERROR_NO_PACKAGE }
    }

    /// The Steam build's id, with the name Windows shows its toasts under.
    /// Written every time: it is two values, and costs less than asking.
    fn register() -> windows::core::Result<()> {
        let key = HSTRING::from(format!(r"Software\Classes\AppUserModelId\{APP_ID}"));
        let name: Vec<u16> = "Oeee Cafe\0".encode_utf16().collect();
        unsafe {
            RegSetKeyValueW(
                HKEY_CURRENT_USER,
                &key,
                &HSTRING::from("DisplayName"),
                REG_SZ.0,
                Some(name.as_ptr().cast()),
                (name.len() * 2) as u32,
            )
            .ok()
        }
    }

    pub fn show(
        window: WebviewWindow,
        title: &str,
        body: &str,
        url: Url,
    ) -> windows::core::Result<()> {
        let xml = XmlDocument::new()?;
        xml.LoadXml(&HSTRING::from(toast_xml(title, body)))?;
        let toast = ToastNotification::CreateToastNotification(&xml)?;
        toast.Activated(&TypedEventHandler::new(move |_, _| {
            let _ = window.unminimize();
            let _ = window.show();
            let _ = window.set_focus();
            // As a link in the page goes, so a drawing in progress is asked
            // about first.
            let path = serde_json::to_string(url.as_str()).unwrap_or_default();
            let _ = window.eval(format!("location.assign({path});"));
            Ok(())
        }))?;
        let notifier = if packaged() {
            ToastNotificationManager::CreateToastNotifier()?
        } else {
            register()?;
            ToastNotificationManager::CreateToastNotifierWithId(&HSTRING::from(APP_ID))?
        };
        notifier.Show(&toast)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn site() -> Url {
        Url::parse("https://oeee.cafe/").unwrap()
    }

    #[test]
    fn a_notification_goes_only_to_a_page_of_the_site() {
        assert_eq!(
            destination(&site(), "/@artist/9c881320").map(String::from),
            Some("https://oeee.cafe/@artist/9c881320".to_string())
        );
        assert_eq!(destination(&site(), "https://evil.example/"), None);
        assert_eq!(destination(&site(), "//evil.example/"), None);
        assert_eq!(destination(&site(), "/\\evil.example/"), None);
        assert_eq!(destination(&site(), "javascript:alert(1)"), None);
    }

    #[test]
    fn the_words_are_text_in_the_toast() {
        let xml = toast_xml("<b>tandemaus</b>", "commented on \"오이\" & more");
        assert!(xml.contains("<text>&lt;b&gt;tandemaus&lt;/b&gt;</text>"));
        assert!(xml.contains("<text>commented on &quot;오이&quot; &amp; more</text>"));
    }
}
