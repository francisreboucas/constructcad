#!/usr/bin/env bash
# Build ConstructCraft.app around the release binary and pack it in a .dmg
# (with a link to /Applications). macOS only: uses sips, iconutil, plutil,
# codesign and hdiutil.
#
# Usage: packaging/macos/build-dmg.sh <version> <constructcraft binary> <output .dmg>
# Run from the repository root. <version> is the release version without the
# leading "v"; the bundle gets its numeric part (1.2.0-rc.1 -> 1.2.0).
set -euo pipefail

version=${1%%[-+]*}
binary=$2
out=$3

work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
root="$work/dmg"
app="$root/ConstructCraft.app"
mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources"
cp "$binary" "$app/Contents/MacOS/constructcraft"
chmod 755 "$app/Contents/MacOS/constructcraft"
sed "s/@VERSION@/$version/g" packaging/macos/Info.plist.in > "$app/Contents/Info.plist"
plutil -lint "$app/Contents/Info.plist"
printf 'APPL????' > "$app/Contents/PkgInfo"

# The icon, from the 512 px application icon.
iconset="$work/ConstructCraft.iconset"
mkdir -p "$iconset"
for size in 16 32 128 256 512; do
    sips -z "$size" "$size" assets/icons/app-icon.png --out "$iconset/icon_${size}x${size}.png" > /dev/null
    double=$((size * 2))
    if [ "$double" -le 512 ]; then
        sips -z "$double" "$double" assets/icons/app-icon.png --out "$iconset/icon_${size}x${size}@2x.png" > /dev/null
    fi
done
iconutil -c icns "$iconset" -o "$app/Contents/Resources/ConstructCraft.icns"

# Not notarized: an ad-hoc signature seals the bundle, so Apple silicon Macs
# run it and Gatekeeper asks once instead of calling it damaged.
codesign --force --deep --sign - "$app"

ln -s /Applications "$root/Applications"
mkdir -p "$(dirname "$out")"
hdiutil create -volname ConstructCraft -srcfolder "$root" -ov -format UDZO "$out"
