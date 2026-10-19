#!/usr/bin/env bash
# Draws the app icon (make_icon.swift) and the sizes Tauri bundles from it,
# then the Store's images (msstore/assets.sh).
#
#   ./icons/make_icons.sh <slice.png>   # needs macOS (swift) and ImageMagick 7
#
# <slice.png> is the 1024px slice on transparency, oeee-cafe-apple's
# scripts/app-icon/slice.png.
set -euo pipefail

here="$(cd "$(dirname "$0")" && pwd)"
slice="$1"
master="$(mktemp -t oeee-icon).png"
trap 'rm -f "$master"' EXIT

swift "$here/make_icon.swift" "$slice" "$master"

for px in 32 64 128; do
  magick "$master" -resize "${px}x${px}" -strip "$here/${px}x${px}.png"
done
magick "$master" -resize 256x256 -strip "$here/128x128@2x.png"
magick "$master" -resize 512x512 -strip "$here/icon.png"
# The .ico holds each size as a PNG, a twelfth of what ImageMagick's
# bitmaps come to.
sizes=(256 64 48 32 24 16)
for px in "${sizes[@]}"; do
  magick "$master" -resize "${px}x${px}" -strip "png32:$master.$px.png"
done
python3 - "$here/icon.ico" "${sizes[@]/#/$master.}" <<'EOF'
import struct, sys
out, *names = sys.argv[1:]
pngs = [open(n + ".png", "rb").read() for n in names]
sizes = [int(n.rsplit(".", 1)[1]) for n in names]
head = struct.pack("<HHH", 0, 1, len(pngs))
offset = 6 + 16 * len(pngs)
entries = b""
for px, data in zip(sizes, pngs):
    entries += struct.pack("<BBBBHHII", px % 256, px % 256, 0, 0, 1, 32, len(data), offset)
    offset += len(data)
open(out, "wb").write(head + entries + b"".join(pngs))
EOF
rm -f "$master".*.png

"$here/../msstore/assets.sh"
