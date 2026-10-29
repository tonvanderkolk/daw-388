#!/bin/sh
# Build "DAW 388.app" and a DMG from release binaries.
#
#   packaging/macos/bundle.sh <version> <out-dir> <binary> [<binary> ...]
#
# Several binaries (Apple Silicon and Intel) are joined into one universal binary with lipo.
# The app is signed ad hoc (no Apple Developer account): it runs, but on first opening macOS
# asks the user to allow it in System Settings, Privacy & Security.
set -eu
version=$1
out=$2
shift 2
here=$(cd "$(dirname "$0")" && pwd)
app="$out/DAW 388.app"
rm -rf "$app"
mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources"
if [ "$#" -gt 1 ]; then
    lipo -create "$@" -output "$app/Contents/MacOS/daw388"
else
    cp "$1" "$app/Contents/MacOS/daw388"
fi
sed "s/@VERSION@/$version/g" "$here/Info.plist" >"$app/Contents/Info.plist"
cp "$here/AppIcon.icns" "$app/Contents/Resources/AppIcon.icns"
printf 'APPL????' >"$app/Contents/PkgInfo"
codesign --force --deep --sign - "$app"

# The DMG: the app and a link to Applications to drag it onto.
staging=$(mktemp -d)
cp -R "$app" "$staging/"
ln -s /Applications "$staging/Applications"
dmg="$out/DAW-388-$version-macos.dmg"
rm -f "$dmg"
hdiutil create -volname "DAW 388" -srcfolder "$staging" -ov -format UDZO "$dmg" >/dev/null
rm -rf "$staging"
echo "$dmg"
