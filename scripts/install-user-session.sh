#!/usr/bin/env bash
# Install tray + GTK UI into ~/.local/bin and enable tray autostart (no root).
set -euo pipefail
repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
mkdir -p "$HOME/.local/bin" "$HOME/.local/share/applications" "$HOME/.config/autostart"
install -m0755 "$repo_root/target/release/legion-control-tray" "$HOME/.local/bin/legion-control-tray"
install -m0755 "$repo_root/target/release/legion-control-ui" "$HOME/.local/bin/legion-control-ui"
cp "$repo_root/data/desktop/org.ratvantage.LegionControl.desktop" "$HOME/.local/share/applications/"
cp "$repo_root/data/desktop/org.ratvantage.LegionControl.Tray.desktop" "$HOME/.local/share/applications/"
cat >"$HOME/.config/autostart/org.ratvantage.LegionControl.Tray.desktop" <<EOF
[Desktop Entry]
Type=Application
Name=Legion Control Tray
Comment=Legion Control status notifier tray
Exec=$HOME/.local/bin/legion-control-tray
Icon=org.ratvantage.LegionControl
Terminal=false
NoDisplay=false
Hidden=false
X-GNOME-Autostart-enabled=true
EOF
echo "Installed ~/.local/bin/legion-control-{tray,ui} and tray autostart."
echo "Ensure ~/.local/bin is on PATH (log out/in if needed)."
