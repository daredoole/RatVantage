# Hardware Control Matrix

Status terms:

- **Confirmed on 82WM:** explicitly reported present by the provided runtime scan.
- **Probe-only:** documented or plausible, but the app must hide it unless the path exists and passes read/write validation on the running system.
- **Unsupported:** do not expose without a future stable ABI and separate safety review.

| Feature name | Path or command | Read values / write values | Root required / reboot required | Status | Fedora 43 notes | Risk notes | Suggested UI placement |
|---|---|---|---|---|---|---|---|
| Battery charge type | `/sys/class/power_supply/BAT0/charge_types` | Read choices. Confirmed: `Fast`, `Standard`, `Long_Life`. Write one exact choice. | Write: yes through daemon. Reboot: no. | Confirmed on 82WM | Prefer this over legacy `conservation_mode` on current kernels. TLP 1.10 also maps Lenovo non-ThinkPad battery care to `charge_types`. | Do not invent thresholds. `Long_Life` is a charge algorithm/mode, not a precise user-visible percentage. | Tray quick menu + Battery page |
| Battery status/capacity/health | `/sys/class/power_supply/BAT0/{status,capacity,health,cycle_count,energy_*}` if present | Read-only telemetry. | Root: no. Reboot: no. | Confirmed generally through power_supply; exact fields probe-only | Use kernel power_supply units. Missing optional attributes are normal. | Avoid confusing charge and energy units. | Overview + Battery page |
| Lenovo platform profile | `/sys/firmware/acpi/platform_profile`, `/sys/firmware/acpi/platform_profile_choices` | Confirmed choices: `quiet`, `balanced`, `balanced-performance`, `performance`. Write exact listed value only. | Write: yes through daemon. Reboot: no. | Confirmed on 82WM | Treat this as the authoritative Lenovo thermal/performance mode. Read choices every start and after resume. | Mode changes can reset fan behavior. Re-read fan curve after profile changes. | Tray quick menu + Profiles page |
| Generic desktop power profile | `powerprofilesctl get/list/set`; D-Bus `org.freedesktop.UPower.PowerProfiles` | Usually `power-saver`, `balanced`, `performance`. | Usually no direct root prompt; daemon-managed. Reboot: no. | Confirmed wrapper | Detect the current D-Bus owner. Do not assume power-profiles-daemon if TLP/tuned owns the API. | Can conflict with direct platform profile writes if multiple daemons fight. | Status in Profiles page; optional sync setting |
| Fan RPM telemetry | `/sys/class/hwmon/hwmon*/fan*_input` matched by `name`/labels | RPM integers. | Root: no. Reboot: no. | Confirmed on 82WM | Prefer direct hwmon reads over parsing `sensors` for daemon logic. | Labels vary; detect and map once, then re-probe on failure. | Overview + tray tooltip |
| Temperature telemetry | `/sys/class/hwmon/hwmon*/temp*_input`, labels, `sensors` for debug | Millidegree Celsius integers. | Root: no. Reboot: no. | Confirmed on 82WM | Use `lm_sensors` as optional debug aid, not core dependency for readings. | Apply plausibility filtering; ignore impossible single-sample spikes. | Overview graphs/readouts |
| Custom fan curve presets | Legion hwmon nodes such as `pwm*_auto_point*_pwm` and matching point files | Confirmed 10-point writable curve. Write complete validated curve only. | Write: yes through daemon. Reboot: no, but may reset after profile/suspend/reboot. | Confirmed on 82WM | Probe exact hwmon path by driver/name; never hardcode `hwmonN`. | Highest MVP risk. Must use clamps, full-curve validation, last-known-good, restore-auto action. | Tray fan presets + Fan Curves page |
| Manual fan curve editor | Same hwmon fan curve nodes | User edits points; daemon validates full curve before write. | Write: yes. Reboot: no. | Confirmed backend; editor deferred | Build after presets are safe. | Bad curves can cause noise, overheating, or firmware rejection. | Fan Curves page only, not tray |
| Restore auto fan behavior | Driver-specific auto/default controls if exposed; otherwise re-apply safe preset | Write known-safe value or preset. | Write: yes. Reboot: no. | Probe-only exact method | Always expose if fan curve control is exposed. | Must not rely on unknown default files. Store initial boot curve if trustworthy; otherwise use conservative preset. | Fan Curves page + emergency tray action |
| Fan target RPM | `fanX_target` if exposed by hwmon | Read/write target RPM. `0` may mean auto. | Write: yes. Reboot: no. | Probe-only | Kernel Lenovo WMI Other docs describe `fanX_target` when exposed. | More dangerous than curve presets; can fight firmware. | Advanced only |
| Fan min/max RPM | `fanX_min`, `fanX_max` if exposed | Read-only limits. | Root: no. Reboot: no. | Probe-only | Use for validation if present. | Absence is normal. | Advanced diagnostics |
| CPU PPT/SPL | `/sys/class/firmware-attributes/lenovo-wmi-other/attributes/ppt_pl1_spl/current_value` | Integer bounded by sibling `min_value`, `max_value`, `scalar_increment`. | Write: yes. Reboot: usually no, but firmware may have pending state. | Probe-only | Requires `lenovo-wmi-other` attribute directory. Must be effective only in `custom` profile per kernel docs. | Can overheat or throttle if abused. Clamp strictly. | Advanced / Experimental |
| CPU SPPT | `/sys/class/firmware-attributes/lenovo-wmi-other/attributes/ppt_pl2_sppt/current_value` | Integer with metadata. | Write: yes. Reboot: usually no. | Probe-only | Same as above. | Medium-duration boost risk. | Advanced / Experimental |
| CPU FPPT | `/sys/class/firmware-attributes/lenovo-wmi-other/attributes/ppt_pl3_fppt/current_value` | Integer with metadata. | Write: yes. Reboot: usually no. | Probe-only | Same as above. | Highest PPT risk. Strong confirmation required. | Advanced / Experimental with warning |
| Firmware attribute metadata | `default_value`, `display_name`, `min_value`, `max_value`, `scalar_increment`, `type` | Read-only metadata. | Root: no. Reboot: no. | Probe-only | Required before showing firmware sliders. | Missing metadata means hide the attribute. | Advanced diagnostics |
| Firmware pending/save state | `/sys/class/firmware-attributes/*/{pending_reboot,save_settings}` if present | Driver-defined. | Write may need root. Reboot may be yes. | Probe-only | Treat as firmware-attributes class behavior, not Lenovo-specific guarantee. | Never claim success before read-back and pending-state check. | Advanced diagnostics |
| `custom` platform profile | Listed value in `platform_profile_choices` | Write `custom` only if listed. | Write: yes. Reboot: no. | Probe-only on this scan | Needed for Lenovo PPT settings to matter. | Do not synthesize `custom`. | Advanced / profile banner |
| `max-power` / Extreme profile | Listed value in `platform_profile_choices` | Write `max-power` only if listed. | Write: yes. Reboot: no. | Probe-only / likely absent | Kernel docs warn some BIOS Extreme Mode implementations are incomplete. | High power and undefined-behavior risk. Hide unless listed and tested. | Not shown by default |
| Y-logo LED | `/sys/class/leds/platform::ylogo/brightness` | `0`, `1`. | Write: usually yes through daemon. Reboot: no. | Confirmed on 82WM | LED sysfs names can vary; probe by exact LED name. | Low risk. | Tray toggle + Appearance page |
| Fn-lock LED indicator | `/sys/class/leds/platform::fnlock/brightness` | `0`, `1`. | Write may require root; but treat as indicator first. Reboot: no. | Confirmed indicator on 82WM | Do not assume this changes functional Fn-lock behavior. | Cosmetic/indicator mismatch risk. | Overview / Appearance page |
| Functional Fn-lock | `/sys/bus/platform/devices/VPC2004:*/fn_lock` | `0`, `1`. | Write: yes. Reboot: no. | Implemented gated write | ideapad_laptop ABI documents it when firmware exposes it. | Changes key behavior; require paired `platform::fnlock` LED corroboration. | Appearance / Keyboard quick apply |
| Touchpad hardware toggle | `/sys/bus/platform/devices/VPC2004:*/touchpad` | `0`, `1`. | Write: yes. Reboot: no. | Probe-only | Desktop settings usually handle touchpad better. | Can remove pointer input until re-enabled. | Advanced / Peripherals |
| Camera power | `/sys/bus/platform/devices/VPC2004:*/camera_power` | `0`, `1`. | Write: yes. Reboot: no. | Implemented gated write | ideapad_laptop ABI says `1` on, `0` off. | Active camera apps may crash or lose device; require explicit dashboard confirmation and recovery guidance. | Appearance / Privacy with confirmation |
| Always-on USB charging | `/sys/bus/platform/devices/VPC2004:*/usb_charging` | Usually `0`, `1`. | Write: yes. Reboot: no. | Probe-only | Feature charges USB devices while computer is off. | Can drain laptop battery when shut down. | Battery / Peripherals |
| Legacy conservation mode | `/sys/bus/platform/devices/VPC2004:*/conservation_mode` | `0`, `1`. | Write: yes. Reboot: no. | Probe-only compatibility | Prefer `charge_types` when present. | Avoid double-controlling battery mode through two APIs. | Hidden compatibility diagnostic |
| Legacy ideapad fan mode | `/sys/bus/platform/devices/VPC2004:*/fan_mode` | `0` silent, `1` standard, `2` dust cleaning, `4` efficient thermal dissipation. | Write: yes. Reboot: no. | Probe-only / not preferred | Hide when Lenovo platform profile/fan curve is available. | Conflicts with modern profile/fan control mental model. | Advanced diagnostics only |
| NVIDIA GPU mode query | `envycontrol --query` | `integrated`, `hybrid`, `nvidia` depending on tool. | Query: no. Reboot: no. | Confirmed wrapper | EnvyControl is optional runtime dependency. | External tool may not match all Fedora/NVIDIA setups. | GPU page + tray status |
| NVIDIA GPU mode switch | `envycontrol -s integrated|hybrid|nvidia` through daemon | Mode string. | Write: yes. Reboot: yes. | Confirmed wrapper, guided only | Must show pending reboot banner. | Misconfiguration can cause black screen. Provide rollback docs. | GPU page only, tray shortcut allowed |
| Reboot now | `systemctl reboot` via logind/systemd or desktop portal | Trigger reboot after user confirmation. | Polkit may require auth. Reboot: yes. | Generic | Only offered after pending GPU mode or firmware change. | Destructive to unsaved work. | GPU page banner |
| Panel backlight | `brightnessctl`, `/sys/class/backlight/nvidia_wmi_ec_backlight/brightness` | Integer or percent. | Usually no through desktop/session; direct sysfs write may need root. Reboot: no. | Confirmed on 82WM | GNOME/KDE already handle this. | Low value; avoid duplicate controls unless requested. | Overview readout or omit |
| DKMS rapid charging | `legion_laptop` module path such as `rapidcharge` if installed | Usually `0`, `1`. | Write: yes. Reboot: no. | Probe-only, non-upstream | Do not require DKMS module. | Battery heat/wear; may conflict with charge type. | Experimental only |
| Win-key lock | DKMS `legion_laptop` path such as `winkey` if installed | Usually `0`, `1`. | Write: yes. Reboot: no. | Probe-only, non-upstream | Optional adapter only. | Key behavior mismatch. | Keyboard page if present |
| Display overdrive | Stable wrapper path if present; no raw WMI | Usually `0`, `1`. | Write: yes. Reboot: unknown/no. | Probe-only / experimental | No confirmed stock Fedora ABI in the scan. | Visual artifacts; uncertain persistence. | Experimental only |
| IO-port LED | `/sys/class/leds/platform::ioport/brightness` if present | Driver/SKU-dependent. | Write: yes. Reboot: no. | Probe-only | LED availability may vary by SKU/driver. | Low physical risk, high compatibility uncertainty. | Appearance page if present |
| Keyboard RGB | External tool only, no native raw EC writes | Tool-specific. | Usually yes or user permissions. Reboot: no. | Unsupported for native app | No stable stock ABI confirmed. | Malformed EC/HID payload risk. | Not planned |
| CPU overclock | Raw WMI/EC/BIOS OC methods | N/A. | N/A. | Unsupported | Use no native backend. | High thermal/warranty risk. | Not planned |
| GPU overclock | Raw WMI/EC or NVIDIA OC | N/A. | N/A. | Unsupported | Out of scope. | High thermal/warranty/stability risk. | Not planned |
| Raw sysfs write | “write arbitrary path/value” | N/A. | Root: yes. | Unsupported | Must never be exposed over D-Bus. | Security vulnerability. | Never |
| Raw WMI method call | Vendor WMI method ID + payload | N/A. | Root/CAP_SYS_ADMIN likely. | Unsupported | No stable API. | EC/firmware breakage risk. | Never |

## Matrix implementation rule

Each row above should map to a daemon capability object:

```json
{
  "id": "battery.charge_type",
  "status": "confirmed|probe_only|unsupported",
  "provider": "power_supply|platform_profile|hwmon|firmware_attributes|ideapad|led|envycontrol",
  "read": { "path": "/sys/class/power_supply/BAT0/charge_types" },
  "write": { "method": "SetBatteryChargeType", "requires_polkit": true },
  "choices": ["Fast", "Standard", "Long_Life"],
  "risk": "low|medium|high",
  "ui": "tray|page|advanced|hidden"
}
```

The UI must render from capability data. It must not contain hardware path assumptions.
