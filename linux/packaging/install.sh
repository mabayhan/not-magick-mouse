#!/bin/sh
# Installs nmmd for the current user. Run from the extracted release folder
# (or after `cargo build --release -p nmm-linux`, from the repo root with
# BIN=target/release/nmmd). Needs sudo once, for the udev rule and the input group.
set -eu
here=$(cd "$(dirname "$0")" && pwd)
bin=${BIN:-$here/nmmd}

[ -x "$bin" ] || { echo "nmmd binary not found at $bin (set BIN=...)" >&2; exit 1; }

install -Dm755 "$bin" "$HOME/.local/bin/nmmd"
install -Dm644 "$here/nmmd.service" "$HOME/.config/systemd/user/nmmd.service"

sudo install -Dm644 "$here/99-nmm.rules" /etc/udev/rules.d/99-nmm.rules
sudo udevadm control --reload-rules
sudo udevadm trigger --subsystem-match=misc --action=change || true
# Reading /dev/input/event* needs the input group.
if ! id -nG "$USER" | tr ' ' '\n' | grep -qx input; then
    sudo usermod -aG input "$USER"
    echo "Added $USER to the 'input' group: log out and back in before starting the service."
fi

systemctl --user daemon-reload
echo "Installed. Start with: systemctl --user enable --now nmmd"
echo "Config: ~/.config/not-magick-mouse/config.toml (reloads automatically)"
