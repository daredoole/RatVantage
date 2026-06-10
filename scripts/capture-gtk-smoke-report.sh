#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'EOF'
Usage: scripts/capture-gtk-smoke-report.sh --output <dir> [options]

Run the GTK dashboard against a private session-bus daemon under Xvfb and write
page screenshots plus supporting text diagnostics.

Options:
  --output <dir>          Required output directory.
  --sysfs-root <root>     Sysfs root for the private daemon. Default: /
  --pages <csv>           Pages to capture. Default: status,profiles,battery,gpu,fans,appearance,automations,settings,diagnostics
  --gsk-renderer <name>   GTK renderer override. Default: cairo
  --capture-delay-ms <n>  Delay before screenshot capture. Default: 3000
  --auto-quit-ms <n>      Auto-close the GTK window after N ms. Default: 5500
  -h, --help              Show this help.
EOF
}

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
output_dir=""
sysfs_root="/"
pages_csv="status,profiles,battery,gpu,fans,appearance,automations,settings,diagnostics"
gsk_renderer="cairo"
capture_delay_ms=3000
auto_quit_ms=5500

while (($#)); do
  case "$1" in
    --output)
      output_dir="${2:?missing value for --output}"
      shift 2
      ;;
    --sysfs-root)
      sysfs_root="${2:?missing value for --sysfs-root}"
      shift 2
      ;;
    --pages)
      pages_csv="${2:?missing value for --pages}"
      shift 2
      ;;
    --gsk-renderer)
      gsk_renderer="${2:?missing value for --gsk-renderer}"
      shift 2
      ;;
    --capture-delay-ms)
      capture_delay_ms="${2:?missing value for --capture-delay-ms}"
      shift 2
      ;;
    --auto-quit-ms)
      auto_quit_ms="${2:?missing value for --auto-quit-ms}"
      shift 2
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

if [[ -z "$output_dir" ]]; then
  echo "--output is required" >&2
  usage >&2
  exit 2
fi

command -v cargo >/dev/null 2>&1 || {
  echo "missing cargo; run from a Rust/Cargo environment" >&2
  exit 1
}

for tool in dbus-daemon xvfb-run import; do
  command -v "$tool" >/dev/null 2>&1 || {
    echo "missing $tool; run: scripts/install-dev-deps-fedora.sh" >&2
    exit 1
  }
done

mkdir -p "$output_dir"
output_dir="$(cd "$output_dir" && pwd)"
mkdir -p "$output_dir/screenshots"

tmpdir="$(mktemp -d)"
bus_address_file="$tmpdir/bus-address.txt"
daemon_log="$output_dir/daemon.log"
dbus_log="$output_dir/dbus.log"
commands_log="$output_dir/commands.log"
environment_txt="$output_dir/environment.txt"
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

log_command() {
  printf '$' >>"$commands_log"
  for arg in "$@"; do
    printf ' %q' "$arg" >>"$commands_log"
  done
  printf '\n' >>"$commands_log"
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

IFS=',' read -r -a pages <<<"$pages_csv"
for page in "${pages[@]}"; do
  case "$page" in
    status|profiles|battery|gpu|fans|appearance|automations|settings|diagnostics) ;;
    *)
      echo "unsupported GTK page: $page" >&2
      exit 2
      ;;
  esac
done

{
  echo "date=$(date --iso-8601=seconds)"
  echo "sysfs_root=$sysfs_root"
  echo "gsk_renderer=$gsk_renderer"
  echo "capture_delay_ms=$capture_delay_ms"
  echo "auto_quit_ms=$auto_quit_ms"
  echo "pages=$pages_csv"
} >"$environment_txt"

capture_delay_seconds="$(awk "BEGIN { printf \"%.3f\", $capture_delay_ms / 1000 }")"

log_command cargo build -q -p legion-control-daemon
log_command cargo build -q -p legion-control-ui --features gtk-ui
cargo build -q -p legion-control-daemon
cargo build -q -p legion-control-ui --features gtk-ui

dbus-daemon --session --print-address=1 --nofork >"$bus_address_file" 2>"$dbus_log" &
private_bus_pid="$!"
bus_address="$(wait_for_file_line "$bus_address_file")"

if [[ -z "$bus_address" ]]; then
  echo "failed to capture private session bus address" >&2
  exit 1
fi

log_command env DBUS_SESSION_BUS_ADDRESS="$bus_address" cargo run -q -p legion-control-daemon -- --session --sysfs-root "$sysfs_root" --state-path "$state_path"
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

log_command cargo run -q -p legion-control-ui -- --status --bus-address "$bus_address"
cargo run -q -p legion-control-ui -- --status --bus-address "$bus_address" >"$output_dir/status.txt"
log_command cargo run -q -p legion-control-ui -- --overview --bus-address "$bus_address"
cargo run -q -p legion-control-ui -- --overview --bus-address "$bus_address" >"$output_dir/overview.txt"
log_command cargo run -q -p legion-control-ui -- --diagnostics --bus-address "$bus_address"
cargo run -q -p legion-control-ui -- --diagnostics --bus-address "$bus_address" >"$output_dir/diagnostics.json"

for page in "${pages[@]}"; do
  page_png="$output_dir/screenshots/$page.png"
  page_log="$output_dir/$page-ui.log"
  log_command xvfb-run -a -s "-screen 0 1280x900x24" bash --noprofile --norc -lc "DBUS_SESSION_BUS_ADDRESS='$bus_address' GSK_RENDERER='$gsk_renderer' ADW_DEBUG_COLOR_SCHEME=prefer-dark GTK_A11Y=none GDK_BACKEND=x11 GDK_DISABLE=dmabuf cargo run -q -p legion-control-ui --features gtk-ui -- --bus-address '$bus_address' --gtk-page '$page' --gtk-auto-quit-ms '$auto_quit_ms'"
  xvfb-run -a -s "-screen 0 1280x900x24" bash --noprofile --norc -lc "
    set -euo pipefail
    export DBUS_SESSION_BUS_ADDRESS='$bus_address'
    export GSK_RENDERER='$gsk_renderer'
    export ADW_DEBUG_COLOR_SCHEME=prefer-dark
    export GTK_A11Y=none
    export GDK_BACKEND=x11
    export GDK_DISABLE=dmabuf
    export RATVANTAGE_GTK_DEFAULT_WIDTH=1200
    export RATVANTAGE_GTK_DEFAULT_HEIGHT=820
    cargo run -q -p legion-control-ui --features gtk-ui -- --bus-address '$bus_address' --gtk-page '$page' --gtk-auto-quit-ms '$auto_quit_ms' >'$page_log' 2>&1 &
    ui_pid=\$!
    sleep '$capture_delay_seconds'
    import -window 'RatVantage' '$page_png' || import -window root '$page_png'
    wait \$ui_pid
  "
done

{
  echo "# GTK Smoke Report"
  echo
  echo "- Sysfs root: \`$sysfs_root\`"
  echo "- Renderer: \`$gsk_renderer\`"
  echo "- Bus address: private session bus"
  echo "- Captured pages: ${pages[*]}"
  echo
  echo "## Artifacts"
  echo
  echo "- \`status.txt\`"
  echo "- \`overview.txt\`"
  echo "- \`diagnostics.json\`"
  echo "- \`daemon.log\`"
  for page in "${pages[@]}"; do
    echo "- \`screenshots/$page.png\`"
  done
} >"$output_dir/report.md"

echo "GTK smoke report written to $output_dir"
