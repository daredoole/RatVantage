#!/usr/bin/env bash
# Refresh the installed dev tray/dashboard, optionally including the system daemon.
set -euo pipefail

usage() {
  cat <<'EOF'
Usage: scripts/update-dev-install.sh [options]

Build and install the current worktree's tray + GTK dashboard into ~/.local/bin,
then restart the user tray. This is the fast path after UI/tray edits.

Options:
  --daemon            Also build, install, and restart the system daemon using
                      the broad current dev write-flag set.
  --no-restart-tray   Install user binaries but do not restart the tray process.
  -h, --help          Show this help.

Examples:
  scripts/update-dev-install.sh
  scripts/update-dev-install.sh --daemon
EOF
}

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
install_daemon=0
restart_tray=1

while [[ $# -gt 0 ]]; do
  case "$1" in
    --daemon)
      install_daemon=1
      shift
      ;;
    --no-restart-tray)
      restart_tray=0
      shift
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      echo "unknown argument: $1" >&2
      usage >&2
      exit 2
      ;;
  esac
done

cd "$repo_root"

if (( install_daemon )); then
  echo "Building release daemon"
  cargo build --release -p legion-control-daemon

  echo "Installing system D-Bus/polkit integration"
  sudo "$repo_root/scripts/install-dev-system-integration.sh"

  echo "Installing system daemon"
  sudo "$repo_root/scripts/install-dev-systemd-ratvantage.sh" \
    "$repo_root/target/release/legion-control-daemon" -- \
    --enable-platform-profile-write \
    --enable-battery-charge-type-write \
    --enable-led-state-write \
    --enable-ideapad-toggle-write \
    --enable-camera-power-write \
    --enable-usb-charging-write \
    --enable-fan-mode-write \
    --enable-gpu-mode-write \
    --enable-cpu-governor-write \
    --enable-cpu-epp-write \
    --enable-firmware-attribute-write \
    --enable-cpu-boost-write \
    --enable-conservation-mode-write \
    --enable-amd-gpu-dpm-write \
    --enable-curve-optimizer-write \
    --enable-hardware-profile-apply \
    --enable-automation-observer

  sudo systemctl daemon-reload
  sudo busctl call org.freedesktop.DBus /org/freedesktop/DBus \
    org.freedesktop.DBus ReloadConfig >/dev/null
  sudo systemctl restart legion-control-daemon.service
fi

echo "Installing user tray/dashboard"
"$repo_root/scripts/install-user-session.sh"

if (( restart_tray )); then
  tray_bin="$HOME/.local/bin/legion-control-tray"
  tray_launcher="$HOME/.local/bin/legion-control-tray-launch"
  tray_log="$HOME/.cache/ratvantage/tray.log"

  echo "Restarting user tray"
  pkill -f "$tray_bin" 2>/dev/null || true
  sleep 0.5
  mkdir -p "$(dirname "$tray_log")"
  nohup "$tray_launcher" >/dev/null 2>&1 &
  sleep 1

  if ! pgrep -f "$tray_bin" >/dev/null 2>&1; then
    echo "Tray is not running; start 'Legion Control Tray' from the app launcher." >&2
    echo "Tray log: $tray_log" >&2
  fi
fi

echo
echo "Installed binaries:"
"$HOME/.local/bin/legion-control-ui" --version 2>/dev/null || true
"$HOME/.local/bin/legion-control-tray" --version 2>/dev/null || true

if command -v systemctl >/dev/null 2>&1; then
  echo
  systemctl is-active legion-control-daemon.service 2>/dev/null \
    | sed 's/^/daemon=/'
fi

echo "Done."
