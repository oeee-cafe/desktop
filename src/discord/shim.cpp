// The few calls the app makes into Discord's Social SDK, as plain C for
// discord.rs. The SDK's own C header is what its C++ header wraps, and
// neither is ours to copy here (discord/README.md), so the layouts of its
// structs are left to its C++ header, which this is compiled against, and
// Rust only ever sees what this file declares.
//
// Every call here is made on one thread, the one discord.rs starts, which
// also runs the SDK's callbacks: nothing in the SDK is shared across threads.

#define DISCORDPP_IMPLEMENTATION
#include "discordpp.h"

#include <cstdint>
#include <cstdio>
#include <cstdlib>
#include <memory>
#include <string>

extern "C" {

// Text as Rust holds it: UTF-8, not NUL-terminated. Empty is none.
struct OeeeText {
    const char* ptr;
    size_t len;
};

// What to show (`Shown` in discord.rs). A room has a party id; without
// one, `join`, `size` and `seats` are not read.
struct OeeeActivity {
    OeeeText details;
    OeeeText state;
    uint64_t start;
    OeeeText party;
    OeeeText join;
    int32_t size;
    int32_t seats;
};

typedef void (*OeeeJoined)(void* context, const char* secret, size_t len);
typedef void (*OeeeShown)(void* context, bool shown);

}

namespace {

std::unique_ptr<discordpp::Client> client;
void* context = nullptr;
OeeeShown shown = nullptr;

std::string text(OeeeText t)
{
    return t.ptr ? std::string(t.ptr, t.len) : std::string();
}

} // namespace

extern "C" {

// Starts the SDK for the application, and says how Discord is to open the
// app for a friend who accepts an invitation: through Steam, when Steam
// started it (`steam_app` is not 0), or else by `launch`.
void oeee_discord_start(uint64_t app,
                        uint32_t steam_app,
                        OeeeText launch,
                        void* ctx,
                        OeeeJoined joined,
                        OeeeShown shown_cb)
{
    context = ctx;
    shown = shown_cb;
    client = std::make_unique<discordpp::Client>();
    // What the SDK says of itself, beside the app's own output: warnings
    // and worse, or everything with OEEE_CAFE_DISCORD_LOG set.
    client->AddLogCallback(
      [](std::string message, discordpp::LoggingSeverity) {
          std::fprintf(stderr, "Discord SDK: %s\n", message.c_str());
      },
      std::getenv("OEEE_CAFE_DISCORD_LOG") ? discordpp::LoggingSeverity::Verbose
                                             : discordpp::LoggingSeverity::Warning);
    client->SetApplicationId(app);
    bool registered = steam_app != 0
      ? client->RegisterLaunchSteamApplication(app, steam_app)
      : client->RegisterLaunchCommand(app, text(launch));
    if (!registered) {
        std::fprintf(stderr, "Discord: could not register how to launch the app\n");
    }
    client->SetActivityJoinCallback([joined](std::string secret) {
        joined(context, secret.data(), secret.size());
    });
}

// Shows `activity` as the player's.
void oeee_discord_show(const OeeeActivity* activity)
{
    if (!client) {
        return;
    }
    discordpp::Activity a;
    a.SetType(discordpp::ActivityTypes::Playing);
    a.SetDetails(text(activity->details));
    if (activity->state.len > 0) {
        a.SetState(text(activity->state));
    }
    if (activity->start != 0) {
        discordpp::ActivityTimestamps timestamps;
        timestamps.SetStart(activity->start);
        a.SetTimestamps(std::move(timestamps));
    }
    if (activity->party.len > 0) {
        discordpp::ActivityParty party;
        party.SetId(text(activity->party));
        party.SetCurrentSize(activity->size);
        party.SetMaxSize(activity->seats);
        party.SetPrivacy(discordpp::ActivityPartyPrivacy::Private);
        a.SetParty(std::move(party));
        discordpp::ActivitySecrets secrets;
        secrets.SetJoin(text(activity->join));
        a.SetSecrets(std::move(secrets));
        // Accepted on a phone too: Discord's app there opens the site at
        // /_discord/join with the secret (the Developer Portal's Deep Link
        // URL), and the site opens the room. Flags, which the header gives
        // no operator to combine.
        using P = discordpp::ActivityGamePlatforms;
        a.SetSupportedPlatforms(static_cast<P>(static_cast<int>(P::Desktop)
                                               | static_cast<int>(P::IOS)
                                               | static_cast<int>(P::Android)));
    }
    client->UpdateRichPresence(std::move(a), [](discordpp::ClientResult result) {
        if (result.Successful()) {
            std::fprintf(stderr, "Discord: activity shown\n");
        } else {
            std::fprintf(stderr, "Discord: %s\n", result.ToString().c_str());
        }
        shown(context, result.Successful());
    });
}

// Runs whatever the SDK has waiting, the callbacks above among it.
void oeee_discord_tick()
{
    discordpp::RunCallbacks();
}

}
