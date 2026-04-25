#!/usr/bin/env bash
set -euo pipefail

hold_seconds=8
bus_args=()

while [[ $# -gt 0 ]]; do
  case "$1" in
    --bus-address)
      if [[ $# -lt 2 ]]; then
        echo "--bus-address requires a value" >&2
        exit 2
      fi
      bus_args=(--bus-address "$2")
      shift 2
      ;;
    --hold-seconds)
      if [[ $# -lt 2 ]]; then
        echo "--hold-seconds requires a value" >&2
        exit 2
      fi
      hold_seconds="$2"
      shift 2
      ;;
    -h|--help)
      echo "Usage: $0 [--bus-address ADDRESS] [--hold-seconds SECONDS]"
      exit 0
      ;;
    *)
      echo "unknown argument: $1" >&2
      exit 2
      ;;
  esac
done

if [[ -z "${DBUS_SESSION_BUS_ADDRESS:-}" ]]; then
  echo "DBUS_SESSION_BUS_ADDRESS is not set; run this from a graphical desktop session." >&2
  exit 1
fi

if ! command -v busctl >/dev/null; then
  echo "busctl is required for StatusNotifier smoke checks." >&2
  exit 1
fi

if ! busctl --user --no-pager get-property \
  org.kde.StatusNotifierWatcher \
  /StatusNotifierWatcher \
  org.kde.StatusNotifierWatcher \
  ProtocolVersion >/dev/null; then
  echo "org.kde.StatusNotifierWatcher is not available on the session bus." >&2
  echo "On GNOME, install and enable the AppIndicator/KStatusNotifier extension first." >&2
  exit 1
fi

if [[ -x target/debug/legion-control-tray ]]; then
  tray_cmd=(target/debug/legion-control-tray)
else
  tray_cmd=(cargo run -q -p legion-control-tray --)
fi

if ! "${tray_cmd[@]}" --status "${bus_args[@]}" >/dev/null; then
  echo "tray status check failed." >&2
  echo "Start the daemon first, or pass --bus-address for a private test daemon." >&2
  exit 1
fi

if ! grep -q '^Hidden=true$' data/desktop/org.ratvantage.LegionControl.Tray.desktop; then
  echo "tray autostart desktop file is not hidden; do not enable autostart before desktop smoke passes." >&2
  exit 1
fi

if ! grep -q '^X-GNOME-Autostart-enabled=false$' data/desktop/org.ratvantage.LegionControl.Tray.desktop; then
  echo "GNOME tray autostart is enabled; keep it disabled before desktop smoke passes." >&2
  exit 1
fi

registered_count() {
  busctl --user --no-pager get-property \
    org.kde.StatusNotifierWatcher \
    /StatusNotifierWatcher \
    org.kde.StatusNotifierWatcher \
    RegisteredStatusNotifierItems \
    | awk '{print $2}'
}

before_count="$(registered_count)"
"${tray_cmd[@]}" "${bus_args[@]}" &
tray_pid="$!"
trap 'kill "$tray_pid" 2>/dev/null || true; wait "$tray_pid" 2>/dev/null || true' EXIT

after_count="$before_count"
for _ in {1..40}; do
  if ! kill -0 "$tray_pid" 2>/dev/null; then
    echo "tray process exited before registering a StatusNotifier item" >&2
    exit 1
  fi
  after_count="$(registered_count)"
  if [[ "$after_count" =~ ^[0-9]+$ && "$before_count" =~ ^[0-9]+$ && "$after_count" -gt "$before_count" ]]; then
    break
  fi
  sleep 0.25
done

if [[ ! "$after_count" =~ ^[0-9]+$ || ! "$before_count" =~ ^[0-9]+$ || "$after_count" -le "$before_count" ]]; then
  echo "StatusNotifier item count did not increase within 10 seconds." >&2
  echo "before=$before_count after=$after_count" >&2
  exit 1
fi

echo "StatusNotifier registration smoke passed."
echo "before=$before_count after=$after_count"
echo "Autostart remains disabled."
echo "Visually confirm the Legion Control tray icon, tooltip, read-only menu, refresh, and quit behavior."
sleep "$hold_seconds"
