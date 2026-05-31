#!/usr/bin/env bash
# Restart the installed RatVantage daemon and user tray.
set -euo pipefail

daemon_unit="legion-control-daemon.service"
tray_launcher="$HOME/.local/bin/legion-control-tray-launch"
tray_bin="$HOME/.local/bin/legion-control-tray"
tray_log="$HOME/.cache/ratvantage/tray.log"

echo "Restarting system daemon: $daemon_unit"
sudo systemctl daemon-reload
sudo systemctl restart "$daemon_unit"
sudo systemctl --no-pager --lines=8 status "$daemon_unit" || true

echo
echo "Restarting user tray"
pkill -f "$tray_bin" 2>/dev/null || true
sleep 0.5

if [[ ! -x "$tray_launcher" ]]; then
  echo "missing tray launcher: $tray_launcher" >&2
  echo "run: scripts/install-user-session.sh" >&2
  exit 1
fi

mkdir -p "$(dirname "$tray_log")"
nohup "$tray_launcher" >/dev/null 2>&1 &
sleep 1

echo
echo "Running RatVantage processes:"
pgrep -af 'legion-control-daemon|legion-control-tray|legion-control-ui' || true

echo
echo "Tray log:"
tail -40 "$tray_log" 2>/dev/null || echo "no tray log yet: $tray_log"

if ! pgrep -f "$tray_bin" >/dev/null 2>&1; then
  echo
  echo "Tray is not running. If this script was not launched from your graphical session,"
  echo "start 'Legion Control Tray' from the app launcher and inspect: $tray_log"
fi
