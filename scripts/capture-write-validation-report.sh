#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'EOF'
Usage: scripts/capture-write-validation-report.sh --output <bundle-dir> [options]

Capture a validation bundle for the currently implemented reversible write surface.

Default mode is plan-only:
- starts a private session bus and read-mostly daemon
- captures status, overview, diagnostics, tray/menu evidence, and write plans
- also captures dry-run plans for fan preset apply and restore-to-auto (read-only;
  fan execution is never driven by this script, even in --execute mode)
- never attempts hardware-changing writes

Execute mode is explicit and requires an already-running privileged daemon:
- add --execute
- target either --system-bus or --bus-address <address>
- the script records set/revert results, but still expects operator review

Options:
  --output <dir>         Required bundle directory.
  --sysfs-root <root>    Sysfs root for plan-only private-daemon runs. Default: /
  --bus-address <addr>   Use an existing daemon on the given D-Bus address.
  --system-bus           Use the system bus instead of a custom bus address.
  --execute              Attempt real reversible writes and then revert them.
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

python3 - "$metadata_json" "$environment_txt" "$target_bus_mode" "$bus_address" \
  "$sysfs_root" "$execute_writes" "$capture_compat_bundle" "$capture_tray_smoke" <<'PY'
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
  printf 'control_id\tlabel\tkind\tavailable\tcurrent\trequested\tmanual_check\treason\tplan_file\tplan_exit\tset_file\tset_exit\trevert_file\trevert_exit\tbefore_overview_file\tafter_overview_file\treverted_overview_file\n'
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
      fan_preset)
        run_ui_capture "$plan_file" --plan-fan-preset "$set_spec" || true
        ;;
      restore_auto_fan)
        run_ui_capture "$plan_file" --plan-restore-auto-fan || true
        ;;
    esac
    plan_exit="$(cat "${plan_file}.exit")"

    if (( execute_writes )) && [[ "$plan_exit" == "0" ]] && [[ "$kind" == "platform_profile" || "$kind" == "battery_charge_type" || "$kind" == "led_state" || "$kind" == "ideapad_toggle" ]]; then
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
      esac
      set_exit="$(cat "${set_file}.exit")"

      after_overview_file="$output/steps/${prefix}-${safe_id}-after-overview.txt"
      run_ui_capture "$after_overview_file" --overview || true

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
      esac
      revert_exit="$(cat "${revert_file}.exit")"

      reverted_overview_file="$output/steps/${prefix}-${safe_id}-reverted-overview.txt"
      run_ui_capture "$reverted_overview_file" --overview || true
    fi
  fi

  printf '%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\n' \
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
    ) = row.split("\t")

    plan_payload = read_json_if_possible(plan_file)
    set_payload = read_json_if_possible(set_file)
    revert_payload = read_json_if_possible(revert_file)

    final_status = "skipped"
    if available == "true":
        if metadata["mode"] == "plan-only":
            final_status = "planned" if plan_exit == "0" else "plan-failed"
        else:
            if set_payload and set_payload.get("applied") and revert_payload and revert_payload.get("applied"):
                final_status = "pass"
            elif set_payload and set_payload.get("status") == "BlockedByAuthorization":
                final_status = "blocked-by-authorization"
            elif set_payload and set_payload.get("status") == "BlockedByPolicy":
                final_status = "blocked-by-policy"
            elif set_payload and set_payload.get("status") == "Failed":
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
