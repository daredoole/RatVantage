#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
script="$repo_root/scripts/capture-compatibility-bundle.sh"
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

ui="$tmp/legion-control-ui"
probe="$tmp/legion-probe"
checker="$tmp/check-openrgb"
bridge_status="$tmp/openrgb-bridge-status"
sdk_evidence="$tmp/openrgb-sdk-evidence"
gpu_mux="$tmp/gpu-mux-evidence"
out="$tmp/bundle"

cat >"$ui" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail
case "${1:-}" in
  --overview)
    echo "Legion Control overview"
    echo "keyboard_rgb_status=research_candidates=0 backend_ready=false"
    ;;
  --diagnostics)
    echo '{"summary":{"capability_count":1},"hardware_profile_drift":{"status":"drifted","profile_id":"rgb_breathing_blue","checked_count":1,"drifted_count":1,"items":[{"action_id":"keyboard_rgb","method":"SetOpenRgbKeyboardRgbSdk","requested_value":"Breathing #333333","readback_value":"Breathing #333333","current_value":"Direct #000000","status":"drifted","detail":"current value differs"}]},"fan_curve_drift":{"status":"drifted","curve_id":"quiet-office","checked_count":1,"drifted_count":1,"detail":"Live fan curve differs","items":[{"path":"hwmon0/pwm1_auto_point1_pwm","saved_value":"80","live_value":"100","status":"drifted"}]},"gpu_switching":{"status":"runtime_candidate_blocked","provider":"envycontrol","current_mode":"hybrid","switch_type":"reboot-required","execution_model":"runtime_candidate","runtime_plan_available":false,"blockers":["gpu_runtime candidate is detected but not promoted","strict GPU mux/session evidence review has not accepted this path"],"evidence":["gpu_runtime_candidate_modes=integrated","gpu_runtime_promotion_ready=false","gpu_runtime_current_mode=hybrid"],"next_action":"run ratvantage-review-gpu-mux-evidence --require-session-restart-confirmed before promoting runtime planning"}}'
    ;;
  --automation-diagnostics)
    if [[ "${RATVANTAGE_TEST_EMPTY_AUTOMATION:-0}" == "1" ]]; then
      echo '{"hardware_profiles":{},"hardware_profile_triggers":{},"automation_rules":{},"last_automation_rule_apply":{},"recent_platform_profile_changes":[],"recent_desktop_power_profile_changes":[]}'
      exit 0
    fi
    echo '{"hardware_profiles":{"quiet_battery":{"label":"Quiet battery"},"periodic_repair":{"label":"Periodic repair"}},"hardware_profile_triggers":{"ac_unplugged":"quiet_battery","desktop_power_profile_changed":"quiet_battery"},"automation_rules":{"quiet_below_30":{"profile_id":"quiet_battery","enabled":true,"kind":"battery_profile_threshold","threshold_percent":30,"cooldown_secs":600},"periodic_idle_correction":{"profile_id":"periodic_repair","enabled":true,"kind":"periodic_idle","cooldown_secs":1800}},"last_automation_rule_apply":{"quiet_below_30":{"rule_id":"quiet_below_30","selected_profile_id":"quiet_battery","completed":true,"message":"profile applied","timestamp_unix_secs":42}},"recent_platform_profile_changes":[{"previous_profile":"balanced","current_profile":"performance","source":"firmware","timestamp_unix_secs":41}],"recent_desktop_power_profile_changes":[{"previous_profile":"balanced","current_profile":"power-saver","source":"desktop_power_profile_observer","timestamp_unix_secs":43}],"last_hardware_profile_apply":{"profile_id":"quiet_battery","profile_label":"Quiet battery","completed":true,"message":"all actions applied","timestamp_unix_secs":40}}'
    ;;
  --reset-diagnostics)
    echo '{"curve_optimizer_all_core_reset":{"ok":true,"plan_command":"legion-control-ui --plan-curve-optimizer-all-core 0","execute_command":"legion-control-ui --reset-curve-optimizer-all-core"},"keyboard_rgb_sdk_recovery":{"ok":true,"value":{"available":true,"current_mode":"Breathing","current_colors":{"left":"#112233"},"plan_command":"legion-control-ui --plan-openrgb-keyboard-rgb-sdk '\''{\"effect\":\"Breathing\"}'\''","recovery_note":"read-only"}},"gpu_mode_pending_recovery":{"ok":true,"value":{"pending":{"requested_mode":"hybrid","previous_mode":"nvidia","reboot_required":true},"clear_command":"legion-control-ui --clear-gpu-mode-pending","verification_command":"legion-control-ui --overview"}},"gpu_switching_recovery":{"ok":true,"value":{"available":true,"status":"reboot_required","current_mode":"hybrid","switch_type":"reboot-required","verification_command":"legion-control-ui --overview","steps":["reboot"]}}}'
    ;;
  *)
    echo "unexpected ui args: $*" >&2
    exit 2
    ;;
esac
EOF
chmod 0755 "$ui"

cat >"$probe" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail
if [[ "$1" != "--json" || "$2" != "--sysfs-root" ]]; then
  echo "unexpected probe args: $*" >&2
  exit 2
fi
echo '{"capabilities":[]}'
EOF
chmod 0755 "$probe"

cat >"$checker" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail
if [[ "$1" != "--output" ]]; then
  echo "unexpected checker args: $*" >&2
  exit 2
fi
mkdir -p "$2"
echo '{"openrgb":{"installed":true}}' >"$2/openrgb-keyboard-rgb-readiness.json"
EOF
chmod 0755 "$checker"

cat >"$bridge_status" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail
if [[ "${1:-}" != "--readiness" || "${3:-}" != "--json" ]]; then
  echo "unexpected bridge status args: $*" >&2
  exit 2
fi
test -f "$2/openrgb-keyboard-rgb-readiness.json"
cat <<'JSON'
{
  "dry_run": {"exists": true, "promotable": false},
  "execute": {"exists": false, "promotable": false},
  "readiness": {"exists": true, "ready_for_execute_evidence": true},
  "next_action": "operator may run execute evidence capture"
}
JSON
EOF
chmod 0755 "$bridge_status"

cat >"$sdk_evidence" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail
if [[ "$1" != "--output" ]]; then
  echo "unexpected sdk evidence args: $*" >&2
  exit 2
fi
mkdir -p "$2"
cat >"$2/openrgb-keyboard-rgb-sdk-evidence.json" <<'JSON'
{
  "keyboard": {"detected": true},
  "result": {"read_back_supported": true, "status": "ok"},
  "sdk": {"connected": true, "protocol_version": 4}
}
JSON
echo "openrgb_sdk_evidence=$2/openrgb-keyboard-rgb-sdk-evidence.json"
EOF
chmod 0755 "$sdk_evidence"

cat >"$gpu_mux" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail
if [[ "${1:-}" != "--phase" || "${2:-}" != "mux-only" || "${3:-}" != "--output" ]]; then
  echo "unexpected gpu mux args: $*" >&2
  exit 2
fi
bundle="$4/pre"
mkdir -p "$bundle"
cat >"$bundle/manifest.txt" <<'TXT'
phase=mux-only
timestamp=2026-06-04T15:00:00-04:00
hostname=fixture
kernel=6.9.0-test
user=tester
TXT
echo "hybrid" >"$bundle/envycontrol-mode.txt"
cat >"$bundle/drm-providers.txt" <<'TXT'
card0=amdgpu
card1=nvidia
TXT
cat >"$bundle/nvidia-pci-enable.txt" <<'TXT'
0000:01:00.0 enable=1 runtime=active
TXT
cat >"$bundle/mux-hardware-indicators.txt" <<'TXT'
=== d3cold_allowed on GPU PCI devices ===
  0000:01:00.0  driver=nvidia  d3cold_allowed=1
=== vgaswitcheroo ===
(not available)
TXT
cat >"$bundle/mux-summary.json" <<'JSON'
{
  "schema_version": 1,
  "read_only": true,
  "phase": "mux-only",
  "timestamp": "2026-06-04T15:00:00-04:00",
  "hostname": "fixture",
  "kernel": "6.9.0-test",
  "current_mode": "hybrid",
  "drm_provider_count": 2,
  "nvidia_pci_entry_count": 1,
  "d3cold_indicator_count": 1,
  "first_d3cold_indicator": "0000:01:00.0  driver=nvidia  d3cold_allowed=1",
  "vgaswitcheroo_checked": true,
  "session_type": "wayland"
}
JSON
echo "gpu_mux_evidence=$4"
EOF
chmod 0755 "$gpu_mux"

"$script" \
  --output "$out" \
  --sysfs-root "$tmp/sysfs" \
  --ui-bin "$ui" \
  --probe-bin "$probe" \
  --openrgb-checker "$checker" \
  --bridge-status-bin "$bridge_status" \
  --sdk-evidence-bin "$sdk_evidence" \
  --gpu-mux-bin "$gpu_mux" >/tmp/ratvantage-compatibility-bundle-test.txt

grep -q "compatibility_bundle=$out/compatibility-bundle.json" \
  /tmp/ratvantage-compatibility-bundle-test.txt
grep -q '"read_only": true' "$out/compatibility-bundle.json"
grep -q '"overview": "overview=ok"' "$out/compatibility-bundle.json"
grep -q '"automation_diagnostics": "automation-diagnostics=ok"' "$out/compatibility-bundle.json"
grep -q '"reset_diagnostics": "reset-diagnostics=ok"' "$out/compatibility-bundle.json"
grep -q '"openrgb_bridge_status": "openrgb-bridge-status=ok"' "$out/compatibility-bundle.json"
grep -q '"openrgb_sdk_evidence": "openrgb-sdk=ok"' "$out/compatibility-bundle.json"
grep -q '"gpu_mux_evidence": "gpu-mux=ok"' "$out/compatibility-bundle.json"
grep -q '"high_value_recovery"' "$out/compatibility-bundle.json"
grep -q '"high_value_drift"' "$out/compatibility-bundle.json"
grep -q '"high_value_gpu_switching"' "$out/compatibility-bundle.json"
grep -q '"high_value_gpu_mux"' "$out/compatibility-bundle.json"
grep -q '"high_value_automation"' "$out/compatibility-bundle.json"
grep -q '"current_mode": "hybrid"' "$out/compatibility-bundle.json"
grep -q '"schema_version": 1' "$out/compatibility-bundle.json"
grep -q '"session_type": "wayland"' "$out/compatibility-bundle.json"
grep -q '"first_d3cold_indicator": "0000:01:00.0  driver=nvidia  d3cold_allowed=1"' "$out/compatibility-bundle.json"
grep -q '"drm_provider_count": 2' "$out/compatibility-bundle.json"
grep -q '"nvidia_pci_entry_count": 1' "$out/compatibility-bundle.json"
grep -q '"automation_rule_count": 2' "$out/compatibility-bundle.json"
grep -q '"hardware_profile_count": 2' "$out/compatibility-bundle.json"
grep -q '"automation_rule_kinds"' "$out/compatibility-bundle.json"
grep -q '"battery_profile_threshold": 1' "$out/compatibility-bundle.json"
grep -q '"periodic_idle": 1' "$out/compatibility-bundle.json"
grep -q '"recent_platform_profile_change_count": 1' "$out/compatibility-bundle.json"
grep -q '"recent_desktop_power_profile_change_count": 1' "$out/compatibility-bundle.json"
grep -q '"first_rule"' "$out/compatibility-bundle.json"
grep -q '"kind": "battery_profile_threshold"' "$out/compatibility-bundle.json"
grep -q '"cooldown_secs": 600' "$out/compatibility-bundle.json"
grep -q '"profile_id": "quiet_battery"' "$out/compatibility-bundle.json"
grep -q '"first_rule_apply"' "$out/compatibility-bundle.json"
grep -q '"selected_profile_id": "quiet_battery"' "$out/compatibility-bundle.json"
grep -q '"first_recent_platform_profile_change"' "$out/compatibility-bundle.json"
grep -q '"current_profile": "performance"' "$out/compatibility-bundle.json"
grep -q '"first_recent_desktop_power_profile_change"' "$out/compatibility-bundle.json"
grep -q '"current_profile": "power-saver"' "$out/compatibility-bundle.json"
grep -q '"source": "desktop_power_profile_observer"' "$out/compatibility-bundle.json"
grep -q '"last_hardware_profile_apply"' "$out/compatibility-bundle.json"
grep -q '"keyboard_rgb_sdk_recovery"' "$out/compatibility-bundle.json"
grep -q '"current_mode": "Breathing"' "$out/compatibility-bundle.json"
grep -q '"plan_command": "legion-control-ui --plan-curve-optimizer-all-core 0"' "$out/compatibility-bundle.json"
grep -q '"execute_command": "legion-control-ui --reset-curve-optimizer-all-core"' "$out/compatibility-bundle.json"
grep -q '"plan_command": "legion-control-ui --plan-openrgb-keyboard-rgb-sdk' "$out/compatibility-bundle.json"
grep -q '"clear_command": "legion-control-ui --clear-gpu-mode-pending"' "$out/compatibility-bundle.json"
grep -q '"verification_command": "legion-control-ui --overview"' "$out/compatibility-bundle.json"
grep -q '"gpu_switching_recovery"' "$out/compatibility-bundle.json"
grep -q '"status": "reboot_required"' "$out/compatibility-bundle.json"
grep -q '"switch_type": "reboot-required"' "$out/compatibility-bundle.json"
grep -q '"hardware_profile_drift"' "$out/compatibility-bundle.json"
grep -q '"fan_curve_drift"' "$out/compatibility-bundle.json"
grep -q '"action_id": "keyboard_rgb"' "$out/compatibility-bundle.json"
grep -q '"current_value": "Direct #000000"' "$out/compatibility-bundle.json"
grep -q '"path": "hwmon0/pwm1_auto_point1_pwm"' "$out/compatibility-bundle.json"
grep -q '"live_value": "100"' "$out/compatibility-bundle.json"
grep -q '"status": "runtime_candidate_blocked"' "$out/compatibility-bundle.json"
grep -q '"switch_type": "reboot-required"' "$out/compatibility-bundle.json"
grep -q '"runtime_plan_available": false' "$out/compatibility-bundle.json"
grep -q "gpu_runtime_candidate_modes=integrated" "$out/compatibility-bundle.json"
grep -q "gpu_runtime_promotion_ready=false" "$out/compatibility-bundle.json"
grep -q "keyboard_rgb_status=research_candidates=0" "$out/logs/overview.stdout"
grep -q "hardware_profile_drift" "$out/logs/diagnostics.stdout"
grep -q "fan_curve_drift" "$out/logs/diagnostics.stdout"
grep -q "runtime_candidate_blocked" "$out/logs/diagnostics.stdout"
grep -q "hardware_profiles" "$out/logs/automation-diagnostics.stdout"
grep -q "quiet_below_30" "$out/logs/automation-diagnostics.stdout"
grep -q "recent_platform_profile_changes" "$out/logs/automation-diagnostics.stdout"
grep -q "recent_desktop_power_profile_changes" "$out/logs/automation-diagnostics.stdout"
grep -q "curve_optimizer_all_core_reset" "$out/logs/reset-diagnostics.stdout"
grep -q "gpu_mode_pending_recovery" "$out/logs/reset-diagnostics.stdout"
grep -q "clear-gpu-mode-pending" "$out/logs/reset-diagnostics.stdout"
grep -q '"capabilities"' "$out/logs/probe.stdout"
test -f "$out/openrgb-readiness/openrgb-keyboard-rgb-readiness.json"
grep -q "operator may run execute evidence capture" "$out/logs/openrgb-bridge-status.json"
grep -q "ready_for_execute_evidence" "$out/logs/openrgb-bridge-status.json"
test -f "$out/openrgb-sdk/openrgb-keyboard-rgb-sdk-evidence.json"
grep -q '"read_back_supported": true' "$out/openrgb-sdk/openrgb-keyboard-rgb-sdk-evidence.json"
test -f "$out/gpu-mux/pre/mux-hardware-indicators.txt"
test -f "$out/gpu-mux/pre/mux-summary.json"
grep -q "d3cold_allowed=1" "$out/gpu-mux/pre/mux-hardware-indicators.txt"
grep -q "RatVantage Compatibility Bundle" "$out/compatibility-bundle.md"
grep -q "automation_diagnostics" "$out/compatibility-bundle.md"
grep -q "reset_diagnostics" "$out/compatibility-bundle.md"
grep -q "high_value_recovery" "$out/compatibility-bundle.md"
grep -q "high_value_drift" "$out/compatibility-bundle.md"
grep -q "high_value_gpu_switching" "$out/compatibility-bundle.md"
grep -q "high_value_gpu_mux" "$out/compatibility-bundle.md"
grep -q "high_value_automation" "$out/compatibility-bundle.md"
grep -q "gpu_switching_next_action" "$out/compatibility-bundle.md"
grep -q "ratvantage-review-gpu-mux-evidence" "$out/compatibility-bundle.md"
grep -q "gpu_switching_first_blocker" "$out/compatibility-bundle.md"
grep -q "gpu_runtime candidate is detected but not promoted" "$out/compatibility-bundle.md"
grep -q 'gpu_mux_current_mode: `hybrid`' "$out/compatibility-bundle.md"
grep -q 'gpu_mux_first_d3cold: `0000:01:00.0  driver=nvidia  d3cold_allowed=1`' "$out/compatibility-bundle.md"
grep -q 'hardware_profile_drift: `drifted (1/1)`' "$out/compatibility-bundle.md"
grep -q 'fan_curve_drift: `drifted (1/1)`' "$out/compatibility-bundle.md"
grep -q 'automation_rule_kinds: `battery_profile_threshold:1, periodic_idle:1`' "$out/compatibility-bundle.md"
grep -q 'automation_first_rule: `battery_profile_threshold` -> `quiet_battery`' "$out/compatibility-bundle.md"
grep -q "openrgb-sdk" "$out/compatibility-bundle.md"
test -f "$out/compatibility-bundle-pr-body.md"
grep -q "RatVantage Compatibility Report" "$out/compatibility-bundle-pr-body.md"
grep -q "recovery_entries" "$out/compatibility-bundle-pr-body.md"
grep -q "drift_entries" "$out/compatibility-bundle-pr-body.md"
grep -q "gpu_switching" "$out/compatibility-bundle-pr-body.md"
grep -q "gpu_switching_next_action" "$out/compatibility-bundle-pr-body.md"
grep -q "gpu_switching_first_blocker" "$out/compatibility-bundle-pr-body.md"
grep -q "gpu_mux_current_mode" "$out/compatibility-bundle-pr-body.md"
grep -q "d3cold_allowed=1" "$out/compatibility-bundle-pr-body.md"
grep -q 'fan_curve_drift: `drifted (1/1)`' "$out/compatibility-bundle-pr-body.md"
grep -q "automation_rule_kinds" "$out/compatibility-bundle-pr-body.md"
grep -q "periodic_idle:1" "$out/compatibility-bundle-pr-body.md"
grep -q "automation_first_rule" "$out/compatibility-bundle-pr-body.md"
grep -q "automation_rules" "$out/compatibility-bundle-pr-body.md"
grep -q "recent_profile_changes" "$out/compatibility-bundle-pr-body.md"
grep -q "recent_desktop_power_changes" "$out/compatibility-bundle-pr-body.md"
grep -q "Read-only capture only" "$out/compatibility-bundle-pr-body.md"

empty_out="$tmp/empty-bundle"
RATVANTAGE_TEST_EMPTY_AUTOMATION=1 "$script" \
  --output "$empty_out" \
  --sysfs-root "$tmp/sysfs" \
  --ui-bin "$ui" \
  --probe-bin "$probe" \
  --openrgb-checker "$checker" \
  --bridge-status-bin "$bridge_status" \
  --sdk-evidence-bin "$sdk_evidence" \
  --gpu-mux-bin "$gpu_mux" >/tmp/ratvantage-empty-compatibility-bundle-test.txt

grep -q '"automation_rule_count": 0' "$empty_out/compatibility-bundle.json"
grep -q '"automation_rule_kinds": {}' "$empty_out/compatibility-bundle.json"
grep -q 'automation_rule_kinds: `none`' "$empty_out/compatibility-bundle.md"
grep -q 'automation_first_rule: `none` -> `none`' "$empty_out/compatibility-bundle.md"
grep -q 'automation_rule_kinds: `none`' "$empty_out/compatibility-bundle-pr-body.md"

echo "capture-compatibility-bundle tests passed"
