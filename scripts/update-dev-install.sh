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
                      If scripts/install-dev-passwordless-updater.sh has been
                      installed once, this runs without a password prompt.
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
  if [[ "${RATVANTAGE_UPDATE_DEV_SKIP_DAEMON_BUILD:-0}" != "1" ]]; then
    echo "Building release daemon"
    cargo build --release -p legion-control-daemon
  fi

  helper="${RATVANTAGE_DEV_UPDATE_HELPER:-/usr/local/sbin/ratvantage-dev-update-daemon}"
  openrgb_setup_helper="${RATVANTAGE_OPENRGB_SETUP_HELPER:-/usr/local/sbin/ratvantage-setup-keyboard-rgb-openrgb-access}"
  sudo_cmd="${RATVANTAGE_SUDO_BIN:-sudo}"
  helper_is_current=0
  if [[ -x "$helper" ]] \
    && grep -q -- 'ratvantage-dev-update-daemon-capability: repo-driven-daemon-args-v1' "$helper" \
    && grep -q -- 'scripts/dev-daemon-args.sh' "$helper" \
    && grep -q -- 'setup-keyboard-rgb-openrgb-access.sh' "$helper"; then
    helper_is_current=1
  fi

  daemon_updated=0
  if [[ -x "$helper" && "$helper_is_current" -eq 1 ]] && "$sudo_cmd" -n "$helper" "$repo_root"; then
    daemon_updated=1
  fi

  if [[ "$daemon_updated" -eq 0 && -x "$helper" && "$helper_is_current" -eq 0 ]]; then
    echo "Passwordless daemon updater is installed but stale." >&2
    echo "It cannot enable OpenRGB access setup or install:" >&2
    echo "  $openrgb_setup_helper" >&2
    echo "Refresh the root-owned helper once:" >&2
    echo "  sudo $repo_root/scripts/install-dev-passwordless-updater.sh" >&2
    if ! "$sudo_cmd" -n true 2>/dev/null && [[ ! -t 0 ]]; then
      echo "No interactive sudo is available in this session; stopping before daemon install." >&2
      exit 1
    fi

    echo "Refreshing passwordless daemon updater"
    "$sudo_cmd" "$repo_root/scripts/install-dev-passwordless-updater.sh"
    "$sudo_cmd" -n "$helper" "$repo_root"
    daemon_updated=1
  fi

  if [[ "$daemon_updated" -eq 0 ]]; then
    echo "Passwordless daemon updater is not installed or failed." >&2
    echo "For non-interactive daemon updates, run once:" >&2
    echo "  sudo $repo_root/scripts/install-dev-passwordless-updater.sh" >&2
    if ! "$sudo_cmd" -n true 2>/dev/null && [[ ! -t 0 ]]; then
      echo "No interactive sudo is available in this session; stopping before daemon install." >&2
      exit 1
    fi

    echo "Installing system D-Bus/polkit integration"
    "$sudo_cmd" "$repo_root/scripts/install-dev-system-integration.sh"

    echo "Installing system daemon"
    daemon_args_file="$repo_root/scripts/dev-daemon-args.sh"
    if [[ ! -x "$daemon_args_file" ]]; then
      echo "daemon args helper must exist and be executable: $daemon_args_file" >&2
      exit 2
    fi
    mapfile -t daemon_args < <("$daemon_args_file")
    "$sudo_cmd" "$repo_root/scripts/install-dev-systemd-ratvantage.sh" \
      "$repo_root/target/release/legion-control-daemon" -- \
      "${daemon_args[@]}"

    "$sudo_cmd" systemctl daemon-reload
    "$sudo_cmd" busctl call org.freedesktop.DBus /org/freedesktop/DBus \
      org.freedesktop.DBus ReloadConfig >/dev/null
    "$sudo_cmd" systemctl restart legion-control-daemon.service
  fi

  if [[ ! -x "$openrgb_setup_helper" ]]; then
    echo "OpenRGB access setup helper is not installed yet:" >&2
    echo "  $openrgb_setup_helper" >&2
    echo "To install the updated root-owned helper, run once:" >&2
    echo "  sudo $repo_root/scripts/install-dev-passwordless-updater.sh" >&2
  fi
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
