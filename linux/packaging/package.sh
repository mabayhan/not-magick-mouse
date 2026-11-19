#!/bin/sh
# Builds nmmd and writes dist/not-magick-mouse-<version>-linux-<arch>.tar.gz
# plus a checksum file. Usage: linux/packaging/package.sh [arch-label]
set -eu
root=$(cd "$(dirname "$0")/../.." && pwd)
cd "$root"
arch=${1:-$(uname -m)}
version=$(grep -m1 '^version = ' linux/daemon/Cargo.toml | cut -d'"' -f2)
name="not-magick-mouse-$version-linux-$arch"

cargo build --release -p nmm-linux

stage="dist/$name"
rm -rf "$stage"
mkdir -p "$stage"
cp target/release/nmmd linux/packaging/nmmd.service linux/packaging/99-nmm.rules linux/packaging/install.sh README.md "$stage/"
chmod +x "$stage/install.sh"
tar -C dist -czf "dist/$name.tar.gz" "$name"
rm -rf "$stage"
(cd dist && sha256sum "$name.tar.gz" > "SHA256SUMS-linux-$arch.txt")
echo "Created dist/$name.tar.gz"
