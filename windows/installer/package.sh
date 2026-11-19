#!/bin/sh
# Builds the Windows release on Linux (also works in Git Bash / WSL) and writes, in dist/:
#   not-magick-mouse-<version>-x86_64-setup.exe     installer (NSIS)
#   not-magick-mouse-<version>-portable.zip         nmm.exe + nmm-gui.exe, no install needed
#   SHA256SUMS-windows.txt
#
# Needs: rust, the x86_64-pc-windows-gnu target, mingw-w64 (gcc + windres), nsis (makensis), zip.
#   Debian/Ubuntu: apt install gcc-mingw-w64-x86-64-posix binutils-mingw-w64-x86-64 nsis zip
#   rustup target add x86_64-pc-windows-gnu
set -eu
root=$(cd "$(dirname "$0")/../.." && pwd)
cd "$root"
target=${TARGET:-x86_64-pc-windows-gnu}
version=$(grep -m1 '^version = ' windows/daemon/Cargo.toml | cut -d'"' -f2)
bindir="$root/target/$target/release"
dist="$root/dist"
mkdir -p "$dist"

cargo build --release --target "$target" -p nmm -p nmm-gui

# Installer
setup="$dist/not-magick-mouse-$version-x86_64-setup.exe"
makensis -V2 \
    -DVERSION="$version" \
    -DBIN_DIR="$bindir" \
    -DREADME="$root/README.md" \
    -DICON="$root/assets/icon.ico" \
    -DOUTFILE="$setup" \
    windows/installer/nmm.nsi

# Portable zip
stage="$dist/portable"
zip="$dist/not-magick-mouse-$version-portable.zip"
rm -rf "$stage" "$zip"
mkdir -p "$stage"
cp "$bindir/nmm.exe" "$bindir/nmm-gui.exe" README.md "$stage/"
(cd "$stage" && zip -q -9 "$zip" nmm.exe nmm-gui.exe README.md)
rm -rf "$stage"

(cd "$dist" && sha256sum "$(basename "$setup")" "$(basename "$zip")" > SHA256SUMS-windows.txt)
echo "Created:"
echo "  $setup"
echo "  $zip"
