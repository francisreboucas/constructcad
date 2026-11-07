#!/usr/bin/env bash
# Build the Debian package from the release binaries with dpkg-deb.
#
# Usage: packaging/linux/build-deb.sh <version> <dir with the release binaries> <amd64|arm64> <output .deb>
# Run from the repository root. <version> is the release version without the
# leading "v" (a pre-release "1.2.0-rc.1" becomes "1.2.0~rc.1", which sorts first).
set -euo pipefail

version=${1//-/\~}
bin_dir=$2
arch=$3
out=$4

work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
pkg="$work/constructcraft"
packaging/linux/install-tree.sh "$bin_dir" "$pkg"

cat > "$pkg/usr/share/doc/constructcraft/copyright" <<'EOF'
Format: https://www.debian.org/doc/packaging-manuals/copyright-format/1.0/
Upstream-Name: ConstructCraft
Source: https://github.com/francisreboucas/constructcraft

Files: *
Copyright: Francis Rebouças and the ConstructCraft contributors
License: GPL-3.0-or-later
 On Debian systems, the complete text of the GNU General Public License
 version 3 can be found in /usr/share/common-licenses/GPL-3.
EOF

# Shared libraries the binaries link to (dpkg-shlibdeps wants a debian/control,
# which may be empty); the window system and GPU libraries are loaded at run
# time, so they are listed by hand.
mkdir -p "$work/shlibs/debian"
touch "$work/shlibs/debian/control"
shlibs=$(cd "$work/shlibs" && dpkg-shlibdeps -O "$pkg/usr/bin/constructcraft" "$pkg/usr/bin/ccad" 2>/dev/null |
    sed -n 's/^shlibs:Depends=//p') || shlibs=""
if [ -z "$shlibs" ]; then
    shlibs="libc6, libgcc-s1"
fi

installed_size=$(du -sk "$pkg/usr" | cut -f1)
mkdir -p "$pkg/DEBIAN"
cat > "$pkg/DEBIAN/control" <<EOF
Package: constructcraft
Replaces: constructcad
Conflicts: constructcad
Provides: constructcad
Version: $version
Section: graphics
Priority: optional
Architecture: $arch
Maintainer: Francis Rebouças <francisreboucas@gmail.com>
Installed-Size: $installed_size
Depends: $shlibs, libxkbcommon0, libvulkan1 | libegl1
Recommends: libxkbcommon-x11-0, libx11-6, libxcursor1, libxi6, libxrandr2, libwayland-client0, xdg-desktop-portal
Homepage: https://github.com/francisreboucas/constructcraft
Description: Free and open source BIM authoring
 ConstructCraft models buildings with walls, floors, roofs, doors, windows,
 structure and building services, and derives plans, sections, elevations,
 3D views, schedules and sheets from the model. It reads and writes IFC.
 .
 The package installs the desktop application (constructcraft) and the ccad
 command line, which runs the same commands from scripts and serves them
 to AI agents over MCP.
EOF

mkdir -p "$(dirname "$out")"
dpkg-deb --root-owner-group --build "$pkg" "$out"
