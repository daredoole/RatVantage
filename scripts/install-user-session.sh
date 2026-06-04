#!/usr/bin/env bash
# Install tray + GTK UI into ~/.local/bin and enable tray autostart (no root).
set -euo pipefail
repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"
cargo build --release -p legion-control-tray -p legion-control-ui -p legion-probe --features legion-control-ui/gtk-ui
mkdir -p \
  "$HOME/.local/bin" \
  "$HOME/.local/libexec/ratvantage" \
  "$HOME/.local/share/applications" \
  "$HOME/.config/autostart"
install -m0755 "$repo_root/target/release/legion-control-tray" "$HOME/.local/bin/legion-control-tray"
install -m0755 "$repo_root/target/release/legion-control-ui" "$HOME/.local/bin/legion-control-ui"
install -m0755 "$repo_root/target/release/legion-probe" "$HOME/.local/bin/legion-probe"
install -m0755 "$repo_root/scripts/check-keyboard-rgb-openrgb.sh" \
  "$HOME/.local/bin/ratvantage-check-keyboard-rgb-openrgb"
install -m0755 "$repo_root/scripts/capture-keyboard-rgb-evidence.sh" \
  "$HOME/.local/bin/ratvantage-capture-keyboard-rgb-evidence"
install -m0755 "$repo_root/scripts/compare-keyboard-rgb-evidence.sh" \
  "$HOME/.local/bin/ratvantage-compare-keyboard-rgb-evidence"
install -m0755 "$repo_root/scripts/setup-keyboard-rgb-openrgb-access.sh" \
  "$HOME/.local/libexec/ratvantage/ratvantage-setup-keyboard-rgb-openrgb-access"
cat >"$HOME/.local/bin/ratvantage-setup-keyboard-rgb-openrgb-access" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail

target_user="${USER:-$(id -un)}"
root_helper=/usr/local/sbin/ratvantage-setup-keyboard-rgb-openrgb-access
local_helper="$HOME/.local/libexec/ratvantage/ratvantage-setup-keyboard-rgb-openrgb-access"

if [[ -x "$root_helper" ]] && sudo -n "$root_helper" --user "$target_user" "$@"; then
  exit 0
fi

echo "Passwordless OpenRGB access setup is not installed or failed." >&2
echo "For no-prompt setup in this dev worktree, run once:" >&2
echo "  sudo scripts/install-dev-passwordless-updater.sh" >&2
echo "Falling back to an interactive sudo setup command." >&2
exec sudo "$local_helper" --user "$target_user" "$@"
EOF
chmod 0755 "$HOME/.local/bin/ratvantage-setup-keyboard-rgb-openrgb-access"
install -m0755 "$repo_root/scripts/capture-keyboard-rgb-openrgb-bridge-evidence.sh" \
  "$HOME/.local/bin/ratvantage-capture-keyboard-rgb-openrgb-bridge-evidence"
install -m0755 "$repo_root/scripts/review-keyboard-rgb-openrgb-bridge-evidence.sh" \
  "$HOME/.local/bin/ratvantage-review-keyboard-rgb-openrgb-bridge-evidence"
install -m0755 "$repo_root/scripts/status-keyboard-rgb-openrgb-bridge-evidence.sh" \
  "$HOME/.local/bin/ratvantage-keyboard-rgb-openrgb-bridge-status"
install -m0755 "$repo_root/scripts/capture-keyboard-rgb-openrgb-sdk-evidence.sh" \
  "$HOME/.local/bin/ratvantage-capture-keyboard-rgb-openrgb-sdk-evidence"
install -m0755 "$repo_root/scripts/capture-keyboard-rgb-openrgb-sdk-write-evidence.sh" \
  "$HOME/.local/bin/ratvantage-capture-keyboard-rgb-openrgb-sdk-write-evidence"
install -m0755 "$repo_root/scripts/openrgb-keyboard-rgb-sdk-helper.sh" \
  "$HOME/.local/bin/ratvantage-openrgb-keyboard-rgb-sdk-helper"
install -m0755 "$repo_root/scripts/openrgb-sdk-server-session.sh" \
  "$HOME/.local/bin/ratvantage-openrgb-sdk-server"
install -m0755 "$repo_root/scripts/capture-compatibility-bundle.sh" \
  "$HOME/.local/bin/ratvantage-capture-compatibility-bundle"
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
echo "Installed ~/.local/bin/legion-control-{tray,ui}, legion-probe, compatibility/RGB access/evidence helpers, and tray autostart."
echo "Ensure ~/.local/bin is on PATH (log out/in if needed)."
