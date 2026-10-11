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
    /// The site or Discord said no: the code or refresh token is no good,
    /// and asking again with it will not help.
    No,
    /// Nobody answered, or not sensibly: worth asking again later.
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
    match agent().post(url.as_str()).send_json(body) {
        Ok(mut response) => response
            .body_mut()
            .read_json::<Tokens>()
            .map_err(|error| Refused::Unreachable(error.to_string())),
        Err(ureq::Error::StatusCode(400 | 401 | 403)) => Err(Refused::No),
        Err(error) => Err(Refused::Unreachable(error.to_string())),
    }
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

/// The refresh token in Windows's Credential Manager.
#[cfg(windows)]
mod kept {
    use windows::core::{HSTRING, PCWSTR, PWSTR};
    use windows::Win32::Security::Credentials::{
        CredDeleteW, CredFree, CredReadW, CredWriteW, CREDENTIALW, CRED_FLAGS,
        CRED_PERSIST_LOCAL_MACHINE, CRED_TYPE_GENERIC,
    };

    /// The credential's name, as Credential Manager lists it.
    const TARGET: &str = "cafe.oeee/discord";

    pub fn keep(refresh_token: &str) {
        let target = HSTRING::from(TARGET);
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

    pub fn kept() -> Option<String> {
        let target = HSTRING::from(TARGET);
        let mut found: *mut CREDENTIALW = std::ptr::null_mut();
        unsafe {
            CredReadW(PCWSTR(target.as_ptr()), CRED_TYPE_GENERIC, None, &mut found).ok()?;
            let credential = &*found;
            let blob = std::slice::from_raw_parts(
                credential.CredentialBlob,
                credential.CredentialBlobSize as usize,
            );
            let token = String::from_utf8(blob.to_vec()).ok();
            CredFree(found.cast());
            token.filter(|token| !token.is_empty())
        }
    }

    pub fn forget() {
        let target = HSTRING::from(TARGET);
        let _ = unsafe { CredDeleteW(PCWSTR(target.as_ptr()), CRED_TYPE_GENERIC, None) };
    }
}

/// Anywhere else -- a developer's build -- nowhere: the player connects
/// again each time the app starts.
#[cfg(not(windows))]
mod kept {
    pub fn keep(_refresh_token: &str) {}

    pub fn kept() -> Option<String> {
        None
    }

    pub fn forget() {}
}
