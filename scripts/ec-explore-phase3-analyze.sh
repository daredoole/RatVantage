#!/usr/bin/env bash
# EC Exploration - Phase 3: Analyze phase1/phase2 dumps
#
# - Finds the WMI fan method ACPI path from disassembled tables
# - Diffs multiple EC snapshots to identify live/changing registers
# - Maps non-zero EC registers to known/unknown categories
# - Extracts temperature-like values (range 0-120) from EC space
# - Extracts RPM-like values from EC space
# - Prints a summary of findings
#
# Usage:
#   ./scripts/ec-explore-phase3-analyze.sh [phase1_dir]
#   ./scripts/ec-explore-phase3-analyze.sh   # auto-picks latest phase1 dir
#
# No root required.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
EC_DIR="$REPO_ROOT/target/ec-exploration"

# Auto-pick latest phase1 dir
if [[ $# -gt 0 ]]; then
    PHASE1_DIR="$1"
else
    PHASE1_DIR=$(ls -dt "$EC_DIR"/phase1-* 2>/dev/null | head -1 || true)
fi

if [[ -z "$PHASE1_DIR" || ! -d "$PHASE1_DIR" ]]; then
    echo "ERROR: No phase1 directory found. Run ec-explore-phase1.sh first."
    exit 1
fi

echo "Analyzing: $PHASE1_DIR"
echo ""

# ── 1. Find WMI B2 method path in disassembled ACPI ──────────────────────
DSL_DIR="$PHASE1_DIR/acpi/dsl"
if [[ -d "$DSL_DIR" ]]; then
    echo "=== WMI Fan Method (B2 / 92549549) ACPI Path ==="
    WMB2_HITS=$(grep -rn "WMB2\|92549549\|B2.*Method\|Method.*B2" "$DSL_DIR/" 2>/dev/null || true)
    if [[ -n "$WMB2_HITS" ]]; then
        echo "$WMB2_HITS" | head -20
        # Extract the method path
        echo ""
        echo "Suggested call path (first match):"
        grep -rn "Method.*WMB2\|WMB2.*Method" "$DSL_DIR/" 2>/dev/null | head -5 || true
    else
        echo "  WMB2 not found in disassembled tables."
        echo "  All Method definitions:"
        grep -rn "^    Method" "$DSL_DIR/DSDT.dsl" 2>/dev/null | head -30 || true
    fi
    echo ""

    echo "=== WMI GUID Blocks in DSDT ==="
    # WMI GUIDs are stored as reversed byte sequences in _WDG packages
    grep -n "_WDG\|ToUUID\|GUID\|8B88\|8835D0\|DBAA" "$DSL_DIR/DSDT.dsl" 2>/dev/null | head -30 || true
    echo ""

    echo "=== Fan-Related ACPI Methods ==="
    grep -n "Fan\|FAN\|GFAN\|SFAN\|FSPD\|FTMP\|FTBL" "$DSL_DIR/DSDT.dsl" 2>/dev/null | head -30 || true
    echo ""
fi

# ── 2. EC I/O space analysis ──────────────────────────────────────────────
SNAP1="$PHASE1_DIR/snapshot-1/ec_io.bin"
if [[ -f "$SNAP1" ]]; then
    echo "=== EC I/O Space (256 bytes ACPI EC) ==="
    python3 - "$PHASE1_DIR" <<'PYEOF'
import sys, os

snap_dir = os.path.join(sys.argv[1], 'snapshot-1')
io_bin = os.path.join(snap_dir, 'ec_io.bin')

if not os.path.exists(io_bin):
    print("  ec_io.bin not found")
    sys.exit(0)

with open(io_bin, 'rb') as f:
    data = f.read()

print(f"  Total bytes: {len(data)}")

# Find non-zero bytes
nonzero = [(i, b) for i, b in enumerate(data) if b != 0]
print(f"  Non-zero bytes: {len(nonzero)}")

# Temperature-like values (20-110°C)
temps = [(i, b) for i, b in nonzero if 20 <= b <= 110]
print(f"\n  Possible temperature values (20-110):")
for off, val in temps:
    print(f"    EC[0x{off:02X}] = {val}°C")

# RPM-like patterns (fan RPM often stored as RPM/100, so 10-80 = 1000-8000 RPM)
rpm_candidates = [(i, b) for i, b in nonzero if 10 <= b <= 80]
print(f"\n  Possible fan speed values (RPM/100, 10-80 = 1000-8000 RPM):")
for off, val in rpm_candidates:
    print(f"    EC[0x{off:02X}] = {val} ({val*100} RPM)")

# All non-zero bytes
print(f"\n  All non-zero bytes:")
for off, val in nonzero:
    print(f"    EC[0x{off:02X}] = 0x{val:02X} ({val:3d})")
PYEOF
    echo ""
fi

# ── 3. EC I/O snapshot diffs ──────────────────────────────────────────────
SNAP_COUNT=$(ls -d "$PHASE1_DIR"/snapshot-* 2>/dev/null | wc -l)
if [[ $SNAP_COUNT -gt 1 ]]; then
    echo "=== EC I/O Snapshot Diffs (live register detection) ==="
    python3 - "$PHASE1_DIR" <<'PYEOF'
import os, glob, sys

phase1 = sys.argv[1]
snaps = sorted(glob.glob(os.path.join(phase1, 'snapshot-*/ec_io.bin')))

if len(snaps) < 2:
    print("  Need ≥2 snapshots for diff")
else:
    bins = []
    for s in snaps:
        with open(s, 'rb') as f:
            bins.append(f.read())

    print(f"  Comparing {len(bins)} snapshots")
    changing = set()
    for i in range(1, len(bins)):
        for off in range(min(len(bins[0]), len(bins[i]))):
            if bins[0][off] != bins[i][off]:
                changing.add(off)

    if changing:
        print(f"  Changing registers ({len(changing)}):")
        for off in sorted(changing):
            vals = [f"0x{b[off]:02X}" for b in bins]
            print(f"    EC[0x{off:02X}]: {' → '.join(vals)}")
    else:
        print("  No changing registers detected between snapshots.")
        print("  Try --repeat 5 during load for better coverage.")
PYEOF
    echo ""
fi

# ── 4. Physical EC RAM analysis ───────────────────────────────────────────
PHYS_BIN="$PHASE1_DIR/ec_phys_ram.bin"
if [[ -f "$PHYS_BIN" ]]; then
    echo "=== Physical EC RAM (0xFE0B0400, 0x600 bytes) ==="
    python3 - "$PHYS_BIN" <<'PYEOF'
import os, sys

phys_bin = sys.argv[1]
if not os.path.exists(phys_bin):
    print("  ec_phys_ram.bin not found")
    exit()

with open(phys_bin, 'rb') as f:
    data = f.read()

print(f"  Total bytes: {len(data)}")

# EC register base is 0xC400, this dump starts at EC offset 0x0000
# So EC reg 0xC400+N maps to data[N]
def ec_reg(n): return 0xC400 + n

nonzero = [(i, b) for i, b in enumerate(data) if b != 0]
print(f"  Non-zero bytes: {len(nonzero)}")

# Known register map (ec_register_offsets_v0)
known = {
    0xC534: 'EXT_FAN_CUR_POINT',
    0xC535: 'EXT_FAN_POINTS_SIZE',
    0xC540: 'EXT_FAN1_BASE[0]',
    0xC541: 'EXT_FAN1_BASE[1]',
    0xC542: 'EXT_FAN1_BASE[2]',
    0xC543: 'EXT_FAN1_BASE[3]',
    0xC544: 'EXT_FAN1_BASE[4]',
    0xC545: 'EXT_FAN1_BASE[5]',
    0xC546: 'EXT_FAN1_BASE[6]',
    0xC547: 'EXT_FAN1_BASE[7]',
    0xC548: 'EXT_FAN1_BASE[8]',
    0xC549: 'EXT_FAN1_BASE[9]',
    0xC550: 'EXT_FAN2_BASE[0]',
    0xC551: 'EXT_FAN2_BASE[1]',
    0xC560: 'EXT_FAN_ACC_BASE[0]',
    0xC570: 'EXT_FAN_DEC_BASE[0]',
    0xC580: 'EXT_CPU_TEMP[0]',
    0xC590: 'EXT_CPU_TEMP_HYST[0]',
    0xC5A0: 'EXT_GPU_TEMP[0]',
    0xC5B0: 'EXT_GPU_TEMP_HYST[0]',
}

print("\n  Known register values:")
for reg, name in sorted(known.items()):
    off = reg - 0xC400
    if off < len(data):
        val = data[off]
        marker = " ← NON-ZERO" if val != 0 else ""
        print(f"    {name:30s} @ 0x{reg:04X} [+0x{off:03X}] = 0x{val:02X} ({val:3d}){marker}")

print(f"\n  All non-zero bytes with register addresses:")
for off, val in nonzero:
    reg = ec_reg(off)
    name = known.get(reg, '?')
    # Guess what type of value this might be
    hint = ""
    if 20 <= val <= 110: hint = " [temp?]"
    elif 10 <= val <= 80: hint = " [fan%? or RPM/100?]"
    elif val in (0x01, 0x02, 0x03, 0x04, 0x05): hint = " [small index?]"
    print(f"    off=0x{off:04X}  reg=0x{reg:04X}  val=0x{val:02X} ({val:3d})  {name}{hint}")
PYEOF
    echo ""
fi

# ── 5. WMI inventory summary ──────────────────────────────────────────────
WMI_INV="$PHASE1_DIR/wmi_inventory.txt"
if [[ -f "$WMI_INV" ]]; then
    echo "=== WMI Device Inventory ==="
    cat "$WMI_INV"
    echo ""
fi

echo "=== Analysis Complete ==="
echo "Phase 1 data: $PHASE1_DIR"
