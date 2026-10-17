#!/usr/bin/env bash
# Draws the Steam store capsules, library assets and app icon from
# capsule.html into this folder, in the format each file's name gives:
#
#   store:   header (920x430), small (462x174), main (1232x706),
#            vertical (748x896)
#   library: library (600x900), library_header (920x430, the header's art),
#            hero (3840x1240), logo (1280 wide, transparent)
#   client:  app_icon (184x184, JPEG), shortcut_icon (ICO of 16 to 256)
#
#   ./steam/capsules/render.sh     # needs Chrome or Edge, and Node
set -euo pipefail

here="$(cd "$(dirname "$0")" && pwd)"
chrome="${CHROME:-}"
for c in "/c/Program Files/Google/Chrome/Application/chrome.exe" \
         "/c/Program Files (x86)/Microsoft/Edge/Application/msedge.exe" \
         google-chrome chromium; do
  [ -n "$chrome" ] && break
  if [ -x "$c" ] || command -v "$c" >/dev/null 2>&1; then chrome="$c"; fi
done

page="$here/capsule.html"
command -v cygpath >/dev/null 2>&1 && page="$(cygpath -m "$page")"

# render <layout> <width,height> [file name]
render() {
  local name="$1" size="$2" out="$here/${3:-$1.png}"
  command -v cygpath >/dev/null 2>&1 && out="$(cygpath -w "$out")"
  "$chrome" --headless=new --disable-gpu --hide-scrollbars \
    --force-device-scale-factor=1 --allow-file-access-from-files \
    --default-background-color=00000000 \
    --window-size="$size" --screenshot="$out" \
    "file:///$page?capsule=$name" 2>/dev/null
  echo "$out"
}

render header 920,430
render small 462,174
render main 1232,706
render vertical 748,896

render library 600,900
render header 920,430 library_header.png
render hero 3840,1240
render logo 1280,280

render icon 184,184 app_icon.jpg

# The shortcut icon, each size drawn on its own, then packed into one ICO.
sizes=(16 24 32 48 64 128 256)
for n in "${sizes[@]}"; do
  render "shortcut&size=$n" "$n,$n" ".shortcut-$n.png" >/dev/null
done
pngs=("${sizes[@]/#/.shortcut-}")
(cd "$here" && node ico.mjs shortcut_icon.ico "${pngs[@]/%/.png}")
rm -f "$here"/.shortcut-*.png
echo "$here/shortcut_icon.ico"
