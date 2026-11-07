#!/usr/bin/env bash
# Install ConstructCraft into a file tree under <root>/usr, the layout shared by
# the .deb package and the AppImage:
#   usr/bin/constructcraft, usr/bin/ccad
#   usr/share/applications/constructcraft.desktop
#   usr/share/icons/hicolor/{256x256,512x512}/apps/constructcraft.png
#   usr/share/mime/packages/constructcraft.xml
#   usr/share/doc/constructcraft/ (README, CHANGELOG, license, sample project)
#
# Usage: packaging/linux/install-tree.sh <dir with the release binaries> <root>
# Run from the repository root.
set -euo pipefail

bin_dir=$1
root=$2
here=packaging/linux

install -Dm755 "$bin_dir/constructcraft" "$root/usr/bin/constructcraft"
install -Dm755 "$bin_dir/ccad" "$root/usr/bin/ccad"
install -Dm644 "$here/constructcraft.desktop" "$root/usr/share/applications/constructcraft.desktop"
install -Dm644 assets/icons/app-icon-256.png "$root/usr/share/icons/hicolor/256x256/apps/constructcraft.png"
install -Dm644 assets/icons/app-icon.png "$root/usr/share/icons/hicolor/512x512/apps/constructcraft.png"
install -Dm644 "$here/constructcraft.xml" "$root/usr/share/mime/packages/constructcraft.xml"
doc="$root/usr/share/doc/constructcraft"
install -Dm644 README.md "$doc/README.md"
install -Dm644 README.pt-BR.md "$doc/README.pt-BR.md"
install -Dm644 CHANGELOG.md "$doc/CHANGELOG.md"
install -Dm644 LICENSE "$doc/LICENSE"
install -Dm644 examples/casa_simples.ccad "$doc/examples/casa_simples.ccad"
