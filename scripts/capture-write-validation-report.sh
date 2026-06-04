#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'EOF'
Usage: scripts/capture-write-validation-report.sh --output <bundle-dir> [options]

Capture a validation bundle for the currently implemented reversible write surface.

Default mode is plan-only:
- starts a private session bus and read-mostly daemon
- captures status, overview, diagnostics, tray/menu evidence, and write plans
- also captures dry-run plans for fan preset apply, restore-to-auto, GPU mode,
  CPU/PPT controls, Curve Optimizer, and saved profile apply when available
- never attempts hardware-changing writes

Execute mode is explicit and requires an already-running privileged daemon:
- add --execute
- target either --system-bus or --bus-address <address>
- the script records set/revert results, but still expects operator review
- prefer --execute-only <control_id> so each PR/evidence bundle exercises one
  write family at a time (still captures plans for other controls)
- advanced controls only execute when --execute-only names their exact control_id

Options:
  --output <dir>         Required bundle directory.
  --sysfs-root <root>    Sysfs root for plan-only private-daemon runs. Default: /
  --bus-address <addr>   Use an existing daemon on the given D-Bus address.
  --system-bus           Use the system bus instead of a custom bus address.
  --execute              Attempt real reversible writes and then revert them.
  --execute-only <id>    With --execute, only apply/revert this control id
                         (see docs/live-write-validation.md). Plans for all
                         controls still run when available.
  --seed-hardware-profile <PROFILE_ID=JSON>
                         Store a daemon hardware profile before capture.
                         Prefix JSON with @ to read it from a file.
  --seed-hardware-profile-trigger <TRIGGER_ID=PROFILE_ID>
                         Store a daemon trigger mapping before capture.
  --skip-compat-bundle   Skip nested compatibility/fixture capture evidence.
  --skip-tray-smoke      Skip StatusNotifier tray smoke/report capture.
  --hold-seconds <n>     Hold window for tray smoke reports. Default: 1
  -h, --help             Show this help.

Examples:
  scripts/capture-write-validation-report.sh \
    --output target/validation/82wm-plan

  scripts/capture-write-validation-report.sh \
    --output target/validation/82wm-live \
    --execute \
    --system-bus

  scripts/capture-write-validation-report.sh \
    --output target/validation/82wm-live-platform-profile \
    --execute \
    --execute-only platform_profile \
    --system-bus
EOF
}

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
compat_script="$repo_root/scripts/capture-compat-report.sh"
tray_smoke_script="$repo_root/scripts/smoke-statusnotifier-tray.sh"

output=""
sysfs_root="/"
bus_address=""
use_system_bus=0
execute_writes=0
execute_only=""
seed_hardware_profiles=()
seed_hardware_profile_triggers=()
capture_compat_bundle=1
capture_tray_smoke=1
hold_seconds=1

while (($#)); do
  case "$1" in
    --output)
      output="${2:?missing value for --output}"
      shift 2
      ;;
    --sysfs-root)
      sysfs_root="${2:?missing value for --sysfs-root}"
      shift 2
      ;;
    --bus-address)
      bus_address="${2:?missing value for --bus-address}"
      shift 2
      ;;
    --system-bus)
      use_system_bus=1
      shift
      ;;
    --execute)
      execute_writes=1
      shift
      ;;
    --execute-only)
      execute_only="${2:?missing value for --execute-only}"
      shift 2
      ;;
    --seed-hardware-profile)
      seed_hardware_profiles+=("${2:?missing value for --seed-hardware-profile}")
      shift 2
      ;;
    --seed-hardware-profile-trigger)
      seed_hardware_profile_triggers+=("${2:?missing value for --seed-hardware-profile-trigger}")
      shift 2
      ;;
    --skip-compat-bundle)
      capture_compat_bundle=0
      shift
      ;;
    --skip-tray-smoke)
      capture_tray_smoke=0
      shift
      ;;
    --hold-seconds)
      hold_seconds="${2:?missing value for --hold-seconds}"
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

if [[ -z "$output" ]]; then
  echo "--output is required" >&2
  usage >&2
  exit 2
fi

if (( use_system_bus )) && [[ -n "$bus_address" ]]; then
  echo "choose either --system-bus or --bus-address" >&2
  exit 2
fi

if (( execute_writes )) && (( !use_system_bus )) && [[ -z "$bus_address" ]]; then
  echo "--execute requires either --system-bus or --bus-address" >&2
  exit 2
fi

if [[ -n "$execute_only" ]] && (( !execute_writes )); then
  echo "--execute-only requires --execute" >&2
  exit 2
fi

command -v cargo >/dev/null 2>&1 || {
  echo "missing cargo; run from a Rust/Cargo environment" >&2
  exit 1
}

command -v python3 >/dev/null 2>&1 || {
  echo "missing python3; install Python 3 to generate validation summaries" >&2
  exit 1
}

command -v dbus-daemon >/dev/null 2>&1 || {
  echo "missing dbus-daemon; run: scripts/install-dev-deps-fedora.sh" >&2
  exit 1
}

mkdir -p "$output"/{before,after,steps,logs}

commands_log="$output/commands.log"
environment_txt="$output/environment.txt"
metadata_json="$output/metadata.json"
controls_json="$output/controls.json"
results_tsv="$output/results.tsv"
report_json="$output/validation-report.json"
report_md="$output/validation-report.md"
operator_md="$output/operator-checklist.md"
daemon_dry_run="$output/logs/daemon-dry-run.txt"
daemon_log="$output/logs/private-daemon.log"
dbus_log="$output/logs/private-dbus.log"
tray_smoke_log="$output/logs/tray-smoke.txt"
state_path="$output/private-daemon-state.toml"
compat_output="$output/compat"
tray_smoke_dir="$output/tray-smoke"
bus_address_file="$output/logs/private-bus-address.txt"

target_bus_mode="system"
private_bus_pid=""
private_daemon_pid=""

cleanup() {
  if [[ -n "$private_daemon_pid" ]]; then
    kill "$private_daemon_pid" 2>/dev/null || true
    wait "$private_daemon_pid" 2>/dev/null || true
  fi
  if [[ -n "$private_bus_pid" ]]; then
    kill "$private_bus_pid" 2>/dev/null || true
    wait "$private_bus_pid" 2>/dev/null || true
  fi
}
trap cleanup EXIT

sanitize_field() {
  printf '%s' "$1" | tr '\t\r\n' '   '
}

record_command() {
  {
    printf '$'
    for arg in "$@"; do
      printf ' %q' "$arg"
    done
    printf '\n'
  } >>"$commands_log"
}

run_capture() {
  local destination="$1"
  shift

  record_command "$@"
  set +e
  "$@" >"$destination" 2>&1
  local exit_code=$?
  set -e
  printf '%s\n' "$exit_code" >"${destination}.exit"
  return "$exit_code"
}

run_ui_capture() {
  local destination="$1"
  shift
  local cmd=(cargo run -q -p legion-control-ui --)
  if [[ -n "$bus_address" ]]; then
    cmd+=("--bus-address" "$bus_address")
  fi
  cmd+=("$@")
  run_capture "$destination" "${cmd[@]}"
}

expand_hardware_profile_seed() {
  local spec="$1"
  local profile_id="${spec%%=*}"
  local profile_json="${spec#*=}"
  if [[ "$profile_id" == "$spec" || -z "$profile_id" || -z "$profile_json" ]]; then
    echo "invalid --seed-hardware-profile value; expected PROFILE_ID=JSON or PROFILE_ID=@PATH" >&2
    exit 2
  fi
  if [[ "$profile_json" == @* ]]; then
    local profile_path="${profile_json#@}"
    if [[ ! -f "$profile_path" ]]; then
      echo "hardware profile seed file not found: $profile_path" >&2
      exit 2
    fi
    profile_json="$(python3 - "$profile_path" <<'PY'
import pathlib
import sys

print(pathlib.Path(sys.argv[1]).read_text().strip())
PY
)"
  fi
  printf '%s=%s' "$profile_id" "$profile_json"
}

run_tray_capture() {
  local destination="$1"
  shift
  local cmd=(cargo run -q -p legion-control-tray --)
  if [[ -n "$bus_address" ]]; then
    cmd+=("--bus-address" "$bus_address")
  fi
  cmd+=("$@")
  run_capture "$destination" "${cmd[@]}"
}

wait_for_file_line() {
  local path="$1"
  local retries="${2:-100}"
  for ((i = 0; i < retries; i++)); do
    if [[ -s "$path" ]]; then
      head -n1 "$path"
      return 0
    fi
    sleep 0.1
  done
  return 1
}

start_private_runtime() {
  target_bus_mode="private-session"
  : >"$bus_address_file"
  dbus-daemon --session --print-address=1 --nofork >"$bus_address_file" 2>"$dbus_log" &
  private_bus_pid="$!"
  bus_address="$(wait_for_file_line "$bus_address_file")"

  if [[ -z "$bus_address" ]]; then
    echo "failed to capture private bus address" >&2
    exit 1
  fi

  local daemon_cmd=(
    cargo run -q -p legion-control-daemon --
    --session
    --sysfs-root "$sysfs_root"
    --state-path "$state_path"
  )
  record_command env "DBUS_SESSION_BUS_ADDRESS=$bus_address" "${daemon_cmd[@]}"
  env DBUS_SESSION_BUS_ADDRESS="$bus_address" \
    "${daemon_cmd[@]}" >"$daemon_log" 2>&1 &
  private_daemon_pid="$!"

  for ((i = 0; i < 100; i++)); do
    if ! kill -0 "$private_daemon_pid" 2>/dev/null; then
      echo "private daemon exited before becoming ready" >&2
      sed -n '1,120p' "$daemon_log" >&2 || true
      exit 1
    fi
    if grep -q 'serving interface=' "$daemon_log"; then
      return 0
    fi
    sleep 0.1
  done

  echo "private daemon did not become ready within 10 seconds" >&2
  exit 1
}

printf 'schema_version=1\n' >"$results_tsv"

{
  printf 'timestamp=%s\n' "$(date --iso-8601=seconds)"
  printf 'host=%s\n' "$(hostname 2>/dev/null || printf 'unknown')"
  printf 'kernel=%s\n' "$(uname -r 2>/dev/null || printf 'unknown')"
  printf 'architecture=%s\n' "$(uname -m 2>/dev/null || printf 'unknown')"
  printf 'desktop=%s\n' "${XDG_CURRENT_DESKTOP:-${DESKTOP_SESSION:-unknown}}"
  printf 'session_type=%s\n' "${XDG_SESSION_TYPE:-unknown}"
  printf 'display=%s\n' "${DISPLAY:-unknown}"
  printf 'wayland_display=%s\n' "${WAYLAND_DISPLAY:-unknown}"
  printf 'dbus_session_bus_address_set=%s\n' "$(if [[ -n "${DBUS_SESSION_BUS_ADDRESS:-}" ]]; then printf true; else printf false; fi)"
  printf 'mode=%s\n' "$(if (( execute_writes )); then printf execute; else printf plan-only; fi)"
  printf 'sysfs_root=%s\n' "$sysfs_root"
} >"$environment_txt"

run_capture "$daemon_dry_run" \
  cargo run -q -p legion-control-daemon -- --dry-run --sysfs-root "$sysfs_root" || true

if (( !execute_writes )) && [[ -z "$bus_address" ]] && (( !use_system_bus )); then
  start_private_runtime
elif (( use_system_bus )); then
  target_bus_mode="system"
else
  target_bus_mode="custom-address"
fi

if (( execute_writes )) && [[ -z "$execute_only" ]]; then
  echo "note: for PR-quality evidence, pass --execute-only <control_id> so only one write family runs apply+revert (see docs/live-write-validation.md)." >&2
fi

seed_index=1
for seed_profile in "${seed_hardware_profiles[@]}"; do
  printf -v seed_prefix '%02d' "$seed_index"
  expanded_seed_profile="$(expand_hardware_profile_seed "$seed_profile")"
  run_ui_capture "$output/logs/seed-${seed_prefix}-hardware-profile.json" \
    --set-hardware-profile "$expanded_seed_profile"
  seed_index=$((seed_index + 1))
done

seed_trigger_index=1
for seed_trigger in "${seed_hardware_profile_triggers[@]}"; do
  printf -v seed_prefix '%02d' "$seed_trigger_index"
  if [[ "$seed_trigger" != *=* ]]; then
    echo "invalid --seed-hardware-profile-trigger value; expected TRIGGER_ID=PROFILE_ID" >&2
    exit 2
  fi
  run_ui_capture "$output/logs/seed-${seed_prefix}-hardware-profile-trigger.json" \
    --set-hardware-profile-trigger "$seed_trigger"
  seed_trigger_index=$((seed_trigger_index + 1))
done

python3 - "$metadata_json" "$environment_txt" "$target_bus_mode" "$bus_address" \
  "$sysfs_root" "$execute_writes" "$capture_compat_bundle" "$capture_tray_smoke" "$execute_only" \
  "${#seed_hardware_profiles[@]}" "${#seed_hardware_profile_triggers[@]}" <<'PY'
import json
import pathlib
import sys

(
    metadata_path,
    environment_path,
    target_bus_mode,
    bus_address,
    sysfs_root,
    execute_writes,
    capture_compat_bundle,
    capture_tray_smoke,
    execute_only,
    seed_hardware_profile_count,
    seed_hardware_profile_trigger_count,
) = sys.argv[1:]

environment = {}
for line in pathlib.Path(environment_path).read_text().splitlines():
    if "=" not in line:
        continue
    key, value = line.split("=", 1)
    environment[key] = value

metadata = {
    "schema_version": 1,
    "mode": "execute" if execute_writes == "1" else "plan-only",
    "target_bus_mode": target_bus_mode,
    "bus_address": bus_address or None,
    "sysfs_root": sysfs_root,
    "capture_compat_bundle": capture_compat_bundle == "1",
    "capture_tray_smoke": capture_tray_smoke == "1",
    "execute_only": execute_only or None,
    "seed_hardware_profile_count": int(seed_hardware_profile_count),
    "seed_hardware_profile_trigger_count": int(seed_hardware_profile_trigger_count),
    "environment": environment,
}

pathlib.Path(metadata_path).write_text(json.dumps(metadata, indent=2) + "\n")
PY

if (( capture_compat_bundle )); then
  run_capture "$output/logs/compat-capture.txt" \
    "$compat_script" --sysfs-root "$sysfs_root" --output "$compat_output" || true
fi

run_ui_capture "$output/before/status.txt" --status || true
run_ui_capture "$output/before/overview.txt" --overview || true
run_ui_capture "$output/before/diagnostics.json" --diagnostics || true

if (( execute_writes )); then
  _diag="$output/before/diagnostics.json"
  python3 - "$_diag" <<'PY' || exit 1
import json
import pathlib
import sys

path = pathlib.Path(sys.argv[1])
text = path.read_text(errors="replace")
stripped = text.lstrip()

if not stripped:
    print(
        "error: execute mode requires a working diagnostics capture from the UI client, "
        "but before/diagnostics.json is empty.\n"
        "Ensure a privileged legion-control-daemon is running and reachable on the bus "
        "you selected (--system-bus or --bus-address <address>).\n"
        "See scripts/install-dev-system-integration.sh and the daemon invocation examples "
        "in the project docs (manual: sudo ./target/release/legion-control-daemon ...).",
        file=sys.stderr,
    )
    sys.exit(1)

if stripped.startswith("Error:"):
    print(
        "error: diagnostics capture looks like a D-Bus/client failure (e.g. ServiceUnknown), "
        "not valid daemon JSON.\n"
        "Execute mode needs a running daemon on the chosen bus (--system-bus or --bus-address).\n"
        "See scripts/install-dev-system-integration.sh and the daemon docs "
        "(sudo ./target/release/legion-control-daemon ...).",
        file=sys.stderr,
    )
    sys.exit(1)

if not stripped.startswith("{"):
    print(
        "error: execute mode expects JSON object diagnostics from the daemon; "
        "before/diagnostics.json does not start with '{'.\n"
        "Ensure the daemon is running and reachable on the selected bus.\n"
        "See scripts/install-dev-system-integration.sh and the project docs for "
        "sudo ./target/release/legion-control-daemon ...",
        file=sys.stderr,
    )
    sys.exit(1)

try:
    doc = json.loads(text)
except json.JSONDecodeError as exc:
    print(
        "error: before/diagnostics.json is not valid JSON; execute mode cannot proceed.\n"
        f"Parse error: {exc}\n"
        "Ensure a running legion-control-daemon is reachable on --system-bus or --bus-address.\n"
        "See scripts/install-dev-system-integration.sh and the daemon examples in the docs.",
        file=sys.stderr,
    )
    sys.exit(1)

if not isinstance(doc, dict) or "raw_probe_report" not in doc:
    print(
        "error: diagnostics JSON is missing the top-level 'raw_probe_report' key "
        "(daemon not serving expected interface / stale or wrong bus).\n"
        "Execute mode requires a live daemon on the chosen bus.\n"
        "See scripts/install-dev-system-integration.sh and "
        "sudo ./target/release/legion-control-daemon ... in the docs.",
        file=sys.stderr,
    )
    sys.exit(1)
PY
fi

run_tray_capture "$output/before/tray-status.txt" --status || true
run_tray_capture "$output/before/tray-tooltip.txt" --tooltip || true
run_tray_capture "$output/before/tray-menu-check.txt" --menu-check || true
run_capture "$output/before/tray-desktop-check.txt" \
  cargo run -q -p legion-control-tray -- --desktop-check || true

python3 - "$output/before/diagnostics.json" "$controls_json" <<'PY'
import json
import pathlib
import sys

diagnostics_path, controls_path = sys.argv[1:]
controls = []

try:
    diagnostics = json.loads(pathlib.Path(diagnostics_path).read_text())
except Exception:
    diagnostics = {}

raw = diagnostics.get("raw_probe_report", {})


def first_alternative(choices, current):
    for choice in choices or []:
        if choice != current:
            return choice
    return None


def bool_spec(name, value):
    return f"{name}={'on' if str(value) in {'1', 'true', 'True'} else 'off'}"


platform = raw.get("platform_profile") or {}
platform_current = platform.get("current")
platform_requested = first_alternative(platform.get("choices", []), platform_current)
controls.append({
    "id": "platform_profile",
    "label": "Platform profile",
    "kind": "platform_profile",
    "available": bool(platform_requested),
    "current": platform_current or "unknown",
    "requested": platform_requested or "unknown",
    "set_spec": platform_requested or "",
    "revert_spec": platform_current or "",
    "reason": "" if platform_requested else "No alternate detected platform profile choice.",
    "manual_check": "Confirm overview, tray state, and system behavior reflect the requested profile before reverting.",
})

battery = raw.get("battery_charge_type") or {}
battery_current = battery.get("current")
battery_requested = first_alternative(battery.get("choices", []), battery_current)
controls.append({
    "id": "battery_charge_type",
    "label": "Battery charge type",
    "kind": "battery_charge_type",
    "available": bool(battery_requested),
    "current": battery_current or "unknown",
    "requested": battery_requested or "unknown",
    "set_spec": battery_requested or "",
    "revert_spec": battery_current or "",
    "reason": "" if battery_requested else "No alternate detected battery charge type choice.",
    "manual_check": "Confirm the charge type read-back and battery telemetry stay consistent before reverting.",
})

leds = {led.get("name"): led for led in raw.get("leds", [])}
ylogo = leds.get("platform::ylogo") or {}
ylogo_brightness = ylogo.get("brightness")
ylogo_available = ylogo.get("max_brightness") == 1 and ylogo_brightness in (0, 1)
if ylogo_available:
    ylogo_requested = "0" if ylogo_brightness == 1 else "1"
    ylogo_revert = "1" if ylogo_brightness == 1 else "0"
else:
    ylogo_requested = "unknown"
    ylogo_revert = "unknown"
controls.append({
    "id": "platform::ylogo",
    "label": "Y-logo LED",
    "kind": "led_state",
    "available": ylogo_available,
    "current": str(ylogo_brightness) if ylogo_brightness is not None else "unknown",
    "requested": ylogo_requested,
    "set_spec": bool_spec("platform::ylogo", ylogo_requested) if ylogo_available else "",
    "revert_spec": bool_spec("platform::ylogo", ylogo_revert) if ylogo_available else "",
    "reason": "" if ylogo_available else "platform::ylogo is not exposed as a binary LED.",
    "manual_check": "Confirm the physical Y-logo LED changes and returns to its original state.",
})

toggles = {toggle.get("name"): toggle for toggle in raw.get("ideapad_toggles", [])}
fnlock_led = leds.get("platform::fnlock") or {}

for toggle_id, label, note, paired_led_required in [
    ("fn_lock", "Fn-lock", "Confirm the indicator LED and actual Fn key behavior both change, then revert.", True),
    ("camera_power", "Camera power", "Confirm camera apps lose and regain the device as expected; restart apps if needed.", False),
    ("usb_charging", "USB charging", "Confirm sysfs read-back first; separate off-state charging behavior is still a slower manual check.", False),
    ("fan_mode", "Fan mode", "Confirm `fan_mode` Auto (0) / Full speed (1) behavior, thermal response, and whether read-back changes or remains unchanged.", False),
]:
    toggle = toggles.get(toggle_id) or {}
    current = toggle.get("current_value")
    path = toggle.get("path")
    available = current in ("0", "1") and bool(path)
    if toggle_id == "fn_lock":
        available = available and fnlock_led.get("max_brightness") == 1 and fnlock_led.get("brightness") in (0, 1)
        available = available and str(fnlock_led.get("brightness")) == str(current)
    requested = "0" if current == "1" else "1" if current == "0" else "unknown"
    controls.append({
        "id": toggle_id,
        "label": label,
        "kind": "ideapad_toggle",
        "available": available,
        "current": current or "unknown",
        "requested": requested,
        "set_spec": bool_spec(toggle_id, requested) if available else "",
        "revert_spec": bool_spec(toggle_id, current) if available else "",
        "reason": "" if available else (
            "Toggle is missing, non-binary, unreadable, or failed paired-indicator preconditions."
        ),
        "manual_check": note,
    })

conservation = toggles.get("conservation_mode") or {}
conservation_current = conservation.get("current_value")
conservation_available = conservation_current in ("0", "1") and bool(conservation.get("path"))
conservation_requested = "0" if conservation_current == "1" else "1" if conservation_current == "0" else "unknown"
controls.append({
    "id": "conservation_mode",
    "label": "Battery conservation mode",
    "kind": "conservation_mode",
    "available": conservation_available,
    "current": conservation_current or "unknown",
    "requested": conservation_requested,
    "set_spec": conservation_requested if conservation_available else "",
    "revert_spec": conservation_current if conservation_available else "",
    "reason": "" if conservation_available else "conservation_mode is missing, non-binary, or unreadable.",
    "manual_check": "Confirm conservation_mode read-back changes and battery charge behavior remains sane before reverting.",
})

cpu = raw.get("cpu_power") or {}

def alternate_choice(current, choices, preferred=()):
    if not current or not isinstance(choices, list):
        return ""
    normalized = [str(choice) for choice in choices if str(choice)]
    for choice in preferred:
        if choice in normalized and choice != current:
            return choice
    for choice in normalized:
        if choice != current:
            return choice
    return ""


def compact_json(value):
    return json.dumps(value, separators=(",", ":"), sort_keys=True)


def alternate_validation_color(current_colors):
    colors = {str(value).lower() for value in (current_colors or {}).values()}
    return "#224466" if colors == {"#333333"} else "#333333"


def keyboard_rgb_request(effect, colors, brightness=40, speed=30):
    request = {
        "effect": effect,
        "colors": colors,
        "brightness": brightness,
    }
    if speed is not None:
        request["speed"] = speed
    return request


def keyboard_rgb_mode_choice(modes, current):
    return alternate_choice(
        current or "",
        modes,
        ("Breathing", "Rainbow Wave", "Spectrum Cycle", "Direct", "Static"),
    )


native_keyboard_rgb = raw.get("keyboard_rgb") or {}
openrgb_keyboard = raw.get("keyboard_rgb_openrgb") or {}
keyboard_rgb_control = None

if openrgb_keyboard.get("backend_ready") and openrgb_keyboard.get("sdk_snapshot_supported"):
    device = (openrgb_keyboard.get("devices") or [{}])[0] or {}
    current_effect = openrgb_keyboard.get("sdk_active_mode") or device.get("current_mode")
    requested_effect = keyboard_rgb_mode_choice(device.get("modes") or [], current_effect)
    current_colors = openrgb_keyboard.get("sdk_colors") or {}
    zones = (
        openrgb_keyboard.get("sdk_color_zones")
        or list(current_colors.keys())
        or ["left_side", "left_center", "right_center", "right_side"]
    )
    requested_color = alternate_validation_color(current_colors)
    requested_colors = {str(zone): requested_color for zone in zones}
    available = bool(current_effect and requested_effect and current_colors)
    keyboard_rgb_control = {
        "id": "keyboard_rgb",
        "label": "Keyboard RGB",
        "kind": "keyboard_rgb_openrgb_sdk",
        "available": available,
        "current": current_effect or "unknown",
        "requested": requested_effect or "unknown",
        "set_spec": compact_json(keyboard_rgb_request(requested_effect, requested_colors)) if available else "",
        "revert_spec": compact_json(keyboard_rgb_request(current_effect, current_colors, brightness=100, speed=None)) if available else "",
        "reason": "" if available else "OpenRGB SDK backend is ready, but mode or color snapshot data is incomplete.",
        "manual_check": "Confirm the keyboard RGB mode/colors change through the daemon, then return to the captured OpenRGB SDK mode/colors.",
    }
elif native_keyboard_rgb:
    current_effect = native_keyboard_rgb.get("current_effect")
    requested_effect = keyboard_rgb_mode_choice(native_keyboard_rgb.get("effects") or [], current_effect)
    current_colors = native_keyboard_rgb.get("current_colors") or {}
    zones = [zone.get("id") for zone in native_keyboard_rgb.get("zones") or [] if zone.get("id")]
    if not current_colors and zones:
        current_colors = {str(zone): "#000000" for zone in zones}
    requested_color = alternate_validation_color(current_colors)
    requested_colors = {str(zone): requested_color for zone in zones}
    available = bool(current_effect and requested_effect and current_colors and zones)
    keyboard_rgb_control = {
        "id": "keyboard_rgb",
        "label": "Keyboard RGB",
        "kind": "keyboard_rgb_native",
        "available": available,
        "current": current_effect or "unknown",
        "requested": requested_effect or "unknown",
        "set_spec": compact_json(keyboard_rgb_request(requested_effect, requested_colors)) if available else "",
        "revert_spec": compact_json(keyboard_rgb_request(current_effect, current_colors, brightness=native_keyboard_rgb.get("current_brightness") or 100, speed=native_keyboard_rgb.get("current_speed"))) if available else "",
        "reason": "" if available else "Native keyboard RGB backend is missing mode, zone, color, or alternate effect data.",
        "manual_check": "Confirm the keyboard RGB mode/colors change through the daemon, then return to the captured native mode/colors.",
    }
else:
    keyboard_rgb_control = {
        "id": "keyboard_rgb",
        "label": "Keyboard RGB",
        "kind": "keyboard_rgb_openrgb_sdk",
        "available": False,
        "current": "unknown",
        "requested": "unknown",
        "set_spec": "",
        "revert_spec": "",
        "reason": "No native keyboard RGB backend and no ready OpenRGB SDK fallback are reported.",
        "manual_check": "Capture OpenRGB SDK readiness before attempting keyboard RGB live write validation.",
    }
controls.append(keyboard_rgb_control)

governor_current = str(cpu.get("governor") or "")
governor_choices = cpu.get("available_governors") or []
governor_requested = alternate_choice(governor_current, governor_choices, ("powersave", "performance"))
governor_available = bool(governor_current and governor_requested and cpu.get("governor_path"))
controls.append({
    "id": "cpu_governor",
    "label": "CPU governor",
    "kind": "cpu_governor",
    "available": governor_available,
    "current": governor_current or "unknown",
    "requested": governor_requested or "unknown",
    "set_spec": governor_requested if governor_available else "",
    "revert_spec": governor_current if governor_available else "",
    "reason": "" if governor_available else "cpu_power.governor, available_governors, alternate choice, or governor_path is missing.",
    "manual_check": "Confirm scaling_governor read-back changes and returns; note active amd-pstate mode and desktop power profile context.",
})

epp_current = str(cpu.get("epp") or "")
epp_choices = cpu.get("available_epp") or []
epp_requested = alternate_choice(epp_current, epp_choices, ("balance_power", "balance_performance", "power", "performance", "default"))
epp_available = bool(epp_current and epp_requested and cpu.get("epp_path"))
controls.append({
    "id": "cpu_epp",
    "label": "CPU EPP",
    "kind": "cpu_epp",
    "available": epp_available,
    "current": epp_current or "unknown",
    "requested": epp_requested or "unknown",
    "set_spec": epp_requested if epp_available else "",
    "revert_spec": epp_current if epp_available else "",
    "reason": "" if epp_available else "cpu_power.epp, available_epp, alternate choice, or epp_path is missing.",
    "manual_check": "Confirm energy_performance_preference read-back changes and returns under amd-pstate-epp.",
})

boost_current_bool = cpu.get("boost")
boost_available = isinstance(boost_current_bool, bool) and bool(cpu.get("boost_path"))
boost_current = "1" if boost_current_bool is True else "0" if boost_current_bool is False else "unknown"
boost_requested = "0" if boost_current == "1" else "1" if boost_current == "0" else "unknown"
controls.append({
    "id": "cpu_boost",
    "label": "CPU boost",
    "kind": "cpu_boost",
    "available": boost_available,
    "current": boost_current,
    "requested": boost_requested,
    "set_spec": boost_requested if boost_available else "",
    "revert_spec": boost_current if boost_available else "",
    "reason": "" if boost_available else "cpu_power.boost or boost_path is missing.",
    "manual_check": "Confirm CPU boost read-back changes and returns; note scheduler/governor context in the bundle.",
})

def next_scalar_value(attr):
    try:
        current = int(str(attr.get("current_value")))
        minimum = int(str(attr.get("min_value")))
        maximum = int(str(attr.get("max_value")))
        step = int(str(attr.get("scalar_increment") or "1"))
    except Exception:
        return None
    if step <= 0:
        return None
    candidate = current + step
    if candidate > maximum:
        candidate = current - step
    if candidate < minimum or candidate > maximum or candidate == current:
        return None
    return str(candidate)

firmware_attrs = {attr.get("name"): attr for attr in raw.get("firmware_attributes", [])}
for attr_id, label in [
    ("ppt_pl1_spl", "Firmware PPT PL1/SPL"),
    ("ppt_pl2_sppt", "Firmware PPT PL2/SPPT"),
    ("ppt_pl3_fppt", "Firmware PPT PL3/FPPT"),
]:
    attr = firmware_attrs.get(attr_id) or {}
    current = attr.get("current_value")
    requested = next_scalar_value(attr)
    available = requested is not None and bool(attr.get("path"))
    controls.append({
        "id": f"firmware_attribute:{attr_id}",
        "label": label,
        "kind": "firmware_attribute",
        "available": available,
        "current": current or "unknown",
        "requested": requested or "unknown",
        "set_spec": f"{attr_id}={requested}" if available else "",
        "revert_spec": f"{attr_id}={current}" if available else "",
        "reason": "" if available else "PPT firmware attribute is missing or lacks integer min/max/current metadata.",
        "manual_check": "Confirm firmware attribute read-back changes and reverts; record thermal/performance observations separately.",
    })

amd_dpm = raw.get("amd_gpu_power_dpm") or {}
dpm_current = amd_dpm.get("current_force_performance_level")
dpm_requested = first_alternative(amd_dpm.get("choices", []), dpm_current)
controls.append({
    "id": "amd_gpu_dpm_force_level",
    "label": "AMD GPU DPM force level",
    "kind": "amd_gpu_dpm_force_level",
    "available": bool(dpm_requested and amd_dpm.get("force_performance_level_path")),
    "current": dpm_current or "unknown",
    "requested": dpm_requested or "unknown",
    "set_spec": dpm_requested or "",
    "revert_spec": dpm_current or "",
    "reason": "" if dpm_requested else "AMD GPU DPM force-level capability or alternate choice is missing.",
    "manual_check": "Confirm DPM force-level read-back changes and GPU clocks/power state remain coherent before reverting.",
})

gpu = raw.get("gpu") or {}
gpu_provider = (gpu.get("provider") or "").strip()
gpu_status = (gpu.get("status") or "").strip()
gpu_mode = (gpu.get("mode") or "").strip()
gpu_choices = ["integrated", "hybrid", "nvidia"]
gpu_requested = None
if gpu_provider == "envycontrol" and gpu_status == "probe_only" and gpu_mode in gpu_choices:
    for candidate in gpu_choices:
        if candidate != gpu_mode:
            gpu_requested = candidate
            break

gpu_available = gpu_requested is not None
controls.append({
    "id": "gpu_mode",
    "label": "GPU mode",
    "kind": "gpu_mode",
    "available": gpu_available,
    "current": gpu_mode or "unknown",
    "requested": gpu_requested or "unknown",
    "set_spec": gpu_requested or "",
    "revert_spec": gpu_mode or "",
    "reason": "" if gpu_available else (
        "GPU mode planning unavailable (missing EnvyControl probe, non-probe status, or unknown current mode)."
    ),
    "manual_check": (
        "`gpu_mode`: EnvyControl mode changes may require reboot/logout and are not auto-reverted; record envycontrol success, reboot guidance, and recovery steps."
    ),
})

controls.append({
    "id": "curve_optimizer_all_core",
    "label": "Curve Optimizer all-core",
    "kind": "curve_optimizer_all_core",
    "available": True,
    "current": "write-only",
    "requested": "-20",
    "set_spec": "-20",
    "revert_spec": "0",
    "reason": "",
    "manual_check": "Confirm RyzenAdj reports success, record write-only state, then reset to 0 and run a stability check outside the harness.",
})

profiles = diagnostics.get("hardware_profiles") or {}
profile_id = next(iter(profiles.keys()), None) if isinstance(profiles, dict) else None
controls.append({
    "id": "hardware_profile",
    "label": "Saved hardware profile apply",
    "kind": "hardware_profile",
    "available": bool(profile_id),
    "current": "n/a",
    "requested": profile_id or "unknown",
    "set_spec": profile_id or "",
    "revert_spec": "",
    "reason": "" if profile_id else "No saved hardware profiles are present in daemon state.",
    "manual_check": "Inspect the per-action profile apply result; profile actions perform their own rollback/read-back where supported.",
})

triggers = diagnostics.get("hardware_profile_triggers") or {}
trigger_id = next(iter(triggers.keys()), None) if isinstance(triggers, dict) else None
controls.append({
    "id": "hardware_profile_trigger",
    "label": "Hardware profile trigger apply",
    "kind": "hardware_profile_trigger",
    "available": bool(trigger_id),
    "current": "n/a",
    "requested": trigger_id or "unknown",
    "set_spec": trigger_id or "",
    "revert_spec": "",
    "reason": "" if trigger_id else "No hardware profile trigger mappings are present in daemon state.",
    "manual_check": "Inspect the resolved trigger preview and per-action apply result; automatic OS observers are not part of this capture.",
})

fan_curves = raw.get("fan_curves") or []
fan_planning_ok = bool(fan_curves)
controls.append({
    "id": "fan_preset_balanced_daily",
    "label": "Fan preset apply (dry-run plan)",
    "kind": "fan_preset",
    "available": fan_planning_ok,
    "current": "n/a",
    "requested": "balanced-daily",
    "set_spec": "balanced-daily",
    "revert_spec": "",
    "reason": "" if fan_planning_ok else "No fan curve capability rows in probe report.",
    "manual_check": (
        "Inspect plan JSON only; ApplyFanPreset is not executed by this harness "
        "(daemon policy / safety gate)."
    ),
})
controls.append({
    "id": "restore_auto_fan",
    "label": "Fan restore auto (dry-run plan)",
    "kind": "restore_auto_fan",
    "available": fan_planning_ok,
    "current": "n/a",
    "requested": "n/a",
    "set_spec": "",
    "revert_spec": "",
    "reason": "" if fan_planning_ok else "No fan curve capability rows in probe report.",
    "manual_check": (
        "Inspect plan JSON only; RestoreAutoFan is not executed by this harness "
        "(daemon policy / safety gate)."
    ),
})

pathlib.Path(controls_path).write_text(json.dumps(controls, indent=2) + "\n")
PY

step_index=1
{
  printf 'control_id\tlabel\tkind\tavailable\tcurrent\trequested\tmanual_check\treason\tplan_file\tplan_exit\tset_file\tset_exit\trevert_file\trevert_exit\tbefore_overview_file\tafter_overview_file\treverted_overview_file\tcurve_state_after_apply_file\tcurve_state_after_revert_file\n'
} >"$results_tsv"

while IFS=$'\t' read -r control_id label kind available current requested manual_check reason set_spec revert_spec; do
  safe_id="${control_id//[:]/_}"
  safe_id="${safe_id//\//_}"

  plan_file=""
  plan_exit=""
  set_file=""
  set_exit=""
  revert_file=""
  revert_exit=""
  before_overview_file=""
  after_overview_file=""
  reverted_overview_file=""
  curve_state_after_apply_file=""
  curve_state_after_revert_file=""

  if [[ "$available" == "true" ]]; then
    printf -v prefix '%02d' "$step_index"
    plan_file="$output/steps/${prefix}-${safe_id}-plan.json"
    step_index=$((step_index + 1))

    case "$kind" in
      platform_profile)
        run_ui_capture "$plan_file" --plan-platform-profile "$set_spec" || true
        ;;
      battery_charge_type)
        run_ui_capture "$plan_file" --plan-battery-charge-type "$set_spec" || true
        ;;
      led_state)
        run_ui_capture "$plan_file" --plan-led-state "$set_spec" || true
        ;;
      ideapad_toggle)
        run_ui_capture "$plan_file" --plan-ideapad-toggle "$set_spec" || true
        ;;
      conservation_mode)
        run_ui_capture "$plan_file" --plan-conservation-mode "$set_spec" || true
        ;;
      cpu_governor)
        run_ui_capture "$plan_file" --plan-cpu-governor "$set_spec" || true
        ;;
      cpu_epp)
        run_ui_capture "$plan_file" --plan-cpu-epp "$set_spec" || true
        ;;
      cpu_boost)
        run_ui_capture "$plan_file" --plan-cpu-boost "$set_spec" || true
        ;;
      firmware_attribute)
        run_ui_capture "$plan_file" --plan-firmware-attribute "$set_spec" || true
        ;;
      amd_gpu_dpm_force_level)
        run_ui_capture "$plan_file" --plan-amd-gpu-dpm-force-level "$set_spec" || true
        ;;
      keyboard_rgb_openrgb_sdk)
        run_ui_capture "$plan_file" --plan-openrgb-keyboard-rgb-sdk "$set_spec" || true
        ;;
      keyboard_rgb_native)
        run_ui_capture "$plan_file" --plan-keyboard-rgb "$set_spec" || true
        ;;
      curve_optimizer_all_core)
        run_ui_capture "$plan_file" "--plan-curve-optimizer-all-core=$set_spec" || true
        ;;
      fan_preset)
        run_ui_capture "$plan_file" --plan-fan-preset "$set_spec" || true
        ;;
      restore_auto_fan)
        run_ui_capture "$plan_file" --plan-restore-auto-fan || true
        ;;
      gpu_mode)
        run_ui_capture "$plan_file" --plan-gpu-mode "$set_spec" || true
        ;;
      hardware_profile)
        run_ui_capture "$plan_file" --plan-hardware-profile "$set_spec" || true
        ;;
      hardware_profile_trigger)
        run_ui_capture "$plan_file" --plan-hardware-profile-trigger "$set_spec" || true
        ;;
    esac
    plan_exit="$(cat "${plan_file}.exit")"

    execute_this=1
    if [[ -n "$execute_only" && "$control_id" != "$execute_only" ]]; then
      execute_this=0
    fi
    if [[ -z "$execute_only" && ( "$kind" == "firmware_attribute" || "$kind" == "conservation_mode" || "$kind" == "cpu_governor" || "$kind" == "cpu_epp" || "$kind" == "cpu_boost" || "$kind" == "amd_gpu_dpm_force_level" || "$kind" == "keyboard_rgb_openrgb_sdk" || "$kind" == "keyboard_rgb_native" || "$kind" == "gpu_mode" || "$kind" == "curve_optimizer_all_core" || "$kind" == "hardware_profile" || "$kind" == "hardware_profile_trigger" ) ]]; then
      execute_this=0
    fi

    if (( execute_writes )) && (( execute_this )) && [[ "$plan_exit" == "0" ]] && [[ "$kind" == "platform_profile" || "$kind" == "battery_charge_type" || "$kind" == "led_state" || "$kind" == "ideapad_toggle" || "$kind" == "firmware_attribute" || "$kind" == "conservation_mode" || "$kind" == "cpu_governor" || "$kind" == "cpu_epp" || "$kind" == "cpu_boost" || "$kind" == "amd_gpu_dpm_force_level" || "$kind" == "keyboard_rgb_openrgb_sdk" || "$kind" == "keyboard_rgb_native" || "$kind" == "gpu_mode" || "$kind" == "curve_optimizer_all_core" || "$kind" == "hardware_profile" || "$kind" == "hardware_profile_trigger" ]]; then
      before_overview_file="$output/steps/${prefix}-${safe_id}-before-overview.txt"
      run_ui_capture "$before_overview_file" --overview || true

      printf -v prefix '%02d' "$step_index"
      set_file="$output/steps/${prefix}-${safe_id}-apply.json"
      step_index=$((step_index + 1))
      case "$kind" in
        platform_profile)
          run_ui_capture "$set_file" --set-platform-profile "$set_spec" || true
          ;;
        battery_charge_type)
          run_ui_capture "$set_file" --set-battery-charge-type "$set_spec" || true
          ;;
        led_state)
          run_ui_capture "$set_file" --set-led-state "$set_spec" || true
          ;;
        ideapad_toggle)
          run_ui_capture "$set_file" --set-ideapad-toggle "$set_spec" || true
          ;;
        conservation_mode)
          run_ui_capture "$set_file" --set-conservation-mode "$set_spec" || true
          ;;
        cpu_governor)
          run_ui_capture "$set_file" --set-cpu-governor "$set_spec" || true
          ;;
        cpu_epp)
          run_ui_capture "$set_file" --set-cpu-epp "$set_spec" || true
          ;;
        cpu_boost)
          run_ui_capture "$set_file" --set-cpu-boost "$set_spec" || true
          ;;
        firmware_attribute)
          run_ui_capture "$set_file" --set-firmware-attribute "$set_spec" || true
          ;;
        amd_gpu_dpm_force_level)
          run_ui_capture "$set_file" --set-amd-gpu-dpm-force-level "$set_spec" || true
          ;;
        keyboard_rgb_openrgb_sdk|keyboard_rgb_native)
          run_ui_capture "$set_file" --set-keyboard-rgb "$set_spec" || true
          ;;
        gpu_mode)
          run_ui_capture "$set_file" --set-gpu-mode "$set_spec" || true
          ;;
        curve_optimizer_all_core)
          run_ui_capture "$set_file" "--set-curve-optimizer-all-core=$set_spec" || true
          ;;
        hardware_profile)
          run_ui_capture "$set_file" --apply-hardware-profile "$set_spec" || true
          ;;
        hardware_profile_trigger)
          run_ui_capture "$set_file" --apply-hardware-profile-trigger "$set_spec" || true
          ;;
      esac
      set_exit="$(cat "${set_file}.exit")"

      after_overview_file="$output/steps/${prefix}-${safe_id}-after-overview.txt"
      run_ui_capture "$after_overview_file" --overview || true
      if [[ "$kind" == "curve_optimizer_all_core" ]]; then
        curve_state_after_apply_file="$output/steps/${prefix}-${safe_id}-last-state-after-apply.json"
        run_ui_capture "$curve_state_after_apply_file" --last-curve-optimizer-all-core || true
      fi

      if [[ -n "$revert_spec" && "$kind" != "gpu_mode" && "$kind" != "hardware_profile" && "$kind" != "hardware_profile_trigger" ]]; then
        printf -v prefix '%02d' "$step_index"
        revert_file="$output/steps/${prefix}-${safe_id}-revert.json"
        step_index=$((step_index + 1))
        case "$kind" in
          platform_profile)
            run_ui_capture "$revert_file" --set-platform-profile "$revert_spec" || true
            ;;
          battery_charge_type)
            run_ui_capture "$revert_file" --set-battery-charge-type "$revert_spec" || true
            ;;
          led_state)
            run_ui_capture "$revert_file" --set-led-state "$revert_spec" || true
            ;;
          ideapad_toggle)
            run_ui_capture "$revert_file" --set-ideapad-toggle "$revert_spec" || true
            ;;
          conservation_mode)
            run_ui_capture "$revert_file" --set-conservation-mode "$revert_spec" || true
            ;;
          cpu_governor)
            run_ui_capture "$revert_file" --set-cpu-governor "$revert_spec" || true
            ;;
          cpu_epp)
            run_ui_capture "$revert_file" --set-cpu-epp "$revert_spec" || true
            ;;
          cpu_boost)
            run_ui_capture "$revert_file" --set-cpu-boost "$revert_spec" || true
            ;;
          firmware_attribute)
            run_ui_capture "$revert_file" --set-firmware-attribute "$revert_spec" || true
            ;;
          amd_gpu_dpm_force_level)
            run_ui_capture "$revert_file" --set-amd-gpu-dpm-force-level "$revert_spec" || true
            ;;
          keyboard_rgb_openrgb_sdk|keyboard_rgb_native)
            run_ui_capture "$revert_file" --set-keyboard-rgb "$revert_spec" || true
            ;;
          curve_optimizer_all_core)
            run_ui_capture "$revert_file" --reset-curve-optimizer-all-core || true
            ;;
        esac
        revert_exit="$(cat "${revert_file}.exit")"

        reverted_overview_file="$output/steps/${prefix}-${safe_id}-reverted-overview.txt"
        run_ui_capture "$reverted_overview_file" --overview || true
        if [[ "$kind" == "curve_optimizer_all_core" ]]; then
          curve_state_after_revert_file="$output/steps/${prefix}-${safe_id}-last-state-after-revert.json"
          run_ui_capture "$curve_state_after_revert_file" --last-curve-optimizer-all-core || true
        fi
      fi
    fi
  fi

  printf '%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\n' \
    "$(sanitize_field "$control_id")" \
    "$(sanitize_field "$label")" \
    "$(sanitize_field "$kind")" \
    "$(sanitize_field "$available")" \
    "$(sanitize_field "$current")" \
    "$(sanitize_field "$requested")" \
    "$(sanitize_field "$manual_check")" \
    "$(sanitize_field "$reason")" \
    "$(sanitize_field "$plan_file")" \
    "$(sanitize_field "$plan_exit")" \
    "$(sanitize_field "$set_file")" \
    "$(sanitize_field "$set_exit")" \
    "$(sanitize_field "$revert_file")" \
    "$(sanitize_field "$revert_exit")" \
    "$(sanitize_field "$before_overview_file")" \
    "$(sanitize_field "$after_overview_file")" \
    "$(sanitize_field "$reverted_overview_file")" \
    "$(sanitize_field "$curve_state_after_apply_file")" \
    "$(sanitize_field "$curve_state_after_revert_file")" \
    >>"$results_tsv"
done < <(
  python3 - "$controls_json" <<'PY'
import json
import pathlib
import sys

controls = json.loads(pathlib.Path(sys.argv[1]).read_text())
for control in controls:
    print(
        "\t".join(
            [
                str(control["id"]),
                str(control["label"]),
                str(control["kind"]),
                "true" if control["available"] else "false",
                str(control["current"]),
                str(control["requested"]),
                str(control["manual_check"]),
                str(control["reason"] or "__none__"),
                str(control["set_spec"]),
                str(control["revert_spec"]),
            ]
        )
    )
PY
)

run_ui_capture "$output/after/status.txt" --status || true
run_ui_capture "$output/after/overview.txt" --overview || true
run_ui_capture "$output/after/diagnostics.json" --diagnostics || true
run_tray_capture "$output/after/tray-status.txt" --status || true
run_tray_capture "$output/after/tray-tooltip.txt" --tooltip || true
run_tray_capture "$output/after/tray-menu-check.txt" --menu-check || true

tray_smoke_status="skipped"
if (( capture_tray_smoke )); then
  if [[ -n "${DBUS_SESSION_BUS_ADDRESS:-}" ]]; then
    tray_cmd=("$tray_smoke_script" "--hold-seconds" "$hold_seconds" "--report-dir" "$tray_smoke_dir")
    if [[ -n "$bus_address" ]]; then
      tray_cmd+=("--bus-address" "$bus_address")
    fi
    if run_capture "$tray_smoke_log" "${tray_cmd[@]}"; then
      tray_smoke_status="passed"
    else
      tray_smoke_status="failed"
    fi
  else
    printf 'skipped: DBUS_SESSION_BUS_ADDRESS is not set\n' >"$tray_smoke_log"
  fi
fi

python3 - "$metadata_json" "$controls_json" "$results_tsv" "$report_json" "$report_md" \
  "$operator_md" "$tray_smoke_status" <<'PY'
import json
import pathlib
import sys

(
    metadata_path,
    controls_path,
    results_path,
    report_json_path,
    report_md_path,
    operator_md_path,
    tray_smoke_status,
) = sys.argv[1:]

metadata = json.loads(pathlib.Path(metadata_path).read_text())
controls = {entry["id"]: entry for entry in json.loads(pathlib.Path(controls_path).read_text())}


def read_exit(path_str):
    if not path_str:
        return None
    try:
        return int(pathlib.Path(f"{path_str}.exit").read_text().strip())
    except Exception:
        return None


def read_json_if_possible(path_str):
    if not path_str:
        return None
    try:
        return json.loads(pathlib.Path(path_str).read_text())
    except Exception:
        return None


rows = pathlib.Path(results_path).read_text().splitlines()
results = []
for row in rows[1:]:
    if not row.strip():
        continue
    (
        control_id,
        label,
        kind,
        available,
        current,
        requested,
        manual_check,
        reason,
        plan_file,
        plan_exit,
        set_file,
        set_exit,
        revert_file,
        revert_exit,
        before_overview_file,
        after_overview_file,
        reverted_overview_file,
        curve_state_after_apply_file,
        curve_state_after_revert_file,
    ) = row.split("\t")

    plan_payload = read_json_if_possible(plan_file)
    set_payload = read_json_if_possible(set_file)
    revert_payload = read_json_if_possible(revert_file)
    curve_state_after_apply_payload = read_json_if_possible(curve_state_after_apply_file)
    curve_state_after_revert_payload = read_json_if_possible(curve_state_after_revert_file)

    final_status = "skipped"
    if available == "true":
        if metadata["mode"] == "plan-only":
            final_status = "planned" if plan_exit == "0" else "plan-failed"
        else:
            if not set_file:
                if plan_exit != "0":
                    final_status = "plan-failed"
                elif metadata.get("execute_only"):
                    final_status = "execute-skipped-filter"
                else:
                    final_status = "execute-not-run"
            elif set_payload and set_payload.get("applied") and revert_payload and revert_payload.get("applied"):
                final_status = "pass"
            elif set_payload and set_payload.get("status") == "BlockedByAuthorization":
                final_status = "blocked-by-authorization"
            elif set_payload and set_payload.get("status") == "BlockedByPolicy":
                final_status = "blocked-by-policy"
            elif set_payload and set_payload.get("status") == "Failed":
                final_status = "failed"
            elif set_payload and "completed" in set_payload:
                message = str(set_payload.get("message") or "").lower()
                if set_payload.get("completed"):
                    final_status = "executed"
                elif "policy" in message:
                    final_status = "blocked-by-policy"
                elif "authorization" in message:
                    final_status = "blocked-by-authorization"
                else:
                    final_status = "failed"
            elif set_exit == "0":
                final_status = "executed"
            else:
                final_status = "execute-failed"

    result = {
        "control_id": control_id,
        "label": label,
        "kind": kind,
        "available": available == "true",
        "current": current,
        "requested": requested,
        "manual_check": manual_check,
        "reason": None if reason in {"", "__none__"} else reason,
        "plan_file": plan_file or None,
        "plan_exit": int(plan_exit) if plan_exit else None,
        "plan": plan_payload,
        "set_file": set_file or None,
        "set_exit": int(set_exit) if set_exit else None,
        "set_result": set_payload,
        "revert_file": revert_file or None,
        "revert_exit": int(revert_exit) if revert_exit else None,
        "revert_result": revert_payload,
        "before_overview_file": before_overview_file or None,
        "after_overview_file": after_overview_file or None,
        "reverted_overview_file": reverted_overview_file or None,
        "curve_state_after_apply_file": curve_state_after_apply_file or None,
        "curve_state_after_apply": curve_state_after_apply_payload,
        "curve_state_after_revert_file": curve_state_after_revert_file or None,
        "curve_state_after_revert": curve_state_after_revert_payload,
        "status": final_status,
    }
    results.append(result)

summary = {
    "schema_version": 1,
    "metadata": metadata,
    "tray_smoke_status": tray_smoke_status,
    "controls": results,
    "notes": {
        "fixture_tests_not_live_validation": True,
        "execute_mode_requires_operator_review": metadata["mode"] == "execute",
    },
}
pathlib.Path(report_json_path).write_text(json.dumps(summary, indent=2) + "\n")

lines = [
    "# Write Validation Report",
    "",
    f"- Mode: `{metadata['mode']}`",
    f"- Target bus: `{metadata['target_bus_mode']}`",
    f"- Sysfs root: `{metadata['sysfs_root']}`",
    f"- Execute-only filter: `{metadata.get('execute_only') or 'none'}`",
    f"- Tray smoke: `{tray_smoke_status}`",
    "",
    "## Summary",
    "",
]

for result in results:
    lines.append(
        f"- `{result['control_id']}`: `{result['status']}`; current `{result['current']}`; requested `{result['requested']}`"
    )

lines.extend(
    [
        "",
        "## Per-control notes",
        "",
    ]
)

for result in results:
    lines.append(f"### {result['label']}")
    lines.append("")
    lines.append(f"- Status: `{result['status']}`")
    lines.append(f"- Available: `{str(result['available']).lower()}`")
    lines.append(f"- Current: `{result['current']}`")
    lines.append(f"- Requested: `{result['requested']}`")
    if result["reason"]:
        lines.append(f"- Skip or warning reason: {result['reason']}")
    if result["plan"]:
        lines.append(
            f"- Plan target: `{result['plan'].get('method', 'unknown')}` via `{result['plan'].get('path', 'unknown')}`"
        )
    if result["set_result"]:
        lines.append(
            f"- Apply result: `{result['set_result'].get('status', 'unknown')}`; message: {result['set_result'].get('message', 'unknown')}"
        )
    if result["revert_result"]:
        lines.append(
            f"- Revert result: `{result['revert_result'].get('status', 'unknown')}`; message: {result['revert_result'].get('message', 'unknown')}"
        )
    lines.append(f"- Manual operator check: {result['manual_check']}")
    lines.append("")

pathlib.Path(report_md_path).write_text("\n".join(lines) + "\n")

operator_lines = [
    "# Operator Checklist",
    "",
    "Use this alongside the generated step files and report bundle.",
    "",
    "## Before execute mode",
    "",
    "- Confirm the daemon is running with only the write flag required for the control under test.",
    "- Prefer `scripts/capture-write-validation-report.sh --execute --execute-only <control_id>` so apply+revert runs for a single family per bundle.",
    "- Confirm the relevant `--plan-*` step succeeded and the rollback value looks sane.",
    "- Close or prepare apps affected by camera or keyboard state changes.",
    "",
    "## During execute mode",
    "",
    "- Execute one control at a time; do not batch changes.",
    "- Review the JSON `WriteExecutionResult` after the apply step before continuing.",
    "- Confirm the manual hardware behavior described for that control before the revert step.",
    "",
    "## After execute mode",
    "",
    "- Confirm the revert step returns the machine to the original state.",
    "- Attach `validation-report.md`, `validation-report.json`, and any tray-smoke bundle to the review.",
    "",
]

for result in results:
    operator_lines.append(f"- `{result['control_id']}`: {result['manual_check']}")

pathlib.Path(operator_md_path).write_text("\n".join(operator_lines) + "\n")
PY

echo "Write validation bundle written to $output"
