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
| Windows | `target/release/oeee-cafe-desktop.exe` | the `.exe` alone; WebView2 ships with Windows 10 and 11 |
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

- **Steam sign-in.** Needs the Steamworks SDK in the app and a server
  endpoint that checks the ticket with `ISteamUserAuth/AuthenticateUserTicket`.
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
