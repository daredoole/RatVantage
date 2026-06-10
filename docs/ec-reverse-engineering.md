# EC Reverse Engineering — Legion Pro 5 16ARX8 (82WM)

## Hardware summary

| Item | Value |
|------|-------|
| Model | Lenovo Legion Pro 5 16ARX8 (82WM) |
| EC chip | ITE IT5507 (ID `0x5507`) |
| Fan/temp access | **WMI3** — not raw EC registers |
| Fan WMI GUID | `92549549-4BDE-4F06-AC04-CE8BF898DBAA` |
| ACPI method object | `_WMB2` |
| **Confirmed ACPI path** | `\_SB.GZFD.WMB2` (from DSDT line 26116) |
| ERAX region | `0xFE0B0400`, size `0xFF` (CPUT@0xB0, GPUT@0xB4, EMOD@0x4B) |
| Fan speed table | `0xFE0B09F0`, 10 bytes (F9FT region, written by WMB2 SET) |
| Super I/O ports | `0x4E / 0x4F` (chip ID, config registers) |

## Key discovery: chip 5507 ≠ chip 8227

Older Legion models (2021-2022) use EC chip **0x8227** and directly expose fan curves via EC RAM registers (`ACCESS_METHOD_EC`). The 82WM uses **0x5507** and routes all fan/temperature access through ACPI WMI methods (`ACCESS_METHOD_WMI3`). Raw EC RAM reads still work but the fan curve table itself lives in firmware, not in a directly-writable EC region.

## WMI interfaces (confirmed working on 82WM)

### WMB2 — `92549549-4BDE-4F06-AC04-CE8BF898DBAA` (ACPI: `\_SB.GZFD.WMB2`)

Call: `echo '\_SB.GZFD.WMB2 0x00 <method_id> 0x00' > /proc/acpi/call`

| Method ID | Direction | Description | Confirmed |
|-----------|-----------|-------------|-----------|
| 1 | GET | Fan full-speed (dust cleaning) state | - |
| 2 | SET | Enable/disable full-speed — **do NOT call** | - |
| 3 | GET | Max fan speed | - |
| 4 | SET | Max fan speed — **do NOT call** | - |
| 5 | GET | Fan curve table — returns 48-byte `WMIFanTableRead` | ✓ |
| 6 | SET | Fan curve table — **do NOT call** | - |
| 7 | GET | Returns 0 on WMI3 models (use WMB5 instead) | ✓ |
| 8 | GET | Returns 0 on WMI3 models (use WMB5 instead) | ✓ |

**Method 5 response** (`WMIFanTableRead`, 48 bytes):
- `FSFL` [u32, bytes 0-3]: flags/level
- `FSS0`–`FSS9` [u32 each, bytes 4-43]: fan speed % per curve point
- All zeros when in automatic mode (custom curve not active)

### WMB5 — `DC2A8805-3A8C-41BA-A6F7-092E0089CD3B` (ACPI: `\_SB.GZFD.WMB5`)

Call: `echo '\_SB.GZFD.WMB5 0x00 0x11 <feature_id>' > /proc/acpi/call`

Method ID 0x11 = GET_FEATURE_VALUE, 0x12 = SET_FEATURE_VALUE.

| Feature ID | EC register | Returns | Confirmed reading |
|------------|-------------|---------|-------------------|
| `0x04030001` | `EC0.FANS` | Fan 1 RPM = `FANS × 100` | **1700 RPM** (0x6a4) |
| `0x04030002` | `EC0.FA2S` | Fan 2 RPM = `FA2S × 100` | **1500 RPM** (0x5dc) |
| `0x05040000` | `EC0.CPUT` | CPU temperature °C | **48°C** (0x30) |
| `0x05050000` | `EC0.GPUT` | GPU temperature °C | **0°C** (power-gated) |
| `0x05010000` | `EC0.CPUS` | CPU speed sensor | - |
| `0x04020000` | `EC0.FNST` | Fan status flags | - |
| `0x00030000` | `EC0.FLBT` | Fan lock / battery throttle bit | - |
| `0x03010001` | `EC0.EACS` | AC cooling switch state | - |
| `0x03010002` | `EC0.ETCS` | Thermal cooling switch state | - |

### Undocumented WMB2 extended method IDs (from DSDT analysis)

These Arg2 values are dispatched inside `WMB2` beyond the LLL-documented IDs 1-8:

| Arg2 value | Description |
|------------|-------------|
| `0x03010001` | Read `EACS` (AC cooling switch state) |
| `0x03010002` | Read `ETCS` (thermal cooling switch state) |
| `0x00030000` | Read `FLBT` (fan lock / battery throttle bit) |
| `0x03030000` | Read AC power state |
| `0x02080000` | PPT (processor power target) calculation |

## ERAX region register map (confirmed from DSDT disassembly)

`OperationRegion (ERAX, SystemMemory, 0xFE0B0400, 0xFF)` — 255 bytes.
Physical address = `0xFE0B0400 + byte_offset`.

### Sensor / thermal registers

| Physical addr | Byte off | Name | Bits | Description |
|---------------|----------|------|------|-------------|
| `0xFE0B04B0`  | `0xB0`   | `CPUT` | 8 | CPU temperature (°C) |
| `0xFE0B04B1`  | `0xB1`   | `CPUS` | 8 | CPU sensor (speed or secondary temp) |
| `0xFE0B04B2`  | `0xB2`   | `PCHS` | 8 | PCH sensor |
| `0xFE0B04B3`  | `0xB3`   | `GPUS` | 8 | GPU sensor (secondary) |
| `0xFE0B04B4`  | `0xB4`   | `GPUT` | 8 | GPU temperature (°C) |
| `0xFE0B04B5`  | `0xB5`   | `SSDS` | 8 | SSD sensor (probably temp °C) |
| `0xFE0B04B6`  | `0xB6`   | `PCHT` | 8 | PCH temperature |
| `0xFE0B04A9`  | `0xA9`   | `THRT` | 8 | Thermal throttle level |

### Fan / power mode registers

| Physical addr | Byte off | Name | Bits | Description |
|---------------|----------|------|------|-------------|
| `0xFE0B0406`  | `0x06`   | `FANS` | 8 | Fan status flags |
| `0xFE0B044B`  | `0x4B`   | `EMOD` | 8 | EC power mode (0=balance, 1=performance, 2=quiet, …) |
| `0xFE0B048D`  | `0x8D`   | `FLBT` | 1 (bit 0) | Fan lock / battery throttle |
| `0xFE0B04FE`  | `0xFE`   | `FA2S` | 8 | Fan 2 status |

### Cooling control flags (all in byte `0x44` = `0xFE0B0444`)

| Bit | Name | Description |
|-----|------|-------------|
| bit 3 | `ACPD` | AC power detection |
| bit 4 | `SACS` | Smart AC cooling switch |
| bit 5 | `EACS` | EC AC cooling switch |
| bit 6 | `STCS` | Smart thermal cooling switch |
| bit 7 | `ETCS` | EC thermal cooling switch |

### Keyboard backlight / RGB

| Physical addr | Byte off | Name | Bits | Description |
|---------------|----------|------|------|-------------|
| `0xFE0B0423`  | `0x23`   | `RGBS` | 1 (bit 0) | RGB status flag |
| `0xFE0B0423`  | `0x23`   | `KBLT` | 1 (bit 1) | Keyboard backlight toggle |
| `0xFE0B0445`  | `0x45`   | `KBGC` | 32 | Keyboard backlight color (ARGB or similar) |
| `0xFE0B042C`  | `0x2C`   | `KBSS` | 32 | Keyboard backlight setting |
| `0xFE0B0026`  | `0x26`   | `KBGS` | 32 | Keyboard backlight get state |

### Battery registers (selection)

| Physical addr | Byte off | Name | Bits | Description |
|---------------|----------|------|------|-------------|
| `0xFE0B04C2`  | `0xC2`   | `B1RC` | 16 | Battery remaining capacity |
| `0xFE0B04C4`  | `0xC4`   | `B1SN` | 16 | Battery identifier field |
| `0xFE0B04C6`  | `0xC6`   | `B1FV` | 16 | Battery full capacity |
| `0xFE0B04C8`  | `0xC8`   | `B1DV` | 16 | Battery design voltage |
| `0xFE0B04CA`  | `0xCA`   | `B1DC` | 16 | Battery design capacity |
| `0xFE0B04D0`  | `0xD0`   | `B1CR` | 16 | Battery charge rate |
| `0xFE0B04D6`  | `0xD6`   | `B1TM` | 8  | Battery temperature |

### Fan speed table (F9FT region)

`OperationRegion (F9FT, SystemMemory, 0xFE0B09F0, 0x20)` — separate region.

| Physical addr | Name | Description |
|---------------|------|-------------|
| `0xFE0B09F0`  | `F9F0` | Fan speed setting 0 |
| `0xFE0B09F1`  | `F9F1` | Fan speed setting 1 |
| … | … | … |
| `0xFE0B09F9`  | `F9F9` | Fan speed setting 9 |

WMB2 method writes these 10 bytes then triggers EC with `NCMD(0x8C, Zero)`.

### LLL register map (chip 8227 / legacy — may not apply to 5507)

For reference: the old `ec_register_offsets_v0` used by LLL for chip 8227:

```
EC reg   Offset  Name
0xC534   0x134   EXT_FAN_CUR_POINT        current fan curve step index
0xC535   0x135   EXT_FAN_POINTS_SIZE      number of active curve points (≤10)
0xC540   0x140   EXT_FAN1_BASE[0..9]      fan1 speed% per curve point
0xC550   0x150   EXT_FAN2_BASE[0..9]      fan2 speed% per curve point
0xC560   0x160   EXT_FAN_ACC_BASE[0..9]   acceleration per point
0xC570   0x170   EXT_FAN_DEC_BASE[0..9]   deceleration per point
0xC580   0x180   EXT_CPU_TEMP[0..9]       CPU temp threshold (max) per point
0xC590   0x190   EXT_CPU_TEMP_HYST[0..9]  CPU temp threshold (min/hyst)
0xC5A0   0x1A0   EXT_GPU_TEMP[0..9]       GPU temp threshold (max)
0xC5B0   0x1B0   EXT_GPU_TEMP_HYST[0..9]  GPU temp threshold (min/hyst)
```

For chip 5507 (82WM), these offsets are **different** — use ERAX table above.

These may exist at different offsets for chip 5507 than for 8227. The physical EC RAM dump + snapshot diffing under load is how we find them.

## Active kernel modules

| Module | WMI GUID | Role |
|--------|----------|------|
| `lenovo_wmi_gamezone` | `887B54E3-DDDC-4B2C-8B88-68A26A8835D0` | Platform profile switching |
| `lenovo_wmi_other` | `887B54E2-DDDC-4B2C-8B88-68A26A8835D0` | obj=A1, 2 instances, setable=0 |
| `legion_laptop` (upstream) | — | Loaded but no platform device (82WM not in upstream match table) |
| — | `92549549-4BDE-4F06-AC04-CE8BF898DBAA` | Fan WMI — **unbound**, needs LLL or acpi_call |

## Exploration workflow

### Prerequisites

```bash
# Mandatory for ACPI disassembly
sudo dnf install acpica-tools

# For full fan curve support (recommended)
sudo ./scripts/ec-explore-install-tools.sh --lll

# Or just raw WMI probing
sudo ./scripts/ec-explore-install-tools.sh --acpi-call
```

### Phase 1 — Read-only EC dump + ACPI tables

```bash
# Single snapshot
sudo ./scripts/ec-explore-phase1.sh

# 5 snapshots 5s apart (good for spotting live registers under load)
sudo ./scripts/ec-explore-phase1.sh --repeat 5
```

Outputs to `target/ec-exploration/phase1-<timestamp>/`:
- `snapshot-N/ec_io.hex` — 256-byte ACPI EC I/O space
- `ec_phys_ram.hex` — 0x600 bytes of physical EC RAM (if `/dev/mem` accessible)
- `acpi/dsl/DSDT.dsl` — disassembled DSDT (contains WMI method bodies)
- `wmi_inventory.txt` — all WMI GUIDs with drivers and object IDs
- `fan_wmi_acpi_hits.txt` — grep hits for fan WMI GUID in ACPI

### Phase 2 — WMI method probing (needs acpi_call or LLL)

```bash
sudo ./scripts/ec-explore-phase2-wmi.sh
```

Reads live fan RPM and temps via WMB5, fan curve table via WMB2. All paths confirmed; no pre-step needed.

Requires acpi_call module:
```bash
git clone --depth=1 https://github.com/mkottman/acpi_call /tmp/acpi_call_src
cd /tmp/acpi_call_src
# Patch for kernel ≥ 5.6 / ≥ 7.0:
sed -i 's|#include <acpi/acpi.h>|#include <linux/acpi.h>|' acpi_call.c
sed -i 's|static struct file_operations|static struct proc_ops|; s|\.owner.*THIS_MODULE,||; s|\.read\s*=|.proc_read =|; s|\.write\s*=|.proc_write =|' acpi_call.c
make && sudo insmod acpi_call.ko
```

### Phase 3 — Analysis

```bash
./scripts/ec-explore-phase3-analyze.sh   # uses latest phase1 dir
./scripts/ec-explore-phase3-analyze.sh target/ec-exploration/phase1-20240101-120000
```

### Identifying register purpose via diff

Run phase1 under different conditions and compare:
```bash
# Snapshot at idle
sudo ./scripts/ec-explore-phase1.sh --repeat 1   # baseline

# Then under CPU+GPU load (run a game / stress test)
sudo ./scripts/ec-explore-phase1.sh --repeat 5

# Diff
./scripts/ec-explore-phase3-analyze.sh   # shows changing offsets
```
Changing offsets under load are almost certainly temperature or fan speed registers.

## Integration path into RatVantage

### Confirmed working (ready to implement)

1. **Fan 1 RPM**: `\_SB.GZFD.WMB5 0x00 0x11 0x04030001` → integer RPM directly
2. **Fan 2 RPM**: `\_SB.GZFD.WMB5 0x00 0x11 0x04030002` → integer RPM directly
3. **CPU temp**: `\_SB.GZFD.WMB5 0x00 0x11 0x05040000` → integer °C
4. **GPU temp**: `\_SB.GZFD.WMB5 0x00 0x11 0x05050000` → integer °C (0 when power-gated)
5. **Fan curve read**: `\_SB.GZFD.WMB2 0x00 0x05 0x00` → 48-byte `WMIFanTableRead`

Implementation note: these calls require the `legion_laptop` kernel module (LLL) to be loaded and bound. On 82WM with kernel 7.0.11, LLL fails to create its platform device — use the `legion-probe` crate to call WMI methods directly via the kernel WMI sysfs interface or via a small Rust shim that calls `wmi_evaluate_method`.

### Not yet integrated (blocked)

6. **Fan curve write**: WMB2 method 6 — blocked until rollback + live validation evidence per AGENTS.md
7. **Power mode set**: WMB5 feature 0x12 path — blocked until daemon gate
8. **GPU dGPU control**: GPUT=0 when power-gated; need wake confirmation

### LLL probe failure root cause (kernel 7.0.11)

`legion_laptop` DKMS module (LLL) loads but `platform_device_register_simple("legion", -1, NULL, 0)` fails silently on Fedora 44 kernel 7.0.11 — no hwmon nodes created. Root cause unknown; likely naming conflict or kernel API change. Fan/temp access via `acpi_call` + direct WMI calls bypasses this entirely.

## Safety

- All exploration scripts are **read-only** (no WMI writes, no EC writes)
- `/dev/mem` access is read-only (`O_RDONLY`)
- `ec_sys` loaded with `write_support=0`
- Phase 2 only calls GET methods (1, 3, 5, 7, 8), never SET (2, 4, 6)
