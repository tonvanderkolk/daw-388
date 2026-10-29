#!/bin/sh
# Turn packaging/icon-1024.png into the icons the builds use: packaging/macos/AppIcon.icns
# (iconutil, macOS only) and packaging/windows/daw388.ico (ImageMagick). Run after changing the
# icon with packaging/make_icon.py; the results are committed, so the CI needs neither tool.
set -eu
cd "$(dirname "$0")"
set_dir=$(mktemp -d)/AppIcon.iconset
mkdir -p "$set_dir"
for size in 16 32 128 256 512; do
    sips -z "$size" "$size" icon-1024.png --out "$set_dir/icon_${size}x${size}.png" >/dev/null
    double=$((size * 2))
    sips -z "$double" "$double" icon-1024.png --out "$set_dir/icon_${size}x${size}@2x.png" >/dev/null
done
iconutil -c icns "$set_dir" -o macos/AppIcon.icns
convert icon-1024.png -define icon:auto-resize=256,128,64,48,32,16 windows/daw388.ico
echo "wrote macos/AppIcon.icns and windows/daw388.ico"
