#!/usr/bin/env bash
# Regression tests for scripts/verify-82wm-live-evidence.sh.

set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
verifier="$repo_root/scripts/verify-82wm-live-evidence.sh"
requirements_file="$repo_root/data/validation/82wm-live-evidence-requirements.tsv"
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

check_required_controls_are_documented() {
  python3 - "$requirements_file" \
    "$repo_root/docs/live-validation-evidence-runbook.md" \
    "$repo_root/docs/live-write-validation.md" \
    "$repo_root/docs/driver-surface-read-write-gui-plan-82wm.md" <<'PY'
import pathlib
import sys

requirements_path = pathlib.Path(sys.argv[1])
doc_paths = [pathlib.Path(path) for path in sys.argv[2:]]
controls = []
for line in requirements_path.read_text().splitlines():
    if not line.strip() or line.startswith("#"):
        continue
    controls.append(line.split("\t", 1)[0])

failed = False
for doc_path in doc_paths:
    text = doc_path.read_text(errors="replace")
    missing = [control_id for control_id in controls if control_id not in text]
    if missing:
        failed = True
        print(f"{doc_path}: missing required control ids: {', '.join(missing)}", file=sys.stderr)

runbook_text = doc_paths[0].read_text(errors="replace")
for line in requirements_path.read_text().splitlines():
    if not line.strip() or line.startswith("#"):
        continue
    control_id, expected_status, _rollback_required, daemon_flag, output_slug = line.split("\t", 5)[:5]
    required_gate = f"--require-control {control_id}={expected_status}"
    if required_gate not in runbook_text:
        failed = True
        print(f"{doc_paths[0]}: missing review gate `{required_gate}`", file=sys.stderr)
    expected_output = f"target/validation/82wm-live-{output_slug}"
    if expected_output not in runbook_text:
        failed = True
        print(f"{doc_paths[0]}: missing output directory `{expected_output}` for {control_id}", file=sys.stderr)
    for doc_path in doc_paths[:2]:
        if daemon_flag not in doc_path.read_text(errors="replace"):
            failed = True
            print(f"{doc_path}: missing daemon flag `{daemon_flag}` for {control_id}", file=sys.stderr)

if failed:
    sys.exit(1)
PY
}

make_report() {
  local root="$1"
  local control_id="$2"
  local status="$3"
  local shape="$4"
  local output_slug="$5"
  python3 - "$root" "$control_id" "$status" "$shape" "$output_slug" <<'PY'
import json
import pathlib
import sys

root = pathlib.Path(sys.argv[1])
control_id, status, shape, output_slug = sys.argv[2:6]
directory = root / f"82wm-live-{output_slug}"
directory.mkdir(parents=True, exist_ok=True)

if control_id.startswith("firmware_attribute:"):
    plan = {
        "method": "SetFirmwareAttribute",
        "path": "/tmp/synthetic",
        "polkit_action": "org.ratvantage.LegionControl1.set-firmware-attribute",
        "readback_required": True,
    }
else:
    method_by_control = {
        "conservation_mode": "SetConservationMode",
        "cpu_governor": "SetCpuGovernor",
        "cpu_epp": "SetCpuEpp",
        "cpu_boost": "SetCpuBoost",
        "fan_mode": "SetIdeapadToggle",
        "amd_gpu_dpm_force_level": "SetAmdGpuDpmForceLevel",
        "curve_optimizer_all_core": "SetCurveOptimizerAllCore",
        "gpu_mode": "SetGpuMode",
    }
    polkit_by_control = {
        "conservation_mode": "org.ratvantage.LegionControl1.set-conservation-mode",
        "cpu_governor": "org.ratvantage.LegionControl1.set-cpu-governor",
        "cpu_epp": "org.ratvantage.LegionControl1.set-cpu-epp",
        "cpu_boost": "org.ratvantage.LegionControl1.set-cpu-boost",
        "fan_mode": "org.ratvantage.LegionControl1.set-ideapad-toggle",
        "amd_gpu_dpm_force_level": "org.ratvantage.LegionControl1.set-amd-gpu-dpm-force-level",
        "curve_optimizer_all_core": "org.ratvantage.LegionControl1.set-curve-optimizer",
        "gpu_mode": "org.ratvantage.LegionControl1.set-gpu-mode",
    }
    if control_id in {"hardware_profile", "hardware_profile_trigger"}:
        plan = {
            "plans": [
                {
                    "method": "SetCpuGovernor",
                    "polkit_action": "org.ratvantage.LegionControl1.set-cpu-governor",
                    "readback_required": True,
                },
                {
                    "method": "SetCpuEpp",
                    "polkit_action": "org.ratvantage.LegionControl1.set-cpu-epp",
                    "readback_required": True,
                },
                {
                    "method": "SetCpuBoost",
                    "polkit_action": "org.ratvantage.LegionControl1.set-cpu-boost",
                    "readback_required": True,
                },
            ]
        }
    else:
        readback_by_control = {
            "conservation_mode": True,
            "cpu_governor": True,
            "cpu_epp": True,
            "cpu_boost": True,
            "fan_mode": True,
            "amd_gpu_dpm_force_level": True,
            "curve_optimizer_all_core": False,
            "gpu_mode": True,
        }
        plan = {
            "method": method_by_control.get(control_id, "Synthetic"),
            "path": "/tmp/synthetic",
            "polkit_action": polkit_by_control.get(
                control_id, "org.ratvantage.LegionControl1.synthetic"
            ),
            "readback_required": readback_by_control.get(control_id, True),
        }

control = {
    "control_id": control_id,
    "status": status,
    "available": True,
    "requested": "synthetic",
    "plan_file": "steps/plan.json",
    "plan_exit": 0,
    "plan": plan,
    "set_file": "steps/apply.json",
    "set_exit": 0,
}

if shape == "pass":
    control.update({
        "set_result": {"applied": True, "status": "Applied"},
        "revert_file": "steps/revert.json",
        "revert_exit": 0,
        "revert_result": {"applied": True, "status": "Applied"},
    })
elif shape == "executed-profile":
    control.update({
        "set_result": {
            "completed": True,
            "message": "hardware profile applied",
            "results": [
                {"action_id": "cpu_governor"},
                {"action_id": "cpu_epp"},
                {"action_id": "cpu_boost"},
            ],
        },
        "revert_file": None,
        "revert_exit": None,
        "revert_result": None,
    })
elif shape == "executed-write":
    control.update({
        "set_result": {"applied": True, "status": "Applied"},
        "revert_file": None,
        "revert_exit": None,
        "revert_result": None,
    })
elif shape == "negative-ppt":
    control.update({
        "current": "70",
        "requested": "71",
        "set_result": {
            "applied": False,
            "status": "failed",
            "message": "failed to write firmware attribute: Device or resource busy (os error 16)",
        },
        "revert_file": "steps/revert.json",
        "revert_exit": 0,
        "revert_result": {
            "applied": False,
            "status": "failed",
            "message": "failed to write firmware attribute: Device or resource busy (os error 16)",
        },
    })
elif shape == "negative-fan-mode":
    control.update({
        "current": "0",
        "requested": "1",
        "set_result": {
            "applied": False,
            "status": "failed",
            "message": "ideapad toggle read-back mismatch after write; restored previous value `0`",
            "readback_value": "0",
        },
        "revert_file": "steps/revert.json",
        "revert_exit": 0,
        "revert_result": {"applied": True, "status": "Applied", "readback_value": "0"},
    })
elif shape == "missing-revert":
    control.update({
        "set_result": {"applied": True, "status": "Applied"},
        "revert_file": None,
        "revert_exit": None,
        "revert_result": None,
    })
else:
    raise SystemExit(f"unknown shape {shape}")

if control_id == "curve_optimizer_all_core":
    control.update({
        "requested": "-20",
        "set_result": {
            "applied": True,
            "status": "Applied",
            "readback_value": "offset=-20 encoded=4294967276 readback=write_only",
        },
        "revert_result": {
            "applied": True,
            "status": "Applied",
            "readback_value": "offset=0 encoded=0 readback=write_only",
        },
        "curve_state_after_apply_file": "steps/curve-last-state-after-apply.json",
        "curve_state_after_apply": {
            "signed_offset": -20,
            "encoded_value": 4294967276,
            "backend": "ryzenadj",
            "readback_status": "write_only",
        },
        "curve_state_after_revert_file": "steps/curve-last-state-after-revert.json",
        "curve_state_after_revert": {
            "signed_offset": 0,
            "encoded_value": 0,
            "backend": "ryzenadj",
            "readback_status": "write_only",
        },
    })

report = {
    "schema_version": 1,
    "metadata": {
        "mode": "execute",
        "execute_only": control_id,
        "target_bus_mode": "system",
        "sysfs_root": "/",
    },
    "controls": [control],
}
(directory / "validation-report.json").write_text(json.dumps(report, indent=2) + "\n")
if control_id == "curve_optimizer_all_core":
    (directory / "operator-checklist.md").write_text(
        "# Operator checklist\n\n"
        "- `curve_optimizer_all_core`: confirm RyzenAdj success, reset to 0, and record stability check notes.\n"
    )
if control_id == "fan_mode":
    (directory / "operator-checklist.md").write_text(
        "# Operator checklist\n\n"
        "- `fan_mode`: confirm Auto (0) to Full speed (1) to Auto (0), and record observed thermal/fan behavior.\n"
    )
if control_id == "gpu_mode":
    (directory / "operator-checklist.md").write_text(
        "# Operator checklist\n\n"
        "- `gpu_mode`: confirm EnvyControl command success, reboot guidance, and recovery path before/after execution.\n"
    )
PY
}

complete="$tmp/complete"
check_required_controls_are_documented
while IFS=$'\t' read -r control_id expected_status _rollback_required _daemon_flag output_slug _description; do
  [[ -z "${control_id:-}" || "$control_id" == \#* ]] && continue
  case "$expected_status:$control_id" in
    pass:*)
      make_report "$complete" "$control_id" "$expected_status" pass "$output_slug"
      ;;
    executed:firmware_attribute:*)
      make_report "$complete" "$control_id" "$expected_status" negative-ppt "$output_slug"
      ;;
    executed:fan_mode)
      make_report "$complete" "$control_id" "$expected_status" negative-fan-mode "$output_slug"
      ;;
    executed:hardware_profile|executed:hardware_profile_trigger)
      make_report "$complete" "$control_id" "$expected_status" executed-profile "$output_slug"
      ;;
    executed:*)
      make_report "$complete" "$control_id" "$expected_status" executed-write "$output_slug"
      ;;
    *)
      echo "unknown verifier test requirement: $control_id=$expected_status" >&2
      exit 2
      ;;
  esac
done <"$requirements_file"
"$verifier" --root "$complete" >/tmp/ratvantage-verify-complete.txt

incomplete="$tmp/incomplete"
make_report "$incomplete" cpu_boost pass missing-revert cpu_boost
if "$verifier" --root "$incomplete" >/tmp/ratvantage-verify-incomplete.txt 2>/tmp/ratvantage-verify-incomplete.err; then
  echo "expected incomplete evidence verifier run to fail" >&2
  exit 1
fi

unrelated_skipped_bundle="$tmp/unrelated-skipped-bundle"
mkdir -p "$unrelated_skipped_bundle/82wm-live-platform_profile"
cat >"$unrelated_skipped_bundle/gpu-mode-only.tsv" <<'EOF'
gpu_mode	executed	false	--enable-gpu-mode-write	gpu_mode	EnvyControl GPU mode execution, one-way/recovery evidence
EOF
python3 - "$unrelated_skipped_bundle/82wm-live-platform_profile/validation-report.json" <<'PY'
import json
import pathlib
import sys

path = pathlib.Path(sys.argv[1])
report = {
    "schema_version": 1,
    "metadata": {
        "mode": "execute",
        "execute_only": "platform_profile",
        "target_bus_mode": "system",
        "sysfs_root": "/",
    },
    "controls": [
        {
            "control_id": "gpu_mode",
            "status": "skipped",
            "available": False,
            "requested": None,
        }
    ],
}
path.write_text(json.dumps(report, indent=2) + "\n")
PY
if "$verifier" --root "$unrelated_skipped_bundle" --requirements "$unrelated_skipped_bundle/gpu-mode-only.tsv" >/tmp/ratvantage-verify-unrelated-skipped.txt 2>/tmp/ratvantage-verify-unrelated-skipped.err; then
  echo "expected unrelated skipped bundle verifier run to fail" >&2
  exit 1
fi
if ! grep -q $'gpu_mode\texecuted\tMISSING' /tmp/ratvantage-verify-unrelated-skipped.txt; then
  echo "expected unrelated skipped bundle to leave gpu_mode marked MISSING" >&2
  exit 1
fi
if grep -q "82wm-live-platform_profile" /tmp/ratvantage-verify-unrelated-skipped.txt; then
  echo "expected unrelated platform_profile bundle not to be reported as gpu_mode evidence" >&2
  exit 1
fi

wrong_execute_only="$tmp/wrong-execute-only"
make_report "$wrong_execute_only" cpu_boost pass pass cpu_boost
python3 - "$wrong_execute_only/82wm-live-cpu_boost/validation-report.json" <<'PY'
import json
import pathlib
import sys

path = pathlib.Path(sys.argv[1])
report = json.loads(path.read_text())
report["metadata"]["execute_only"] = "conservation_mode"
path.write_text(json.dumps(report, indent=2) + "\n")
PY
if "$verifier" --root "$wrong_execute_only" >/tmp/ratvantage-verify-wrong-execute-only.txt 2>/tmp/ratvantage-verify-wrong-execute-only.err; then
  echo "expected wrong execute_only verifier run to fail" >&2
  exit 1
fi

wrong_plan_method="$tmp/wrong-plan-method"
cp -R "$complete" "$wrong_plan_method"
python3 - "$wrong_plan_method/82wm-live-cpu_boost/validation-report.json" <<'PY'
import json
import pathlib
import sys

path = pathlib.Path(sys.argv[1])
report = json.loads(path.read_text())
report["controls"][0]["plan"]["method"] = "Synthetic"
path.write_text(json.dumps(report, indent=2) + "\n")
PY
if "$verifier" --root "$wrong_plan_method" >/tmp/ratvantage-verify-wrong-plan-method.txt 2>/tmp/ratvantage-verify-wrong-plan-method.err; then
  echo "expected wrong plan method verifier run to fail" >&2
  exit 1
fi
if ! grep -q "plan method is 'Synthetic', expected 'SetCpuBoost'" /tmp/ratvantage-verify-wrong-plan-method.txt; then
  echo "expected wrong plan method verifier output to explain the expected CPU boost method" >&2
  exit 1
fi

wrong_plan_polkit="$tmp/wrong-plan-polkit"
cp -R "$complete" "$wrong_plan_polkit"
python3 - "$wrong_plan_polkit/82wm-live-cpu_boost/validation-report.json" <<'PY'
import json
import pathlib
import sys

path = pathlib.Path(sys.argv[1])
report = json.loads(path.read_text())
report["controls"][0]["plan"]["polkit_action"] = "org.ratvantage.LegionControl1.synthetic"
path.write_text(json.dumps(report, indent=2) + "\n")
PY
if "$verifier" --root "$wrong_plan_polkit" >/tmp/ratvantage-verify-wrong-plan-polkit.txt 2>/tmp/ratvantage-verify-wrong-plan-polkit.err; then
  echo "expected wrong plan polkit verifier run to fail" >&2
  exit 1
fi
if ! grep -q "plan polkit_action is 'org.ratvantage.LegionControl1.synthetic', expected 'org.ratvantage.LegionControl1.set-cpu-boost'" /tmp/ratvantage-verify-wrong-plan-polkit.txt; then
  echo "expected wrong plan polkit verifier output to explain the expected CPU boost action" >&2
  exit 1
fi

wrong_plan_readback="$tmp/wrong-plan-readback"
cp -R "$complete" "$wrong_plan_readback"
python3 - "$wrong_plan_readback/82wm-live-curve_optimizer_all_core/validation-report.json" <<'PY'
import json
import pathlib
import sys

path = pathlib.Path(sys.argv[1])
report = json.loads(path.read_text())
report["controls"][0]["plan"]["readback_required"] = True
path.write_text(json.dumps(report, indent=2) + "\n")
PY
if "$verifier" --root "$wrong_plan_readback" >/tmp/ratvantage-verify-wrong-plan-readback.txt 2>/tmp/ratvantage-verify-wrong-plan-readback.err; then
  echo "expected wrong plan readback verifier run to fail" >&2
  exit 1
fi
if ! grep -q "plan readback_required is True, expected False" /tmp/ratvantage-verify-wrong-plan-readback.txt; then
  echo "expected wrong plan readback verifier output to explain Curve Optimizer write-only status" >&2
  exit 1
fi

wrong_ppt_negative="$tmp/wrong-ppt-negative"
cp -R "$complete" "$wrong_ppt_negative"
python3 - "$wrong_ppt_negative/82wm-live-ppt_pl1_spl/validation-report.json" <<'PY'
import json
import pathlib
import sys

path = pathlib.Path(sys.argv[1])
report = json.loads(path.read_text())
report["controls"][0]["set_result"]["message"] = "synthetic firmware failure"
path.write_text(json.dumps(report, indent=2) + "\n")
PY
if "$verifier" --root "$wrong_ppt_negative" >/tmp/ratvantage-verify-wrong-ppt-negative.txt 2>/tmp/ratvantage-verify-wrong-ppt-negative.err; then
  echo "expected wrong PPT negative evidence verifier run to fail" >&2
  exit 1
fi
if ! grep -q "negative PPT apply result does not show firmware EBUSY" /tmp/ratvantage-verify-wrong-ppt-negative.txt; then
  echo "expected wrong PPT negative verifier output to explain missing EBUSY signature" >&2
  exit 1
fi

wrong_fan_negative="$tmp/wrong-fan-negative"
cp -R "$complete" "$wrong_fan_negative"
python3 - "$wrong_fan_negative/82wm-live-fan_mode/validation-report.json" <<'PY'
import json
import pathlib
import sys

path = pathlib.Path(sys.argv[1])
report = json.loads(path.read_text())
report["controls"][0]["set_result"]["readback_value"] = "1"
path.write_text(json.dumps(report, indent=2) + "\n")
PY
if "$verifier" --root "$wrong_fan_negative" >/tmp/ratvantage-verify-wrong-fan-negative.txt 2>/tmp/ratvantage-verify-wrong-fan-negative.err; then
  echo "expected wrong fan-mode negative evidence verifier run to fail" >&2
  exit 1
fi
if ! grep -q "negative fan-mode readback value does not match the original current value" /tmp/ratvantage-verify-wrong-fan-negative.txt; then
  echo "expected wrong fan-mode negative verifier output to explain readback mismatch evidence" >&2
  exit 1
fi

missing_profile_plan_method="$tmp/missing-profile-plan-method"
cp -R "$complete" "$missing_profile_plan_method"
python3 - "$missing_profile_plan_method/82wm-live-hardware_profile/validation-report.json" <<'PY'
import json
import pathlib
import sys

path = pathlib.Path(sys.argv[1])
report = json.loads(path.read_text())
report["controls"][0]["plan"]["plans"] = [{"method": "SetCpuBoost"}]
path.write_text(json.dumps(report, indent=2) + "\n")
PY
if "$verifier" --root "$missing_profile_plan_method" >/tmp/ratvantage-verify-missing-profile-plan-method.txt 2>/tmp/ratvantage-verify-missing-profile-plan-method.err; then
  echo "expected missing profile plan method verifier run to fail" >&2
  exit 1
fi
if ! grep -q "plan payload is missing expected method 'SetCpuGovernor'" /tmp/ratvantage-verify-missing-profile-plan-method.txt; then
  echo "expected missing profile plan method verifier output to explain missing CPU governor plan" >&2
  exit 1
fi

missing_profile_plan_polkit="$tmp/missing-profile-plan-polkit"
cp -R "$complete" "$missing_profile_plan_polkit"
python3 - "$missing_profile_plan_polkit/82wm-live-hardware_profile/validation-report.json" <<'PY'
import json
import pathlib
import sys

path = pathlib.Path(sys.argv[1])
report = json.loads(path.read_text())
report["controls"][0]["plan"]["plans"] = [
    {
        "method": "SetCpuGovernor",
        "polkit_action": "org.ratvantage.LegionControl1.set-cpu-governor",
    },
    {
        "method": "SetCpuEpp",
        "polkit_action": "org.ratvantage.LegionControl1.set-cpu-epp",
    },
    {"method": "SetCpuBoost"},
]
path.write_text(json.dumps(report, indent=2) + "\n")
PY
if "$verifier" --root "$missing_profile_plan_polkit" >/tmp/ratvantage-verify-missing-profile-plan-polkit.txt 2>/tmp/ratvantage-verify-missing-profile-plan-polkit.err; then
  echo "expected missing profile plan polkit verifier run to fail" >&2
  exit 1
fi
if ! grep -q "plan payload is missing expected polkit action 'org.ratvantage.LegionControl1.set-cpu-boost'" /tmp/ratvantage-verify-missing-profile-plan-polkit.txt; then
  echo "expected missing profile plan polkit verifier output to explain missing CPU boost action" >&2
  exit 1
fi

wrong_bundle_slug="$tmp/wrong-bundle-slug"
cp -R "$complete" "$wrong_bundle_slug"
mv "$wrong_bundle_slug/82wm-live-cpu_boost" "$wrong_bundle_slug/cpu_boost"
if "$verifier" --root "$wrong_bundle_slug" >/tmp/ratvantage-verify-wrong-bundle-slug.txt 2>/tmp/ratvantage-verify-wrong-bundle-slug.err; then
  echo "expected wrong bundle slug verifier run to fail" >&2
  exit 1
fi
if ! grep -q "bundle directory is 'cpu_boost', expected '82wm-live-cpu_boost'" /tmp/ratvantage-verify-wrong-bundle-slug.txt; then
  echo "expected wrong bundle slug verifier output to explain the directory mismatch" >&2
  exit 1
fi

missing_operator_checklist="$tmp/missing-operator-checklist"
cp -R "$complete" "$missing_operator_checklist"
rm "$missing_operator_checklist/82wm-live-curve_optimizer_all_core/operator-checklist.md"
if "$verifier" --root "$missing_operator_checklist" >/tmp/ratvantage-verify-missing-operator-checklist.txt 2>/tmp/ratvantage-verify-missing-operator-checklist.err; then
  echo "expected missing Curve Optimizer operator checklist verifier run to fail" >&2
  exit 1
fi
if ! grep -q "operator stability checklist is missing" /tmp/ratvantage-verify-missing-operator-checklist.txt; then
  echo "expected missing operator checklist verifier output to explain the missing checklist" >&2
  exit 1
fi

missing_curve_state="$tmp/missing-curve-state"
cp -R "$complete" "$missing_curve_state"
python3 - "$missing_curve_state/82wm-live-curve_optimizer_all_core/validation-report.json" <<'PY'
import json
import pathlib
import sys

path = pathlib.Path(sys.argv[1])
report = json.loads(path.read_text())
control = report["controls"][0]
control.pop("curve_state_after_apply_file", None)
control.pop("curve_state_after_apply", None)
control.pop("curve_state_after_revert_file", None)
control.pop("curve_state_after_revert", None)
path.write_text(json.dumps(report, indent=2) + "\n")
PY
if "$verifier" --root "$missing_curve_state" >/tmp/ratvantage-verify-missing-curve-state.txt 2>/tmp/ratvantage-verify-missing-curve-state.err; then
  echo "expected missing Curve Optimizer state verifier run to fail" >&2
  exit 1
fi
if ! grep -q "Curve Optimizer last-write state after apply is missing" /tmp/ratvantage-verify-missing-curve-state.txt; then
  echo "expected missing Curve Optimizer state verifier output to explain the missing apply state" >&2
  exit 1
fi
if ! grep -q "Curve Optimizer last-write state after reset is missing" /tmp/ratvantage-verify-missing-curve-state.txt; then
  echo "expected missing Curve Optimizer state verifier output to explain the missing reset state" >&2
  exit 1
fi

missing_fan_mode_checklist="$tmp/missing-fan-mode-checklist"
cp -R "$complete" "$missing_fan_mode_checklist"
rm "$missing_fan_mode_checklist/82wm-live-fan_mode/operator-checklist.md"
if "$verifier" --root "$missing_fan_mode_checklist" >/tmp/ratvantage-verify-missing-fan-mode-checklist.txt 2>/tmp/ratvantage-verify-missing-fan-mode-checklist.err; then
  echo "expected missing fan-mode operator checklist verifier run to fail" >&2
  exit 1
fi
if ! grep -q "operator fan-mode checklist is missing" /tmp/ratvantage-verify-missing-fan-mode-checklist.txt; then
  echo "expected missing fan-mode checklist verifier output to explain the missing checklist" >&2
  exit 1
fi

missing_gpu_mode_checklist="$tmp/missing-gpu-mode-checklist"
cp -R "$complete" "$missing_gpu_mode_checklist"
rm "$missing_gpu_mode_checklist/82wm-live-gpu_mode/operator-checklist.md"
if "$verifier" --root "$missing_gpu_mode_checklist" >/tmp/ratvantage-verify-missing-gpu-mode-checklist.txt 2>/tmp/ratvantage-verify-missing-gpu-mode-checklist.err; then
  echo "expected missing GPU-mode operator checklist verifier run to fail" >&2
  exit 1
fi
if ! grep -q "operator GPU-mode recovery checklist is missing" /tmp/ratvantage-verify-missing-gpu-mode-checklist.txt; then
  echo "expected missing GPU-mode checklist verifier output to explain the missing checklist" >&2
  exit 1
fi

missing_profile_action="$tmp/missing-profile-action"
cp -R "$complete" "$missing_profile_action"
python3 - "$missing_profile_action/82wm-live-hardware_profile/validation-report.json" <<'PY'
import json
import pathlib
import sys

path = pathlib.Path(sys.argv[1])
report = json.loads(path.read_text())
result = report["controls"][0]["set_result"]
result["results"] = [{"action_id": "cpu_boost"}]
path.write_text(json.dumps(report, indent=2) + "\n")
PY
if "$verifier" --root "$missing_profile_action" >/tmp/ratvantage-verify-missing-profile-action.txt 2>/tmp/ratvantage-verify-missing-profile-action.err; then
  echo "expected missing profile action verifier run to fail" >&2
  exit 1
fi
if ! grep -q "profile apply run is missing 'cpu_governor' action result" /tmp/ratvantage-verify-missing-profile-action.txt; then
  echo "expected missing profile action verifier output to explain the missing CPU governor action" >&2
  exit 1
fi

missing_trigger_profile_action="$tmp/missing-trigger-profile-action"
cp -R "$complete" "$missing_trigger_profile_action"
python3 - "$missing_trigger_profile_action/82wm-live-hardware_profile_trigger/validation-report.json" <<'PY'
import json
import pathlib
import sys

path = pathlib.Path(sys.argv[1])
report = json.loads(path.read_text())
result = report["controls"][0]["set_result"]
result["results"] = [{"action_id": "cpu_boost"}]
path.write_text(json.dumps(report, indent=2) + "\n")
PY
if "$verifier" --root "$missing_trigger_profile_action" >/tmp/ratvantage-verify-missing-trigger-profile-action.txt 2>/tmp/ratvantage-verify-missing-trigger-profile-action.err; then
  echo "expected missing trigger profile action verifier run to fail" >&2
  exit 1
fi
if ! grep -q "profile apply run is missing 'cpu_governor' action result" /tmp/ratvantage-verify-missing-trigger-profile-action.txt; then
  echo "expected missing trigger profile action verifier output to explain the missing CPU governor action" >&2
  exit 1
fi

non_live_metadata="$tmp/non-live-metadata"
cp -R "$complete" "$non_live_metadata"
python3 - "$non_live_metadata/82wm-live-cpu_boost/validation-report.json" <<'PY'
import json
import pathlib
import sys

path = pathlib.Path(sys.argv[1])
report = json.loads(path.read_text())
report["metadata"]["target_bus_mode"] = "private-session"
report["metadata"]["sysfs_root"] = "tests/fixtures/sysfs-82wm-confirmed"
path.write_text(json.dumps(report, indent=2) + "\n")
PY
if "$verifier" --root "$non_live_metadata" >/tmp/ratvantage-verify-non-live-metadata.txt 2>/tmp/ratvantage-verify-non-live-metadata.err; then
  echo "expected non-live metadata verifier run to fail" >&2
  exit 1
fi
if ! grep -q "target_bus_mode is 'private-session'" /tmp/ratvantage-verify-non-live-metadata.txt; then
  echo "expected non-live metadata verifier output to explain the private-session bus" >&2
  exit 1
fi
if ! grep -q "sysfs_root is 'tests/fixtures/sysfs-82wm-confirmed'" /tmp/ratvantage-verify-non-live-metadata.txt; then
  echo "expected non-live metadata verifier output to explain the fixture sysfs root" >&2
  exit 1
fi

echo "verify-82wm-live-evidence tests passed"
