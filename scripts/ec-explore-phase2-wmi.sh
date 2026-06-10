#!/usr/bin/env bash
# EC Exploration - Phase 2: WMI fan method probing via acpi_call
#
# Calls the fan WMI methods directly:
#   GUID: 92549549-4BDE-4F06-AC04-CE8BF898DBAA  (object _WMB2)
#   Method IDs (from LenovoLegionLinux analysis):
#     1 = GET fan full-speed state
#     2 = SET fan full-speed (dust cleaning mode — NOT called here)
#     3 = GET max fan speed
#     4 = SET max fan speed (NOT called here)
#     5 = GET fan table (current fan curve, all 10 points)
#     6 = SET fan table (NOT called here)
#     7 = GET current fan speeds (fan1 RPM, fan2 RPM)
#     8 = GET current sensor temps (CPU temp, GPU temp)
#
# Prerequisites:
#   sudo dnf install dkms kernel-devel
#   git clone https://github.com/nix-community/acpi_call /tmp/acpi_call
#   OR: install from Fedora COPR
#
#   Quick install option (if acpi_call DKMS is available):
#   See: https://github.com/mkottman/acpi_call
#
# Usage:  sudo ./scripts/ec-explore-phase2-wmi.sh
#
# Output: target/ec-exploration/phase2-<timestamp>/

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

if [[ $EUID -ne 0 ]]; then
    echo "ERROR: must run as root (sudo $0)"
    exit 1
fi

TIMESTAMP=$(date +%Y%m%d-%H%M%S)
OUT_DIR="$REPO_ROOT/target/ec-exploration/phase2-$TIMESTAMP"
mkdir -p "$OUT_DIR"

log() { echo "[$(date +%H:%M:%S)] $*" | tee -a "$OUT_DIR/phase2.log"; }

log "=== EC Exploration Phase 2: WMI Fan Method Probing ==="
log "Output: $OUT_DIR"

# ── Check acpi_call ────────────────────────────────────────────────────────
if ! lsmod | grep -q '^acpi_call '; then
    log "Loading acpi_call module..."
    if modprobe acpi_call 2>/dev/null; then
        log "  acpi_call loaded OK"
    else
        cat >&2 <<'EOF'
ERROR: acpi_call module not available.

Install options (choose one):

Option A — Build from source (needs kernel-devel):
  sudo dnf install dkms kernel-devel kernel-headers
  git clone https://github.com/mkottman/acpi_call /tmp/acpi_call_src
  cd /tmp/acpi_call_src
  make
  sudo make install   # installs via DKMS
  sudo modprobe acpi_call

Option B — Install LenovoLegionLinux DKMS instead (recommended):
  This provides the full fan curve interface AND WMI3 support for 82WM.
  See: https://github.com/johnfanv2/LenovoLegionLinux
  sudo dnf install dkms kernel-devel kernel-headers
  git clone https://github.com/johnfanv2/LenovoLegionLinux /tmp/lll
  cd /tmp/lll && sudo make dkms_install

EOF
        exit 1
    fi
fi

ACPI_CALL=/proc/acpi/call
if [[ ! -f "$ACPI_CALL" ]]; then
    log "ERROR: $ACPI_CALL not found even though acpi_call is loaded"
    exit 1
fi

log "acpi_call ready: $ACPI_CALL"

# ── WMI method caller ─────────────────────────────────────────────────────
# WMI method calls work via:
# 1. Find the ACPI path for the WMI GUID's _WMxx method
# 2. Call via acpi_call: echo '\_SB.WMI1.WMB2 <instance> <method_id> <input_buf>' > /proc/acpi/call
#
# WMI GUID 92549549-4BDE-4F06-AC04-CE8BF898DBAA → object B2 → method _WMB2
# The ACPI path needs to be found from the disassembled tables.
# Common paths: \_SB.WMI2.WMB2, \_SB.PC00.WMI2.WMB2, etc.

FAN_GUID="92549549-4BDE-4F06-AC04-CE8BF898DBAA"
FAN_OBJ="B2"

# Confirmed paths from 82WM DSDT disassembly:
#   WMB2 (92549549) = fan curve table (method 5), full-speed (1), max speed (3)
#   WMB5 (DC2A8805) = live fan RPM (feature 0x04030001/2), temps (0x05040000/5)
FAN_ACPI_PATH="\_SB.GZFD.WMB2"
FEAT_ACPI_PATH="\_SB.GZFD.WMB5"
FEAT_METHOD=0x11   # WMI_METHOD_ID_GET_FEATURE_VALUE = 17

log "WMB2 path: $FAN_ACPI_PATH (fan curve/full-speed)"
log "WMB5 path: $FEAT_ACPI_PATH (live fan RPM + temps)"

# ── acpi_call helper ───────────────────────────────────────────────────────
acpi_get() {
    local call="$1"
    echo "$call" > "$ACPI_CALL"
    cat "$ACPI_CALL"
}

# ── Live fan speeds via WMB5 (confirmed working: FANS*100 / FA2S*100) ─────
log ""
log "=== WMB5 feature 0x04030001: Fan 1 RPM (EC0.FANS × 100) ==="
result=$(acpi_get "$FEAT_ACPI_PATH 0x00 $FEAT_METHOD 0x04030001")
log "  Raw: $result"
python3 -c "
import re
m = re.search(r'0x([0-9A-Fa-f]+)', '''$result''')
if m:
    v = int(m.group(1), 16)
    print(f'  fan1_rpm = {v}  (EC FANS={v//100})')
" 2>/dev/null || true
echo "fan1_rpm_raw=$result" > "$OUT_DIR/wmb5_fan1_rpm.txt"

log ""
log "=== WMB5 feature 0x04030002: Fan 2 RPM (EC0.FA2S × 100) ==="
result=$(acpi_get "$FEAT_ACPI_PATH 0x00 $FEAT_METHOD 0x04030002")
log "  Raw: $result"
python3 -c "
import re
m = re.search(r'0x([0-9A-Fa-f]+)', '''$result''')
if m:
    v = int(m.group(1), 16)
    print(f'  fan2_rpm = {v}  (EC FA2S={v//100})')
" 2>/dev/null || true
echo "fan2_rpm_raw=$result" >> "$OUT_DIR/wmb5_fan1_rpm.txt"

log ""
log "=== WMB5 feature 0x05040000: CPU temperature (EC0.CPUT °C) ==="
result=$(acpi_get "$FEAT_ACPI_PATH 0x00 $FEAT_METHOD 0x05040000")
log "  Raw: $result"
python3 -c "
import re
m = re.search(r'0x([0-9A-Fa-f]+)', '''$result''')
if m:
    v = int(m.group(1), 16)
    print(f'  cpu_temp = {v}°C')
" 2>/dev/null || true
echo "cpu_temp_raw=$result" > "$OUT_DIR/wmb5_temps.txt"

log ""
log "=== WMB5 feature 0x05050000: GPU temperature (EC0.GPUT °C) ==="
result=$(acpi_get "$FEAT_ACPI_PATH 0x00 $FEAT_METHOD 0x05050000")
log "  Raw: $result"
python3 -c "
import re
m = re.search(r'0x([0-9A-Fa-f]+)', '''$result''')
if m:
    v = int(m.group(1), 16)
    print(f'  gpu_temp = {v}°C  (0=power-gated)')
" 2>/dev/null || true
echo "gpu_temp_raw=$result" >> "$OUT_DIR/wmb5_temps.txt"

log ""
log "=== WMB5 feature 0x05010000: CPU speed sensor (EC0.CPUS) ==="
result=$(acpi_get "$FEAT_ACPI_PATH 0x00 $FEAT_METHOD 0x05010000")
log "  Raw: $result"
echo "cpus_raw=$result" >> "$OUT_DIR/wmb5_temps.txt"

log ""
log "=== WMB5 feature 0x04020000: Fan status flags (EC0.FNST) ==="
result=$(acpi_get "$FEAT_ACPI_PATH 0x00 $FEAT_METHOD 0x04020000")
log "  Raw: $result"
echo "fnst_raw=$result" >> "$OUT_DIR/wmb5_temps.txt"

# ── Method 5: GET fan curve table ─────────────────────────────────────────
log ""
log "=== Method 5: GET Fan Curve Table ==="
if [[ -n "$FAN_ACPI_PATH" ]]; then
    result=$(call_wmi_method "$FAN_ACPI_PATH" 0 5 "0x00 0x00 0x00 0x00" 2>/dev/null || echo "failed")
    log "  Raw result (first 200 chars): ${result:0:200}"
    echo "method=5 result=$result" > "$OUT_DIR/method5_fan_table.txt"

    python3 - <<'PYEOF' >> "$OUT_DIR/method5_fan_table.txt" 2>&1 || true
import re, sys

result_raw = open('/dev/stdin', 'r').read() if not sys.stdin.isatty() else ""
# Parse from the logged file instead
PYEOF

    # Parse fan table
    python3 - <<PYEOF >> "$OUT_DIR/method5_fan_table.txt" 2>&1 || true
result = """$result"""
import re
vals = re.findall(r'0x([0-9A-Fa-f]+)', result)
if vals:
    ints = [int(v, 16) for v in vals]
    print(f"Parsed {len(ints)} values:")
    # Fan table structure from LLL (wmi_write_fancurve_custom):
    # 88 bytes total = buffer with fan speeds + temps at even/odd offsets
    # F000=powermode, then alternating fan1/fan2 speeds
    for i, v in enumerate(ints):
        print(f"  [{i:2d}] = 0x{v:02X} ({v:3d})")
PYEOF
fi

# ── Method 1: GET full-speed state ────────────────────────────────────────
log ""
log "=== Method 1: GET Fan Full-Speed State ==="
if [[ -n "$FAN_ACPI_PATH" ]]; then
    result=$(call_wmi_method "$FAN_ACPI_PATH" 0 1 "0x00 0x00 0x00 0x00" 2>/dev/null || echo "failed")
    log "  Raw result: $result"
    echo "method=1 result=$result" > "$OUT_DIR/method1_fullspeed.txt"
fi

# ── Method 3: GET max fan speed ───────────────────────────────────────────
log ""
log "=== Method 3: GET Max Fan Speed ==="
if [[ -n "$FAN_ACPI_PATH" ]]; then
    result=$(call_wmi_method "$FAN_ACPI_PATH" 0 3 "0x00 0x00 0x00 0x00" 2>/dev/null || echo "failed")
    log "  Raw result: $result"
    echo "method=3 result=$result" > "$OUT_DIR/method3_maxspeed.txt"
fi

log ""
log "=== Phase 2 Complete ==="
log "Output: $OUT_DIR"
echo ""
echo "Next steps:"
echo "  Review: $OUT_DIR/"
echo "  If ACPI path was not found:"
echo "    1. Run phase1 with iasl installed to get DSDT.dsl"
echo "    2. grep -r 'WMB2' target/ec-exploration/*/acpi/dsl/"
echo "    3. Update FAN_ACPI_PATH in this script"
echo "  Then: sudo ./scripts/ec-explore-phase3-analyze.sh <phase1_dir> <phase2_dir>"
