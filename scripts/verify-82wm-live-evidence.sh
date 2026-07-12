#!/usr/bin/env bash
# Verify the complete 82WM live write-evidence set.

set -euo pipefail

usage() {
  cat <<'EOF'
Usage: scripts/verify-82wm-live-evidence.sh [options]

Scan validation-report.json files and fail unless the required 82WM live evidence
controls are present with the expected execute-mode status and supporting
apply/revert artifacts.

Options:
  --root <dir>       Directory containing validation bundles.
                     Default: target/validation
  --requirements <file>
                     TSV matrix with control_id and expected_status columns.
                     Default: data/validation/82wm-live-evidence-requirements.tsv
  --require <id=status>
                     Add or override a required control/status pair.
                     May be passed more than once.
  -h, --help         Show this help.

Default required controls are listed in the requirements TSV.
EOF
}

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
root="target/validation"
requirements_file="$repo_root/data/validation/82wm-live-evidence-requirements.tsv"
extra_requirements=()

while [[ $# -gt 0 ]]; do
  case "$1" in
    --root)
      root="${2:?missing value for --root}"
      shift 2
      ;;
    --requirements)
      requirements_file="${2:?missing value for --requirements}"
      shift 2
      ;;
    --require)
      extra_requirements+=("${2:?missing value for --require}")
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

if [[ ! -d "$root" ]]; then
  echo "validation root does not exist: $root" >&2
  exit 2
fi

command -v python3 >/dev/null 2>&1 || {
  echo "missing python3" >&2
  exit 1
}

if [[ ! -f "$requirements_file" ]]; then
  echo "requirements file does not exist: $requirements_file" >&2
  exit 2
fi

python3 - "$root" "$requirements_file" "${extra_requirements[@]}" <<'PY'
import json
import pathlib
import sys

root = pathlib.Path(sys.argv[1])
requirements_path = pathlib.Path(sys.argv[2])
requirements = {}

for line_number, line in enumerate(requirements_path.read_text().splitlines(), 1):
    if not line.strip() or line.startswith("#"):
        continue
    parts = line.split("\t", 5)
    if len(parts) < 5:
        print(
            f"invalid requirements row {requirements_path}:{line_number}; expected control_id, expected_status, rollback_required, daemon_flag, output_slug",
            file=sys.stderr,
        )
        sys.exit(2)
    control_id, status, rollback_required, daemon_flag, output_slug = parts[:5]
    description = parts[5] if len(parts) > 5 else ""
    if not control_id or not status:
        print(
            f"invalid requirements row {requirements_path}:{line_number}; missing control_id or expected_status",
            file=sys.stderr,
        )
        sys.exit(2)
    if rollback_required not in {"true", "false"}:
        print(
            f"invalid rollback_required for {control_id}: {rollback_required!r}; expected true or false",
            file=sys.stderr,
        )
        sys.exit(2)
    requirements[control_id] = {
        "expected_status": status,
        "rollback_required": rollback_required == "true",
        "daemon_flag": daemon_flag,
        "output_slug": output_slug,
        "description": description,
    }

for raw in sys.argv[3:]:
    if "=" not in raw:
        print(f"invalid requirement {raw!r}; expected control_id=status", file=sys.stderr)
        sys.exit(2)
    control_id, status = raw.split("=", 1)
    if not control_id or not status:
        print(f"invalid requirement {raw!r}; expected control_id=status", file=sys.stderr)
        sys.exit(2)
    requirements[control_id] = {
        "expected_status": status,
        "rollback_required": status == "pass",
        "daemon_flag": "extra command-line requirement",
        "output_slug": control_id.replace(":", "_"),
        "description": "extra command-line requirement",
    }

reports = []
for path in sorted(root.rglob("validation-report.json")):
    try:
        report = json.loads(path.read_text())
    except Exception as exc:
        reports.append({"path": path, "error": str(exc)})
        continue
    reports.append({"path": path, "report": report})

def expected_plan_methods(control_id):
    if control_id.startswith("firmware_attribute:"):
        return ["SetFirmwareAttribute"]
    return {
        "conservation_mode": ["SetConservationMode"],
        "cpu_governor": ["SetCpuGovernor"],
        "cpu_epp": ["SetCpuEpp"],
        "cpu_boost": ["SetCpuBoost"],
        "fan_mode": ["SetIdeapadToggle"],
        "amd_gpu_dpm_force_level": ["SetAmdGpuDpmForceLevel"],
        "keyboard_rgb": ["SetKeyboardRgb", "SetOpenRgbKeyboardRgbSdk"],
        "curve_optimizer_all_core": ["SetCurveOptimizerAllCore"],
        "gpu_mode": ["SetGpuMode"],
        "hardware_profile": [
            "SetCpuGovernor",
            "SetCpuEpp",
            "SetCpuMaxFrequency",
            "SetCpuBoost",
        ],
        "hardware_profile_trigger": [
            "SetCpuGovernor",
            "SetCpuEpp",
            "SetCpuMaxFrequency",
            "SetCpuBoost",
        ],
    }.get(control_id, [])

def expected_plan_polkit_actions(control_id):
    if control_id.startswith("firmware_attribute:"):
        return ["org.ratvantage.LegionControl1.set-firmware-attribute"]
    return {
        "conservation_mode": ["org.ratvantage.LegionControl1.set-conservation-mode"],
        "cpu_governor": ["org.ratvantage.LegionControl1.set-cpu-governor"],
        "cpu_epp": ["org.ratvantage.LegionControl1.set-cpu-epp"],
        "cpu_max_khz": ["org.ratvantage.LegionControl1.set-cpu-max-frequency"],
        "cpu_boost": ["org.ratvantage.LegionControl1.set-cpu-boost"],
        "fan_mode": ["org.ratvantage.LegionControl1.set-ideapad-toggle"],
        "amd_gpu_dpm_force_level": [
            "org.ratvantage.LegionControl1.set-amd-gpu-dpm-force-level"
        ],
        "wifi_power_save": ["org.ratvantage.LegionControl1.set-wifi-power-save"],
        "keyboard_rgb": ["org.ratvantage.LegionControl1.set-keyboard-rgb"],
        "curve_optimizer_all_core": [
            "org.ratvantage.LegionControl1.set-curve-optimizer"
        ],
        "gpu_mode": ["org.ratvantage.LegionControl1.set-gpu-mode"],
        "hardware_profile": [
            "org.ratvantage.LegionControl1.set-cpu-governor",
            "org.ratvantage.LegionControl1.set-cpu-epp",
            "org.ratvantage.LegionControl1.set-cpu-max-frequency",
            "org.ratvantage.LegionControl1.set-cpu-boost",
        ],
        "hardware_profile_trigger": [
            "org.ratvantage.LegionControl1.set-cpu-governor",
            "org.ratvantage.LegionControl1.set-cpu-epp",
            "org.ratvantage.LegionControl1.set-cpu-max-frequency",
            "org.ratvantage.LegionControl1.set-cpu-boost",
        ],
    }.get(control_id, [])

def expected_plan_readback_required(control_id):
    if control_id.startswith("firmware_attribute:"):
        return [True]
    return {
        "conservation_mode": [True],
        "cpu_governor": [True],
        "cpu_epp": [True],
        "cpu_boost": [True],
        "fan_mode": [True],
        "amd_gpu_dpm_force_level": [True],
        "keyboard_rgb": [True],
        "curve_optimizer_all_core": [False],
        "gpu_mode": [True],
        "hardware_profile": [True, True, True, True],
        "hardware_profile_trigger": [True, True, True, True],
    }.get(control_id, [])

def expected_negative_evidence(control_id):
    if control_id.startswith("firmware_attribute:ppt_"):
        return "firmware_ebusy"
    if control_id == "fan_mode":
        return "fan_mode_unchanged"
    return None

def evidence_checks(control_id, requirement, metadata, control, bundle_dir):
    problems = []
    expected = requirement["expected_status"]
    expected_bundle_name = f"82wm-live-{requirement['output_slug']}"
    target_bus_mode = metadata.get("target_bus_mode")
    if target_bus_mode not in {"system", "custom-address"}:
        problems.append(
            f"target_bus_mode is {target_bus_mode!r}, expected 'system' or 'custom-address'"
        )
    if metadata.get("sysfs_root") != "/":
        problems.append(
            f"sysfs_root is {metadata.get('sysfs_root')!r}, expected '/' for live evidence"
        )
    if bundle_dir.name != expected_bundle_name:
        problems.append(
            f"bundle directory is {bundle_dir.name!r}, expected {expected_bundle_name!r}"
        )
    if metadata.get("execute_only") != control_id:
        problems.append(
            f"execute_only is {metadata.get('execute_only')!r}, expected {control_id!r}"
        )
    if control.get("available") is not True:
        problems.append("control is not marked available")
    if control.get("plan_exit") != 0:
        problems.append(f"plan_exit is {control.get('plan_exit')!r}, expected 0")
    if not control.get("plan_file") or not control.get("plan"):
        problems.append("plan artifact/payload is missing")
    elif not isinstance(control.get("plan"), dict):
        problems.append("plan payload is not a JSON object")
    else:
        expected_methods = expected_plan_methods(control_id)
        if control_id in {"hardware_profile", "hardware_profile_trigger"}:
            planned_methods = [
                plan.get("method")
                for plan in control["plan"].get("plans") or []
                if isinstance(plan, dict)
            ]
            planned_polkit_actions = [
                plan.get("polkit_action")
                for plan in control["plan"].get("plans") or []
                if isinstance(plan, dict)
            ]
            planned_readback_required = [
                plan.get("readback_required")
                for plan in control["plan"].get("plans") or []
                if isinstance(plan, dict)
            ]
            for method in expected_methods:
                if method not in planned_methods:
                    problems.append(
                        f"plan payload is missing expected method {method!r}"
                    )
            for polkit_action in expected_plan_polkit_actions(control_id):
                if polkit_action not in planned_polkit_actions:
                    problems.append(
                        f"plan payload is missing expected polkit action {polkit_action!r}"
                    )
            for readback_required in expected_plan_readback_required(control_id):
                if readback_required not in planned_readback_required:
                    problems.append(
                        f"plan payload is missing readback_required={readback_required!r}"
                    )
        elif expected_methods:
            actual_method = control["plan"].get("method")
            if actual_method not in expected_methods:
                problems.append(
                    f"plan method is {actual_method!r}, expected one of {expected_methods!r}"
                )
            expected_polkit_actions = expected_plan_polkit_actions(control_id)
            if expected_polkit_actions:
                actual_polkit_action = control["plan"].get("polkit_action")
                if actual_polkit_action not in expected_polkit_actions:
                    problems.append(
                        f"plan polkit_action is {actual_polkit_action!r}, expected one of {expected_polkit_actions!r}"
                    )
            expected_readback_required = expected_plan_readback_required(control_id)
            if expected_readback_required:
                actual_readback_required = control["plan"].get("readback_required")
                if actual_readback_required not in expected_readback_required:
                    problems.append(
                        f"plan readback_required is {actual_readback_required!r}, expected one of {expected_readback_required!r}"
                    )
    if not control.get("set_file") or control.get("set_exit") != 0:
        problems.append("apply artifact is missing or apply command failed")
    if control_id == "curve_optimizer_all_core":
        operator_checklist = bundle_dir / "operator-checklist.md"
        if not operator_checklist.is_file():
            problems.append("operator stability checklist is missing")
        else:
            checklist_text = operator_checklist.read_text(errors="replace").lower()
            for term in ("curve_optimizer_all_core", "reset", "stability"):
                if term not in checklist_text:
                    problems.append(
                        f"operator stability checklist does not mention {term!r}"
                    )
    if control_id == "fan_mode":
        operator_checklist = bundle_dir / "operator-checklist.md"
        if not operator_checklist.is_file():
            problems.append("operator fan-mode checklist is missing")
        else:
            checklist_text = operator_checklist.read_text(errors="replace").lower()
            for term in ("fan_mode", "auto", "full speed", "thermal"):
                if term not in checklist_text:
                    problems.append(
                        f"operator fan-mode checklist does not mention {term!r}"
                    )
    if control_id == "gpu_mode":
        operator_checklist = bundle_dir / "operator-checklist.md"
        if not operator_checklist.is_file():
            problems.append("operator GPU-mode recovery checklist is missing")
        else:
            checklist_text = operator_checklist.read_text(errors="replace").lower()
            for term in ("gpu_mode", "envycontrol", "reboot", "recovery"):
                if term not in checklist_text:
                    problems.append(
                        f"operator GPU-mode recovery checklist does not mention {term!r}"
                    )

    set_result = control.get("set_result")
    revert_result = control.get("revert_result")
    if control_id == "curve_optimizer_all_core":
        try:
            requested_offset = int(str(control.get("requested")))
        except Exception:
            requested_offset = None
            problems.append(
                f"Curve Optimizer requested value is not an integer: {control.get('requested')!r}"
            )
        if isinstance(set_result, dict):
            readback_value = str(set_result.get("readback_value") or "")
            for term in ("encoded=", "readback=write_only"):
                if term not in readback_value:
                    problems.append(
                        f"Curve Optimizer apply readback_value does not mention {term!r}"
                    )
        apply_state = control.get("curve_state_after_apply")
        if not control.get("curve_state_after_apply_file") or not isinstance(
            apply_state, dict
        ):
            problems.append("Curve Optimizer last-write state after apply is missing")
        else:
            if requested_offset is not None and apply_state.get("signed_offset") != requested_offset:
                problems.append(
                    "Curve Optimizer last-write state after apply does not match requested offset"
                )
            if apply_state.get("backend") != "ryzenadj":
                problems.append(
                    f"Curve Optimizer apply backend is {apply_state.get('backend')!r}, expected 'ryzenadj'"
                )
            if apply_state.get("readback_status") != "write_only":
                problems.append(
                    "Curve Optimizer apply state is not marked write_only"
                )
            if not isinstance(apply_state.get("encoded_value"), int):
                problems.append("Curve Optimizer apply state is missing encoded_value")
        if isinstance(revert_result, dict):
            readback_value = str(revert_result.get("readback_value") or "")
            for term in ("offset=0", "readback=write_only"):
                if term not in readback_value:
                    problems.append(
                        f"Curve Optimizer reset readback_value does not mention {term!r}"
                    )
        revert_state = control.get("curve_state_after_revert")
        if not control.get("curve_state_after_revert_file") or not isinstance(
            revert_state, dict
        ):
            problems.append("Curve Optimizer last-write state after reset is missing")
        else:
            if revert_state.get("signed_offset") != 0:
                problems.append(
                    "Curve Optimizer last-write state after reset is not offset 0"
                )
            if revert_state.get("backend") != "ryzenadj":
                problems.append(
                    f"Curve Optimizer reset backend is {revert_state.get('backend')!r}, expected 'ryzenadj'"
                )
            if revert_state.get("readback_status") != "write_only":
                problems.append(
                    "Curve Optimizer reset state is not marked write_only"
                )
            if not isinstance(revert_state.get("encoded_value"), int):
                problems.append("Curve Optimizer reset state is missing encoded_value")
    negative_evidence = expected_negative_evidence(control_id)
    if expected == "executed" and negative_evidence == "firmware_ebusy":
        if not isinstance(set_result, dict):
            problems.append("negative PPT evidence is missing apply result")
        else:
            if set_result.get("applied") is not False:
                problems.append("negative PPT apply result did not report applied=false")
            message = str(set_result.get("message") or "")
            if "Device or resource busy" not in message and "os error 16" not in message:
                problems.append(
                    "negative PPT apply result does not show firmware EBUSY"
                )
        if not control.get("revert_file") or control.get("revert_exit") != 0:
            problems.append("negative PPT evidence is missing revert attempt artifact")
        if not isinstance(revert_result, dict):
            problems.append("negative PPT evidence is missing revert result")
        else:
            if revert_result.get("applied") is not False:
                problems.append("negative PPT revert result did not report applied=false")
            message = str(revert_result.get("message") or "")
            if "Device or resource busy" not in message and "os error 16" not in message:
                problems.append(
                    "negative PPT revert result does not show firmware EBUSY"
                )
    elif expected == "executed" and negative_evidence == "fan_mode_unchanged":
        if not isinstance(set_result, dict):
            problems.append("negative fan-mode evidence is missing apply result")
        else:
            if set_result.get("applied") is not False:
                problems.append("negative fan-mode apply result did not report applied=false")
            message = str(set_result.get("message") or "")
            if "read-back mismatch" not in message or "restored previous value" not in message:
                problems.append(
                    "negative fan-mode apply result does not show read-back mismatch and restore"
                )
            if str(set_result.get("readback_value") or "") != str(control.get("current") or ""):
                problems.append(
                    "negative fan-mode readback value does not match the original current value"
                )
        if not control.get("revert_file") or control.get("revert_exit") != 0:
            problems.append("negative fan-mode evidence is missing revert artifact")
        if not (isinstance(revert_result, dict) and revert_result.get("applied") is True):
            problems.append("negative fan-mode revert result is not applied=true")
    elif requirement["rollback_required"]:
        if not (isinstance(set_result, dict) and set_result.get("applied") is True):
            problems.append("apply result is not WriteExecutionResult(applied=true)")
        if not control.get("revert_file") or control.get("revert_exit") != 0:
            problems.append("revert artifact is missing or revert command failed")
        if not (isinstance(revert_result, dict) and revert_result.get("applied") is True):
            problems.append("revert result is not WriteExecutionResult(applied=true)")
        if expected != "pass":
            problems.append(f"rollback_required=true expects status pass, got {expected}")
    elif expected == "executed":
        if control.get("revert_file"):
            problems.append("one-way executed evidence unexpectedly has a revert artifact")
        if isinstance(set_result, dict) and "completed" in set_result:
            if set_result.get("completed") is not True:
                problems.append("profile apply run did not complete")
            results = set_result.get("results")
            if not results:
                problems.append("profile apply run has no per-action results")
            if control_id in {"hardware_profile", "hardware_profile_trigger"} and isinstance(
                results, list
            ):
                action_ids = {
                    result.get("action_id")
                    for result in results
                    if isinstance(result, dict)
                }
                for action_id in ("cpu_governor", "cpu_epp", "cpu_boost"):
                    if action_id not in action_ids:
                        problems.append(
                            f"profile apply run is missing {action_id!r} action result"
                        )
        elif isinstance(set_result, dict) and "applied" in set_result:
            if set_result.get("applied") is not True:
                problems.append("write execution result did not apply")
        else:
            problems.append("apply payload is not a recognized execution result")
    return problems


best = {}
for item in reports:
    report = item.get("report")
    if not report:
        continue
    metadata = report.get("metadata") or {}
    if metadata.get("mode") != "execute":
        continue
    for control in report.get("controls") or []:
        control_id = control.get("control_id")
        if control_id not in requirements:
            continue
        status = control.get("status")
        requirement = requirements[control_id]
        expected = requirement["expected_status"]
        expected_bundle_name = f"82wm-live-{requirement['output_slug']}"
        bundle_dir = item["path"].parent
        if (
            bundle_dir.name != expected_bundle_name
            and metadata.get("execute_only") != control_id
        ):
            continue
        problems = evidence_checks(control_id, requirement, metadata, control, item["path"].parent)
        valid = status == expected and not problems
        # Prefer fully valid evidence; otherwise keep the first matching-status row as a diagnostic.
        existing = best.get(control_id)
        if (
            existing is None
            or (not existing["valid"] and valid)
            or (existing["status"] != expected and status == expected)
        ):
            best[control_id] = {
                "status": status,
                "expected": expected,
                "valid": valid,
                "problems": problems,
                "path": str(item["path"].parent),
                "execute_only": metadata.get("execute_only"),
                "available": control.get("available"),
                "requested": control.get("requested"),
                "description": requirement["description"],
                "rollback_required": requirement["rollback_required"],
                "daemon_flag": requirement["daemon_flag"],
                "output_slug": requirement["output_slug"],
            }

print("control_id\texpected\tactual\trollback\tdaemon_flag\toutput_slug\tavailable\texecute_only\tbundle")
failed = False
for control_id, requirement in requirements.items():
    expected = requirement["expected_status"]
    row = best.get(control_id)
    if row is None:
        failed = True
        rollback = "true" if requirement["rollback_required"] else "false"
        print(f"{control_id}\t{expected}\tMISSING\t{rollback}\t{requirement['daemon_flag']}\t{requirement['output_slug']}\t\t\t")
        if requirement["description"]:
            print(f"  - {requirement['description']}")
        print(f"  - expected bundle: target/validation/82wm-live-{requirement['output_slug']}")
        continue
    actual = row["status"]
    if actual != expected or not row["valid"]:
        failed = True
    print(
        f"{control_id}\t{expected}\t{actual}\t{str(row['rollback_required']).lower()}\t{row['daemon_flag']}\t{row['output_slug']}\t{row.get('available')}"
        f"\t{row.get('execute_only') or ''}\t{row['path']}"
    )
    if row["description"]:
        print(f"  - {row['description']}")
    for problem in row["problems"]:
        print(f"  - {problem}")

if failed:
    print("", file=sys.stderr)
    print(
        "82WM live evidence is incomplete. Capture one execute bundle per missing/failing control, "
        "then rerun this verifier.",
        file=sys.stderr,
    )
    sys.exit(1)

print("")
print(f"82WM live evidence complete for {len(requirements)} controls.")
PY
