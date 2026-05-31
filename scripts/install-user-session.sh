#!/usr/bin/env bash
# Install tray + GTK UI into ~/.local/bin and enable tray autostart (no root).
set -euo pipefail
repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"
cargo build --release -p legion-control-tray -p legion-control-ui --features legion-control-ui/gtk-ui
mkdir -p "$HOME/.local/bin" "$HOME/.local/share/applications" "$HOME/.config/autostart"
install -m0755 "$repo_root/target/release/legion-control-tray" "$HOME/.local/bin/legion-control-tray"
install -m0755 "$repo_root/target/release/legion-control-ui" "$HOME/.local/bin/legion-control-ui"
cat >"$HOME/.local/bin/legion-control-tray-launch" <<EOF
#!/usr/bin/env bash
set -euo pipefail
mkdir -p "\$HOME/.cache/ratvantage"
{
  printf '\\n[%s] starting Legion Control Tray\\n' "\$(date --iso-8601=seconds)"
  exec "$HOME/.local/bin/legion-control-tray"
} >>"\$HOME/.cache/ratvantage/tray.log" 2>&1
EOF
chmod 0755 "$HOME/.local/bin/legion-control-tray-launch"
cat >"$HOME/.local/share/applications/org.ratvantage.LegionControl.desktop" <<EOF
[Desktop Entry]
Type=Application
Name=Legion Control
GenericName=RatVantage
Comment=Probe Lenovo Legion hardware capabilities on Fedora (RatVantage)
Exec=env GSK_RENDERER=cairo $HOME/.local/bin/legion-control-ui
Icon=org.ratvantage.LegionControl
Terminal=false
Categories=Settings;HardwareSettings;
Keywords=RatVantage;Legion;Lenovo;Fan;Battery;Power;Hardware;
EOF
cat >"$HOME/.local/share/applications/org.ratvantage.LegionControl.Tray.desktop" <<EOF
[Desktop Entry]
Type=Application
Name=Legion Control Tray
Comment=Read-only Legion Control tray/status helper
Exec=$HOME/.local/bin/legion-control-tray-launch
Icon=org.ratvantage.LegionControl
Terminal=false
NoDisplay=false
Hidden=false
X-GNOME-Autostart-enabled=true
EOF
cat >"$HOME/.config/autostart/org.ratvantage.LegionControl.Tray.desktop" <<EOF
[Desktop Entry]
Type=Application
Name=Legion Control Tray
Comment=Legion Control status notifier tray
Exec=$HOME/.local/bin/legion-control-tray-launch
Icon=org.ratvantage.LegionControl
Terminal=false
NoDisplay=false
Hidden=false
X-GNOME-Autostart-enabled=true
EOF
echo "Installed ~/.local/bin/legion-control-{tray,ui} and tray autostart."
echo "Ensure ~/.local/bin is on PATH (log out/in if needed)."
