#!/bin/sh
# Builds nmmd and nmm-gui and writes, into dist/:
#   not-magick-mouse-<version>-linux-<arch>.tar.gz   any distribution with systemd (install.sh inside)
#   not-magick-mouse_<version>_<deb-arch>.deb        Debian, Ubuntu, Mint, ...
#   not-magick-mouse-<version>-1.<arch>.rpm          Fedora, RHEL/Rocky/Alma, openSUSE, ...
#   SHA256SUMS-linux-<arch>.txt
# Usage: linux/packaging/package.sh [arch]   (default: this machine's)
# With cargo-zigbuild installed (pip install cargo-zigbuild) the binaries are linked
# against an old glibc (GLIBC=2.31 by default) so they also run on older distributions.
# The .deb needs dpkg-deb and the .rpm needs rpmbuild; a missing tool skips that package.
set -eu
umask 022 # some runners use 000, which dpkg-deb rejects
root=$(cd "$(dirname "$0")/../.." && pwd)
cd "$root"
arch=${1:-$(uname -m)}
version=$(grep -m1 '^version = ' linux/daemon/Cargo.toml | cut -d'"' -f2)
name="not-magick-mouse-$version-linux-$arch"
pk=linux/packaging

if command -v cargo-zigbuild >/dev/null 2>&1; then
    target="$arch-unknown-linux-gnu"
    cargo zigbuild --release --target "$target.${GLIBC:-2.31}" -p nmm-linux -p nmm-gui
    bin="target/$target/release"
else
    echo "cargo-zigbuild not found: building against this machine's glibc"
    cargo build --release -p nmm-linux -p nmm-gui
    bin=target/release
fi

mkdir -p dist

# ---- tarball ----
stage="dist/$name"
rm -rf "$stage"
mkdir -p "$stage"
cp "$bin/nmmd" "$bin/nmm-gui" "$pk/nmmd.service" "$pk/99-nmm.rules" "$pk/not-magick-mouse.desktop" "$pk/install.sh" README.md "$stage/"
cp assets/icon256.png "$stage/not-magick-mouse.png"
chmod +x "$stage/install.sh"
tar -C dist -czf "dist/$name.tar.gz" "$name"
rm -rf "$stage"
echo "Created dist/$name.tar.gz"

# ---- file tree shared by the .deb and .rpm ----
tree=$(mktemp -d)
trap 'rm -rf "$tree"' EXIT
install -Dm755 "$bin/nmmd" "$tree/usr/bin/nmmd"
install -Dm755 "$bin/nmm-gui" "$tree/usr/bin/nmm-gui"
mkdir -p "$tree/usr/lib/systemd/user"
sed 's#%h/.local/bin/nmmd#/usr/bin/nmmd#' "$pk/nmmd.service" > "$tree/usr/lib/systemd/user/nmmd.service"
install -Dm644 "$pk/99-nmm.rules" "$tree/usr/lib/udev/rules.d/99-nmm.rules"
install -Dm644 "$pk/not-magick-mouse.desktop" "$tree/usr/share/applications/not-magick-mouse.desktop"
install -Dm644 assets/icon256.png "$tree/usr/share/icons/hicolor/256x256/apps/not-magick-mouse.png"
install -Dm644 README.md "$tree/usr/share/doc/not-magick-mouse/README.md"

# Maintainer scripts, identical for both formats. The user service is enabled for every user
# (it starts at their next login) and the udev rule is applied right away.
cat > "$tree.post" <<'EOS'
udevadm control --reload-rules >/dev/null 2>&1 || true
udevadm trigger --subsystem-match=misc --subsystem-match=input --action=change >/dev/null 2>&1 || true
systemctl --global enable nmmd.service >/dev/null 2>&1 || true
echo "not-magick-mouse: log out and back in (or run: systemctl --user enable --now nmmd), then open it from the application menu."
EOS
cat > "$tree.preun" <<'EOS'
systemctl --global disable nmmd.service >/dev/null 2>&1 || true
EOS

# ---- .deb ----
if command -v dpkg-deb >/dev/null 2>&1; then
    case "$arch" in
        x86_64) deb_arch=amd64 ;;
        aarch64) deb_arch=arm64 ;;
        *) deb_arch=$arch ;;
    esac
    d=$(mktemp -d)
    cp -a "$tree/." "$d/"
    mkdir -p "$d/DEBIAN"
    size=$(du -sk "$tree" | cut -f1)
    cat > "$d/DEBIAN/control" <<EOF
Package: not-magick-mouse
Version: $version
Architecture: $deb_arch
Maintainer: not-magick-mouse <noreply@invalid>
Installed-Size: $size
Depends: libc6, libgl1, libxkbcommon0
Recommends: xdg-utils, libwayland-client0, libxkbcommon-x11-0
Suggests: gnome-shell-extension-appindicator
Section: utils
Priority: optional
Homepage: https://github.com/mabayhan/not-magick-mouse
Description: Mouse button remapper with a tray icon and settings window
 Remaps mouse buttons per mouse and per application (evdev/uinput, works on X11
 and Wayland). Includes the nmmd daemon (systemd user service), the nmm-gui
 settings window and a tray icon.
EOF
    { echo '#!/bin/sh'; echo 'set -e'; cat "$tree.post"; } > "$d/DEBIAN/postinst"
    { echo '#!/bin/sh'; echo 'set -e'; echo 'if [ "$1" = remove ] || [ "$1" = purge ]; then'; cat "$tree.preun"; echo 'fi'; } > "$d/DEBIAN/prerm"
    chmod 755 "$d/DEBIAN/postinst" "$d/DEBIAN/prerm"
    dpkg-deb --root-owner-group --build "$d" "dist/not-magick-mouse_${version}_${deb_arch}.deb" >/dev/null
    rm -rf "$d"
    echo "Created dist/not-magick-mouse_${version}_${deb_arch}.deb"
else
    echo "dpkg-deb not found: skipping the .deb"
fi

# ---- .rpm ----
if command -v rpmbuild >/dev/null 2>&1; then
    top=$(mktemp -d)
    mkdir -p "$top/SPECS"
    cat > "$top/SPECS/nmm.spec" <<EOF
Name: not-magick-mouse
Version: $version
Release: 1
Summary: Mouse button remapper with a tray icon and settings window
License: Unspecified
URL: https://github.com/mabayhan/not-magick-mouse
AutoReqProv: no
Requires: mesa-libGL, libxkbcommon
Recommends: xdg-utils
%global debug_package %{nil}
%global __os_install_post %{nil}

%description
Remaps mouse buttons per mouse and per application (evdev/uinput, works on X11
and Wayland). Includes the nmmd daemon (systemd user service), the nmm-gui
settings window and a tray icon.

%install
mkdir -p %{buildroot}
cp -a $tree/. %{buildroot}/

%post
$(cat "$tree.post")

%preun
if [ "\$1" = 0 ]; then
$(cat "$tree.preun")
fi

%files
/usr/bin/nmmd
/usr/bin/nmm-gui
/usr/lib/systemd/user/nmmd.service
/usr/lib/udev/rules.d/99-nmm.rules
/usr/share/applications/not-magick-mouse.desktop
/usr/share/icons/hicolor/256x256/apps/not-magick-mouse.png
/usr/share/doc/not-magick-mouse/README.md
EOF
    rpmbuild -bb --quiet --target "$arch-linux" --define "_topdir $top" "$top/SPECS/nmm.spec"
    cp "$top"/RPMS/*/*.rpm dist/
    rm -rf "$top"
    echo "Created dist/not-magick-mouse-$version-1.$arch.rpm"
else
    echo "rpmbuild not found: skipping the .rpm"
fi
rm -f "$tree.post" "$tree.preun"

(
    cd dist
    : > "SHA256SUMS-linux-$arch.txt"
    for f in "$name.tar.gz" "not-magick-mouse_${version}_"*.deb "not-magick-mouse-$version-1."*.rpm; do
        [ -f "$f" ] && sha256sum "$f" >> "SHA256SUMS-linux-$arch.txt"
    done
    cat "SHA256SUMS-linux-$arch.txt"
)
