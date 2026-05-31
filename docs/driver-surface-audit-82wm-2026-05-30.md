# Driver surface audit: 82WM Legion Pro 5 16ARX8

Date: 2026-05-30

Scope: compare the live kernel/device-driver surface on this machine with what RatVantage currently detects, exposes through the daemon, and facilitates in the GTK/tray UX.

Important caveat: this audit was run against the current working tree, which already has many uncommitted changes. Treat it as a current-local-state audit, not a released-version statement.

## Host

- Hardware: `82WM`, `Legion Pro 5 16ARX8`
- Kernel: `7.0.10-201.fc44.x86_64`
- Loaded relevant modules: `legion_laptop`, `ideapad_laptop`, `platform_profile`, `amdgpu`, `nvidia`
- Not loaded: `amd_pstate` as a module, though CPU frequency policy reports `amd-pstate-epp`

## Summary

RatVantage is not yet an exhaustive GUI for every tuning knob exposed by this kernel and device driver stack.

It does have a broad read-only probe and a conservative write model. It detects most of the visible live surfaces on this 82WM: platform profiles, battery charge type, battery telemetry, ideapad toggles, LEDs, Lenovo WMI firmware attributes, hwmon/thermal sensors, CPU governor/EPP, AMD GPU DPM state, and EnvyControl GPU mode.

The real write UX is narrower:

- Strongest coverage: platform profile, battery charge type, CPU governor, CPU EPP, selected LEDs, `fn_lock`, `camera_power`, `usb_charging`.
- Partial or still-validation-needed coverage: `fan_mode` appears wired in the current tree, but it is not part of the older handoff's listed reversible write set.
- Read-only or planning-only: Lenovo WMI firmware power limits, AMD GPU DPM controls, EnvyControl GPU mode execution, fan curves, automations/settings persistence.
- Not currently facilitated as a first-class control: `conservation_mode`.

## Coverage matrix

| Surface | Live driver/sysfs exposure | RatVantage probe | Daemon write | GUI/tray facilitation | Gap |
|---|---|---:|---:|---:|---|
| Platform profile | `/sys/firmware/acpi/platform_profile`, current `low-power`, choices `low-power balanced performance max-power custom`, writable mode `644` | Yes | Yes, gated `SetPlatformProfile` | Yes, GTK Profiles and tray | Mostly covered. `custom` profile sub-knobs are not modeled as a profile-specific editor. |
| Battery charge type | `/sys/class/power_supply/BAT0/charge_types`, current parsed from `[Long_Life]`, choices `Fast Standard Long_Life`; no separate `charge_type` file | Yes | Yes, gated `SetBatteryChargeType` | Yes, GTK Battery and tray | Covered, but implementation depends on bracketed `charge_types` semantics on this model. |
| Battery telemetry | capacity/status/health/cycle/energy/voltage/manufacturer/model files | Yes | No, correctly read-only | Yes, read-only | Covered as telemetry. |
| CPU governor | `amd-pstate-epp`, current governor `powersave`, choices `performance powersave` | Yes | Yes, gated `SetCpuGovernor` | Yes, GTK Profiles | Covered. |
| CPU EPP | current `power`, choices `default performance balance_performance balance_power power` | Yes | Yes, gated `SetCpuEpp` | Yes, GTK Profiles | Covered. |
| CPU boost | `/sys/devices/system/cpu/cpufreq/boost`, current `1` | Yes as read-only field | No | Read-only only | Missing write validator/rollback if this should become a control. |
| Ideapad `fn_lock` | `/sys/bus/platform/drivers/ideapad_acpi/VPC2004:00/fn_lock`, current `1`, writable mode `644` | Yes | Yes, restricted `SetIdeapadToggle` | Yes, appearance/tray | Covered. |
| Ideapad `camera_power` | current `0`, writable mode `644` | Yes | Yes, warning-gated | Yes, dashboard confirmation | Covered with appropriate warning UX. |
| Ideapad `usb_charging` | current `0`, writable mode `644` | Yes | Yes, warning-gated | Yes, dashboard confirmation | Covered with appropriate warning UX. |
| Ideapad `fan_mode` | current `0`, writable mode `644` | Yes | Current tree has policy flag and handler path | Current tree shows Fan Mode controls | Needs live execute evidence and documentation alignment before treating as fully supported. |
| Ideapad `conservation_mode` | current `1`, writable mode `644` | Yes | No explicit policy allowance found | No first-class control | Gap. Likely should map either to Battery or Appearance with validator/readback. |
| Lenovo WMI PPT: `ppt_pl1_spl` | current `70`, range `50..85`, step `1`, writable `current_value` | Yes | No | Read-only group only | Major gap. Needs range validator, polkit action, readback, rollback, UI slider/spin row. |
| Lenovo WMI PPT: `ppt_pl2_sppt` | current `85`, range `60..130`, step `1`, writable `current_value` | Yes | No | Read-only group only | Major gap. Same as above. |
| Lenovo WMI PPT: `ppt_pl3_fppt` | current `102`, range `70..150`, step `1`, writable `current_value` | Yes | No | Read-only group only | Major gap. Same as above. |
| AMD GPU DPM force level | `card1/device/power_dpm_force_performance_level`, current `auto`; RatVantage models choices `auto`, `low` | Yes | No | Read-only/status only | Gap. Needs safe choice discovery, validator, and confirmation model. |
| AMD GPU clocks | `pp_dpm_sclk`, `pp_dpm_mclk`; current selected SCLK `600Mhz`, MCLK `1600Mhz` | Yes, read-only selected clocks | No | Read-only only | Correctly read-only for now. Manual clock writes are not modeled. |
| EnvyControl GPU mode | provider `envycontrol`, current `hybrid` | Yes | No direct execution; pending state only | GTK GPU planning and pending reboot | Intentionally planning-only by policy. |
| Fan curves | No live `pwm*_auto_point*` files on this host | Missing | No execution | GTK Fans scratchpad, live/saved read-only, dry-run plans | No live driver surface to expose here. Execution remains disabled by policy. |
| Fan RPM sensors | No `fan*_input` surfaced in live hwmon audit | No useful live fan telemetry on this host | No | Read-only when present | Hardware/driver currently does not expose fan RPM here. |
| Thermal/hwmon temps | NVMe, Wi-Fi, AMDGPU edge, k10temp, SPD temps | Yes | No | Read-only status/diagnostics | Covered as telemetry. |
| LEDs | Many input/network LEDs plus `platform::fnlock`; no `platform::ylogo` on this host | Yes | Generic LED write path exists | UI focuses relevant LEDs | Generic LEDs are detected, but UI should keep filtering to device-relevant controls. |
| Desktop power profiles | D-Bus PowerProfiles probe | Yes | GUI can call PPD directly | GTK Profiles desktop section | Separate from kernel sysfs; covered as desktop integration. |
| Automations | App-level concept only | N/A | No engine | Planning-only UI | Not implemented. |
| Settings persistence | App-level concept only | N/A | Partial state for GPU pending/fan snapshot/map | Mostly planning/status | Not a driver gap, but UX implies more than persists today. |

## Highest-value gaps

1. **Firmware power-limit controls**

   The kernel exposes three Lenovo WMI firmware attributes with complete metadata:

   - `ppt_pl1_spl`: sustained CPU power limit, current `70`, range `50..85`
   - `ppt_pl2_sppt`: slow package power tracking limit, current `85`, range `60..130`
   - `ppt_pl3_fppt`: fast package power tracking limit, current `102`, range `70..150`

   RatVantage already detects these. The missing work is a typed write contract: range validator, daemon writer, polkit action, readback, rollback-on-mismatch, live execute evidence, and GTK controls. This is the clearest "driver exposes tuning but RatVantage does not facilitate it" gap.

2. **Battery conservation mode**

   `conservation_mode` is exposed by `ideapad_acpi` and currently reads `1`. RatVantage detects it, but it is not a first-class write target. Since this overlaps conceptually with battery health/charge behavior, it should be evaluated carefully against `BAT0/charge_types` before exposing both.

3. **Fan mode validation**

   `fan_mode` is exposed by `ideapad_acpi` and current local UI strings show `Auto (0)` / `Full speed (1)`. Before calling it fully supported, the project should add explicit roadmap/handoff coverage plus fixture and live execute evidence. It is a smaller surface than full fan curves but still thermal behavior.

4. **AMD GPU DPM**

   The driver exposes DPM force/state and clock tables. RatVantage reads the surface but does not provide write controls. This should remain lower priority than firmware PPT because GPU power controls have higher regressions and vendor/driver variance.

5. **CPU boost**

   `boost` is detected read-only and is writable on many systems. It is a plausible control, but it needs policy and rollback. It is less device-specific than Lenovo WMI PPT and may belong behind an "advanced CPU" grouping.

## Recommended implementation order

1. Add a generic firmware-attribute write contract for scalar integer attributes, but allowlist only the three observed PPT IDs for 82WM at first.
2. Add GTK controls for those PPT attributes in Profiles as numeric spin rows/sliders using min/max/step metadata.
3. Add fixture coverage for writable `current_value`, rejected out-of-range writes, readback mismatch rollback, and missing metadata.
4. Add a live write-validation harness slice for one PPT attribute at a time.
5. Decide whether `conservation_mode` is redundant with `charge_types=Long_Life` on this platform before exposing it.
6. Promote `fan_mode` only after live evidence and docs catch up.
7. Leave EnvyControl GPU switching and full fan-curve writes planning-only until the existing high-risk policy checklist is satisfied.

## Bottom line

RatVantage is close to broad read-only coverage for this machine, but it is not close to exhaustive write/control coverage. The most important missing GUI/write surface is not full fan curves. It is the Lenovo WMI firmware power-limit attributes that the kernel already exposes with min/max/step metadata.
