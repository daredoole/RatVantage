#!/usr/bin/env bash
# Capture a read-only RatVantage compatibility bundle for model support review.
set -euo pipefail

usage() {
  cat <<'EOF'
Usage: scripts/capture-compatibility-bundle.sh --output <dir> [options]

Captures read-only hardware support evidence:
  - legion-control-ui --overview
  - legion-control-ui --diagnostics
  - legion-control-ui --automation-diagnostics
  - legion-control-ui --reset-diagnostics
  - legion-probe --json
  - OpenRGB keyboard RGB readiness, when the checker is available
  - OpenRGB bridge evidence status, when the status helper is available
  - GPU mux/session-restart evidence, when the helper is available

Options:
  --output <dir>          Required output directory.
  --sysfs-root <root>     Sysfs root for probe capture. Default: /.
  --ui-bin <path>         legion-control-ui binary. Default: PATH lookup.
  --probe-bin <path>      legion-probe binary. Default: PATH lookup.
  --openrgb-checker <p>   OpenRGB readiness checker. Default: repo script/PATH lookup.
  --bridge-status-bin <p> OpenRGB bridge status helper. Default: repo script/PATH lookup.
  --sdk-evidence-bin <p>  OpenRGB SDK evidence helper. Default: repo script/PATH lookup.
  --gpu-mux-bin <path>    GPU mux evidence helper. Default: repo script/PATH lookup.
  --skip-openrgb          Do not capture OpenRGB readiness.
  --skip-bridge-status    Do not capture OpenRGB bridge evidence status.
  --skip-sdk-evidence     Do not capture OpenRGB SDK read-back evidence.
  --skip-gpu-mux-evidence Do not capture GPU mux/session-restart evidence.
  -h, --help              Show this help.

This script is read-only. It does not write sysfs, hidraw, i2c, WMI, or EC.
EOF
}

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
output=""
sysfs_root="/"
ui_bin="${LEGION_CONTROL_UI_BIN:-legion-control-ui}"
probe_bin="${LEGION_PROBE_BIN:-legion-probe}"
openrgb_checker="${RATVANTAGE_OPENRGB_CHECKER:-}"
bridge_status_bin="${RATVANTAGE_OPENRGB_BRIDGE_STATUS_BIN:-}"
sdk_evidence_bin="${RATVANTAGE_OPENRGB_SDK_EVIDENCE_BIN:-}"
gpu_mux_bin="${RATVANTAGE_GPU_MUX_EVIDENCE_BIN:-}"
skip_openrgb=0
skip_bridge_status=0
skip_sdk_evidence=0
skip_gpu_mux_evidence=0

while [[ $# -gt 0 ]]; do
  case "$1" in
    --output)
      output="${2:?missing value for --output}"
      shift 2
      ;;
    --sysfs-root)
      sysfs_root="${2:?missing value for --sysfs-root}"
      shift 2
      ;;
    --ui-bin)
      ui_bin="${2:?missing value for --ui-bin}"
      shift 2
      ;;
    --probe-bin)
      probe_bin="${2:?missing value for --probe-bin}"
      shift 2
      ;;
    --openrgb-checker)
      openrgb_checker="${2:?missing value for --openrgb-checker}"
      shift 2
      ;;
    --bridge-status-bin)
      bridge_status_bin="${2:?missing value for --bridge-status-bin}"
      shift 2
      ;;
    --sdk-evidence-bin)
      sdk_evidence_bin="${2:?missing value for --sdk-evidence-bin}"
      shift 2
      ;;
    --gpu-mux-bin)
      gpu_mux_bin="${2:?missing value for --gpu-mux-bin}"
      shift 2
      ;;
    --skip-openrgb)
      skip_openrgb=1
      shift
      ;;
    --skip-bridge-status)
      skip_bridge_status=1
      shift
      ;;
    --skip-sdk-evidence)
      skip_sdk_evidence=1
      shift
      ;;
    --skip-gpu-mux-evidence)
      skip_gpu_mux_evidence=1
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

if [[ -z "$output" ]]; then
  echo "--output is required" >&2
  usage >&2
  exit 2
fi

command -v python3 >/dev/null 2>&1 || {
  echo "missing python3; install Python 3 to generate compatibility metadata" >&2
  exit 1
}

resolve_command() {
  local candidate="$1"
  if [[ "$candidate" == */* ]]; then
    [[ -x "$candidate" ]] && printf '%s\n' "$candidate"
    return
  fi
  command -v "$candidate" 2>/dev/null || true
}

ui_path="$(resolve_command "$ui_bin")"
probe_path="$(resolve_command "$probe_bin")"
if [[ -z "$openrgb_checker" ]]; then
  if [[ -x "$repo_root/scripts/check-keyboard-rgb-openrgb.sh" ]]; then
    openrgb_checker="$repo_root/scripts/check-keyboard-rgb-openrgb.sh"
  else
    openrgb_checker="$(resolve_command ratvantage-check-keyboard-rgb-openrgb)"
  fi
fi
if [[ -z "$bridge_status_bin" ]]; then
  if [[ -x "$repo_root/scripts/status-keyboard-rgb-openrgb-bridge-evidence.sh" ]]; then
    bridge_status_bin="$repo_root/scripts/status-keyboard-rgb-openrgb-bridge-evidence.sh"
  else
    bridge_status_bin="$(resolve_command ratvantage-keyboard-rgb-openrgb-bridge-status)"
  fi
fi
if [[ -z "$sdk_evidence_bin" ]]; then
  if [[ -x "$repo_root/scripts/capture-keyboard-rgb-openrgb-sdk-evidence.sh" ]]; then
    sdk_evidence_bin="$repo_root/scripts/capture-keyboard-rgb-openrgb-sdk-evidence.sh"
  else
    sdk_evidence_bin="$(resolve_command ratvantage-capture-keyboard-rgb-openrgb-sdk-evidence)"
  fi
fi
if [[ -z "$gpu_mux_bin" ]]; then
  if [[ -x "$repo_root/scripts/capture-gpu-mux-evidence.sh" ]]; then
    gpu_mux_bin="$repo_root/scripts/capture-gpu-mux-evidence.sh"
  else
    gpu_mux_bin="$(resolve_command ratvantage-capture-gpu-mux-evidence)"
  fi
fi

mkdir -p "$output/logs"

run_capture() {
  local name="$1"
  shift
  local stdout_path="$output/logs/$name.stdout"
  local stderr_path="$output/logs/$name.stderr"
  if timeout 30 "$@" >"$stdout_path" 2>"$stderr_path"; then
    printf '%s=ok\n' "$name"
  else
    local status=$?
    printf '%s=failed:%s\n' "$name" "$status"
  fi
}

overview_status="skipped"
diagnostics_status="skipped"
automation_diagnostics_status="skipped"
reset_diagnostics_status="skipped"
probe_status="skipped"
openrgb_status="skipped"
bridge_status="skipped"
sdk_evidence_status="skipped"
gpu_mux_status="skipped"

if [[ -n "$ui_path" ]]; then
  overview_status="$(run_capture overview "$ui_path" --overview)"
  diagnostics_status="$(run_capture diagnostics "$ui_path" --diagnostics)"
  automation_diagnostics_status="$(run_capture automation-diagnostics "$ui_path" --automation-diagnostics)"
  reset_diagnostics_status="$(run_capture reset-diagnostics "$ui_path" --reset-diagnostics)"
else
  printf 'ui_missing=%s\n' "$ui_bin" >"$output/logs/ui.stderr"
fi

if [[ -n "$probe_path" ]]; then
  probe_status="$(run_capture probe "$probe_path" --json --sysfs-root "$sysfs_root")"
else
  printf 'probe_missing=%s\n' "$probe_bin" >"$output/logs/probe.stderr"
fi

if [[ "$skip_openrgb" -eq 0 && -n "$openrgb_checker" && -x "$openrgb_checker" ]]; then
  if timeout 30 "$openrgb_checker" --output "$output/openrgb-readiness" \
    >"$output/logs/openrgb-readiness.stdout" \
    2>"$output/logs/openrgb-readiness.stderr"; then
    openrgb_status="openrgb-readiness=ok"
  else
    openrgb_status="openrgb-readiness=failed:$?"
  fi
fi

if [[ "$skip_bridge_status" -eq 0 && -n "$bridge_status_bin" && -x "$bridge_status_bin" ]]; then
  if timeout 30 "$bridge_status_bin" --readiness "$output/openrgb-readiness" --json \
    >"$output/logs/openrgb-bridge-status.json" \
    2>"$output/logs/openrgb-bridge-status.stderr"; then
    bridge_status="openrgb-bridge-status=ok"
  else
    bridge_status="openrgb-bridge-status=failed:$?"
  fi
fi

if [[ "$skip_sdk_evidence" -eq 0 && -n "$sdk_evidence_bin" && -x "$sdk_evidence_bin" ]]; then
  if timeout 45 "$sdk_evidence_bin" --output "$output/openrgb-sdk" \
    >"$output/logs/openrgb-sdk.stdout" \
    2>"$output/logs/openrgb-sdk.stderr"; then
    sdk_evidence_status="openrgb-sdk=ok"
  else
    sdk_evidence_status="openrgb-sdk=failed:$?"
  fi
fi

if [[ "$skip_gpu_mux_evidence" -eq 0 && -n "$gpu_mux_bin" && -x "$gpu_mux_bin" ]]; then
  if timeout 45 "$gpu_mux_bin" --phase mux-only --output "$output/gpu-mux" \
    >"$output/logs/gpu-mux.stdout" \
    2>"$output/logs/gpu-mux.stderr"; then
    gpu_mux_status="gpu-mux=ok"
  else
    gpu_mux_status="gpu-mux=failed:$?"
  fi
fi

python3 - "$output" "$sysfs_root" "$ui_path" "$probe_path" "$openrgb_checker" \
  "$bridge_status_bin" "$overview_status" "$diagnostics_status" "$probe_status" \
  "$openrgb_status" "$bridge_status" "$automation_diagnostics_status" \
  "$reset_diagnostics_status" "$sdk_evidence_bin" "$sdk_evidence_status" \
  "$gpu_mux_bin" "$gpu_mux_status" <<'PY'
import datetime as dt
import json
import pathlib
import sys

out = pathlib.Path(sys.argv[1])

def read_json_log(relative_path):
    path = out / relative_path
    if not path.exists():
        return None
    try:
        return json.loads(path.read_text())
    except json.JSONDecodeError:
        return None

def reset_recovery_summary():
    reset = read_json_log("logs/reset-diagnostics.stdout")
    if not isinstance(reset, dict):
        return {"available": False, "reason": "reset diagnostics JSON was not captured"}
    summary = {}
    for key in (
        "curve_optimizer_all_core_reset",
        "firmware_ppt_reset_defaults",
        "restore_auto_fan",
        "custom_thermal_restore_auto_fan",
        "keyboard_rgb_sdk_recovery",
        "gpu_mode_pending_recovery",
        "gpu_switching_recovery",
    ):
        item = reset.get(key)
        if not isinstance(item, dict):
            continue
        entry = {"ok": item.get("ok")}
        for item_key in ("plan_command", "execute_command"):
            if item_key in item:
                entry[item_key] = item[item_key]
        value = item.get("value")
        if isinstance(value, dict):
            for value_key in (
                "available",
                "status",
                "current_mode",
                "switch_type",
                "reason",
                "plan_command",
                "clear_command",
                "verification_command",
                "next_action",
            ):
                if value_key in value:
                    entry[value_key] = value[value_key]
        elif "error" in item:
            entry["error"] = item["error"]
        summary[key] = entry
    return summary

def first_item_summary(item):
    if not isinstance(item, dict):
        return None
    summary = {}
    for key in (
        "action_id",
        "method",
        "requested_value",
        "readback_value",
        "current_value",
        "path",
        "saved_value",
        "live_value",
        "status",
    ):
        if key in item:
            summary[key] = item[key]
    return summary or None

def drift_report_summary(report):
    if not isinstance(report, dict):
        return None
    summary = {}
    for key in ("status", "profile_id", "curve_id", "checked_count", "drifted_count", "detail"):
        if key in report:
            summary[key] = report[key]
    items = report.get("items")
    if isinstance(items, list) and items:
        summary["first_item"] = first_item_summary(items[0])
    return summary or None

def high_value_drift_summary():
    diagnostics = read_json_log("logs/diagnostics.stdout")
    if not isinstance(diagnostics, dict):
        return {"available": False, "reason": "diagnostics JSON was not captured"}
    summary = {}
    for key in ("hardware_profile_drift", "fan_curve_drift"):
        item = drift_report_summary(diagnostics.get(key))
        if item is not None:
            summary[key] = item
    return summary

def high_value_gpu_switching_summary():
    diagnostics = read_json_log("logs/diagnostics.stdout")
    if not isinstance(diagnostics, dict):
        return {"available": False, "reason": "diagnostics JSON was not captured"}
    gpu = diagnostics.get("gpu_switching")
    if not isinstance(gpu, dict):
        return {"available": False, "reason": "gpu switching diagnostics were not captured"}
    summary = {}
    for key in (
        "status",
        "provider",
        "current_mode",
        "switch_type",
        "execution_model",
        "runtime_plan_available",
        "next_action",
    ):
        if key in gpu:
            summary[key] = gpu[key]
    for key in ("blockers", "evidence"):
        value = gpu.get(key)
        if isinstance(value, list):
            summary[key] = value[:3]
    return summary

def read_text(relative_path):
    path = out / relative_path
    if not path.exists():
        return None
    return path.read_text(errors="replace")

def first_line(relative_path):
    text = read_text(relative_path)
    if text is None:
        return None
    for line in text.splitlines():
        line = line.strip()
        if line:
            return line
    return None

def high_value_gpu_mux_summary():
    mux_summary = read_json_log("gpu-mux/pre/mux-summary.json")
    if isinstance(mux_summary, dict):
        summary = {"available": True}
        for key in (
            "schema_version",
            "phase",
            "current_mode",
            "drm_provider_count",
            "nvidia_pci_entry_count",
            "d3cold_indicator_count",
            "first_d3cold_indicator",
            "vgaswitcheroo_checked",
            "timestamp",
            "kernel",
            "hostname",
            "session_type",
        ):
            if key in mux_summary:
                summary[key] = mux_summary[key]
        return summary

    manifest_text = read_text("gpu-mux/pre/manifest.txt")
    if manifest_text is None:
        return {"available": False, "reason": "GPU mux evidence was not captured"}
    summary = {
        "available": True,
        "phase": "mux-only",
        "current_mode": first_line("gpu-mux/pre/envycontrol-mode.txt") or "unknown",
        "drm_provider_count": map_count((read_text("gpu-mux/pre/drm-providers.txt") or "").splitlines()),
        "nvidia_pci_entry_count": map_count((read_text("gpu-mux/pre/nvidia-pci-enable.txt") or "").splitlines()),
    }
    manifest = {}
    for line in manifest_text.splitlines():
        if "=" in line:
            key, value = line.split("=", 1)
            manifest[key] = value
    for key in ("timestamp", "kernel", "hostname"):
        if key in manifest:
            summary[key] = manifest[key]
    indicators = read_text("gpu-mux/pre/mux-hardware-indicators.txt") or ""
    d3cold_lines = [
        line.strip()
        for line in indicators.splitlines()
        if "driver=" in line and "d3cold_allowed=" in line
    ]
    if d3cold_lines:
        summary["first_d3cold_indicator"] = d3cold_lines[0]
        summary["d3cold_indicator_count"] = len(d3cold_lines)
    if "vgaswitcheroo" in indicators:
        summary["vgaswitcheroo_checked"] = True
    return summary

def map_count(value):
    if isinstance(value, dict):
        return len(value)
    if isinstance(value, list):
        return len(value)
    return 0

def first_map_item_summary(value, keys):
    if isinstance(value, dict):
        iterator = value.items()
    elif isinstance(value, list):
        iterator = enumerate(value)
    else:
        return None

    for item_key, item_value in iterator:
        if not isinstance(item_value, dict):
            continue
        summary = {"id": str(item_key)}
        for key in keys:
            if key in item_value:
                summary[key] = item_value[key]
        return summary
    return None

def high_value_automation_summary():
    automation = read_json_log("logs/automation-diagnostics.stdout")
    if not isinstance(automation, dict):
        return {"available": False, "reason": "automation diagnostics JSON was not captured"}
    automation_rules = automation.get("automation_rules")

    summary = {
        "hardware_profile_count": map_count(automation.get("hardware_profiles")),
        "hardware_profile_trigger_count": map_count(automation.get("hardware_profile_triggers")),
        "automation_rule_count": map_count(automation_rules),
        "last_automation_rule_apply_count": map_count(automation.get("last_automation_rule_apply")),
        "recent_platform_profile_change_count": map_count(
            automation.get("recent_platform_profile_changes")
        ),
        "recent_desktop_power_profile_change_count": map_count(
            automation.get("recent_desktop_power_profile_changes")
        ),
    }
    if isinstance(automation_rules, dict):
        kinds = {}
        for rule in automation_rules.values():
            if isinstance(rule, dict):
                kind = rule.get("kind") or rule.get("trigger_kind") or "unknown"
                kinds[kind] = kinds.get(kind, 0) + 1
        summary["automation_rule_kinds"] = dict(sorted(kinds.items()))
    first_rule = first_map_item_summary(
        automation_rules,
        (
            "kind",
            "profile_id",
            "enabled",
            "trigger",
            "trigger_kind",
            "cooldown_secs",
            "cooldown_seconds",
            "threshold_percent",
            "when_below_or_equal",
            "require_ac",
            "ac_profile_id",
            "battery_profile_id",
            "fast_charge_profile_id",
            "protect_profile_id",
        ),
    )
    if first_rule is not None:
        summary["first_rule"] = first_rule
    first_rule_apply = first_map_item_summary(
        automation.get("last_automation_rule_apply"),
        ("rule_id", "selected_profile_id", "completed", "message", "timestamp_unix_secs"),
    )
    if first_rule_apply is not None:
        summary["first_rule_apply"] = first_rule_apply
    first_change = first_map_item_summary(
        automation.get("recent_platform_profile_changes"),
        ("previous_profile", "current_profile", "source", "timestamp_unix_secs"),
    )
    if first_change is not None:
        summary["first_recent_platform_profile_change"] = first_change
    first_desktop_power_change = first_map_item_summary(
        automation.get("recent_desktop_power_profile_changes"),
        ("previous_profile", "current_profile", "source", "timestamp_unix_secs"),
    )
    if first_desktop_power_change is not None:
        summary["first_recent_desktop_power_profile_change"] = first_desktop_power_change
    last_apply = automation.get("last_hardware_profile_apply")
    if isinstance(last_apply, dict):
        summary["last_hardware_profile_apply"] = {
            key: last_apply[key]
            for key in ("profile_id", "profile_label", "completed", "message", "timestamp_unix_secs")
            if key in last_apply
        }
    return summary

report = {
    "schema_version": 1,
    "generated_at_utc": dt.datetime.now(dt.timezone.utc).replace(microsecond=0).isoformat(),
    "sysfs_root": sys.argv[2],
    "tools": {
        "legion_control_ui": sys.argv[3] or None,
        "legion_probe": sys.argv[4] or None,
        "openrgb_checker": sys.argv[5] or None,
        "openrgb_bridge_status": sys.argv[6] or None,
        "openrgb_sdk_evidence": sys.argv[14] or None,
        "gpu_mux_evidence": sys.argv[16] or None,
    },
    "captures": {
        "overview": sys.argv[7],
        "diagnostics": sys.argv[8],
        "probe": sys.argv[9],
        "openrgb_readiness": sys.argv[10],
        "openrgb_bridge_status": sys.argv[11],
        "automation_diagnostics": sys.argv[12],
        "reset_diagnostics": sys.argv[13],
        "openrgb_sdk_evidence": sys.argv[15],
        "gpu_mux_evidence": sys.argv[17],
    },
    "safety": {
        "read_only": True,
        "no_sysfs_writes": True,
        "no_hidraw_writes": True,
        "no_i2c_writes": True,
    },
    "high_value_recovery": reset_recovery_summary(),
    "high_value_drift": high_value_drift_summary(),
    "high_value_gpu_switching": high_value_gpu_switching_summary(),
    "high_value_gpu_mux": high_value_gpu_mux_summary(),
    "high_value_automation": high_value_automation_summary(),
}
out.joinpath("compatibility-bundle.json").write_text(json.dumps(report, indent=2, sort_keys=True) + "\n")

def first_list_value(value):
    if isinstance(value, list) and value:
        return value[0]
    return "none"

def drift_status(key):
    item = report["high_value_drift"].get(key)
    if isinstance(item, dict):
        status = item.get("status", "unknown")
        drifted = item.get("drifted_count")
        checked = item.get("checked_count")
        if drifted is not None and checked is not None:
            return f"{status} ({drifted}/{checked})"
        return status
    return "unavailable"

gpu_switching = report["high_value_gpu_switching"]
gpu_first_blocker = first_list_value(gpu_switching.get("blockers"))
gpu_first_evidence = first_list_value(gpu_switching.get("evidence"))
gpu_mux = report["high_value_gpu_mux"]
gpu_mux_mode = gpu_mux.get("current_mode", "unavailable")
gpu_mux_first_d3cold = gpu_mux.get("first_d3cold_indicator", "none")
hardware_profile_drift = drift_status("hardware_profile_drift")
fan_curve_drift = drift_status("fan_curve_drift")
automation = report["high_value_automation"]
automation_first_rule = automation.get("first_rule")
automation_first_rule_kind = "none"
automation_first_rule_profile = "none"
if isinstance(automation_first_rule, dict):
    automation_first_rule_kind = (
        automation_first_rule.get("kind")
        or automation_first_rule.get("trigger_kind")
        or "unknown"
    )
    automation_first_rule_profile = (
        automation_first_rule.get("profile_id")
        or automation_first_rule.get("ac_profile_id")
        or automation_first_rule.get("fast_charge_profile_id")
        or "none"
    )
automation_rule_kinds = automation.get("automation_rule_kinds")
if isinstance(automation_rule_kinds, dict) and automation_rule_kinds:
    automation_rule_kinds_text = ", ".join(
        f"{kind}:{count}" for kind, count in automation_rule_kinds.items()
    )
else:
    automation_rule_kinds_text = "none"

lines = [
    "# RatVantage Compatibility Bundle",
    "",
    f"- generated_at_utc: `{report['generated_at_utc']}`",
    f"- sysfs_root: `{report['sysfs_root']}`",
    f"- overview: `{report['captures']['overview']}`",
    f"- diagnostics: `{report['captures']['diagnostics']}`",
    f"- automation_diagnostics: `{report['captures']['automation_diagnostics']}`",
    f"- reset_diagnostics: `{report['captures']['reset_diagnostics']}`",
    f"- probe: `{report['captures']['probe']}`",
    f"- openrgb_readiness: `{report['captures']['openrgb_readiness']}`",
    f"- openrgb_bridge_status: `{report['captures']['openrgb_bridge_status']}`",
    f"- gpu_mux_evidence: `{report['captures']['gpu_mux_evidence']}`",
    f"- high_value_recovery: `{len(report['high_value_recovery'])}` entries",
    f"- high_value_drift: `{len(report['high_value_drift'])}` entries",
    f"- high_value_gpu_switching: `{report['high_value_gpu_switching'].get('status', 'unknown')}`",
    f"- high_value_gpu_mux: `{gpu_mux.get('phase', 'unavailable')}`",
    f"- high_value_automation: `{report['high_value_automation'].get('automation_rule_count', 0)}` rules",
    "",
    "## High-Value Summary",
    f"- gpu_switching_next_action: `{gpu_switching.get('next_action', 'unknown')}`",
    f"- gpu_switching_first_blocker: `{gpu_first_blocker}`",
    f"- gpu_switching_first_evidence: `{gpu_first_evidence}`",
    f"- gpu_mux_current_mode: `{gpu_mux_mode}`",
    f"- gpu_mux_first_d3cold: `{gpu_mux_first_d3cold}`",
    f"- hardware_profile_drift: `{hardware_profile_drift}`",
    f"- fan_curve_drift: `{fan_curve_drift}`",
    f"- automation_rule_kinds: `{automation_rule_kinds_text}`",
    f"- automation_first_rule: `{automation_first_rule_kind}` -> `{automation_first_rule_profile}`",
    "",
    "## Files",
    "- `logs/overview.stdout`",
    "- `logs/diagnostics.stdout`",
    "- `logs/automation-diagnostics.stdout`",
    "- `logs/reset-diagnostics.stdout`",
    "- `logs/probe.stdout`",
    "- `openrgb-readiness/openrgb-keyboard-rgb-readiness.json` when captured",
    "- `logs/openrgb-bridge-status.json` when captured",
    "- `openrgb-sdk/openrgb-keyboard-rgb-sdk-evidence.json` when captured",
    "- `gpu-mux/pre/` when captured",
    "",
    "## Safety",
    "- Read-only capture only.",
    "- No sysfs, hidraw, i2c, WMI, or EC writes.",
]
out.joinpath("compatibility-bundle.md").write_text("\n".join(lines) + "\n")

pr_lines = [
    "## RatVantage Compatibility Report",
    "",
    "### Capture",
    f"- generated_at_utc: `{report['generated_at_utc']}`",
    f"- sysfs_root: `{report['sysfs_root']}`",
    f"- overview: `{report['captures']['overview']}`",
    f"- diagnostics: `{report['captures']['diagnostics']}`",
    f"- automation_diagnostics: `{report['captures']['automation_diagnostics']}`",
    f"- reset_diagnostics: `{report['captures']['reset_diagnostics']}`",
    f"- probe: `{report['captures']['probe']}`",
    f"- gpu_mux_evidence: `{report['captures']['gpu_mux_evidence']}`",
    "",
    "### High-Value Evidence",
    f"- recovery_entries: `{len(report['high_value_recovery'])}`",
    f"- drift_entries: `{len(report['high_value_drift'])}`",
    f"- gpu_switching: `{report['high_value_gpu_switching'].get('status', 'unknown')}`",
    f"- gpu_switching_next_action: `{gpu_switching.get('next_action', 'unknown')}`",
    f"- gpu_switching_first_blocker: `{gpu_first_blocker}`",
    f"- gpu_mux_current_mode: `{gpu_mux_mode}`",
    f"- gpu_mux_first_d3cold: `{gpu_mux_first_d3cold}`",
    f"- hardware_profile_drift: `{hardware_profile_drift}`",
    f"- fan_curve_drift: `{fan_curve_drift}`",
    f"- automation_rules: `{report['high_value_automation'].get('automation_rule_count', 0)}`",
    f"- automation_rule_kinds: `{automation_rule_kinds_text}`",
    f"- automation_first_rule: `{automation_first_rule_kind}` -> `{automation_first_rule_profile}`",
    f"- recent_profile_changes: `{report['high_value_automation'].get('recent_platform_profile_change_count', 0)}`",
    f"- recent_desktop_power_changes: `{report['high_value_automation'].get('recent_desktop_power_profile_change_count', 0)}`",
    "",
    "### Safety",
    "- Read-only capture only.",
    "- No sysfs, hidraw, i2c, WMI, or EC writes.",
    "",
    "### Attached Bundle Files",
    "- `compatibility-bundle.json`",
    "- `compatibility-bundle.md`",
    "- `logs/overview.stdout`",
    "- `logs/diagnostics.stdout`",
    "- `logs/automation-diagnostics.stdout`",
    "- `logs/reset-diagnostics.stdout`",
    "- `logs/probe.stdout`",
    "- `gpu-mux/pre/` when captured.",
    "- OpenRGB readiness/SDK evidence files when captured.",
]
out.joinpath("compatibility-bundle-pr-body.md").write_text("\n".join(pr_lines) + "\n")
PY

echo "compatibility_bundle=$output/compatibility-bundle.json"
