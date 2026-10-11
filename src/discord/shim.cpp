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

// What the SDK says back, each handed the context discord.rs started it
// with.
struct OeeeCallbacks {
    // A friend's join secret, from an invitation accepted in Discord.
    void (*joined)(void* context, OeeeText secret);
    // Whether Discord took the activity last shown.
    void (*shown)(void* context, bool shown);
    // The player said yes in Discord's consent screen, or did not: a code
    // for the site to trade (src/discord.rs in oeee-cafe/web), the redirect
    // it was given for, and the PKCE verifier it was asked with. All empty
    // for no.
    void (*authorized)(void* context, OeeeText code, OeeeText redirect, OeeeText verifier);
    // The connection's state: discordpp::Client::Status, or -1 for a token
    // Discord would not take.
    void (*status)(void* context, int32_t status);
    // Something about the player's friends changed.
    void (*people)(void* context);
    // Whether an invitation reached `user`.
    void (*invited)(void* context, uint64_t user, bool sent);
    // The access token is about to run out.
    void (*expiring)(void* context);
};

// One of the player's friends, or the player: `group` is
// discordpp::RelationshipGroupType -- playing Oeee Cafe, online elsewhere,
// or offline -- and `status` discordpp::StatusType.
typedef void (*OeeePerson)(void* data, uint64_t id, OeeeText name, int32_t status, int32_t group);

}

namespace {

std::unique_ptr<discordpp::Client> client;
uint64_t application = 0;
void* context = nullptr;
OeeeCallbacks said{};

std::string text(OeeeText t)
{
    return t.ptr ? std::string(t.ptr, t.len) : std::string();
}

OeeeText of(std::string const& s)
{
    return OeeeText{s.data(), s.size()};
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
                        const OeeeCallbacks* callbacks)
{
    application = app;
    context = ctx;
    said = *callbacks;
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
    client->SetActivityJoinCallback(
      [](std::string secret) { said.joined(context, of(secret)); });
    client->SetStatusChangedCallback(
      [](discordpp::Client::Status status, discordpp::Client::Error, int32_t) {
          said.status(context, static_cast<int32_t>(status));
      });
    client->SetRelationshipCreatedCallback([](uint64_t, bool) { said.people(context); });
    client->SetRelationshipDeletedCallback([](uint64_t, bool) { said.people(context); });
    client->SetRelationshipGroupsUpdatedCallback([](uint64_t) { said.people(context); });
    client->SetUserUpdatedCallback([](uint64_t) { said.people(context); });
    client->SetTokenExpirationCallback([]() { said.expiring(context); });
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
        said.shown(context, result.Successful());
    });
}

// Asks the player, in Discord's overlay or their browser, to let the app see
// their friends and invite them: the presence scopes, which are those. The
// code that comes back is traded by the site, which holds the client
// secret; the SDK's own GetToken is for public clients only.
void oeee_discord_authorize()
{
    if (!client) {
        return;
    }
    auto verifier = client->CreateAuthorizationCodeVerifier();
    discordpp::AuthorizationArgs args;
    args.SetClientId(application);
    args.SetScopes(discordpp::Client::GetDefaultPresenceScopes());
    args.SetCodeChallenge(verifier.Challenge());
    std::string kept = verifier.Verifier();
    client->Authorize(
      std::move(args),
      [kept](discordpp::ClientResult result, std::string code, std::string redirect) {
          if (!result.Successful()) {
              std::fprintf(stderr, "Discord: not authorized: %s\n", result.ToString().c_str());
              said.authorized(context, OeeeText{}, OeeeText{}, OeeeText{});
              return;
          }
          said.authorized(context, of(code), of(redirect), of(kept));
      });
}

// Connects as the player whose access token this is.
void oeee_discord_connect(OeeeText access_token)
{
    if (!client) {
        return;
    }
    client->UpdateToken(discordpp::AuthorizationTokenType::Bearer,
                        text(access_token),
                        [](discordpp::ClientResult result) {
                            if (!result.Successful()) {
                                std::fprintf(stderr,
                                             "Discord: token not taken: %s\n",
                                             result.ToString().c_str());
                                said.status(context, -1);
                                return;
                            }
                            client->Connect();
                        });
}

// Lets go of the player's account. The activity goes on by the Discord app
// beside this one, as before they connected.
void oeee_discord_disconnect()
{
    if (client) {
        client->Disconnect();
    }
}

// The player, connected: their id and name.
void oeee_discord_me(OeeePerson each, void* data)
{
    if (!client) {
        return;
    }
    auto me = client->GetCurrentUserV2();
    if (!me) {
        return;
    }
    std::string name = me->DisplayName();
    each(data, me->Id(), of(name), static_cast<int32_t>(me->Status()), 0);
}

// The player's Discord friends, those playing Oeee Cafe first, then those
// online elsewhere, then the rest.
void oeee_discord_friends(OeeePerson each, void* data)
{
    if (!client) {
        return;
    }
    for (auto group : {discordpp::RelationshipGroupType::OnlinePlayingGame,
                       discordpp::RelationshipGroupType::OnlineElsewhere,
                       discordpp::RelationshipGroupType::Offline}) {
        for (auto& relationship : client->GetRelationshipsByGroup(group)) {
            if (relationship.DiscordRelationshipType() != discordpp::RelationshipType::Friend) {
                continue;
            }
            auto user = relationship.User();
            if (!user) {
                continue;
            }
            std::string name = user->DisplayName();
            each(data,
                 user->Id(),
                 of(name),
                 static_cast<int32_t>(user->Status()),
                 static_cast<int32_t>(group));
        }
    }
}

// Sends `user` an invitation to the activity showing, which has to be a room
// a friend can join.
void oeee_discord_invite(uint64_t user)
{
    if (!client) {
        return;
    }
    client->SendActivityInvite(user, "", [user](discordpp::ClientResult result) {
        if (!result.Successful()) {
            std::fprintf(stderr, "Discord: invitation not sent: %s\n", result.ToString().c_str());
        }
        said.invited(context, user, result.Successful());
    });
}

// Runs whatever the SDK has waiting, the callbacks above among it.
void oeee_discord_tick()
{
    discordpp::RunCallbacks();
}

}
