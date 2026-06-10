#!/usr/bin/env bash
# EC Exploration - Install required tools
#
# Installs:
#   - acpica-tools (iasl for ACPI disassembly)
#   - kernel-devel + dkms (for acpi_call and/or LLL module)
#   - Optionally: LenovoLegionLinux DKMS module (recommended for 82WM)
#   - Optionally: acpi_call DKMS module (minimal, for raw WMI probing)
#
# Usage:
#   sudo ./scripts/ec-explore-install-tools.sh [--lll | --acpi-call | --iasl-only]

set -euo pipefail

MODE="lll"   # default: install LLL DKMS (full fan curve support)

while [[ $# -gt 0 ]]; do
    case "$1" in
        --lll)        MODE="lll" ;;
        --acpi-call)  MODE="acpi_call" ;;
        --iasl-only)  MODE="iasl" ;;
        *) echo "Unknown: $1"; exit 1 ;;
    esac
    shift
done

if [[ $EUID -ne 0 ]]; then
    echo "ERROR: must run as root (sudo $0)"
    exit 1
fi

echo "=== Installing EC exploration tools (mode: $MODE) ==="

# Always install iasl
echo "Installing acpica-tools (iasl)..."
dnf install -y acpica-tools 2>&1 | tail -5
echo "  iasl: $(iasl --version 2>&1 | head -1)"

if [[ "$MODE" == "iasl" ]]; then
    echo "Done (iasl-only mode)."
    exit 0
fi

# Kernel build deps
echo "Installing kernel build dependencies..."
KVER=$(uname -r)
dnf install -y dkms "kernel-devel-$KVER" kernel-headers gcc make 2>&1 | tail -10

if [[ "$MODE" == "lll" ]]; then
    echo ""
    echo "=== Installing LenovoLegionLinux kernel module ==="
    echo "This provides WMI3 fan curve control for 82WM (chip 5507)."
    echo ""

    LLL_DIR=/tmp/LenovoLegionLinux
    if [[ -d "$LLL_DIR" ]]; then
        echo "  Updating existing clone..."
        git -C "$LLL_DIR" pull
    else
        echo "  Cloning LenovoLegionLinux..."
        git clone --depth=1 https://github.com/johnfanv2/LenovoLegionLinux.git "$LLL_DIR"
    fi

    KM_DIR="$LLL_DIR/kernel_module"
    cd "$KM_DIR"

    echo "  Building kernel module (kernel $(uname -r))..."
    make clean 2>&1 | tail -3 || true
    make 2>&1 | tail -20

    if [[ ! -f legion-laptop.ko ]]; then
        echo "ERROR: Build failed — legion-laptop.ko not produced"
        echo "Check kernel compatibility. LLL was tested on 6.x; kernel 7.x may need patches."
        echo "Review $KM_DIR/6_10_patch/ for guidance."
        exit 1
    fi

    echo "  Installing module..."
    make install 2>&1 | tail -5
    depmod -a

    # Unload upstream legion_laptop if loaded (conflicts)
    if lsmod | grep -q '^legion_laptop '; then
        echo "  Removing upstream legion_laptop..."
        rmmod legion_laptop 2>/dev/null || true
    fi

    echo "  Loading LLL module with force=1..."
    modprobe legion-laptop force=1 || insmod legion-laptop.ko force=1 || true
    sleep 1

    echo "  Checking hwmon for legion node..."
    for d in /sys/class/hwmon/hwmon*/; do
        name=$(cat "$d/name" 2>/dev/null || echo "?")
        echo "  $d: $name"
        if [[ "$name" == *legion* ]]; then
            echo "  *** Found legion hwmon: $d ***"
            ls "$d"
        fi
    done

    echo ""
    echo "LLL installed. If legion hwmon appeared, fan data is accessible via:"
    echo "  /sys/class/hwmon/hwmon<N>/fan{1,2}_input  (current RPM)"
    echo "  /sys/class/hwmon/hwmon<N>/temp{1,2,3}_input  (CPU/GPU/IC temps)"
    echo "  /sys/class/hwmon/hwmon<N>/pwm{1,2}  (fan curve points)"

elif [[ "$MODE" == "acpi_call" ]]; then
    echo ""
    echo "=== Installing acpi_call DKMS module ==="

    ACPICALL_DIR=/tmp/acpi_call_src
    if [[ -d "$ACPICALL_DIR" ]]; then
        git -C "$ACPICALL_DIR" pull
    else
        git clone --depth=1 https://github.com/mkottman/acpi_call.git "$ACPICALL_DIR"
    fi

    cd "$ACPICALL_DIR"
    make
    # Install via DKMS if available, else just insmod
    if command -v dkms &>/dev/null; then
        # Copy to DKMS tree
        VER=$(grep '^PACKAGE_VERSION' dkms.conf 2>/dev/null | cut -d= -f2 | tr -d '"' || echo "1.2.2")
        mkdir -p /usr/src/acpi_call-$VER
        cp -r . /usr/src/acpi_call-$VER/
        dkms install acpi_call/$VER 2>&1 | tail -10
    else
        insmod acpi_call.ko
    fi

    modprobe acpi_call || insmod "$ACPICALL_DIR/acpi_call.ko"
    echo "  acpi_call loaded: $(ls /proc/acpi/call 2>/dev/null && echo OK || echo FAILED)"
fi

echo ""
echo "=== Tool installation complete ==="
echo "Next: sudo ./scripts/ec-explore-phase1.sh"
