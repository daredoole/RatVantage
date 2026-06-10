#!/usr/bin/env bash
# EC Exploration - Phase 1: Read-only EC dump + ACPI table extraction
#
# What this does:
#   1. Loads ec_sys (read-only) and dumps the 256-byte ACPI EC I/O space
#   2. Attempts physical EC RAM read via /dev/mem at 0xFE0B0400 (82WM chip 5507)
#   3. Copies DSDT + all SSDTs for disassembly
#   4. Disassembles ACPI tables with iasl (if available)
#   5. Inventories all WMI devices with GUIDs, object IDs, drivers
#   6. Reads gamezone platform-profile choices
#
# Usage:  sudo ./scripts/ec-explore-phase1.sh [--repeat N]
#
# --repeat N : take N snapshots 5 seconds apart (to spot live register changes)
#
# Prerequisites (install if not present):
#   sudo dnf install acpica-tools   # for iasl ACPI disassembly
#
# Output: target/ec-exploration/phase1-<timestamp>/

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

REPEAT=1
while [[ $# -gt 0 ]]; do
    case "$1" in
        --repeat) REPEAT="$2"; shift 2 ;;
        *) echo "Unknown arg: $1"; exit 1 ;;
    esac
done

if [[ $EUID -ne 0 ]]; then
    echo "ERROR: must run as root (sudo $0)"
    exit 1
fi

TIMESTAMP=$(date +%Y%m%d-%H%M%S)
OUT_DIR="$REPO_ROOT/target/ec-exploration/phase1-$TIMESTAMP"
mkdir -p "$OUT_DIR"

log() { echo "[$(date +%H:%M:%S)] $*" | tee -a "$OUT_DIR/phase1.log"; }

log "=== EC Exploration Phase 1 ==="
log "Output: $OUT_DIR"

# ── 1. Load ec_sys read-only (optional — kernel 7.0.11 has it disabled) ──────
EC_IO=/sys/kernel/debug/ec/ec0/io
EC_SYS_OK=false

if [[ -f "$EC_IO" ]]; then
    log "ec debugfs already present: $EC_IO"
    EC_SYS_OK=true
elif lsmod | grep -q '^ec_sys '; then
    log "ec_sys loaded but no debugfs node — skipping EC I/O dump"
else
    log "Attempting to load ec_sys (read-only, write_support=0)..."
    if modprobe ec_sys write_support=0 2>/dev/null; then
        sleep 0.5
        if [[ -f "$EC_IO" ]]; then
            log "  ec_sys OK: $EC_IO present"
            EC_SYS_OK=true
        else
            log "  ec_sys loaded but no debugfs node"
        fi
    else
        log "  ec_sys not available in this kernel (CONFIG_ACPI_EC_DEBUGFS=n)"
        log "  Skipping EC I/O space dump — continuing with ACPI table extraction"
    fi
fi

# ── 2. EC I/O space snapshots (256 bytes, standard ACPI EC space) ──────────
take_ec_snapshot() {
    local idx="$1"
    local snap_dir="$OUT_DIR/snapshot-$idx"
    mkdir -p "$snap_dir"
    local ts; ts=$(date +%s)

    if [[ -f "$EC_IO" ]]; then
        dd if="$EC_IO" bs=256 count=1 2>/dev/null > "$snap_dir/ec_io.bin"
        xxd "$snap_dir/ec_io.bin" > "$snap_dir/ec_io.hex"
        # Extract chip ID bytes — ITE chip ID in ACPI EC space may be at 0x20/0x21
        # or accessible via Super I/O ports 0x4E/0x4F (different space)
        printf "Snapshot %d at %s\n" "$idx" "$(date)" > "$snap_dir/meta.txt"
        printf "EC I/O byte 0x20: 0x%02x\n" "0x$(xxd -s 0x20 -l 1 -p "$snap_dir/ec_io.bin")" >> "$snap_dir/meta.txt"
        printf "EC I/O byte 0x21: 0x%02x\n" "0x$(xxd -s 0x21 -l 1 -p "$snap_dir/ec_io.bin")" >> "$snap_dir/meta.txt"
        log "  Snapshot $idx: ec_io.bin (256 bytes) saved"
    else
        log "  Snapshot $idx: ec_io not available"
    fi

    # Also read known sensor regions from extended EC space via Super I/O if accessible
    # Super I/O base: 0x4E/0x4F — we can probe indirectly via iotools if installed
    if command -v isacmd &>/dev/null 2>&1; then
        isacmd rb 0x62 >> "$snap_dir/acpi_ec_data_port.txt" 2>&1 || true
    fi

    echo "$ts" > "$snap_dir/timestamp.txt"
}

if [[ "$EC_SYS_OK" == "true" ]]; then
    log "Taking $REPEAT EC snapshot(s)..."
    for i in $(seq 1 "$REPEAT"); do
        take_ec_snapshot "$i"
        if [[ $i -lt $REPEAT ]]; then
            log "  Waiting 5 seconds before next snapshot..."
            sleep 5
        fi
    done

    if [[ $REPEAT -gt 1 ]]; then
        log "Diffing snapshots to find live registers..."
        first_hex="$OUT_DIR/snapshot-1/ec_io.hex"
        for i in $(seq 2 "$REPEAT"); do
            diff "$first_hex" "$OUT_DIR/snapshot-$i/ec_io.hex" > "$OUT_DIR/diff-snapshot1-vs-$i.txt" 2>&1 || true
        done
        log "  Diffs saved"
    fi
else
    log "Skipping EC I/O snapshots (ec_sys not available)"
fi

# ── 3. Physical EC RAM via /dev/mem at 0xFE0B0400 ─────────────────────────
# WMI3 models (chip 5507, 82WM) have EC RAM mapped at 0xFE0B0400, size 0x600
# This region contains the extended registers (0xC400-0xC9FF relative to EC base)
log "Physical EC RAM (0xFE0B0400): blocked by CONFIG_STRICT_DEVMEM=y"
log "  EC data only accessible via LLL kernel module after installation"
log "  See: sudo ./scripts/ec-explore-install-tools.sh --lll"

# ── 4. ACPI table extraction ───────────────────────────────────────────────
log "Extracting ACPI tables..."
ACPI_DIR="$OUT_DIR/acpi"
mkdir -p "$ACPI_DIR"

TABLES=(DSDT SSDT1 SSDT2 SSDT3 SSDT4 SSDT5 SSDT6 SSDT7 SSDT8 SSDT9 SSDT10 SSDT11 SSDT12 SSDT13 SSDT14 SSDT15 SSDT16)
for tbl in "${TABLES[@]}"; do
    src="/sys/firmware/acpi/tables/$tbl"
    if [[ -f "$src" ]]; then
        cp "$src" "$ACPI_DIR/$tbl.bin"
        log "  Copied $tbl ($(wc -c < "$ACPI_DIR/$tbl.bin") bytes)"
    fi
done

# ── 5. ACPI disassembly ────────────────────────────────────────────────────
if command -v iasl &>/dev/null; then
    log "Disassembling ACPI tables with iasl..."
    DSL_DIR="$ACPI_DIR/dsl"
    mkdir -p "$DSL_DIR"
    for bin in "$ACPI_DIR"/*.bin; do
        base=$(basename "${bin%.bin}")
        iasl -d "$bin" -p "$DSL_DIR/$base" 2>&1 | tail -3 | sed "s/^/  $base: /"
    done
    log "  Disassembled to $DSL_DIR/"

    # Search for the fan WMI GUID (92549549) in disassembled code
    FAN_GUID_HITS=$(grep -r "92549549\|B2\b.*Method\|WMB2\|Fan\|FAN" "$DSL_DIR/" 2>/dev/null | head -40 || true)
    if [[ -n "$FAN_GUID_HITS" ]]; then
        echo "$FAN_GUID_HITS" > "$OUT_DIR/fan_wmi_acpi_hits.txt"
        log "  Found fan WMI references in ACPI → fan_wmi_acpi_hits.txt"
    fi

    # Search for all WMI method handlers
    grep -r "WMI\|_WM\b" "$DSL_DIR/" 2>/dev/null > "$OUT_DIR/all_wmi_acpi_hits.txt" || true
    log "  WMI references saved → all_wmi_acpi_hits.txt"
else
    log "  iasl not found. Install: sudo dnf install acpica-tools"
    log "  Skipping ACPI disassembly. Binary tables saved in $ACPI_DIR/"
fi

# ── 6. WMI device inventory ────────────────────────────────────────────────
log "Inventorying WMI devices..."
{
    printf "%-55s %-6s %-4s %-4s %-30s\n" "GUID" "obj_id" "inst" "exp" "driver"
    printf '%s\n' "$(printf '─%.0s' {1..110})"
    for dev in /sys/bus/wmi/devices/*/; do
        guid=$(basename "$dev")
        oid=$(cat "$dev/object_id" 2>/dev/null || echo "?")
        cnt=$(cat "$dev/instance_count" 2>/dev/null || echo "?")
        exp=$(cat "$dev/expensive" 2>/dev/null || echo "?")
        set=$(cat "$dev/setable" 2>/dev/null || echo "-")
        drv=$(readlink "$dev/driver" 2>/dev/null | xargs basename 2>/dev/null || echo "UNBOUND")
        printf "%-55s %-6s %-4s %-4s %-20s setable=%s\n" "$guid" "$oid" "$cnt" "$exp" "$drv" "$set"
    done
} > "$OUT_DIR/wmi_inventory.txt"
cat "$OUT_DIR/wmi_inventory.txt"
log "  Saved → wmi_inventory.txt"

# ── 7. Platform profile state ─────────────────────────────────────────────
log "Platform profile state..."
{
    echo "=== Platform Profile ==="
    for f in /sys/firmware/acpi/platform_profile /sys/devices/virtual/platform-profile/default/profile; do
        [[ -f "$f" ]] && echo "$f: $(cat "$f")"
    done
    for d in /sys/bus/wmi/devices/*/platform-profile/*/; do
        [[ -d "$d" ]] || continue
        echo "choices: $(cat "$d/choices" 2>/dev/null)"
        echo "profile: $(cat "$d/profile" 2>/dev/null)"
    done
} > "$OUT_DIR/platform_profile.txt"

# ── 8. lenovo_wmi_other (887B54E2) data block read ────────────────────────
# This device has object_id=A1 (data block WDA1) with 2 instances
log "Attempting lenovo_wmi_other data block read..."
WMI_OTHER=/sys/bus/wmi/devices/887B54E2-DDDC-4B2C-8B88-68A26A8835D0-9
if [[ -d "$WMI_OTHER" ]]; then
    # If a driver owns it, it may expose a 'data' sysfs file
    ls "$WMI_OTHER/" > "$OUT_DIR/wmi_other_files.txt" 2>&1
    log "  Files: $(cat "$OUT_DIR/wmi_other_files.txt" | tr '\n' ' ')"
fi

# ── 9. hwmon sensors ──────────────────────────────────────────────────────
log "Reading all hwmon sensors..."
{
    for d in /sys/class/hwmon/hwmon*/; do
        name=$(cat "$d/name" 2>/dev/null || echo "?")
        echo "=== $d ($name) ==="
        for f in "$d"temp*_input "$d"fan*_input "$d"in*_input "$d"power*_input; do
            [[ -f "$f" ]] && printf "  %-40s = %s\n" "$(basename "$f")" "$(cat "$f" 2>/dev/null)"
        done
    done
} > "$OUT_DIR/hwmon_sensors.txt"
log "  Saved → hwmon_sensors.txt"

log "=== Phase 1 Complete ==="
log "Output: $OUT_DIR"
echo ""
echo "Next steps:"
echo "  1. Install iasl: sudo dnf install acpica-tools"
echo "     Then re-run to get ACPI disassembly"
echo "  2. Look at: $OUT_DIR/ec_io.hex (EC I/O space)"
echo "  3. Review: $OUT_DIR/wmi_inventory.txt"
echo "  4. Run Phase 2 for WMI method probing (requires acpi_call):"
echo "     sudo ./scripts/ec-explore-phase2-wmi.sh"
