#!/usr/bin/env bash
# Capture GPU MUX and session-restart switching evidence for RatVantage.
#
# Run --phase pre before switching, then switch + restart SDDM, then run --phase post.
# Compares pre/post to determine if hardware MUX and session-restart switching work.
set -euo pipefail

usage() {
  cat <<'EOF'
Usage: scripts/capture-gpu-mux-evidence.sh --phase <pre|post|mux-only> --output <dir> [options]

Captures GPU MUX hardware evidence and session-restart switching state.

Phases:
  pre         Capture state before GPU mode switch. Run before envycontrol -s <mode>.
  post        Capture state after SDDM restart. Run after logging back in.
  mux-only    Read-only MUX hardware probe, no switching test. Safe to run anytime.
  compare     Compare pre/post bundles and report whether session-restart switching works.

Options:
  --output <dir>   Required output directory (created if absent).
  --pre-dir <dir>  Pre-phase bundle dir (for --phase compare). Default: <output>/pre
  -h, --help       Show this help.

Workflow:
  1. scripts/capture-gpu-mux-evidence.sh --phase pre --output target/validation/gpu-mux
  2. sudo envycontrol -s hybrid   # or -s nvidia, whichever is NOT your current mode
  3. sudo systemctl restart sddm  # WARNING: kills your session. Log back in.
  4. scripts/capture-gpu-mux-evidence.sh --phase post --output target/validation/gpu-mux
  5. scripts/capture-gpu-mux-evidence.sh --phase compare --output target/validation/gpu-mux

This script is READ-ONLY except for writing the output directory.
It does not write sysfs, WMI, EC, or any hardware path.
EOF
}

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
phase=""
output=""
pre_dir=""

while [[ $# -gt 0 ]]; do
  case "$1" in
    --phase) phase="$2"; shift 2 ;;
    --output) output="$2"; shift 2 ;;
    --pre-dir) pre_dir="$2"; shift 2 ;;
    -h|--help) usage; exit 0 ;;
    *) echo "Unknown option: $1" >&2; usage; exit 1 ;;
  esac
done

if [[ -z "$phase" ]] || [[ -z "$output" ]]; then
  echo "Error: --phase and --output are required." >&2
  usage; exit 1
fi

case "$phase" in
  pre|post|mux-only|compare) ;;
  *) echo "Error: --phase must be pre, post, mux-only, or compare." >&2; exit 1 ;;
esac

# ── helpers ──────────────────────────────────────────────────────────────────

log() { echo "[gpu-mux-evidence] $*"; }

capture_file() {
  local label="$1" path="$2" dest="$3"
  if [[ -r "$path" ]]; then
    cat "$path" > "$dest" 2>/dev/null || echo "(read error)" > "$dest"
  else
    echo "(not found: $path)" > "$dest"
  fi
}

run_cmd() {
  local dest="$1"; shift
  "$@" > "$dest" 2>&1 || echo "(command failed: $*)" >> "$dest"
}

# ── phase: compare ────────────────────────────────────────────────────────────

if [[ "$phase" == "compare" ]]; then
  pre_dir="${pre_dir:-$output/pre}"
  post_dir="$output/post"

  if [[ ! -d "$pre_dir" ]]; then
    echo "Error: pre-phase bundle not found at $pre_dir" >&2
    echo "Run --phase pre first." >&2
    exit 1
  fi
  if [[ ! -d "$post_dir" ]]; then
    echo "Error: post-phase bundle not found at $post_dir" >&2
    echo "Run --phase post after SDDM restart." >&2
    exit 1
  fi

  report="$output/compare-report.txt"
  pre_mode="$(cat "$pre_dir/envycontrol-mode.txt" 2>/dev/null || echo unknown)"
  post_mode="$(cat "$post_dir/envycontrol-mode.txt" 2>/dev/null || echo unknown)"
  pre_mods="$({ grep -E 'nvidia|amdgpu' "$pre_dir/lsmod.txt" 2>/dev/null || true; } | awk '{print $1}' | sort | tr '\n' ' ')"
  post_mods="$({ grep -E 'nvidia|amdgpu' "$post_dir/lsmod.txt" 2>/dev/null || true; } | awk '{print $1}' | sort | tr '\n' ' ')"
  pre_drm="$(cat "$pre_dir/drm-providers.txt" 2>/dev/null | tr '\n' '|' || true)"
  post_drm="$(cat "$post_dir/drm-providers.txt" 2>/dev/null | tr '\n' '|' || true)"
  pre_nv="$(cat "$pre_dir/nvidia-pci-enable.txt" 2>/dev/null || echo unknown)"
  post_nv="$(cat "$post_dir/nvidia-pci-enable.txt" 2>/dev/null || echo unknown)"

  {
    echo "=== GPU MUX Session-Restart Switch Comparison ==="
    echo "Generated: $(date)"
    echo "Pre bundle:  $pre_dir"
    echo "Post bundle: $post_dir"
    echo ""

    echo "--- EnvyControl mode ---"
    echo "  pre:  $pre_mode"
    echo "  post: $post_mode"
    if [[ "$pre_mode" != "$post_mode" ]]; then
      echo "  RESULT: mode changed ✓"
    else
      echo "  RESULT: mode unchanged — switch did not take effect"
    fi
    echo ""

    echo "--- Kernel modules (nvidia/amdgpu) ---"
    echo "  pre:  $pre_mods"
    echo "  post: $post_mods"
    if [[ "$pre_mods" != "$post_mods" ]]; then
      echo "  RESULT: driver set changed ✓"
    else
      echo "  RESULT: driver set unchanged"
    fi
    echo ""

    echo "--- DRM providers ---"
    echo "  pre:  $pre_drm"
    echo "  post: $post_drm"
    if [[ "$pre_drm" != "$post_drm" ]]; then
      echo "  RESULT: DRM topology changed ✓"
    else
      echo "  RESULT: DRM topology unchanged"
    fi
    echo ""

    echo "--- NVIDIA PCI enable state ---"
    echo "  pre:  $pre_nv"
    echo "  post: $post_nv"
    if [[ "$pre_nv" != "$post_nv" ]]; then
      echo "  RESULT: NVIDIA PCI state changed ✓  — hardware MUX or runtime PM confirmed"
    else
      echo "  RESULT: NVIDIA PCI state unchanged"
    fi
    echo ""

    echo "--- Summary ---"
    if [[ "$pre_mode" != "$post_mode" ]] && [[ "$pre_mods" != "$post_mods" ]]; then
      echo "  SESSION-RESTART SWITCHING: CONFIRMED WORKING"
      echo "  Reboot is not required — display manager restart sufficient."
    elif [[ "$pre_mode" != "$post_mode" ]]; then
      echo "  PARTIAL: mode changed but drivers unchanged — investigate DRM state."
    else
      echo "  SESSION-RESTART SWITCHING: NOT CONFIRMED"
      echo "  Mode did not change after SDDM restart. Reboot may still be required,"
      echo "  or EnvyControl switch was not applied before SDDM restart."
    fi
  } | tee "$report"
  python3 - "$output/compare-summary.json" "$pre_mode" "$post_mode" "$pre_mods" "$post_mods" "$pre_drm" "$post_drm" "$pre_nv" "$post_nv" <<'PY'
import json
import sys
from pathlib import Path

(
    output,
    pre_mode,
    post_mode,
    pre_modules,
    post_modules,
    pre_drm,
    post_drm,
    pre_nvidia_pci,
    post_nvidia_pci,
) = sys.argv[1:]

summary = {
    "schema_version": 1,
    "read_only": True,
    "pre_mode": pre_mode,
    "post_mode": post_mode,
    "mode_changed": pre_mode != post_mode,
    "pre_modules": pre_modules.strip(),
    "post_modules": post_modules.strip(),
    "kernel_modules_changed": pre_modules != post_modules,
    "pre_drm_providers": pre_drm,
    "post_drm_providers": post_drm,
    "drm_topology_changed": pre_drm != post_drm,
    "pre_nvidia_pci": pre_nvidia_pci,
    "post_nvidia_pci": post_nvidia_pci,
    "nvidia_pci_state_changed": pre_nvidia_pci != post_nvidia_pci,
}
summary["session_restart_switching_confirmed"] = (
    summary["mode_changed"] and summary["kernel_modules_changed"]
)
Path(output).write_text(json.dumps(summary, indent=2, sort_keys=True) + "\n")
PY
  log "Report written to $report"
  log "Summary written to $output/compare-summary.json"
  exit 0
fi

# ── phases: pre / post / mux-only ─────────────────────────────────────────────

if [[ "$phase" == "pre" ]] || [[ "$phase" == "mux-only" ]]; then
  bundle_dir="$output/pre"
else
  bundle_dir="$output/post"
fi

mkdir -p "$bundle_dir"
log "Capturing $phase state → $bundle_dir"

# ── 1. EnvyControl mode ───────────────────────────────────────────────────────
log "  envycontrol mode"
if command -v envycontrol &>/dev/null; then
  envycontrol --query > "$bundle_dir/envycontrol-mode.txt" 2>&1 || \
    echo "(envycontrol --query failed)" > "$bundle_dir/envycontrol-mode.txt"
else
  echo "(envycontrol not found)" > "$bundle_dir/envycontrol-mode.txt"
fi

# ── 2. Kernel modules ─────────────────────────────────────────────────────────
log "  kernel modules"
lsmod > "$bundle_dir/lsmod.txt" 2>&1

# legion_laptop binding state
{
  echo "=== /sys/bus/platform/drivers/legion/ ==="
  ls /sys/bus/platform/drivers/legion/ 2>/dev/null || echo "(driver dir empty or absent)"
  echo ""
  echo "=== legion_laptop refcnt ==="
  cat /sys/module/legion_laptop/refcnt 2>/dev/null || echo "(not loaded)"
} > "$bundle_dir/legion-laptop-binding.txt"

# ── 3. PCI GPU topology ───────────────────────────────────────────────────────
log "  PCI GPU topology"
{
  echo "=== lspci GPU devices ==="
  lspci -nn | grep -E '3D|VGA|Display' 2>/dev/null || echo "(lspci not available)"
  echo ""
  echo "=== lspci verbose GPU ==="
  lspci -nn -v 2>/dev/null | grep -A 20 -E '3D controller|VGA compatible' || echo "(lspci -v not available)"
} > "$bundle_dir/lspci-gpu.txt"

# Per-GPU PCI sysfs detail
{
  echo "=== PCI devices with GPU classes (0x030000 VGA, 0x030200 3D) ==="
  for class_file in /sys/bus/pci/devices/*/class; do
    class_val="$(cat "$class_file" 2>/dev/null || true)"
    # VGA = 0x030000, 3D = 0x030200, Display = 0x038000
    if [[ "$class_val" == 0x0300* ]] || [[ "$class_val" == 0x0302* ]] || [[ "$class_val" == 0x0380* ]]; then
      dev_path="$(dirname "$class_file")"
      dev_id="$(basename "$dev_path")"
      echo ""
      echo "--- $dev_id (class=$class_val) ---"
      echo "  enable:         $(cat "$dev_path/enable" 2>/dev/null || echo n/a)"
      echo "  vendor:         $(cat "$dev_path/vendor" 2>/dev/null || echo n/a)"
      echo "  device:         $(cat "$dev_path/device" 2>/dev/null || echo n/a)"
      echo "  d3cold_allowed: $(cat "$dev_path/d3cold_allowed" 2>/dev/null || echo n/a)"
      echo "  runtime_status: $(cat "$dev_path/power/runtime_status" 2>/dev/null || echo n/a)"
      echo "  runtime_active: $(cat "$dev_path/power/runtime_active_time" 2>/dev/null || echo n/a)"
      echo "  power_state:    $(cat "$dev_path/power_state" 2>/dev/null || echo n/a)"
      echo "  remove_exists:  $([ -f "$dev_path/remove" ] && echo yes || echo no)"
      echo "  rescan_exists:  $([ -f "$dev_path/rescan" ] && echo yes || echo no)"
      echo "  driver:         $(readlink "$dev_path/driver" 2>/dev/null | xargs basename 2>/dev/null || echo none)"
    fi
  done
} > "$bundle_dir/pci-gpu-sysfs.txt"

# NVIDIA enable state (summary for compare phase)
{
  for class_file in /sys/bus/pci/devices/*/class; do
    class_val="$(cat "$class_file" 2>/dev/null || true)"
    dev_path="$(dirname "$class_file")"
    driver="$(readlink "$dev_path/driver" 2>/dev/null | xargs basename 2>/dev/null || true)"
    if [[ "$driver" == "nvidia" ]] || [[ "$driver" == "nouveau" ]]; then
      echo "$(basename "$dev_path") enable=$(cat "$dev_path/enable" 2>/dev/null) runtime=$(cat "$dev_path/power/runtime_status" 2>/dev/null)"
    fi
  done
} > "$bundle_dir/nvidia-pci-enable.txt"

# ── 4. MUX hardware indicators ────────────────────────────────────────────────
log "  hardware MUX indicators"
{
  echo "=== d3cold_allowed on GPU PCI devices ==="
  echo "(d3cold_allowed=1 on dGPU = D3cold power resource = hardware MUX present)"
  for class_file in /sys/bus/pci/devices/*/class; do
    class_val="$(cat "$class_file" 2>/dev/null || true)"
    if [[ "$class_val" == 0x0300* ]] || [[ "$class_val" == 0x0302* ]]; then
      dev_path="$(dirname "$class_file")"
      dev_id="$(basename "$dev_path")"
      d3cold="$(cat "$dev_path/d3cold_allowed" 2>/dev/null || echo n/a)"
      driver="$(readlink "$dev_path/driver" 2>/dev/null | xargs basename 2>/dev/null || echo none)"
      echo "  $dev_id  driver=$driver  d3cold_allowed=$d3cold"
    fi
  done
  echo ""
  echo "=== vgaswitcheroo ==="
  cat /sys/kernel/debug/vgaswitcheroo/switch 2>/dev/null || echo "(not available — common on PRIME-only systems)"
  echo ""
  echo "=== switcheroo-control (if running) ==="
  if command -v gdbus &>/dev/null; then
    gdbus call --system --dest net.hadess.SwitcherooControl \
      --object-path /net/hadess/SwitcherooControl \
      --method org.freedesktop.DBus.Properties.Get \
      net.hadess.SwitcherooControl GPUs 2>/dev/null || echo "(switcheroo-control not responding)"
  else
    echo "(gdbus not available)"
  fi
  echo ""
  echo "=== gpu_mux sysfs paths ==="
  find /sys -name 'gpu_mux*' -o -name 'mux_control*' 2>/dev/null | head -10 || echo "(none found)"
  echo ""
  echo "=== ACPI _PR3 (D3cold) GPU nodes ==="
  find /sys/bus/acpi/devices -name 'power_state' 2>/dev/null | while read -r f; do
    node="$(dirname "$f")"
    hid="$(cat "$node/hid" 2>/dev/null || true)"
    # GPU ACPI nodes typically have HID PNP0C09 (EC) or specific GPU HID
    if grep -qi 'VGA\|GPU\|NVID\|ATI\|AMD' "$node/path" 2>/dev/null || \
       [[ "$(cat "$node/path" 2>/dev/null)" == *"GPU"* ]]; then
      echo "  $(cat "$node/path" 2>/dev/null): $(cat "$f" 2>/dev/null)"
    fi
  done || true
} > "$bundle_dir/mux-hardware-indicators.txt"

# ── 5. WMI GameZone device ────────────────────────────────────────────────────
log "  WMI gamezone device"
wmi_guid="887B54E3-DDDC-4B2C-8B88-68A26A8835D0-3"
wmi_path="/sys/bus/wmi/devices/$wmi_guid"
{
  echo "=== GameZone WMI device ($wmi_guid) ==="
  if [[ -d "$wmi_path" ]]; then
    echo "  object_id:       $(cat "$wmi_path/object_id" 2>/dev/null || echo n/a)"
    echo "  instance_count:  $(cat "$wmi_path/instance_count" 2>/dev/null || echo n/a)"
    echo "  expensive:       $(cat "$wmi_path/expensive" 2>/dev/null || echo n/a)"
    echo "  driver_override: $(cat "$wmi_path/driver_override" 2>/dev/null || echo n/a)"
    echo "  driver:          $(readlink "$wmi_path/driver" 2>/dev/null | xargs basename 2>/dev/null || echo none)"
    echo "  setable (wmi data block write flag):"
    find "$wmi_path" -name 'setable' -exec cat {} \; 2>/dev/null || echo "  (setable attr absent)"
    echo ""
    echo "  platform-profile choices:"
    cat "$wmi_path/platform-profile/platform-profile-0/choices" 2>/dev/null || echo "  (not found)"
    echo "  current profile:"
    cat "$wmi_path/platform-profile/platform-profile-0/profile" 2>/dev/null || echo "  (not found)"
  else
    echo "  (WMI device not found at $wmi_path)"
  fi
  echo ""
  echo "=== All loaded lenovo/legion WMI modules ==="
  lsmod | grep -E 'lenovo|legion|wmi' || echo "(none)"
} > "$bundle_dir/wmi-gamezone.txt"

# ── 6. DRM state ──────────────────────────────────────────────────────────────
log "  DRM state"
{
  echo "=== DRM render/card devices ==="
  for drm in /sys/class/drm/card*; do
    [[ -d "$drm" ]] || continue
    echo "  $(basename "$drm"): driver=$(readlink "$drm/device/driver" 2>/dev/null | xargs basename 2>/dev/null || echo none)"
    echo "    pci: $(readlink "$drm/device" 2>/dev/null | xargs basename 2>/dev/null || echo n/a)"
  done
  echo ""
  echo "=== DRM connectors ==="
  for conn in /sys/class/drm/card*/*/status; do
    [[ -f "$conn" ]] || continue
    echo "  $(basename "$(dirname "$conn")"): $(cat "$conn" 2>/dev/null)"
  done
  echo ""
  echo "=== Render nodes ==="
  for rn in /sys/class/drm/renderD*; do
    [[ -d "$rn" ]] || continue
    echo "  $(basename "$rn"): driver=$(readlink "$rn/device/driver" 2>/dev/null | xargs basename 2>/dev/null || echo none)"
  done
} > "$bundle_dir/drm-state.txt"

# DRM providers summary (for compare)
{
  for drm in /sys/class/drm/card*; do
    [[ -d "$drm" ]] || continue
    driver="$(readlink "$drm/device/driver" 2>/dev/null | xargs basename 2>/dev/null || echo none)"
    echo "$(basename "$drm")=$driver"
  done
} > "$bundle_dir/drm-providers.txt"

# ── 7. NVIDIA state ───────────────────────────────────────────────────────────
log "  NVIDIA driver state"
{
  echo "=== nvidia-smi ==="
  if command -v nvidia-smi &>/dev/null; then
    nvidia-smi --query-gpu=name,pci.bus_id,power.draw,display_mode --format=csv,noheader 2>/dev/null \
      || { nvidia-smi 2>&1 || true; } | head -30
  else
    echo "(nvidia-smi not found)"
  fi
  echo ""
  echo "=== /proc/driver/nvidia/gpus/ ==="
  if [[ -d /proc/driver/nvidia/gpus ]]; then
    for g in /proc/driver/nvidia/gpus/*/information; do
      [[ -f "$g" ]] || continue
      echo "GPU: $(dirname "$g" | xargs basename)"
      { grep -E 'Model|Bus Location|GPU-00' "$g" 2>/dev/null || cat "$g"; } || true
      echo ""
    done
  else
    echo "(nvidia driver not active)"
  fi
} > "$bundle_dir/nvidia-state.txt"

# ── 8. EnvyControl config files ───────────────────────────────────────────────
log "  envycontrol config state"
{
  echo "=== /etc/modprobe.d/ ==="
  ls /etc/modprobe.d/ 2>/dev/null
  echo ""
  for f in /etc/modprobe.d/*nvidia* /etc/modprobe.d/*envycontrol* /etc/modprobe.d/*blacklist*; do
    [[ -f "$f" ]] || continue
    echo "--- $f ---"
    cat "$f"
    echo ""
  done
  echo "=== /etc/X11/xorg.conf.d/ ==="
  ls /etc/X11/xorg.conf.d/ 2>/dev/null || echo "(absent)"
  for f in /etc/X11/xorg.conf.d/*nvidia* /etc/X11/xorg.conf.d/*envycontrol*; do
    [[ -f "$f" ]] || continue
    echo "--- $f ---"
    cat "$f"
    echo ""
  done
  echo "=== /etc/udev/rules.d/ (GPU) ==="
  for f in /etc/udev/rules.d/*nvidia* /etc/udev/rules.d/*envycontrol*; do
    [[ -f "$f" ]] || continue
    echo "--- $f ---"
    cat "$f"
    echo ""
  done
} > "$bundle_dir/envycontrol-config.txt"

# ── 9. Session / display manager info ─────────────────────────────────────────
log "  display manager state"
{
  echo "=== SDDM service ==="
  systemctl show sddm --property=ActiveState,SubState,MainPID 2>/dev/null || echo "(systemctl unavailable)"
  echo ""
  echo "=== Active display sessions ==="
  loginctl list-sessions 2>/dev/null || echo "(loginctl unavailable)"
  echo ""
  echo "=== WAYLAND_DISPLAY / DISPLAY ==="
  echo "  WAYLAND_DISPLAY=${WAYLAND_DISPLAY:-unset}"
  echo "  DISPLAY=${DISPLAY:-unset}"
  echo "  XDG_SESSION_TYPE=${XDG_SESSION_TYPE:-unset}"
} > "$bundle_dir/session-state.txt"

# ── 10. Manifest ──────────────────────────────────────────────────────────────
{
  echo "phase=$phase"
  echo "timestamp=$(date -Iseconds)"
  echo "hostname=$(hostname)"
  echo "kernel=$(uname -r)"
  echo "user=$(whoami)"
} > "$bundle_dir/manifest.txt"

log "Done. Bundle: $bundle_dir"
echo ""

# ── Print next steps ──────────────────────────────────────────────────────────
if [[ "$phase" == "pre" ]]; then
  current_mode="$(cat "$bundle_dir/envycontrol-mode.txt" 2>/dev/null | tr -d '[:space:]')"
  case "$current_mode" in
    integrated) suggested="hybrid" ;;
    hybrid)     suggested="nvidia" ;;
    nvidia)     suggested="hybrid" ;;
    *)          suggested="hybrid" ;;
  esac
  echo "Current mode: $current_mode"
  echo ""
  echo "Next steps:"
  echo "  1. sudo envycontrol -s $suggested"
  echo ""
  echo "  **WARNING: Step 2 will kill your display session.**"
  echo "  Save all work before running:"
  echo "  2. sudo systemctl restart sddm"
  echo ""
  echo "  3. Log back in, then run:"
  echo "     scripts/capture-gpu-mux-evidence.sh --phase post --output $output"
  echo ""
  echo "  4. Compare results:"
  echo "     scripts/capture-gpu-mux-evidence.sh --phase compare --output $output"
fi

if [[ "$phase" == "post" ]]; then
  echo "Post-state captured. Run compare:"
  echo "  scripts/capture-gpu-mux-evidence.sh --phase compare --output $output"
fi

if [[ "$phase" == "mux-only" ]]; then
  echo "MUX hardware evidence captured in $bundle_dir"
  echo "Key file: $bundle_dir/mux-hardware-indicators.txt"
  echo ""
  echo "Hotswap feasibility indicators:"
  echo "  d3cold_allowed=1 on dGPU  → hardware MUX present (session-restart likely sufficient)"
  echo "  vgaswitcheroo present      → kernel MUX framework active"
  echo "  d3cold_allowed=0 on dGPU  → PRIME offload only (reboot likely required)"
fi
