//! The player's Discord account, linked to the app: the tokens it is had by,
//! and where they are kept.
//!
//! The Oeee Cafe application is a confidential client -- the site signs
//! people in with it and keeps its secret -- so a code is traded for tokens,
//! and a refresh token for new ones, by the site (`/api/discord/token`,
//! src/discord.rs in oeee-cafe/web), which adds the secret and keeps
//! nothing. The SDK's own GetToken and RefreshToken are for public clients
//! only.
//!
//! The refresh token is what stays: Windows's Credential Manager holds it
//! for the user signed in to Windows, which is the place Windows has for a
//! secret an app keeps, and where Settings shows it. Access tokens last a
//! week and are only ever in memory.

use std::time::Duration;

use serde::Deserialize;
use url::Url;

/// How long the site gets to trade a token. It asks Discord in turn.
const TIMEOUT: Duration = Duration::from_secs(20);

#[derive(Clone, Debug, Deserialize)]
pub struct Tokens {
    pub access_token: String,
    pub refresh_token: String,
}

/// Why no tokens came back.
#[derive(Debug)]
pub enum Refused {
    /// Discord said no, through the site: the code or refresh token is no
    /// good, and asking again with it will not help.
    No,
    /// Nobody answered, or not sensibly, or not now -- the site too busy or
    /// not itself, or something in front of it saying no for it: worth
    /// asking again later.
    Unreachable(String),
}

/// Trades the code Discord's consent screen gave for tokens.
pub fn trade(site: &Url, code: &str, redirect: &str, verifier: &str) -> Result<Tokens, Refused> {
    ask(
        site,
        "api/discord/token",
        serde_json::json!({
            "grant_type": "authorization_code",
            "code": code,
            "redirect_uri": redirect,
            "code_verifier": verifier,
        }),
    )
}

/// New tokens for a refresh token, which the old one no longer works after.
pub fn refresh(site: &Url, refresh_token: &str) -> Result<Tokens, Refused> {
    ask(
        site,
        "api/discord/token",
        serde_json::json!({
            "grant_type": "refresh_token",
            "refresh_token": refresh_token,
        }),
    )
}

/// Tells Discord the app is done with the player's account, so it no longer
/// appears among their authorized apps. Best effort: the token is forgotten
/// here either way.
pub fn revoke(site: &Url, refresh_token: &str) {
    let Ok(url) = site.join("api/discord/revoke") else {
        return;
    };
    if let Err(error) = agent()
        .post(url.as_str())
        .send_json(serde_json::json!({ "token": refresh_token }))
    {
        eprintln!("Discord: could not revoke: {error}");
    }
}

fn ask(site: &Url, path: &str, body: serde_json::Value) -> Result<Tokens, Refused> {
    let url = site
        .join(path)
        .map_err(|error| Refused::Unreachable(error.to_string()))?;
    let mut response = agent()
        .post(url.as_str())
        .config()
        .http_status_as_error(false)
        .build()
        .send_json(body)
        .map_err(|error| Refused::Unreachable(error.to_string()))?;
    let status = response.status().as_u16();
    if (200..300).contains(&status) {
        return response
            .body_mut()
            .read_json::<Tokens>()
            .map_err(|error| Refused::Unreachable(error.to_string()));
    }
    let said = response.body_mut().read_to_string().unwrap_or_default();
    if refused(status, &said) {
        Err(Refused::No)
    } else {
        Err(Refused::Unreachable(format!("{status}: {said}")))
    }
}

/// Whether the site's answer is Discord's no to the grant itself, the one
/// answer that means the code or refresh token will never be any good, and
/// a kept account is forgotten for it. The site says it with a 400 and its
/// own words for it, or passes on Discord's `invalid_grant`. Anything else
/// -- 429 for too many asks, the site down, a proxy's 401 or 403 in front
/// of it, a 404 from a site with no Discord to trade with -- says nothing of
/// the grant, and is asked again later.
fn refused(status: u16, said: &str) -> bool {
    status == 400 && (said.contains("Discord would not trade") || said.contains("invalid_grant"))
}

fn agent() -> ureq::Agent {
    let config = ureq::Agent::config_builder().timeout_global(Some(TIMEOUT));
    // Windows' own TLS, trusting what Windows trusts (Cargo.toml).
    #[cfg(windows)]
    let config = config.tls_config(
        ureq::tls::TlsConfig::builder()
            .provider(ureq::tls::TlsProvider::NativeTls)
            .root_certs(ureq::tls::RootCerts::PlatformVerifier)
            .build(),
    );
    config.build().into()
}

pub use kept::{forget, keep, kept};

/// The credential's name for `site`, as Credential Manager lists it: the
/// name it has always had for oeee.cafe, and the site's origin after it for
/// any other a development build is pointed at (`OEEE_CAFE_URL`), so that
/// the account connected through one site is never sent to another.
#[cfg_attr(not(windows), allow(dead_code))]
fn target(site: &Url) -> String {
    const TARGET: &str = "cafe.oeee/discord";
    if crate::site::is_production(site) {
        TARGET.to_string()
    } else {
        format!("{TARGET}@{}", site.origin().ascii_serialization())
    }
}

/// The refresh token in Windows's Credential Manager.
#[cfg(windows)]
mod kept {
    use url::Url;
    use windows::core::{HSTRING, PCWSTR, PWSTR};
    use windows::Win32::Security::Credentials::{
        CredDeleteW, CredFree, CredReadW, CredWriteW, CREDENTIALW, CRED_FLAGS,
        CRED_PERSIST_LOCAL_MACHINE, CRED_TYPE_GENERIC,
    };

    use super::target;

    /// Keeps `refresh_token` for `site`. An empty one is no token, and would
    /// only be read back as none.
    pub fn keep(site: &Url, refresh_token: &str) {
        if refresh_token.is_empty() {
            return;
        }
        let target = HSTRING::from(target(site));
        let mut blob = refresh_token.as_bytes().to_vec();
        let credential = CREDENTIALW {
            Flags: CRED_FLAGS(0),
            Type: CRED_TYPE_GENERIC,
            TargetName: PWSTR(target.as_ptr() as *mut u16),
            CredentialBlobSize: blob.len() as u32,
            CredentialBlob: blob.as_mut_ptr(),
            // This user on this computer, not roamed to others: the token is
            // the app's on this computer, as the setting that made it says.
            Persist: CRED_PERSIST_LOCAL_MACHINE,
            ..Default::default()
        };
        if let Err(error) = unsafe { CredWriteW(&credential, 0) } {
            eprintln!("Discord: could not keep the account: {error}");
        }
    }

    pub fn kept(site: &Url) -> Option<String> {
        let target = HSTRING::from(target(site));
        let mut found: *mut CREDENTIALW = std::ptr::null_mut();
        unsafe {
            CredReadW(PCWSTR(target.as_ptr()), CRED_TYPE_GENERIC, None, &mut found).ok()?;
            let credential = &*found;
            // A credential with no blob has a null pointer for it, which no
            // slice may be made from, even an empty one.
            let size = credential.CredentialBlobSize as usize;
            let token = if credential.CredentialBlob.is_null() || size == 0 {
                None
            } else {
                let blob = std::slice::from_raw_parts(credential.CredentialBlob, size);
                String::from_utf8(blob.to_vec()).ok()
            };
            CredFree(found.cast());
            token.filter(|token| !token.is_empty())
        }
    }

    pub fn forget(site: &Url) {
        let target = HSTRING::from(target(site));
        let _ = unsafe { CredDeleteW(PCWSTR(target.as_ptr()), CRED_TYPE_GENERIC, None) };
    }
}

/// Anywhere else -- Linux, for now -- nowhere: the player connects again
/// each time the app starts.
#[cfg(not(windows))]
mod kept {
    use url::Url;

    pub fn keep(_site: &Url, _refresh_token: &str) {}

    pub fn kept(_site: &Url) -> Option<String> {
        None
    }

    pub fn forget(_site: &Url) {}
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_discords_no_to_the_grant_forgets_it() {
        let site_said =
            r#"{"error":{"code":"VALIDATION_ERROR","message":"Discord would not trade that"}}"#;
        assert!(refused(400, site_said));
        assert!(refused(400, r#"{"error":"invalid_grant"}"#));
        for (status, said) in [
            (400, "Bad Request"),
            (401, r#"{"error":"invalid_grant"}"#),
            (403, "Forbidden"),
            (404, "Discord not found"),
            (429, "Too Many Requests"),
            (500, "Something went wrong"),
            (502, "Bad Gateway"),
            (503, site_said),
        ] {
            assert!(!refused(status, said), "{status} {said}");
        }
    }

    #[test]
    fn the_account_is_kept_by_the_site_it_was_connected_through() {
        let target = |site: &str| target(&Url::parse(site).unwrap());
        assert_eq!(target("https://oeee.cafe/"), "cafe.oeee/discord");
        assert_eq!(
            target("https://oeee.test/"),
            "cafe.oeee/discord@https://oeee.test"
        );
        assert_eq!(
            target("http://127.0.0.1:8765/"),
            "cafe.oeee/discord@http://127.0.0.1:8765"
        );
        assert_eq!(
            target("http://oeee.cafe/"),
            "cafe.oeee/discord@http://oeee.cafe"
        );
    }
}
