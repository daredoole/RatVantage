#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'EOF'
Usage: scripts/run-local-session-app.sh --frontend <mode> [options]

Start a private session-bus daemon and run a local RatVantage frontend against it.

Frontend modes:
  status         legion-control-ui --status
  overview       legion-control-ui --overview
  diagnostics    legion-control-ui --diagnostics
  tray-status    legion-control-tray --status
  tray-tooltip   legion-control-tray --tooltip
  menu-check     legion-control-tray --menu-check
  tray           legion-control-tray foreground tray process
  ui             legion-control-ui GTK shell foreground window

Options:
  --frontend <mode>      Required frontend mode.
  --sysfs-root <root>    Sysfs root for the private daemon. Default: /
  --gsk-renderer <name>  Set GSK_RENDERER for GTK UI mode.
  --gdk-disable <value>  Set GDK_DISABLE for GTK UI mode.
  --gtk-page <page>      GTK page for UI mode: status, profiles, battery, fans, appearance, diagnostics.
  --gtk-auto-quit-ms <n> Auto-close GTK UI after N milliseconds.
  --dry-run              Print commands instead of running them.
  -h, --help             Show this help.
EOF
}

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
frontend=""
sysfs_root="/"
gsk_renderer=""
gdk_disable=""
gtk_page=""
gtk_auto_quit_ms=""
dry_run=0

while (($#)); do
  case "$1" in
    --frontend)
      frontend="${2:?missing value for --frontend}"
      shift 2
      ;;
    --sysfs-root)
      sysfs_root="${2:?missing value for --sysfs-root}"
      shift 2
      ;;
    --gsk-renderer)
      gsk_renderer="${2:?missing value for --gsk-renderer}"
      shift 2
      ;;
    --gdk-disable)
      gdk_disable="${2:?missing value for --gdk-disable}"
      shift 2
      ;;
    --gtk-page)
      gtk_page="${2:?missing value for --gtk-page}"
      shift 2
      ;;
    --gtk-auto-quit-ms)
      gtk_auto_quit_ms="${2:?missing value for --gtk-auto-quit-ms}"
      shift 2
      ;;
    --dry-run)
      dry_run=1
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

if [[ -z "$frontend" ]]; then
  echo "--frontend is required" >&2
  usage >&2
  exit 2
fi

case "$frontend" in
  status|overview|diagnostics|tray-status|tray-tooltip|menu-check|tray|ui) ;;
  *)
    echo "unsupported frontend: $frontend" >&2
    exit 2
    ;;
esac

command -v cargo >/dev/null 2>&1 || {
  echo "missing cargo; run from a Rust/Cargo environment" >&2
  exit 1
}

command -v dbus-daemon >/dev/null 2>&1 || {
  echo "missing dbus-daemon; run: scripts/install-dev-deps-fedora.sh" >&2
  exit 1
}

tmpdir="$(mktemp -d)"
bus_address_file="$tmpdir/bus-address.txt"
daemon_log="$tmpdir/daemon.log"
dbus_log="$tmpdir/dbus.log"
state_path="$tmpdir/state.toml"
private_bus_pid=""
daemon_pid=""

cleanup() {
  if [[ -n "$daemon_pid" ]]; then
    kill "$daemon_pid" 2>/dev/null || true
    wait "$daemon_pid" 2>/dev/null || true
  fi
  if [[ -n "$private_bus_pid" ]]; then
    kill "$private_bus_pid" 2>/dev/null || true
    wait "$private_bus_pid" 2>/dev/null || true
  fi
  rm -rf "$tmpdir"
}
trap cleanup EXIT

print_command() {
  printf '$'
  for arg in "$@"; do
    printf ' %q' "$arg"
  done
  printf '\n'
}

wait_for_file_line() {
  local path="$1"
  for _ in {1..100}; do
    if [[ -s "$path" ]]; then
      head -n1 "$path"
      return 0
    fi
    sleep 0.1
  done
  return 1
}

if (( dry_run )); then
  print_command dbus-daemon --session --print-address=1 --nofork
  print_command env DBUS_SESSION_BUS_ADDRESS='<private-bus>' \
    cargo run -q -p legion-control-daemon -- --session --sysfs-root "$sysfs_root" --state-path "$state_path"
  case "$frontend" in
    status)
      print_command cargo run -q -p legion-control-ui -- --status --bus-address '<private-bus>'
      ;;
    overview)
      print_command cargo run -q -p legion-control-ui -- --overview --bus-address '<private-bus>'
      ;;
    diagnostics)
      print_command cargo run -q -p legion-control-ui -- --diagnostics --bus-address '<private-bus>'
      ;;
    tray-status)
      print_command cargo run -q -p legion-control-tray -- --status --bus-address '<private-bus>'
      ;;
    tray-tooltip)
      print_command cargo run -q -p legion-control-tray -- --tooltip --bus-address '<private-bus>'
      ;;
    menu-check)
      print_command cargo run -q -p legion-control-tray -- --menu-check --bus-address '<private-bus>'
      ;;
    tray)
      print_command cargo run -q -p legion-control-tray -- --bus-address '<private-bus>'
      ;;
    ui)
      if [[ -n "$gsk_renderer" ]]; then
        gtk_args=()
        if [[ -n "$gtk_page" ]]; then
          gtk_args+=(--gtk-page "$gtk_page")
        fi
        if [[ -n "$gtk_auto_quit_ms" ]]; then
          gtk_args+=(--gtk-auto-quit-ms "$gtk_auto_quit_ms")
        fi
        print_command env GSK_RENDERER="$gsk_renderer" cargo run -q -p legion-control-ui --features gtk-ui -- --bus-address '<private-bus>' "${gtk_args[@]}"
      else
        gtk_args=()
        if [[ -n "$gtk_page" ]]; then
          gtk_args+=(--gtk-page "$gtk_page")
        fi
        if [[ -n "$gtk_auto_quit_ms" ]]; then
          gtk_args+=(--gtk-auto-quit-ms "$gtk_auto_quit_ms")
        fi
        print_command cargo run -q -p legion-control-ui --features gtk-ui -- --bus-address '<private-bus>' "${gtk_args[@]}"
      fi
      ;;
  esac
  exit 0
fi

dbus-daemon --session --print-address=1 --nofork >"$bus_address_file" 2>"$dbus_log" &
private_bus_pid="$!"
bus_address="$(wait_for_file_line "$bus_address_file")"

if [[ -z "$bus_address" ]]; then
  echo "failed to capture private session bus address" >&2
  exit 1
fi

env DBUS_SESSION_BUS_ADDRESS="$bus_address" \
  cargo run -q -p legion-control-daemon -- --session --sysfs-root "$sysfs_root" --state-path "$state_path" \
  >"$daemon_log" 2>&1 &
daemon_pid="$!"

for _ in {1..100}; do
  if ! kill -0 "$daemon_pid" 2>/dev/null; then
    echo "private daemon exited before becoming ready" >&2
    sed -n '1,120p' "$daemon_log" >&2 || true
    exit 1
  fi
  if grep -q 'serving interface=' "$daemon_log"; then
    break
  fi
  sleep 0.1
done

case "$frontend" in
  status)
    exec cargo run -q -p legion-control-ui -- --status --bus-address "$bus_address"
    ;;
  overview)
    exec cargo run -q -p legion-control-ui -- --overview --bus-address "$bus_address"
    ;;
  diagnostics)
    exec cargo run -q -p legion-control-ui -- --diagnostics --bus-address "$bus_address"
    ;;
  tray-status)
    exec cargo run -q -p legion-control-tray -- --status --bus-address "$bus_address"
    ;;
  tray-tooltip)
    exec cargo run -q -p legion-control-tray -- --tooltip --bus-address "$bus_address"
    ;;
  menu-check)
    exec cargo run -q -p legion-control-tray -- --menu-check --bus-address "$bus_address"
    ;;
  tray)
    exec cargo run -q -p legion-control-tray -- --bus-address "$bus_address"
    ;;
  ui)
    ui_cmd=(cargo run -q -p legion-control-ui --features gtk-ui -- --bus-address "$bus_address")
    if [[ -n "$gtk_page" ]]; then
      ui_cmd+=(--gtk-page "$gtk_page")
    fi
    if [[ -n "$gtk_auto_quit_ms" ]]; then
      ui_cmd+=(--gtk-auto-quit-ms "$gtk_auto_quit_ms")
    fi
    if [[ -n "$gsk_renderer" || -n "$gdk_disable" ]]; then
      env_vars=()
      if [[ -n "$gsk_renderer" ]]; then
        env_vars+=("GSK_RENDERER=$gsk_renderer")
      fi
      if [[ -n "$gdk_disable" ]]; then
        env_vars+=("GDK_DISABLE=$gdk_disable")
      fi
      exec env "${env_vars[@]}" "${ui_cmd[@]}"
    fi
    exec "${ui_cmd[@]}"
    ;;
esac
