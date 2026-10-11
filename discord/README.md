# Discord's Social SDK

The app talks to Discord through its Social SDK (`src/discord.rs`), which is
Discord's to hand out and is not kept in this repository. Every build with the
`discord` feature (the Steam build by default, and the Microsoft Store's
build) needs it unpacked here first:

1. Sign in to the [Discord Developer Portal](https://discord.com/developers/applications),
   open the Oeee Cafe application (`1558602531133722646`), and under
   **Discord Social SDK > Downloads** download the SDK (1.10.19337 is the
   one built against). It has every platform in it.
2. Unpack its `discord_social_sdk/` folder here as `social_sdk/`. Windows
   needs only these; the voice files (`krisp`), Android's and the debug
   symbols can be left out:

   ```
   discord/social_sdk/include/discordpp.h
   discord/social_sdk/include/cdiscord.h
   discord/social_sdk/lib/release/discord_partner_sdk.lib
   discord/social_sdk/bin/release/discord_partner_sdk.dll
   discord/social_sdk/lib/release/arm64/discord_partner_sdk.lib
   discord/social_sdk/bin/release/arm64/discord_partner_sdk.dll
   ```

`build.rs` compiles `src/discord/shim.cpp` against `include/`, links
`lib/release/`, and copies `discord_partner_sdk.dll` beside the binary in
`target/<profile>/`, as it does Steam's library. The app does not start without
it, so it ships beside the `.exe` too: in Steam's depot, and in the MSIX
(`msstore/package.ps1`). `social_sdk/` is gitignored.

On Linux it links `lib/release/libdiscord_partner_sdk.so` instead, copies it
beside the binary the same way, and has the binary look for it there
(`-rpath $ORIGIN`), so it ships beside the binary too. The SDK has no Linux
library for ARM, and an ARM Linux build with the feature stops and says so.

A build without the SDK leaves the feature out:

```bash
cargo build --no-default-features --features steam
```

## In the Developer Portal

- **Discord Social SDK**: enabled for the application, which is what lets it
  download the SDK at all.
- **Rich Presence > Art Assets**: nothing is needed. Discord shows the
  application's own icon when an activity names no image.
- The application's name is what Discord shows as the game: "Playing Oeee Cafe".
- **Rich Presence > Deep Link URL**: `https://oeee.cafe`. A friend who accepts
  an invitation in Discord's phone app is sent there, at
  `/_discord/join?secret=...`, which the site turns into the room (oeee-cafe/web,
  `discord_join`), in the browser or in the iOS app. Only once the site has
  that route: the activity offers phones the invitation either way.
- **OAuth2 > Redirects**: `http://127.0.0.1/callback`, for connecting a
  player's account to the app (below): the SDK listens there, on the
  player's own computer, for Discord's consent screen to answer. Beside the
  site's own `https://oeee.cafe/auth/discord/callback`, for Sign in with
  Discord. Leave **Public Client** off: the site trades the app's codes
  with the secret (`/api/discord/token` in oeee-cafe/web), so the
  application stays the confidential client the site's sign-in needs.
- **Interactions Endpoint URL**: none. Nothing here is a bot.

## What it does

- The player's Discord profile shows what they are doing on the site, in the
  site's words in their language: "Drawing together · 오이카페 모에화 (2 of 4)".
- In a collaborative room a friend could enter, the activity is a party with a
  join secret: the player can invite friends with the **+** button in any
  Discord chat, and friends can **Ask to Join** from their profile, which the
  player accepts in Discord.
- A friend who accepts has their own Oeee Cafe app opened by Discord and taken
  into the room. Discord opens it the way the app told it to, on the friend's
  own computer, the last time it ran: through Steam when Steam started it,
  through `oeee-cafe://` otherwise. So a friend needs to have opened the app
  once, and an Oeee Cafe account; the site decides, as it always does,
  whether they may enter and whether there is a seat.
- A friend who accepts on their phone is sent to the room on the site (the
  Deep Link URL above), and needs no app at all.
- In a room, **Invite** beside Share lists the player's Discord friends and
  sends the one picked an invitation, without going to Discord. The first
  time, it asks them to connect their Discord account to the app: Discord's
  own consent screen, in its overlay or the browser, for the friends list
  and invitations (the SDK's presence scopes) and nothing more. The account
  page shows it under **In this app**, with **Disconnect**. The refresh
  token is kept in Windows's Credential Manager (`cafe.oeee/discord`), for
  this Windows user on this computer; disconnecting forgets it and hands it
  back to Discord, which takes the app off the player's authorized apps.

Testing needs two Discord accounts that are friends, each with Discord's
desktop app running, on two computers (or one Windows user account each):
Discord does not deliver an invitation to the account that sent it.

When nothing shows: the app prints `Discord: activity shown` each time
Discord takes the activity, and the SDK's warnings beside it; set
`OEEE_CAFE_DISCORD_LOG=1` for everything the SDK says. Shown, but not on
the profile, is Discord's own setting: User Settings > Activity Privacy >
Share your detected activities with others.
