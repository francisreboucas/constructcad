#!/usr/bin/env bash
# Build the AppImage from the release binaries with appimagetool.
#
# Usage: packaging/linux/build-appimage.sh <dir with the release binaries> <x86_64|aarch64>
#                                          <output .AppImage> <appimagetool> <type2 runtime>
# Run from the repository root, on the oldest supported Ubuntu LTS (the binaries
# use the glibc of the build machine), with patchelf and the libraries below
# installed, on the architecture being packaged. The release workflow downloads
# pinned, checksum-verified copies of appimagetool and of the runtime.
set -euo pipefail

bin_dir=$1
arch=$2
out=$3
appimagetool=$4
runtime=$5

# How ldconfig names the architecture of a library.
case $arch in
    x86_64) ld_arch=x86-64 ;;
    aarch64) ld_arch=AArch64 ;;
    *)
        echo "error: unsupported architecture $arch" >&2
        exit 1
        ;;
esac

work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
appdir="$work/ConstructCraft.AppDir"
packaging/linux/install-tree.sh "$bin_dir" "$appdir"

# winit loads the keyboard and X11 libraries with dlopen and fails without them;
# minimal systems lack some. Bundle them with the libraries they need (and their
# copyright files); RUNPATH lets the binaries find them, and the libraries each
# other. libX11, libX11-xcb, libxcb and libwayland-client stay with the host
# (AppImage excludelist).
bundled_libs="libxkbcommon.so.0 libxkbcommon-x11.so.0 libxcb-xkb.so.1
    libXcursor.so.1 libXrender.so.1 libXfixes.so.3 libXi.so.6 libXext.so.6"
mkdir -p "$appdir/usr/lib"
for lib in $bundled_libs; do
    path=$(ldconfig -p | awk -v l="$lib" -v a="$ld_arch" '$1 == l && index($0, "," a ")") { print $NF; exit }')
    if [ -z "$path" ]; then
        echo "error: $lib not found on the build machine" >&2
        exit 1
    fi
    cp -L "$path" "$appdir/usr/lib/$lib"
    patchelf --set-rpath '$ORIGIN' "$appdir/usr/lib/$lib"
    pkg=$(dpkg -S "$(readlink -f "$path")" | head -n1 | cut -d: -f1)
    install -Dm644 "/usr/share/doc/$pkg/copyright" "$appdir/usr/share/doc/$pkg/copyright"
done
for exe in constructcraft ccad; do
    patchelf --set-rpath '$ORIGIN/../lib' "$appdir/usr/bin/$exe"
done

# The AppDir root holds the desktop file, the icon and the entry point.
cp packaging/linux/constructcraft.desktop "$appdir/constructcraft.desktop"
cp assets/icons/app-icon-256.png "$appdir/constructcraft.png"
ln -s constructcraft.png "$appdir/.DirIcon"
cat > "$appdir/AppRun" <<'EOF'
#!/bin/sh
# Start ConstructCraft from wherever the AppImage is mounted.
here="$(dirname "$(readlink -f "$0")")"
exec "$here/usr/bin/constructcraft" "$@"
EOF
chmod 755 "$appdir/AppRun"

mkdir -p "$(dirname "$out")"
# The runtime option goes first; it unpacks appimagetool instead of mounting
# it, so the build machine needs no FUSE.
ARCH=$arch "$appimagetool" --appimage-extract-and-run --no-appstream --runtime-file "$runtime" "$appdir" "$out"
