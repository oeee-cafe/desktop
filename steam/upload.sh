#!/usr/bin/env bash
# Build the Windows app and upload it to Steam with SteamPipe.
#
#   STEAM_APP_ID=...  STEAM_DEPOT_WINDOWS=...  STEAM_USER=...
#   ./steam/upload.sh [description]
#
# Run it on Windows (Git Bash). It builds the release program
# (`cargo tauri build --no-bundle`), puts it in steam/content/windows/ with
# Steam's library and Discord's beside it (Discord's Social SDK has to be
# unpacked first: discord/README.md), and uploads that folder, so what goes
# up is always what the checkout builds. NO_BUILD=1 uploads the folder as it
# is.
#
# The IDs are on the app's Steamworks page (SteamPipe > Depots). steamcmd is
# looked for on PATH, or named by STEAMCMD (e.g. STEAMCMD=/c/steamcmd/steamcmd.exe);
# it asks for the account's password and Steam Guard code itself.
#
# The build is set live on no branch. Choose the branch in Steamworks
# (SteamPipe > Builds), which is where Valve's review picks it up.
set -euo pipefail

here="$(cd "$(dirname "$0")" && pwd)"
root="$(cd "$here/.." && pwd)"
: "${STEAM_APP_ID:?set STEAM_APP_ID}"
: "${STEAM_USER:?set STEAM_USER, a Steamworks account with build upload rights}"
steamcmd="${STEAMCMD:-steamcmd}"
command -v "$steamcmd" >/dev/null || { echo "no steamcmd: put it on PATH or set STEAMCMD" >&2; exit 1; }
description="${1:-$(git -C "$here" rev-parse --short HEAD)}"

# steamcmd on Windows reads Windows paths, not Git Bash's /c/... ones.
native() { if command -v cygpath >/dev/null; then cygpath -m "$1"; else echo "$1"; fi; }

content="$here/content/windows"
if [ -z "${NO_BUILD:-}" ]; then
  case "$(uname -s)" in
    MINGW*|MSYS*|CYGWIN*) ;;
    *) echo "the Windows depot is built on Windows; NO_BUILD=1 uploads steam/content/windows/ as it is" >&2; exit 1 ;;
  esac
  sdk="$root/discord/social_sdk/bin/release/discord_partner_sdk.dll"
  [ -f "$sdk" ] || { echo "no $sdk: unpack Discord's Social SDK first (discord/README.md)" >&2; exit 1; }
  (cd "$root" && cargo tauri build --no-bundle)
  mkdir -p "$content"
  cp "$root/target/release/oeee-cafe.exe" \
     "$here/redistributable/win64/steam_api64.dll" \
     "$sdk" \
     "$content/"
  echo "staged $(ls "$content" | tr '\n' ' ')"
fi

output="$here/output"
mkdir -p "$output"
script="$output/app_build_${STEAM_APP_ID}.vdf"

depots=""
add_depot() {
  local id="$1" platform="$2"
  [ -n "$id" ] || return 0
  if [ -z "$(ls -A "$here/content/$platform" 2>/dev/null)" ]; then
    echo "skipping $platform: steam/content/$platform is empty" >&2
    return 0
  fi
  depots+="
    \"$id\"
    {
      \"ContentRoot\" \"$(native "$here/content/$platform")\"
      \"FileMapping\"
      {
        \"LocalPath\" \"*\"
        \"DepotPath\" \".\"
        \"Recursive\" \"1\"
      }
    }"
}
add_depot "${STEAM_DEPOT_WINDOWS:-}" windows

if [ -z "$depots" ]; then
  echo "nothing to upload" >&2
  exit 1
fi

cat > "$script" <<VDF
"AppBuild"
{
  "AppID" "$STEAM_APP_ID"
  "Desc" "$description"
  "BuildOutput" "$(native "$output")"
  "Depots"
  {$depots
  }
}
VDF

"$steamcmd" +login "$STEAM_USER" +run_app_build "$(native "$script")" +quit
