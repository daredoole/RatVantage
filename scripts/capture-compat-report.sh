#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'EOF'
Usage: scripts/capture-compat-report.sh --output <bundle-dir> [--sysfs-root <root>]

Capture a read-only RatVantage compatibility bundle from a Legion machine.

The bundle contains:
- fixture/                narrow read-only sysfs snapshot
- probe.json             legion-probe JSON against the captured fixture
- compat-report.json     machine and capability summary for review
- compat-report.md       contributor-facing markdown summary
- pull-request-body.md   ready-to-paste PR body

This wrapper never writes to the source sysfs tree.
EOF
}

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
capture_script="$repo_root/scripts/capture-sysfs-fixture.sh"
output=""
sysfs_root="/"

while (($#)); do
  case "$1" in
    --sysfs-root)
      sysfs_root="${2:?missing value for --sysfs-root}"
      shift 2
      ;;
    --output)
      output="${2:?missing value for --output}"
      shift 2
      ;;
    --help|-h)
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

command -v cargo >/dev/null 2>&1 || {
  echo "missing cargo; run from a Rust/Cargo environment" >&2
  exit 1
}

command -v python3 >/dev/null 2>&1 || {
  echo "missing python3; install Python 3 to generate compatibility summaries" >&2
  exit 1
}

mkdir -p "$output"
fixture_dir="$output/fixture"
probe_json="$output/probe.json"
report_json="$output/compat-report.json"
report_md="$output/compat-report.md"
pr_body="$output/pull-request-body.md"

rm -rf "$fixture_dir"

"$capture_script" --sysfs-root "$sysfs_root" --output "$fixture_dir"

(
  cd "$repo_root"
  cargo run -q -p legion-probe -- --json --sysfs-root "$fixture_dir" >"$probe_json"
)

read_value() {
  local path="$1"
  if [[ -r "$path" ]]; then
    tr -d '\0' <"$path" | sed 's/[[:space:]]*$//'
  else
    printf 'unknown'
  fi
}

os_pretty_name="unknown"
if [[ -r /etc/os-release ]]; then
  os_pretty_name="$(
    . /etc/os-release
    printf '%s' "${PRETTY_NAME:-${NAME:-unknown}}"
  )"
fi

kernel_version="$(uname -r 2>/dev/null || printf 'unknown')"
architecture="$(uname -m 2>/dev/null || printf 'unknown')"
desktop_session="${XDG_CURRENT_DESKTOP:-${DESKTOP_SESSION:-unknown}}"
session_type="${XDG_SESSION_TYPE:-unknown}"
host_name="$(hostname 2>/dev/null || printf 'unknown')"
sys_vendor="$(read_value "$fixture_dir/sys/class/dmi/id/sys_vendor")"
product_name="$(read_value "$fixture_dir/sys/class/dmi/id/product_name")"
product_version="$(read_value "$fixture_dir/sys/class/dmi/id/product_version")"
product_sku="$(read_value "$fixture_dir/sys/class/dmi/id/product_sku")"
manifest_path="$fixture_dir/fixture-manifest.txt"

python3 - "$probe_json" "$manifest_path" "$report_json" "$report_md" "$pr_body" \
  "$host_name" "$os_pretty_name" "$kernel_version" "$architecture" \
  "$desktop_session" "$session_type" "$sys_vendor" "$product_name" \
  "$product_version" "$product_sku" <<'PY'
import json
import pathlib
import sys

(
    probe_json_path,
    manifest_path,
    report_json_path,
    report_md_path,
    pr_body_path,
    host_name,
    os_pretty_name,
    kernel_version,
    architecture,
    desktop_session,
    session_type,
    sys_vendor,
    product_name,
    product_version,
    product_sku,
) = sys.argv[1:]


def clean_text(value: str) -> str:
    value = (value or "").strip()
    return value if value else "unknown"


def clean_list(values):
    cleaned = []
    seen = set()
    for value in values:
        text = clean_text(str(value))
        if text == "unknown":
            continue
        if text in seen:
            continue
        seen.add(text)
        cleaned.append(text)
    return cleaned


probe = json.loads(pathlib.Path(probe_json_path).read_text())
manifest_lines = pathlib.Path(manifest_path).read_text().splitlines()
captured_files = [line for line in manifest_lines if line.startswith("captured:")]
skipped_files = [line for line in manifest_lines if line.startswith("skipped unreadable:")]

capabilities = probe.get("capabilities", [])
status_counts = {}
available_capabilities = []
missing_capabilities = []

for capability in capabilities:
    status = clean_text(capability.get("status", "unknown"))
    status_counts[status] = status_counts.get(status, 0) + 1
    entry = {
        "id": clean_text(capability.get("id", "unknown")),
        "label": clean_text(capability.get("label", "unknown")),
        "status": status,
        "risk": clean_text(capability.get("risk", "unknown")),
    }
    if status == "missing":
        missing_capabilities.append(entry)
    else:
        available_capabilities.append(entry)

platform_profile = probe.get("platform_profile") or {}
battery_charge_type = probe.get("battery_charge_type") or {}
gpu = probe.get("gpu") or {}

summary = {
    "capture_schema_version": 1,
    "host": {
        "host_name": clean_text(host_name),
        "os_pretty_name": clean_text(os_pretty_name),
        "kernel_version": clean_text(kernel_version),
        "architecture": clean_text(architecture),
        "desktop_session": clean_text(desktop_session),
        "session_type": clean_text(session_type),
    },
    "hardware": {
        "sys_vendor": clean_text(sys_vendor),
        "product_name": clean_text(product_name),
        "product_version": clean_text(product_version),
        "product_sku": clean_text(product_sku),
    },
    "capture": {
        "captured_file_count": len(captured_files),
        "skipped_file_count": len(skipped_files),
    },
    "probe_summary": {
        "capability_status_counts": status_counts,
        "available_capabilities": available_capabilities,
        "missing_capabilities": missing_capabilities,
        "platform_profile_choices": clean_list(platform_profile.get("choices", [])),
        "battery_charge_type_choices": clean_list(
            battery_charge_type.get("choices", [])
        ),
        "fan_curve_ids": clean_list(
            curve.get("id", "unknown") for curve in probe.get("fan_curves", [])
        ),
        "sensor_labels": clean_list(
            sensor.get("label") or sensor.get("kind", "unknown")
            for sensor in probe.get("telemetry", {}).get("sensors", [])
        ),
        "led_names": clean_list(
            led.get("name", "unknown") for led in probe.get("leds", [])
        ),
        "firmware_attribute_names": clean_list(
            attr.get("name", "unknown")
            for attr in probe.get("firmware_attributes", [])
        ),
        "ideapad_toggle_names": clean_list(
            toggle.get("name", "unknown") for toggle in probe.get("ideapad_toggles", [])
        ),
        "gpu": {
            "provider": clean_text(gpu.get("provider", "unknown")),
            "status": clean_text(gpu.get("status", "unknown")),
            "mode": clean_text(gpu.get("mode", "unknown")),
        },
    },
}

pathlib.Path(report_json_path).write_text(json.dumps(summary, indent=2) + "\n")

available_ids = ", ".join(item["id"] for item in available_capabilities) or "none"
missing_ids = ", ".join(item["id"] for item in missing_capabilities) or "none"
platform_choices = ", ".join(summary["probe_summary"]["platform_profile_choices"]) or "none"
battery_choices = (
    ", ".join(summary["probe_summary"]["battery_charge_type_choices"]) or "none"
)
fan_curve_ids = ", ".join(summary["probe_summary"]["fan_curve_ids"]) or "none"
led_names = ", ".join(summary["probe_summary"]["led_names"]) or "none"
firmware_names = ", ".join(summary["probe_summary"]["firmware_attribute_names"]) or "none"
toggle_names = ", ".join(summary["probe_summary"]["ideapad_toggle_names"]) or "none"

report_lines = [
    "# RatVantage Compatibility Report",
    "",
    "## Machine",
    f"- Vendor: {summary['hardware']['sys_vendor']}",
    f"- Product: {summary['hardware']['product_name']}",
    f"- Version: {summary['hardware']['product_version']}",
    f"- SKU: {summary['hardware']['product_sku']}",
    "",
    "## Host environment",
    f"- Host name: {summary['host']['host_name']}",
    f"- OS: {summary['host']['os_pretty_name']}",
    f"- Kernel: {summary['host']['kernel_version']}",
    f"- Architecture: {summary['host']['architecture']}",
    f"- Desktop: {summary['host']['desktop_session']}",
    f"- Session type: {summary['host']['session_type']}",
    "",
    "## Probe summary",
    f"- Captured files: {summary['capture']['captured_file_count']}",
    f"- Skipped unreadable files: {summary['capture']['skipped_file_count']}",
    f"- Available capabilities: {available_ids}",
    f"- Missing capabilities: {missing_ids}",
    f"- Platform profile choices: {platform_choices}",
    f"- Battery charge type choices: {battery_choices}",
    f"- GPU provider/status/mode: {summary['probe_summary']['gpu']['provider']} / {summary['probe_summary']['gpu']['status']} / {summary['probe_summary']['gpu']['mode']}",
    f"- Fan curve IDs: {fan_curve_ids}",
    f"- LED names: {led_names}",
    f"- Firmware attributes: {firmware_names}",
    f"- Ideapad toggles: {toggle_names}",
    "",
    "## Bundle files",
    "- `fixture/` narrow read-only sysfs snapshot",
    "- `probe.json` full `legion-probe` output against the captured fixture",
    "- `compat-report.json` structured reviewer summary",
    "- `pull-request-body.md` ready-to-paste PR text",
    "",
    "## Reviewer notes",
    "- Review `fixture/fixture-manifest.txt` before merging.",
    "- Remove serial numbers or user-identifying values if any appear.",
    "- Keep the submission read-only; do not add write-only sysfs paths.",
]
pathlib.Path(report_md_path).write_text("\n".join(report_lines) + "\n")

pr_lines = [
    "## Hardware compatibility submission",
    "",
    f"- Machine: {summary['hardware']['sys_vendor']} {summary['hardware']['product_name']}",
    f"- Product version: {summary['hardware']['product_version']}",
    f"- Product SKU: {summary['hardware']['product_sku']}",
    f"- OS: {summary['host']['os_pretty_name']}",
    f"- Kernel: {summary['host']['kernel_version']}",
    f"- Desktop/session: {summary['host']['desktop_session']} / {summary['host']['session_type']}",
    "",
    "## Included artifacts",
    "",
    "- [x] `fixture/` from `scripts/capture-compat-report.sh`",
    "- [x] `probe.json`",
    "- [x] `compat-report.json`",
    "- [x] `compat-report.md`",
    "",
    "## Capability summary",
    "",
    f"- Available capabilities: {available_ids}",
    f"- Missing capabilities: {missing_ids}",
    f"- Platform profile choices: {platform_choices}",
    f"- Battery charge type choices: {battery_choices}",
    f"- GPU provider/status/mode: {summary['probe_summary']['gpu']['provider']} / {summary['probe_summary']['gpu']['status']} / {summary['probe_summary']['gpu']['mode']}",
    "",
    "## Safety checklist",
    "",
    "- [ ] I reviewed `fixture/fixture-manifest.txt`.",
    "- [ ] I removed serial numbers or user-identifying values if any appeared.",
    "- [ ] This submission contains only read-only probe inputs under `sys/`.",
    "- [ ] I ran `./scripts/ci-local.sh` after adding or updating the fixture locally.",
]
pathlib.Path(pr_body_path).write_text("\n".join(pr_lines) + "\n")
PY

echo "compatibility bundle written to $output"
echo "  fixture: $fixture_dir"
echo "  probe: $probe_json"
echo "  summary: $report_json"
echo "  report: $report_md"
echo "  pr body: $pr_body"
