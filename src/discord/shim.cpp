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
#include <string>
#include <unordered_map>

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
    // for no. `generation` is the one oeee_discord_authorize was given.
    void (*authorized)(void* context,
                       uint64_t generation,
                       OeeeText code,
                       OeeeText redirect,
                       OeeeText verifier);
    // Whether the SDK took the access token oeee_discord_update_token gave
    // it, with the generation it was given.
    void (*token)(void* context, uint64_t generation, bool taken);
    // The connection's state, discordpp::Client::Status, and when it ended,
    // why: discordpp::Client::Error and the gateway's close code.
    void (*status)(void* context, int32_t status, int32_t error, int32_t detail);
    // Something about the player's friends changed.
    void (*people)(void* context);
    // Whether an invitation reached `user`.
    void (*invited)(void* context, uint64_t user, bool sent);
    // The access token is about to run out.
    void (*expiring)(void* context);
    // A friend pressed Ask to Join on the player's activity: who, what
    // Discord calls them, and the party they asked into.
    void (*requested)(void* context, uint64_t user, OeeeText name, OeeeText party);
};

// One of the player's friends, or the player: `group` is
// discordpp::RelationshipGroupType -- playing Oeee Cafe, online elsewhere,
// or offline -- and `status` discordpp::StatusType.
typedef void (*OeeePerson)(void* data, uint64_t id, OeeeText name, int32_t status, int32_t group);

}

namespace {

// Never deleted, on purpose. The thread that calls the SDK is still running
// it when the app exits, and a static unique_ptr's destructor, run by the C
// runtime on the way out, would tear the client down under a RunCallbacks
// on that thread. The process is ending; the system takes it back.
discordpp::Client* client = nullptr;
uint64_t application = 0;
void* context = nullptr;
OeeeCallbacks said{};
// The requests to join not yet answered, by who sent them: what answering
// one needs is the request itself.
std::unordered_map<uint64_t, discordpp::ActivityInvite> requests;

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
    client = new discordpp::Client();
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
      [](discordpp::Client::Status status, discordpp::Client::Error error, int32_t detail) {
          if (error != discordpp::Client::Error::None) {
              std::fprintf(stderr,
                           "Discord: %s: %s (%d)\n",
                           discordpp::Client::StatusToString(status).c_str(),
                           discordpp::Client::ErrorToString(error).c_str(),
                           static_cast<int>(detail));
          }
          said.status(context,
                      static_cast<int32_t>(status),
                      static_cast<int32_t>(error),
                      detail);
      });
    client->SetRelationshipCreatedCallback([](uint64_t, bool) { said.people(context); });
    client->SetRelationshipDeletedCallback([](uint64_t, bool) { said.people(context); });
    client->SetRelationshipGroupsUpdatedCallback([](uint64_t) { said.people(context); });
    client->SetUserUpdatedCallback([](uint64_t) { said.people(context); });
    client->SetTokenExpirationCallback([]() { said.expiring(context); });
    // Invitations the player is sent are Discord's own to show; only a
    // friend's request to join is the room's.
    client->SetActivityInviteCreatedCallback([](discordpp::ActivityInvite invite) {
        if (invite.Type() != discordpp::ActivityActionTypes::JoinRequest) {
            return;
        }
        uint64_t sender = invite.SenderId();
        std::string name;
        if (auto user = client->GetUser(sender)) {
            name = user->DisplayName();
        }
        std::string party = invite.PartyId();
        requests.insert_or_assign(sender, invite);
        said.requested(context, sender, of(name), of(party));
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
        said.shown(context, result.Successful());
    });
}

// Asks the player, in Discord's overlay or their browser, to let the app see
// their friends and invite them: the presence scopes, which are those. The
// code that comes back is traded by the site, which holds the client
// secret; the SDK's own GetToken is for public clients only. `generation`
// comes back with the answer, so discord.rs can tell an answer it stopped
// waiting for.
void oeee_discord_authorize(uint64_t generation)
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
      [kept, generation](
        discordpp::ClientResult result, std::string code, std::string redirect) {
          if (!result.Successful()) {
              std::fprintf(stderr, "Discord: not authorized: %s\n", result.ToString().c_str());
              said.authorized(context, generation, OeeeText{}, OeeeText{}, OeeeText{});
              return;
          }
          said.authorized(context, generation, of(code), of(redirect), of(kept));
      });
}

// Gives the SDK the player's access token: a new one for a client already
// connected, which it goes on with, or the one to connect by next. Whether
// it took it comes back with `generation`, and discord.rs decides whether
// to connect -- by then the player may have asked to disconnect.
void oeee_discord_update_token(uint64_t generation, OeeeText access_token)
{
    if (!client) {
        return;
    }
    client->UpdateToken(discordpp::AuthorizationTokenType::Bearer,
                        text(access_token),
                        [generation](discordpp::ClientResult result) {
                            if (!result.Successful()) {
                                std::fprintf(stderr,
                                             "Discord: token not taken: %s\n",
                                             result.ToString().c_str());
                            }
                            said.token(context, generation, result.Successful());
                        });
}

// Connects as the player whose access token the SDK last took.
void oeee_discord_connect()
{
    if (client) {
        client->Connect();
    }
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

// Lets `user`, who asked to join, into the activity showing: their request
// answered with an invitation. Whether it went comes back as an
// invitation's does.
void oeee_discord_let_in(uint64_t user)
{
    auto found = requests.find(user);
    if (!client || found == requests.end()) {
        said.invited(context, user, false);
        return;
    }
    client->SendActivityJoinRequestReply(found->second, [user](discordpp::ClientResult result) {
        if (result.Successful()) {
            requests.erase(user);
        } else {
            std::fprintf(stderr, "Discord: not let in: %s\n", result.ToString().c_str());
        }
        said.invited(context, user, result.Successful());
    });
}

// Lets `user`'s request go unanswered.
void oeee_discord_forget_request(uint64_t user)
{
    requests.erase(user);
}

// Runs whatever the SDK has waiting, the callbacks above among it.
void oeee_discord_tick()
{
    discordpp::RunCallbacks();
}

}
