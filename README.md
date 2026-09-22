# Oeee Cafe for Steam

A Tauri app that opens oeee.cafe in its own window. It holds no copy of the
site, so what it shows is always what is deployed.

- It opens on `loader/index.html`, which checks the site answers and shows a
  "can't be reached" page with a retry button when it does not. On Windows a
  page that fails later -- nothing answering, or a gateway's 502, 503, 504 or
  Cloudflare 52x in the site's place -- goes back there too, and trying
  again opens the page the player was going to (`src/offline.rs`). The
  site's own error pages, a 404 or a 500, are shown as they are.
- Links on the site stay in the window; links anywhere else, including
  `target="_blank"` ones, open in the player's browser.
- The site is given no access to Tauri's APIs (`capabilities/default.json`
  names only the core defaults). Don't add `tauri-plugin-dialog`: it
  replaces `window.confirm` on every page with one that returns a Promise,
  which htmx takes as "yes".
- A page that would stop a browser leaving it (the painter, with an unsaved
  drawing) is asked about before the window closes, and the player can stay;
  only the Leave button leaves, not Esc or the dialog's close box.
- A download is saved where the player says, in the system's Save dialog
  (`src/downloads.rs`). A `.pch` replay is never handed over: not as a
  download, and not as a link for the browser to download.
- The browser shows through as little as it can: no right-click menu except
  on text fields, selections and images; on Windows, no browser find bar,
  print preview, autofill suggestions or offer to save a password
  (`src/webview2.rs`); and a window background matching the site's theme,
  so a load does not flash.
- On Windows the right-click menu is WebView2's trimmed to what a program's
  has -- Cut, Copy, Paste and spelling in a field, Copy on a selection, Save
  and Copy on a picture -- and a link's is the app's own Copy link
  (`src/context_menu.rs`). The page's `alert()`, `confirm()` and leaving a
  page with unsaved work are the system's dialogs, as closing the window is
  (`src/dialogs.rs`), not WebView2's "oeee.cafe says".
- On Windows the keys a program answers to are the app's (`src/keys.rs`):
  Alt+Left and Alt+Right, F5 and Ctrl+R, the keyboard's Back, Forward and
  Refresh keys, and Ctrl+W, which asks over a drawing as the close button
  does. Ctrl+F is the site's search, Ctrl+N a new drawing, Ctrl+, the
  account page, Ctrl+1 to Ctrl+5 the site's sections and Ctrl+/ its list of
  shortcuts.
- On Windows the site's toolbar is the title bar: the window has none of its
  own, and minimise, maximise and close are drawn at the toolbar's end
  (`src/caption.js`), with Snap Layouts on maximise (`src/snap.rs`).

There is no macOS build: on the Mac, Oeee Cafe is the
[iOS app](https://github.com/oeee-cafe/ios) as a universal app.

## Signing in with Steam

Started by Steam, the app marks every page `data-steam-app`, and the site's
sign-in page shows "Sign in with Steam" (the account page, "Link your Steam
account"). That link goes to `/auth/steam/app`. The app stops the
navigation, asks Steam for a Web API ticket (`GetAuthTicketForWebApi`, with
the identity `oeee-cafe`) and posts it to `/auth/steam` from the page, as a
form on the page would. The site checks it with Steam itself, so the app is
trusted with nothing (`src/steam.rs`).

When Steam says a DLC has been installed (`DlcInstalled_t`: the Supporter
Pack, bought in the overlay or the store while the app is open), the app
gets a fresh ticket and posts it in the background to `/auth/steam/refresh`
from the page showing. The site asks Steam what that account owns and
records it, so the supporter badge shows at once rather than at the site's
daily recheck. It signs nobody in and moves no page, so a drawing in
progress is left alone. The crate does not wrap that callback, so
`steamworks` is built with `raw-bindings` for `src/steam/client.rs` to
register it.

Without Steam, the app starts as before and the link never shows. Steam's
library still has to be beside the binary: the app links it and will not
start without it. `build.rs` puts it in `target/<profile>/`, and
`tauri.conf.json` puts it in the Linux bundle; see the table below
for Windows. The copies in `steam/redistributable/` are the ones the
`steamworks` crate was built against; replace them together with it.

To have Steam start a development build, put the app id in a
`steam_appid.txt` in the directory you run it from.

Everything Steam is behind the `steam` feature, on by default. A build
without it (`--no-default-features`, as for the Microsoft Store) neither
links nor needs Steam's library, and never marks a page `data-steam-app`.

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

## Achievements

First drawing, first relay, first collaboration and buying the app on
Steam are unlocked by the site, not the app: it records them however the player drew, and sends them
to Steam with the publisher key (`SetUserStatsForGame`). Linking Steam hands
over everything already earned. `STEAM_SUPPORTER` goes to a Steam account
that owns the app outright (`CheckAppOwnership`: not a Family Sharing loan,
a free weekend or a site licence), checked at each Steam sign-in. In Steamworks (Stats & Achievements) the
API names are `FIRST_DRAWING`, `FIRST_RELAY`, `FIRST_COLLABORATION` and
`STEAM_SUPPORTER`, each
"Set By: Official GS" so that only the server can unlock them.

The icons Steamworks asks for are in `steam/achievements/`: `<NAME>.jpg`
for earned and `<NAME>_locked.jpg` for not yet, 64 by 64, one pair per API
name. They are the profile's Material Symbols badges (Apache 2.0, Google),
drawn by `steam/achievements/render.mjs` from the path data in `icons.json`;
run it again after changing either.

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
| Linux | `target/release/bundle/appimage/*.AppImage` | the AppImage |

Steam installs files; it does not run installers, so ship the executable
rather than the NSIS installer. Set each depot's launch option in Steamworks
(Installation > General) to the file inside it.

Then upload:

```bash
STEAM_APP_ID=... STEAM_USER=... \
STEAM_DEPOT_WINDOWS=... STEAM_DEPOT_LINUX=... \
./steam/upload.sh "0.1.0"
```

`steamcmd` asks for the password and Steam Guard code itself. Pick the branch
the build goes live on in Steamworks (SteamPipe > Builds).

## Building for the Microsoft Store

The Store gets an MSIX bundle of the app without Steam: an x64 package and
an ARM64 one, of which Windows installs the one that suits the machine.
`msstore/package.ps1` packs it on Windows from the manifest in
`msstore/AppxManifest.xml` and the images in `msstore/Assets/`. It needs
Rust with both Windows targets, the Tauri CLI, Visual Studio's C++ build
tools for x64 and ARM64 (the "MSVC ... ARM64 build tools" component), the
Windows SDK, and the package's identity from Partner Center (Product
management > Product identity):

```powershell
rustup target add x86_64-pc-windows-msvc aarch64-pc-windows-msvc
$env:MSSTORE_IDENTITY_NAME = '...'
$env:MSSTORE_PUBLISHER = 'CN=...'
$env:MSSTORE_PUBLISHER_DISPLAY_NAME = '...'
.\msstore\package.ps1
```

The `.msixbundle` lands in `target\msstore\`, unsigned: upload it to the
submission's Packages page and the Store signs it. `-Architectures x64`
packs one architecture only, for trying it quicker. Its version is
`tauri.conf.json`'s with a `.0` after it, which the Store requires; raise
the version for each submission. To install it on your own machine first,
make a certificate with the Publisher value as its subject, trust it, and
sign with it:

```powershell
$cert = New-SelfSignedCertificate -Type Custom -Subject $env:MSSTORE_PUBLISHER `
  -KeyUsage DigitalSignature -CertStoreLocation Cert:\CurrentUser\My `
  -TextExtension @('2.5.29.37={text}1.3.6.1.5.5.7.3.3', '2.5.29.19={text}')
$env:MSSTORE_PFX_PASSWORD = '...'
Export-PfxCertificate $cert -FilePath dev.pfx -Password (ConvertTo-SecureString $env:MSSTORE_PFX_PASSWORD -AsPlainText -Force)
Import-Certificate -FilePath (Export-Certificate $cert -FilePath dev.cer).FullName -CertStoreLocation Cert:\LocalMachine\TrustedPeople  # as administrator
.\msstore\package.ps1 -Pfx dev.pfx
```

With Developer Mode on, the unpacked package can be tried without a
certificate: register its folder, open it from Start, and remove it after.

```powershell
Add-AppxPackage -Register target\msstore\layout-x64\AppxManifest.xml
Get-AppxPackage -Name $env:MSSTORE_IDENTITY_NAME | Remove-AppxPackage
```

Packaged, the app keeps WebView2's data and the window's place in the
package's own folder (`%LOCALAPPDATA%\Packages\<family>\LocalCache\`), so
uninstalling takes them with it -- unless `%LOCALAPPDATA%\cafe.oeee.desktop`
already exists, as it does where the app has run unpackaged: Windows lets
a package change folders it finds there, so on a development machine it
shares them with `cargo run` and the Steam build.

The images come from `icons/icon.png`; run `msstore/assets.sh` again after
changing it.

## Not done yet

- **Linux.** The build has not run. WebKitGTK is expected to show
  `beforeunload` itself, and closing the window and saving a download go
  through `rfd`'s GTK dialogs, as on Windows through its own. A page that
  fails mid-session is not caught there (`src/offline.rs` watches WebView2
  only), so it shows WebKitGTK's error page.
- **The keys, dialogs and right-click menu on Windows.** Type-checked and
  unit-tested from macOS (`cargo check --target x86_64-pc-windows-msvc`),
  not yet run. Worth trying: Alt+Left and Ctrl+R over an unsaved drawing
  (the system's leave question, then the right page), `alert()` from the
  Steam sign-in failure, and the menu on a drawing, a link, a field with a
  misspelling and empty space (none). The mouse's back and forward buttons
  were not looked at.
- **Leaving the real painter.** On Windows, leaving and closing were tried on
  a page with the painter's `beforeunload` handler, not in the painter
  itself, signed in.
- **The MSIX.** The x64 package has been packed, registered unsigned and
  run from Start. Not yet: the ARM64 package (it needs Visual Studio's ARM64
  build tools), a signed install, a Store submission, and a look at the
  taskbar icon and the unread dot in the packaged app.
- **Icons.** Generated from the 256px `static/favicon.png` in oeee-cafe/web; regenerate from a
  1024px source with `cargo tauri icon <file>` before release.
