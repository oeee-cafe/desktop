#!/usr/bin/env bash
# Draws the images the MSIX package names (AppxManifest.xml) from
# icons/icon.png, into msstore/Assets/. Run it again after changing the icon.
#
#   ./msstore/assets.sh      # needs ImageMagick 7 (`magick`)
#
# makepri (package.ps1) picks among the variants by the name's qualifier:
# `scale-*` by the display's scale, `targetsize-*` by the pixel size the
# taskbar, Start and Explorer ask for, and `altform-unplated` for the
# taskbar and Start, which show the mark with no tile behind it.
set -euo pipefail

here="$(cd "$(dirname "$0")" && pwd)"
source="$here/../icons/icon.png"
out="$here/Assets"
mkdir -p "$out"

# The mark at `size`, centred on a transparent square of `canvas`.
draw() {
  local size="$1" canvas="$2" name="$3"
  magick "$source" -resize "${size}x${size}" \
    -background none -gravity center -extent "${canvas}x${canvas}" \
    "$out/$name"
}

# The app's icon, on the taskbar and in Start and Explorer.
for scale in 100 200 400; do
  px=$((44 * scale / 100))
  draw "$px" "$px" "Square44x44Logo.scale-$scale.png"
done
for px in 16 24 32 48 256; do
  draw "$px" "$px" "Square44x44Logo.targetsize-$px.png"
  draw "$px" "$px" "Square44x44Logo.targetsize-${px}_altform-unplated.png"
  draw "$px" "$px" "Square44x44Logo.targetsize-${px}_altform-lightunplated.png"
done

# A pinned tile on Windows 10: the mark two-thirds across, room around it.
for scale in 100 200; do
  px=$((150 * scale / 100))
  draw $((px * 2 / 3)) "$px" "Square150x150Logo.scale-$scale.png"
done

# The package's logo, in the installer and the Store's own lists.
for scale in 100 200 400; do
  px=$((50 * scale / 100))
  draw "$px" "$px" "StoreLogo.scale-$scale.png"
done
