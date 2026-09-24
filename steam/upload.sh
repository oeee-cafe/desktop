#!/usr/bin/env bash
# Upload the Windows build to Steam with SteamPipe.
#
#   STEAM_APP_ID=...  STEAM_DEPOT_WINDOWS=...  STEAM_USER=...
#   ./steam/upload.sh [description]
#
# The IDs are on the app's Steamworks page (SteamPipe > Depots). Place the
# unpacked build in steam/content/windows/ first -- see README.md; with the
# depot's variable unset or that folder empty there is nothing to upload.
#
# The build is set live on no branch. Choose the branch in Steamworks
# (SteamPipe > Builds), which is where Valve's review picks it up.
set -euo pipefail

here="$(cd "$(dirname "$0")" && pwd)"
: "${STEAM_APP_ID:?set STEAM_APP_ID}"
: "${STEAM_USER:?set STEAM_USER, a Steamworks account with build upload rights}"
description="${1:-$(git -C "$here" rev-parse --short HEAD)}"

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
      \"ContentRoot\" \"$here/content/$platform\"
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
  "BuildOutput" "$output"
  "Depots"
  {$depots
  }
}
VDF

steamcmd +login "$STEAM_USER" +run_app_build "$script" +quit
