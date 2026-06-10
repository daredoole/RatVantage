#!/usr/bin/env bash
# Build and load acpi_call.ko patched for kernel >= 5.6 / >= 7.0
# Patches: <acpi/acpi.h> → <linux/acpi.h>, file_operations → proc_ops
#
# Usage: sudo ./scripts/acpi-call-patch-and-build.sh
# After: /proc/acpi/call is available for WMI probing

set -euo pipefail

SRC=/tmp/acpi_call_src

if [[ ! -d "$SRC" ]]; then
    echo "Cloning acpi_call..."
    git clone --depth=1 https://github.com/mkottman/acpi_call "$SRC"
fi

cd "$SRC"

echo "Patching for kernel >= 5.6..."
# Replace old ACPI header
sed -i 's|#include <acpi/acpi.h>|#include <linux/acpi.h>|' acpi_call.c
# Replace file_operations with proc_ops
sed -i 's/static struct file_operations proc_acpi_operations/static struct proc_ops proc_acpi_operations/' acpi_call.c
sed -i '/proc_acpi_operations = {/,/};/{
    s/\.owner[[:space:]]*=[[:space:]]*THIS_MODULE,//
    s/\.read[[:space:]]*=/.proc_read =/
    s/\.write[[:space:]]*=/.proc_write =/
}' acpi_call.c

echo "Building..."
make

echo "Loading module..."
if lsmod | grep -q '^acpi_call '; then
    sudo rmmod acpi_call
fi
sudo insmod acpi_call.ko

if [[ -f /proc/acpi/call ]]; then
    echo "acpi_call loaded: /proc/acpi/call ready"
else
    echo "ERROR: /proc/acpi/call not created"
    exit 1
fi

echo ""
echo "Quick sensor test (82WM):"
echo "  Fan 1 RPM:"
echo '\_SB.GZFD.WMB5 0x00 0x11 0x04030001' | sudo tee /proc/acpi/call > /dev/null
RPM=$(sudo cat /proc/acpi/call)
echo "    Raw: $RPM"
python3 -c "
import re
m = re.search(r'0x([0-9A-Fa-f]+)', '$RPM')
if m: print(f'    fan1 = {int(m.group(1),16)} RPM')
" 2>/dev/null || true

echo "  CPU temp:"
echo '\_SB.GZFD.WMB5 0x00 0x11 0x05040000' | sudo tee /proc/acpi/call > /dev/null
TEMP=$(sudo cat /proc/acpi/call)
python3 -c "
import re
m = re.search(r'0x([0-9A-Fa-f]+)', '$TEMP')
if m: print(f'    cpu = {int(m.group(1),16)}°C')
" 2>/dev/null || true
