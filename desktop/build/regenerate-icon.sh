#!/usr/bin/env bash
# Regenerate icon.png + icon.icns from icon.svg.
#
# We use macOS's `qlmanage` (WebKit-backed thumbnailer) instead of
# ImageMagick because IM's SVG renderer drops `<g>` stroke inheritance and
# produces an all-black PNG. qlmanage flattens transparent areas of the SVG
# to white, so we post-process with ImageMagick to mask the alpha channel
# to a rounded-rect matching the squircle drawn in the SVG — otherwise the
# icon ships with a hard white box around it in the Dock/Launchpad.

set -euo pipefail
cd "$(dirname "$0")"

tmp=$(mktemp -d)
trap "rm -rf $tmp" EXIT

qlmanage -t -s 1024 -o "$tmp" icon.svg > /dev/null
magick "$tmp/icon.svg.png" \
  \( -size 1024x1024 xc:none -fill white -draw "roundrectangle 100,100 924,924 184,184" \) \
  -alpha set -compose CopyOpacity -composite icon.png

mkdir -p icon.iconset
sips -z 16 16     icon.png --out icon.iconset/icon_16x16.png > /dev/null
sips -z 32 32     icon.png --out icon.iconset/icon_16x16@2x.png > /dev/null
sips -z 32 32     icon.png --out icon.iconset/icon_32x32.png > /dev/null
sips -z 64 64     icon.png --out icon.iconset/icon_32x32@2x.png > /dev/null
sips -z 128 128   icon.png --out icon.iconset/icon_128x128.png > /dev/null
sips -z 256 256   icon.png --out icon.iconset/icon_128x128@2x.png > /dev/null
sips -z 256 256   icon.png --out icon.iconset/icon_256x256.png > /dev/null
sips -z 512 512   icon.png --out icon.iconset/icon_256x256@2x.png > /dev/null
sips -z 512 512   icon.png --out icon.iconset/icon_512x512.png > /dev/null
cp icon.png icon.iconset/icon_512x512@2x.png
iconutil -c icns icon.iconset -o icon.icns
rm -rf icon.iconset

echo "regenerated icon.png ($(stat -f%z icon.png) bytes) and icon.icns ($(stat -f%z icon.icns) bytes)"
