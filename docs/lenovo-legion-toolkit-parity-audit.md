# Lenovo Legion Toolkit parity audit

Reference source: `LenovoLegionToolkit-Team/LenovoLegionToolkit` at commit `6f19ef4`,
local clone `/tmp/LenovoLegionToolkit`.

This compares LLT library feature classes against RatVantage's implemented Linux
surfaces. Treat LLT as a behavior map, not as a directly portable backend: LLT is
Windows-first and uses Lenovo Energy driver IOCTLs, WMI classes, NVAPI, registry,
and UEFI variables. RatVantage must keep the daemon/polkit/sysfs safety model.

## Public release parity contract

RatVantage 0.1.0 claims parity only for behavior backed by a stable Linux surface on the
running machine. A feature is release-supported when it is discovered dynamically, validated,
policy-gated, read back after writes, recoverable, and covered by fixture or live evidence.
Windows-only vendor methods are not parity gaps that may be filled with raw EC/WMI calls.

- **Supported:** platform and desktop power profiles, battery modes, CPU governor/EPP/boost and
  scaling caps, detected firmware attributes and device toggles, AMD GPU DPM, telemetry,
  evidence-backed OpenRGB keyboard lighting, profiles, automations, diagnostics, and tray actions.
- **Guided or plan-only:** reboot-based GPU switching, unpromoted runtime GPU switching, and fan
  curve planning. The UI must state the evidence or reboot requirement and never imply execution.
- **Unsupported by design:** firmware flashing, arbitrary sysfs/WMI/EC writes, CPU/GPU
  overclocking, and Windows display features without a stable Linux API.

This is safe Linux behavioral parity, not byte-for-byte parity with a Windows vendor utility.

## Backend map

| LLT backend | LLT examples | RatVantage equivalent | Status |
|---|---|---|---|
| Lenovo Energy driver IOCTL | `BatteryFeature`, `AlwaysOnUSBFeature`, `FnLockFeature`, white keyboard backlight | Kernel/platform sysfs when present | Partial; Linux exposes fewer modes for some features. |
| LenovoGameZone WMI | hybrid/iGPU mode, G-Sync, OverDrive, WinKey, touchpad lock | `platform_profile`, optional EnvyControl, probe-only WMI research | Partial; do not add raw WMI writes without Linux driver evidence. |
| LenovoOther capability WMI | generic feature get/set, dGPU notify, InstantBoot | `lenovo-wmi-other` firmware attributes where exposed | Partial/probe-only depending on host. |
| LenovoFan/Cpu/Gpu WMI GodMode | fan table, full-speed fan, CPU/GPU power limits, temperature limits | hwmon fan curves, firmware attributes, Ryzen backends | Partial; fan execution intentionally not shipped. |
| Lenovo lighting WMI / Spectrum devices | RGB keyboard, panel logo, ports backlight | LED sysfs, OpenRGB bridge/SDK, RGB candidates | Partial; native raw RGB still unsupported. |
| Windows/NVIDIA APIs | HDR, refresh/resolution, GPU telemetry/OC, power plan | Desktop services, amdgpu sysfs, optional tools | Out of scope unless a stable Linux API exists. |

## Feature findings

| LLT feature | LLT values/backend | RatVantage status | Action |
|---|---|---|---|
| Battery charge mode | `Conservation`, `Normal`, `RapidCharge` via `IOCTL_ENERGY_BATTERY_CHARGE_MODE` | Implemented as `/sys/class/power_supply/BAT0/charge_types`: `Long_Life`, `Standard`, `Fast`; legacy `conservation_mode` also exists | Prefer `charge_types`. Keep daemon conflict rule that blocks `battery_charge_type` plus `conservation_mode` in one profile. Do not promise exact charge percentages. |
| Always-on USB | `Off`, `OnWhenSleeping`, `OnAlways` via `IOCTL_ENERGY_SETTINGS` | Implemented as ideapad `usb_charging` boolean | Partial. UI/docs should not imply LLT's tri-state unless Linux exposes a tri-state surface. |
| Fn lock | `Off`, `On` via `IOCTL_ENERGY_SETTINGS` | Implemented when ideapad `fn_lock` exists | OK if detected dynamically. |
| Platform/power mode | `Quiet`, `Balance`, `Performance`, `Extreme`, `GodMode`; thermal event values differ by one for extreme/godmode | Implemented as kernel `platform_profile` choices | Mostly OK for standard modes. Missing LLT `Extreme`/`GodMode` unless kernel exposes equivalent choices; avoid hardcoded numeric mapping. |
| Fan full speed / fan table | `LenovoFanMethod` WMI, GodMode controller versions V1-V4 | Probe detects hwmon curves; daemon has dry-run planning only | Correctly conservative. Do not ship fan apply/restore until live readback and rollback evidence exists. |
| CPU/GPU power limits | LLT `LenovoCpuMethod`/`LenovoGpuMethod` WMI GodMode limits | Firmware PPT attributes and AMD/Ryzen backends | Partial. Current firmware attribute writes are only valid when exposed and validated; LLT names should not be copied as guaranteed Linux controls. |
| Hybrid / GPU working mode | `HybridModeState`: `On`, `OnIGPUOnly`, `OnAuto`, `UMA`, `Off`; GameZone/capability/feature-flag backends | Optional EnvyControl wrapper: `integrated`, `hybrid`, `nvidia`; reboot/pending flow | Partial. EnvyControl is not LLT's EC mode API and should remain guided/reboot-aware. |
| dGPU runtime notify/status | LLT DGPU notify backends call capability/feature-flag/GameZone methods | GPU runtime capability/probe-only | Partial. Avoid presenting runtime disconnect as guaranteed. |
| OverDrive | Capability or GameZone OD WMI | Listed as possible WMI/firmware-adjacent feature, not a stable shipped write | Missing or probe-only. Needs Linux surface proof before UI promotion. |
| WinKey / touchpad lock | GameZone WMI toggles | No dedicated capability except any generic platform toggle that may appear | Missing. Add only if a Linux platform attribute exists and validates. |
| Keyboard RGB | Lenovo lighting/Spectrum/driver paths, ownership handoff | RGB candidates plus OpenRGB bridge/SDK gated writes | Partial. This is the right Linux direction; native EC/HID writes remain unsupported. |
| Panel logo / ports backlight | Lenovo lighting or Spectrum | LED sysfs for detected LEDs | Partial. Good when LEDs are discovered dynamically; no trademark/logo assets. |
| White keyboard backlight | Energy driver or Lenovo lighting | LED sysfs/OpenRGB only if detected | Partial. Avoid assuming LLT levels map to Linux brightness values. |
| Instant boot / flip to start | capability/feature-flag/UEFI | No stable Linux control | Missing. Requires UEFI or platform attribute evidence. |
| Battery night charge | `IOCTL_ENERGY_BATTERY_NIGHT_CHARGE` | No equivalent | Missing. |
| HDR, refresh rate, resolution, DPI | Windows display APIs | KDE KScreen refresh automation for the internal panel; resolution/DPI/HDR remain desktop-owned | Partial. Refresh changes run unprivileged in the tray with read-back and rollback; the root daemon remains out of scope. |
| GPU overclock / NVIDIA telemetry | LLT NVAPI/GameZone support checks | AMD DPM force level, no NVIDIA OC | Mostly missing by design; do not add unsafe OC paths. |

## Likely corrections

1. Keep battery charge mode centered on `charge_types`; treat legacy
   `conservation_mode` as fallback/older-kernel support, not a second independent
   battery-care feature.
2. Audit any UI copy for always-on USB. RatVantage has a boolean Linux toggle,
   while LLT has three states.
3. Keep fan curve execution and WMI/GodMode power-limit writes unpromoted until
   the Linux backend has readback, validation, explicit daemon flags, and rollback.
4. Add missing parity entries to the roadmap as probe-only candidates, not UI
   promises: OverDrive, WinKey lock, touchpad lock, InstantBoot/flip-to-start,
   battery night charge, and richer keyboard/logo/ports lighting.
