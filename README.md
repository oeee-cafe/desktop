# Oeee Cafe for Steam

A Tauri app that opens oeee.cafe in its own window. It holds no copy of the
site, so what it shows is always what is deployed.

- It opens on `loader/index.html`, which checks the site answers and shows a
  "can't be reached" page with a retry button when it does not.
- Links on the site stay in the window; links anywhere else, including
  `target="_blank"` ones, open in the player's browser.
- The site is given no access to Tauri's APIs (`capabilities/default.json`
  names only the core defaults). Don't add `tauri-plugin-dialog`: it
  replaces `window.confirm` on every page with one that returns a Promise,
  which htmx takes as "yes".
- A page that would stop a browser leaving it (the painter, with an unsaved
  drawing) is asked about before the window closes or the app quits, and the
  player can stay.
- The browser shows through as little as it can: no right-click menu except
  on text fields, selections and images; on Windows, no F5, Ctrl+F, Ctrl+P,
  autofill suggestions or offer to save a password (`src/webview2.rs`); and
  a window background matching the site's theme, so a load does not flash.
- On macOS, `alert()`, `confirm()` and the `beforeunload` prompt are native
  dialogs (`src/macos.rs`). WKWebView shows none of them on its own, and
  wry's delegate leaves them out.

## Signing in with Steam

Started by Steam, the app marks every page `data-steam-app`, and the site's
sign-in page shows "Sign in with Steam" (the account page, "Link your Steam
account"). That link goes to `/auth/steam/app`. The app stops the
navigation, asks Steam for a Web API ticket (`GetAuthTicketForWebApi`, with
the identity `oeee-cafe`) and posts it to `/auth/steam` from the page, as a
form on the page would. The site checks it with Steam itself, so the app is
trusted with nothing (`src/steam.rs`).

Without Steam, the app starts as before and the link never shows. Steam's
library still has to be beside the binary: the app links it and will not
start without it. `build.rs` puts it in `target/<profile>/`, and
`tauri.conf.json` puts it in the macOS and Linux bundles; see the table below
for Windows. The copies in `steam/redistributable/` are the ones the
`steamworks` crate was built against; replace them together with it.

To have Steam start a development build, put the app id in a
`steam_appid.txt` in the directory you run it from.

## Rich presence

The site says what each page is in `<meta name="oeee-presence">`: drawing,
a relay, drawing together in a collaborative room, a banner, a replay, and
the community when it is public. After every page load the app reads it and
sets `steam_display` to a token of `steam/rich_presence.vdf`, with
`%community%` filled in; a collaborative room also sets
`steam_player_group`, so friends in one room show together. A page without
the tag is browsing. Upload `steam/rich_presence.vdf` in Steamworks
(Community > Rich Presence Localization) whenever it changes; a token the
app sets that Steam has not been given shows nothing.

The site itself is [oeee-cafe/web](https://github.com/oeee-cafe/web); the
other clients are [oeee-cafe/ios](https://github.com/oeee-cafe/ios) and
[oeee-cafe/android](https://github.com/oeee-cafe/android).

## Running it

```bash
cargo install tauri-cli --version "^2" --locked   # once
cargo run                                         # against oeee.cafe
OEEE_CAFE_URL=https://oeee.test/ cargo run        # against a local server
cargo test
```

## Building for Steam

Bundles are built on the platform they are for:

```bash
cargo tauri build
```

| Platform | Bundle | What goes in `steam/content/<platform>/` |
| --- | --- | --- |
| Windows | `target/release/oeee-cafe-desktop.exe` | the `.exe` and `steam/redistributable/win64/steam_api64.dll` beside it; WebView2 ships with Windows 10 and 11 |
| macOS | `target/release/bundle/macos/Oeee Cafe.app` | the `.app` |
| Linux | `target/release/bundle/appimage/*.AppImage` | the AppImage |

Steam installs files; it does not run installers, so ship the executable
(Windows) and the `.app` (macOS) rather than the NSIS installer or a `.dmg`.
Set each depot's launch option in Steamworks (Installation > General) to the
file inside it.

Then upload:

```bash
STEAM_APP_ID=... STEAM_USER=... \
STEAM_DEPOT_WINDOWS=... STEAM_DEPOT_MACOS=... STEAM_DEPOT_LINUX=... \
./steam/upload.sh "0.1.0"
```

`steamcmd` asks for the password and Steam Guard code itself. Pick the branch
the build goes live on in Steamworks (SteamPipe > Builds).

## Not done yet

- **Leaving the painter on Windows and Linux.** Tested on macOS only. WebView2
  and WebKitGTK are expected to show `beforeunload` and `confirm()`
  themselves, and the close and quit question there goes through `rfd`; none
  of it has run on either yet.
- **Downloads.** Saving an image or `.pch` from the site has not been tried in
  the webview.
- **Icons.** Generated from the 256px `static/favicon.png` in oeee-cafe/web; regenerate from a
  1024px source with `cargo tauri icon <file>` before release.
- **macOS signing and notarisation**, needed for the `.app` to open without a
  Gatekeeper warning.
