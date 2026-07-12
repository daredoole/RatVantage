#!/usr/bin/env bash
set -euo pipefail

hold_seconds=8
bus_args=()
report_dir=""

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
    --report-dir)
      if [[ $# -lt 2 ]]; then
        echo "--report-dir requires a value" >&2
        exit 2
      fi
      report_dir="$2"
      shift 2
      ;;
    -h|--help)
      echo "Usage: $0 [--bus-address ADDRESS] [--hold-seconds SECONDS] [--report-dir DIR]"
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

if ! grep -q '^Hidden=false$' data/desktop/org.ratvantage.LegionControl.Tray.desktop; then
  echo "tray autostart desktop file must be enabled for the packaged session experience." >&2
  exit 1
fi

if ! grep -q '^X-GNOME-Autostart-enabled=true$' data/desktop/org.ratvantage.LegionControl.Tray.desktop; then
  echo "GNOME tray autostart flag must be enabled; icon visibility still depends on shell support." >&2
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

registered_items_raw() {
  busctl --user --no-pager get-property \
    org.kde.StatusNotifierWatcher \
    /StatusNotifierWatcher \
    org.kde.StatusNotifierWatcher \
    RegisteredStatusNotifierItems
}

registered_items() {
  registered_items_raw | grep -o '"[^"]*"' | tr -d '"' || true
}

write_report() {
  local dir="$1"
  local item="$2"
  local service="$3"
  local path="$4"
  mkdir -p "$dir"

  {
    printf 'desktop=%s\n' "${XDG_CURRENT_DESKTOP:-unknown}"
    printf 'session=%s\n' "${XDG_SESSION_TYPE:-unknown}"
    printf 'wayland_display=%s\n' "${WAYLAND_DISPLAY:-unknown}"
    printf 'display=%s\n' "${DISPLAY:-unknown}"
    printf 'dbus_session_bus=%s\n' "${DBUS_SESSION_BUS_ADDRESS:-unknown}"
    printf 'timestamp=%s\n' "$(date --iso-8601=seconds)"
  } >"$dir/environment.txt"

  printf 'before=%s\nafter=%s\n' "$before_count" "$after_count" >"$dir/watcher-counts.txt"
  registered_items_raw >"$dir/watcher-items.txt"
  busctl --user --no-pager get-property \
    org.kde.StatusNotifierWatcher \
    /StatusNotifierWatcher \
    org.kde.StatusNotifierWatcher \
    ProtocolVersion >"$dir/watcher-protocol.txt"

  "${tray_cmd[@]}" --status "${bus_args[@]}" >"$dir/tray-status.txt"
  "${tray_cmd[@]}" --tooltip "${bus_args[@]}" >"$dir/tray-tooltip.txt"
  "${tray_cmd[@]}" --menu-check "${bus_args[@]}" >"$dir/tray-menu-check.txt"
  "${tray_cmd[@]}" --desktop-check >"$dir/tray-desktop-check.txt"

  {
    printf 'registered_item=%s\n' "${item:-unknown}"
    printf 'service=%s\n' "${service:-unknown}"
    printf 'path=%s\n' "${path:-unknown}"
    if [[ -n "$service" && -n "$path" ]]; then
      busctl --user --no-pager get-property "$service" "$path" org.kde.StatusNotifierItem Id || true
      busctl --user --no-pager get-property "$service" "$path" org.kde.StatusNotifierItem Title || true
      busctl --user --no-pager get-property "$service" "$path" org.kde.StatusNotifierItem Category || true
      busctl --user --no-pager get-property "$service" "$path" org.kde.StatusNotifierItem Status || true
      busctl --user --no-pager get-property "$service" "$path" org.kde.StatusNotifierItem IconName || true
      busctl --user --no-pager get-property "$service" "$path" org.kde.StatusNotifierItem Menu || true
      busctl --user --no-pager get-property "$service" "$path" org.kde.StatusNotifierItem ToolTip || true
    fi
  } >"$dir/item-properties.txt"

  cat >"$dir/smoke-report.md" <<EOF
# StatusNotifier Smoke Report

- Desktop: ${XDG_CURRENT_DESKTOP:-unknown}
- Session type: ${XDG_SESSION_TYPE:-unknown}
- Wayland display: ${WAYLAND_DISPLAY:-unknown}
- Display: ${DISPLAY:-unknown}
- Bus override: ${bus_args[*]:-none}
- Registered item count: before=$before_count after=$after_count
- Registered item: ${item:-unknown}
- Service: ${service:-unknown}
- Path: ${path:-unknown}
- Autostart desktop file enabled: yes

## Included files

- \`environment.txt\`
- \`watcher-counts.txt\`
- \`watcher-protocol.txt\`
- \`watcher-items.txt\`
- \`tray-status.txt\`
- \`tray-tooltip.txt\`
- \`tray-menu-check.txt\`
- \`tray-desktop-check.txt\`
- \`item-properties.txt\`

## Manual checks still required

- Tray icon appears in the desktop panel.
- Tooltip looks correct in the shell UI.
- Menu reflects runtime profile, battery, preset, and pending-state rows.
- Menu exposes dashboard, refresh, and quit.
- Quit removes the tray item.
EOF
}

before_items="$(registered_items)"
before_count="$(registered_count)"
"${tray_cmd[@]}" "${bus_args[@]}" &
tray_pid="$!"
trap 'kill "$tray_pid" 2>/dev/null || true; wait "$tray_pid" 2>/dev/null || true' EXIT

after_count="$before_count"
after_items="$before_items"
for _ in {1..40}; do
  if ! kill -0 "$tray_pid" 2>/dev/null; then
    echo "tray process exited before registering a StatusNotifier item" >&2
    exit 1
  fi
  after_items="$(registered_items)"
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
echo "Autostart desktop entry is enabled."
echo "Visually confirm the RatVantage tray icon, tooltip, guarded menu, refresh, and quit behavior."

new_item="$(
  comm -13 \
    <(printf '%s\n' "$before_items" | sed '/^$/d' | sort -u) \
    <(printf '%s\n' "$after_items" | sed '/^$/d' | sort -u) \
    | head -n1
)"

if [[ -z "$new_item" ]]; then
  new_item="$(printf '%s\n' "$after_items" | sed '/^$/d' | tail -n1)"
fi

item_service=""
item_path=""
if [[ -n "$new_item" && "$new_item" == */* ]]; then
  item_service="${new_item%%/*}"
  item_path="/${new_item#*/}"
fi

if [[ -n "$report_dir" ]]; then
  write_report "$report_dir" "$new_item" "$item_service" "$item_path"
  echo "Smoke report written to $report_dir"
fi

sleep "$hold_seconds"
