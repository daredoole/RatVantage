#!/usr/bin/env bash
set -euo pipefail

need_pkg_config() {
  local package="$1"
  local installer="$2"

  if ! pkg-config --exists "$package"; then
    echo "missing pkg-config package: $package" >&2
    echo "run: $installer" >&2
    exit 1
  fi
}

command -v dbus-daemon >/dev/null 2>&1 || {
  echo "missing dbus-daemon; run: scripts/install-dev-deps-fedora.sh" >&2
  exit 1
}

command -v xvfb-run >/dev/null 2>&1 || {
  echo "missing xvfb-run; run: scripts/install-dev-deps-fedora.sh" >&2
  exit 1
}

command -v import >/dev/null 2>&1 || {
  echo "missing import; run: scripts/install-dev-deps-fedora.sh" >&2
  exit 1
}

command -v desktop-file-validate >/dev/null 2>&1 || {
  echo "missing desktop-file-validate; run: scripts/install-dev-deps-fedora.sh" >&2
  exit 1
}

command -v appstreamcli >/dev/null 2>&1 || {
  echo "missing appstreamcli; run: scripts/install-dev-deps-fedora.sh" >&2
  exit 1
}

rust_minor="$(rustc --version | awk '{print $2}' | cut -d. -f2)"
if (( rust_minor < 92 )); then
  echo "rustc 1.92+ required for gtk-rs; current: $(rustc --version)" >&2
  echo "run: rustup toolchain install stable" >&2
  exit 1
fi

need_pkg_config gtk4 "scripts/install-dev-deps-fedora.sh"
need_pkg_config libadwaita-1 "scripts/install-dev-deps-fedora.sh"

cargo fmt --all --check
cargo test --workspace
xvfb-run -a cargo test -p legion-control-ui --features gtk-ui --test gtk_shell
cargo clippy --all-targets --all-features -- -D warnings
scripts/validate-packaging.sh
fixture_tmp="$(mktemp -d)"
trap 'rm -rf "$fixture_tmp"' EXIT
scripts/capture-sysfs-fixture.sh \
  --sysfs-root tests/fixtures/sysfs-82wm-confirmed \
  --output "$fixture_tmp/captured" >/tmp/ratvantage-fixture-capture.txt
scripts/capture-compat-report.sh \
  --sysfs-root tests/fixtures/sysfs-82wm-confirmed \
  --output "$fixture_tmp/compat" >/tmp/ratvantage-compat-capture.txt
scripts/capture-write-validation-report.sh \
  --sysfs-root tests/fixtures/sysfs-82wm-confirmed \
  --output "$fixture_tmp/write-validation" \
  --skip-compat-bundle \
  --skip-tray-smoke >/tmp/ratvantage-write-validation.txt
python3 - "$fixture_tmp/write-validation/validation-report.json" <<'PY'
import json
import pathlib
import sys

report = json.loads(pathlib.Path(sys.argv[1]).read_text())
controls = {control.get("control_id"): control for control in report.get("controls", [])}
required_planned = [
    "fan_mode",
    "conservation_mode",
    "cpu_governor",
    "cpu_epp",
    "cpu_boost",
    "firmware_attribute:ppt_pl1_spl",
    "firmware_attribute:ppt_pl2_sppt",
    "firmware_attribute:ppt_pl3_fppt",
    "amd_gpu_dpm_force_level",
    "curve_optimizer_all_core",
]
missing = [control_id for control_id in required_planned if control_id not in controls]
if missing:
    raise SystemExit(f"write-validation fixture report is missing controls: {', '.join(missing)}")
bad = [
    control_id
    for control_id in required_planned
    if controls[control_id].get("status") != "planned"
    or controls[control_id].get("plan_exit") != 0
    or not controls[control_id].get("plan")
]
if bad:
    raise SystemExit(f"write-validation fixture report did not plan controls cleanly: {', '.join(bad)}")
PY
scripts/capture-write-validation-report.sh \
  --sysfs-root tests/fixtures/sysfs-82wm-confirmed \
  --output "$fixture_tmp/write-validation-profile" \
  --skip-compat-bundle \
  --skip-tray-smoke \
  --seed-hardware-profile 'validation_cpu_driver={"schema_version":1,"label":"Validation CPU driver behavior","actions":{"cpu_governor":"powersave","cpu_epp":"balance_performance","cpu_boost":"1"}}' \
  --seed-hardware-profile-trigger 'manual=validation_cpu_driver' >/tmp/ratvantage-write-validation-profile.txt
python3 - "$fixture_tmp/write-validation-profile/validation-report.json" <<'PY'
import json
import pathlib
import sys

report = json.loads(pathlib.Path(sys.argv[1]).read_text())
metadata = report.get("metadata") or {}
if metadata.get("seed_hardware_profile_count") != 1:
    raise SystemExit("seeded write-validation report did not record one hardware profile seed")
if metadata.get("seed_hardware_profile_trigger_count") != 1:
    raise SystemExit("seeded write-validation report did not record one hardware profile trigger seed")
controls = {control.get("control_id"): control for control in report.get("controls", [])}
for control_id in ("hardware_profile", "hardware_profile_trigger"):
    control = controls.get(control_id)
    if not control:
        raise SystemExit(f"seeded write-validation report is missing {control_id}")
    if control.get("status") != "planned" or control.get("plan_exit") != 0:
        raise SystemExit(f"seeded write-validation report did not plan {control_id} cleanly")
    methods = [
        plan.get("method")
        for plan in ((control.get("plan") or {}).get("plans") or [])
    ]
    if methods != ["SetCpuGovernor", "SetCpuEpp", "SetCpuBoost"]:
        raise SystemExit(f"{control_id} planned methods were {methods!r}")
PY
scripts/test-verify-82wm-live-evidence.sh >/tmp/ratvantage-verify-82wm-live-evidence.txt
scripts/test-review-write-validation-bundle.sh >/tmp/ratvantage-review-write-validation-bundle.txt
scripts/run-local-session-app.sh \
  --frontend status \
  --sysfs-root tests/fixtures/sysfs-82wm-confirmed >/tmp/ratvantage-local-session-status.txt
scripts/capture-gtk-smoke-report.sh \
  --sysfs-root tests/fixtures/sysfs-82wm-confirmed \
  --pages status,battery,gpu,fans \
  --output "$fixture_tmp/gtk-smoke" >/tmp/ratvantage-gtk-smoke.txt
cargo run -p legion-probe -- --json --sysfs-root "$fixture_tmp/captured" >/tmp/ratvantage-captured-probe.json
cargo run -p legion-probe -- --json --sysfs-root tests/fixtures/sysfs-82wm-confirmed >/tmp/ratvantage-probe.json
cargo run -p legion-control-daemon -- --dry-run >/tmp/ratvantage-daemon.txt

echo "local CI passed"
