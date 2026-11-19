#!/bin/sh
# Installs not-magick-mouse for the current user (any distribution with systemd). Run from
# the extracted release folder, or after `cargo build --release -p nmm-linux -p nmm-gui`
# from the repo root with BIN_DIR=target/release. Needs sudo once, for the udev rule.
# On Debian/Ubuntu/Fedora/RHEL prefer the .deb / .rpm packages instead.
set -eu
here=$(cd "$(dirname "$0")" && pwd)
bindir=${BIN_DIR:-$here}
icon=${ICON:-$here/not-magick-mouse.png}
[ -f "$icon" ] || icon=$here/../../assets/icon256.png

[ -x "$bindir/nmmd" ] || { echo "nmmd not found in $bindir (set BIN_DIR=...)" >&2; exit 1; }

install -Dm755 "$bindir/nmmd" "$HOME/.local/bin/nmmd"
[ -x "$bindir/nmm-gui" ] && install -Dm755 "$bindir/nmm-gui" "$HOME/.local/bin/nmm-gui"
install -Dm644 "$here/nmmd.service" "$HOME/.config/systemd/user/nmmd.service"
# The desktop entry points at the installed GUI by full path, so it works even when ~/.local/bin is not in PATH.
sed "s#^Exec=nmm-gui#Exec=$HOME/.local/bin/nmm-gui#" "$here/not-magick-mouse.desktop" > "$here/.nmm.desktop.tmp"
install -Dm644 "$here/.nmm.desktop.tmp" "$HOME/.local/share/applications/not-magick-mouse.desktop"
rm -f "$here/.nmm.desktop.tmp"
install -Dm644 "$icon" "$HOME/.local/share/icons/hicolor/256x256/apps/not-magick-mouse.png"
command -v update-desktop-database >/dev/null 2>&1 && update-desktop-database "$HOME/.local/share/applications" || true
command -v gtk-update-icon-cache >/dev/null 2>&1 && gtk-update-icon-cache -q -t "$HOME/.local/share/icons/hicolor" || true

sudo install -Dm644 "$here/99-nmm.rules" /etc/udev/rules.d/99-nmm.rules
sudo udevadm control --reload-rules
sudo udevadm trigger --subsystem-match=misc --subsystem-match=input --action=change || true
# Fallback for systems without logind seat ACLs: reading /dev/input/event* then needs the input group.
if ! id -nG "$USER" | tr ' ' '\n' | grep -qx input; then
    sudo usermod -aG input "$USER" || true
    echo "Added $USER to the 'input' group (only needed without seat ACLs): log out and back in if the daemon cannot open your mouse."
fi

systemctl --user daemon-reload
echo "Installed. Start with: systemctl --user enable --now nmmd"
echo "Then open the settings from your application menu (not-magick-mouse) or the tray icon."
echo "Config: ~/.config/not-magick-mouse/config.toml (reloads automatically)"
echo "On GNOME the tray icon needs the 'AppIndicator and KStatusNotifierItem Support' extension."
