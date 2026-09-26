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
  (`src/downloads.rs`).
- The browser shows through as little as it can: no right-click menu except
  on text fields, selections and images; on Windows, no browser find bar,
  print preview, autofill suggestions or offer to save a password
  (`src/webview2.rs`); and a window background matching the site's theme,
  so a load does not flash.
- On Windows the right-click menu is WebView2's trimmed to what a program's
  has -- Cut, Copy, Paste and spelling in a field, Copy on a selection, Save
  and Copy on a picture -- and a link's is the app's own Copy link
  (`src/context_menu.rs`). Leaving a page with unsaved work is asked in the
  system's dialog, as closing the window is (`src/dialogs.rs`), not
  WebView2's "oeee.cafe says". WebView2's other script dialogs are off: the
  site asks everything else in its own, and calls neither `alert()` nor
  `confirm()`.
- On Windows the keys a program answers to are the app's (`src/keys.rs`):
  Alt+Left and Alt+Right, F5 and Ctrl+R, the keyboard's Back, Forward and
  Refresh keys, and Ctrl+W, which asks over a drawing as the close button
  does. Ctrl+F is the site's search, Ctrl+N a new drawing, Ctrl+, the
  account page, Ctrl+1 to Ctrl+5 the site's sections and Ctrl+/ its list of
  shortcuts. F11 is full screen, and Ctrl+Plus, Ctrl+Minus and Ctrl+0 make
  the window bigger, smaller and back, kept from launch to launch
  (`src/zoom.rs`).
- On Windows the site's toolbar is the title bar: the window has none of its
  own, and minimise, maximise and close are drawn at the toolbar's end by
  the site (`app_caption.jinja` in oeee-cafe/web), which asks for the
  window on the bridge (`src/chrome.rs`), with Snap Layouts on maximise
  (`src/snap.rs`).

There is no macOS build: on the Mac, Oeee Cafe is the
[iOS app](https://github.com/oeee-cafe/ios) as a universal app.

## Signing in with Steam

Started by Steam, the app ends its user agent with
`OeeeCafe platform/windows store/steam` rather than
`OeeeCafe platform/windows` alone, and
the site marks every page `data-store="steam"` from that before it paints.
Only then does its sign-in page show "Sign in with Steam" (the account
page, "Link your Steam account"). The page takes that press itself: it sends
the app `signIn` with the provider `steam` on the bridge, the app asks Steam
for a Web API ticket (`GetAuthTicketForWebApi`, with the identity
`oeee-cafe`) and hands it back by calling `oeeeApp.signIn.answer({ticket})`,
or `answer({})` when Steam would not give one, and the page posts it to the
site -- or tells the player, in its own words, that it could not. The site
checks the ticket with Steam itself, so the app is trusted with nothing, and
knows none of the site's routes (`src/steam.rs`; `app_sign_in.jinja` in
oeee-cafe/web).

## The Supporter Pack

The Supporter Pack is a DLC, and `/supporter` sells it the way every app's
page sells (`app_store.jinja` in oeee-cafe/web): pressing its button sends
`purchase` with the DLC's app id, and the app opens the overlay on that
DLC's store page. The buying happens there, where the app cannot see it,
so the app does not try to: when Steam says a DLC has been installed
(`DlcInstalled_t` -- the pack has no content, so owning it is installing
it), the app gets a fresh ticket and hands it to the page as proof, with
`oeeeApp.store.purchased([ticket])`. The page posts it, and the site asks
Steam what that account owns and records it, so the supporter badge shows
at once rather than at the site's daily recheck. It signs nobody in, and
the page reloads only when something was recorded. The crate does not wrap
that callback, so `steamworks` is built with `raw-bindings` for
`src/steam/client.rs` to register it.

Before that, `/supporter` asks for `prices`. Steamworks has no way to say
what a DLC costs, so the app asks Steam's public store API
(`store.steampowered.com/api/appdetails?appids=...&filters=price_overview`)
in the player's country, which Steamworks does know (`GetIPCountry`), and
answers `oeeeApp.store.prices` with Steam's own `final_formatted` for each
DLC -- "₩ 5,500" in Korea. It asks off the main thread, in one request for
every DLC, over Windows' own TLS on Windows and rustls elsewhere (`ureq`,
built only with the `steam` feature). A DLC Steam has no price for, or
Steam's store not answering, leaves the button without a price, which the
page allows; the failure is logged.

Without Steam, the app starts as before and the button never shows. Steam's
library still has to be beside the binary: the app links it and will not
start without it. `build.rs` puts it in `target/<profile>/`, and the depot
has it beside the `.exe` (below). The copy in `steam/redistributable/` is
the one the `steamworks` crate was built against; replace it along with
the crate.

To have Steam start a development build, put the app id in a
`steam_appid.txt` in the directory you run it from.

Everything Steam is behind the `steam` feature, on by default. A build
without it (`--no-default-features`, as for the Microsoft Store) neither
links nor needs Steam's library, never names Steam, and sells through the
Microsoft Store instead (below).

## The Supporter Pack in the Microsoft Store

In the Store's build the Supporter Pack is a durable add-on, sold through
`Windows.Services.Store` (`src/microsoft.rs`). Running as the Store's
package, the app ends its user agent with `OeeeCafe platform/windows
store/microsoft`, and the site marks every page `data-store="microsoft"`;
run unpackaged -- `cargo run --no-default-features` -- it names no store,
since the Store answers nothing to an app it cannot identify, and the page
offers nothing to buy.

The page names each add-on by its Store ID. `prices` is answered with each
one's price as the Store formats it (`GetStoreProductsAsync`), and
`purchase` shows the Store's own purchase dialog over the window
(`RequestPurchaseAsync`; a desktop app has to give the Store its window
with `IInitializeWithWindow` first). Closing the dialog buys nothing and
nothing is said. Once the player owns the add-on -- just bought, or already
-- the app proves it to the site the way the Store asks a service to:

1. it asks the page for `oeeeApp.store.ticket()`, which asks the site and
   resolves to `{ticket, user}`: an Azure AD access token for the Store's
   collections service, and the site's own id for the player;
2. it hands those to the Store (`GetCustomerCollectionsIdAsync`), which
   answers with a Microsoft Store ID key for that customer;
3. it hands the key to the page with `oeeeApp.store.purchased([key])`,
   and the site asks the collections service what the customer owns.

Tauri's `eval` brings nothing back out of the page, so for step 1 the app
evaluates a script that awaits `ticket()` itself and emits the answer as
the app's own `oeee-store-ticket` event, with the number the app asked
with. The site's pages can already emit events, for the bridge
(`core:event:allow-emit`), so this needs no new permission, and no new
message in the site's bridge contract: only `ticket()`. A page without it,
or one that answers null, leaves the purchase unproven until the site's own
recheck, and the app logs it.

For any of this to work, Partner Center needs the add-ons (durable, their
Store IDs given to the site's `/supporter`), and the site needs an Azure
AD application associated with the app in Partner Center (Product
management > Product identity, "Associate Azure AD application"), whose
tokens for the collections service are what `ticket()` hands out.

## Rich presence

The site says what each page is in the `page` message it sends the app
(`src/bridge.rs`; `app_bridge.jinja` in oeee-cafe/web): drawing, a relay,
drawing together in a collaborative room, a banner, a replay, and the
community when it is public. It sends one on every page and again after a
boosted navigation, and the app sets `steam_display` to a token of
`steam/rich_presence.vdf`, with `%community%` filled in; a collaborative room
also sets `steam_player_group`, so friends in one room show together. A page
that says nothing is browsing, and so is the bundled loader. Upload `steam/rich_presence.vdf` in Steamworks
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

`cargo test` holds the app to the site's contract with every app
(`frontend/shared/appContract.json` in oeee-cafe/web): every message the
page sends parses, the user agent is marked as the site reads it, the
calls around leaving are the site's word for word, and every
`window.oeeeApp` member the app calls and every command its keys send is
one the page has (`src/contract.rs`). The copy it reads is
`src/testdata/appContract.json`; fetch it again after the site changes
the contract, from a checkout beside this one or from `OEEE_CAFE_WEB`:

```bash
./scripts/sync-app-contract.sh
OEEE_CAFE_WEB=~/src/oeee-cafe-web ./scripts/sync-app-contract.sh
```

## Building for Steam

Build on Windows:

```bash
cargo tauri build
```

and put `target/release/oeee-cafe-desktop.exe` in `steam/content/windows/`,
with `steam/redistributable/win64/steam_api64.dll` beside it; WebView2
ships with Windows 10 and 11. Steam installs files and runs no installer,
so the build makes none (`tauri.conf.json` leaves `bundle` inactive, and
keeps only the `.exe`'s icon and copyright). Set the depot's launch option
in Steamworks (Installation > General) to the `.exe`.

Then upload:

```bash
STEAM_APP_ID=... STEAM_USER=... \
STEAM_DEPOT_WINDOWS=... \
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
uninstalling takes them with it -- unless `%LOCALAPPDATA%\cafe.oeee`
already exists, as it does where the app has run unpackaged: Windows lets
a package change folders it finds there, so on a development machine it
shares them with `cargo run` and the Steam build.

The images come from `icons/icon.png`; run `msstore/assets.sh` again after
changing it.

## Not done yet

- **The keys, dialogs and right-click menu on Windows.** Type-checked and
  unit-tested from macOS (`cargo check --target x86_64-pc-windows-msvc`),
  not yet run. Worth trying: Alt+Left and Ctrl+R over an unsaved drawing
  (the system's leave question, then the right page, with the site's
  loading bar up), and the menu on a drawing, a link, a field with a
  misspelling and empty space (none). The mouse's back and forward buttons
  were not looked at.
- **Leaving the real painter.** On Windows, leaving and closing were tried on
  a page with the painter's `beforeunload` handler, not in the painter
  itself, signed in.
- **Selling.** Steam's prices, the Store's prices, purchase dialog and
  key have been type-checked and unit-tested from macOS, not run: there is
  no Steam client or Windows here. Worth trying: the price in two
  countries, buying in the overlay, and in a sideloaded Store package the
  dialog's owner window, cancelling it, and buying an add-on already owned.
- **The MSIX.** The x64 package has been packed, registered unsigned and
  run from Start. Not yet: the ARM64 package (it needs Visual Studio's ARM64
  build tools), a signed install, a Store submission, and a look at the
  taskbar icon and the unread dot in the packaged app.
- **Icons.** Generated from the 256px `static/favicon.png` in oeee-cafe/web; regenerate from a
  1024px source with `cargo tauri icon <file>` before release.
