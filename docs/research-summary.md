# Research Summary

## Executive summary

Build this as a probe-driven Fedora hardware control app, not as a hardcoded Lenovo Legion 82WM script.

The Lenovo Legion Pro 5 16ARX8, product 82WM, has several high-value Linux control surfaces already confirmed by the provided runtime scan: battery charge type, Lenovo platform profile, custom fan-curve hwmon nodes, fan and temperature telemetry, Y-logo LED control, Fn-lock LED indication, EC-backed backlight, generic power profile integration, and NVIDIA mode reporting through EnvyControl.

The safest practical product is:

- an unprivileged GTK4/libadwaita dashboard;
- an optional tray/status process using AppIndicator/KStatusNotifier where the desktop supports it;
- a privileged root system daemon on the system D-Bus;
- polkit-gated write methods;
- runtime capability probing on every boot;
- strict value validation before any write;
- no arbitrary sysfs writer exposed to the GUI.

The major conclusion from merging the three model outputs is that **confirmed local sysfs presence matters more than model-family assumptions**. Kernel documentation supports Lenovo Gaming WMI, platform profiles, firmware-attributes, and ideapad_laptop controls, but many of those controls are conditional. A missing path should hide the feature, not produce an error or a disabled-but-mysterious widget.

## Trust model used for this merge

1. **Highest trust:** controls explicitly reported as present by the provided runtime scan.
2. **High trust but not necessarily present:** upstream kernel documentation for `platform_profile`, `power_supply`, `lenovo-wmi-gamezone`, `lenovo-wmi-other`, `firmware-attributes`, and `ideapad_laptop`.
3. **Medium trust:** maintained tools such as `powerprofilesctl`, TLP, `brightnessctl`, `lm_sensors`, and EnvyControl.
4. **Low trust for v1 product decisions:** model claims that a control is “confirmed” without a matching path in the provided scan.
5. **Unsupported:** raw WMI method invocation, raw EC memory writes, arbitrary sysfs writes, and overclocking controls without a stable userspace ABI.

## Confirmed controls on this machine

These are suitable for the MVP, subject to normal safety validation.

| Feature | Confirmed interface | Confirmed values or behavior | MVP decision | Confidence |
|---|---|---:|---|---:|
| Battery charge type | `/sys/class/power_supply/BAT0/charge_types` | `Fast`, `Standard`, `Long_Life` | Expose in Battery page and tray quick menu | High |
| Lenovo platform profile | `/sys/firmware/acpi/platform_profile` and `/sys/firmware/acpi/platform_profile_choices` | `quiet`, `balanced`, `balanced-performance`, `performance` | Expose as primary profile selector | High |
| Generic desktop power profile | `powerprofilesctl` / `org.freedesktop.UPower.PowerProfiles` | `power-saver`, `balanced`, `performance` | Show status; do not treat as Lenovo-specific source of truth | Medium-high |
| Fan curves | Legion hwmon-style `pwm*_auto_point*` controls | 10-point writable fan curve controls reported | Expose presets first; editor later | High |
| Fan telemetry | hwmon `fan*_input` | Fan RPM | Expose in Overview | High |
| Temperature telemetry | hwmon `temp*_input` and/or `sensors` | CPU/GPU/IC-style telemetry where labels exist | Expose in Overview with sanity filtering | High |
| NVIDIA GPU mode | `envycontrol --query` | Current mode reported; switch flow available through EnvyControl | Expose as guided reboot-required workflow | Medium-high |
| Y-logo LED | `/sys/class/leds/platform::ylogo/brightness` | `0`, `1` | Expose as simple toggle | High |
| Fn-lock LED indicator | `/sys/class/leds/platform::fnlock/brightness` | `0`, `1` | Show as indicator only unless functional `fn_lock` path appears | Medium-high |
| Panel backlight | `/sys/class/backlight/nvidia_wmi_ec_backlight/brightness`, `brightnessctl` | Integer brightness; max reported by runtime scan | Read/display only or defer to desktop | Medium |

## Candidate controls that need probing

These should be represented in the daemon's capability registry but hidden unless the path exists and passes read/write tests.

| Feature | Candidate path or command | Probe rule | Why not confirmed yet | Confidence if present |
|---|---|---|---|---:|
| CPU PPT/SPL | `/sys/class/firmware-attributes/lenovo-wmi-other/attributes/ppt_pl1_spl/current_value` | Directory exists, has `min_value`, `max_value`, `scalar_increment`, read-back succeeds | Kernel supports it, but provided scan did not confirm the attribute directory | Medium |
| CPU SPPT | `/sys/class/firmware-attributes/lenovo-wmi-other/attributes/ppt_pl2_sppt/current_value` | Same as above | Same as above | Medium |
| CPU FPPT | `/sys/class/firmware-attributes/lenovo-wmi-other/attributes/ppt_pl3_fppt/current_value` | Same as above; require stronger warning | Same as above | Medium-low |
| `custom` platform profile | listed in `/sys/firmware/acpi/platform_profile_choices` | Only expose if literally listed | Provided choices did not include `custom` | Medium |
| `max-power` / Extreme profile | listed in `/sys/firmware/acpi/platform_profile_choices` | Only expose if literally listed | Provided choices did not include `max-power`; undefined behavior risk exists on some firmware | Low |
| Fan target RPM | hwmon `fanX_target` | Only if driver exposes it; validate `0=auto` and min/max metadata | Not reported in scan | Medium-low |
| Functional Fn-lock | `/sys/bus/platform/devices/VPC2004:*/fn_lock` | Path exists, read/write of `0`/`1` works, LED and behavior agree | Only LED indicator confirmed | Medium |
| Touchpad hardware toggle | `/sys/bus/platform/devices/VPC2004:*/touchpad` | Path exists; user confirms warning | Not reported in scan | Medium |
| Camera power | `/sys/bus/platform/devices/VPC2004:*/camera_power` | Path exists; warn that active camera apps may break | Not reported in scan | Medium |
| Always-on USB charging | `/sys/bus/platform/devices/VPC2004:*/usb_charging` | Path exists; read/write `0`/`1` | Not reported in scan | Medium |
| Legacy conservation mode | `/sys/bus/platform/devices/VPC2004:*/conservation_mode` | Use only if `charge_types` is absent or as read-only compatibility | Newer Lenovo non-ThinkPad support is better represented by `charge_types` | Low-medium |
| Legacy ideapad fan mode | `/sys/bus/platform/devices/VPC2004:*/fan_mode` | Hide if Legion platform profile + fan curve are available | Can conflict conceptually with platform profile and custom fan curve | Low |
| DKMS rapid charge | `legion_laptop` module-specific path such as `rapidcharge` | Optional adapter only; never a hard dependency | Not upstream stable for this app | Low |
| Win-key lock | `legion_laptop` module-specific `winkey` path | Optional adapter only | Not upstream stable for this app | Low |
| Display overdrive | Stable sysfs wrapper if present | Optional advanced toggle only after read-back | WMI methods exist, but no confirmed stable stock Fedora path | Low |
| IO-port LED | `/sys/class/leds/platform::ioport/brightness` if present | Probe LEDs by name; never assume SKU support | SKU/driver dependent | Low |
| Keyboard 4-zone RGB | External HID tool or stable sysfs path | Do not implement native EC writes | No stable stock ABI confirmed | Low |

## Unsupported or risky controls

These are not planned for MVP and should remain unavailable unless a stable, documented, read-back-verified interface appears.

| Feature | Decision | Reason |
|---|---|---|
| Arbitrary sysfs writer | Unsupported | Violates safety model; turns the daemon into a root write primitive. |
| Raw WMI method invoker | Unsupported | No safe public API, vendor-specific payloads, high breakage risk. |
| Raw EC memory writes | Unsupported | Risk of fan halt, invalid embedded-controller state, and difficult recovery. |
| CPU/GPU overclocking | Unsupported | No stable stock userspace ABI confirmed for this model; thermal and warranty risk. |
| Native keyboard RGB EC payloads | Unsupported | Unstandardized and easy to get wrong. Use external proven tools only if later needed. |
| Runtime MUX / G-Sync WMI switching | Unsupported for now | Use EnvyControl-guided OS mode flow only; do not invent a hardware MUX backend. |
| GPU TGP through Lenovo WMI | Unsupported for now | Not confirmed as stable stock ABI for this machine. |
| Fixed fan target RPM as normal UI | Not planned for normal UI | Curve/preset control is safer. Fixed RPM can fight firmware behavior. |
| `max-power` if not listed | Unsupported | Must not be invented. Expose only when platform profile choices include it. |

## Contradictions between ChatGPT, Gemini, and Claude

### 1. Architecture language and UI stack

- ChatGPT short response recommended Rust + GTK4/libadwaita + zbus.
- ChatGPT research response recommended Python for the fastest Fedora-first v1.
- Gemini argued Rust is the better daemon/frontend stack.
- Claude argued for Python, GTK3, and AyatanaAppIndicator3.

**Resolution:** use Rust for the daemon, shared types, probe CLI, and GTK4/libadwaita dashboard. Keep tray support optional and isolated. For GNOME, AppIndicator support depends on the extension; for KDE, StatusNotifier is native. A separate GTK3/Ayatana tray helper is acceptable if direct Rust SNI/AppIndicator support is not good enough, but the primary dashboard should stay GTK4/libadwaita.

### 2. Battery control: `charge_types` versus `conservation_mode` and `rapid_charging`

- ChatGPT treated `/sys/class/power_supply/BAT0/charge_types` as confirmed and legacy `conservation_mode` as probe-only.
- Gemini marked `conservation_mode` and firmware `rapid_charging` as confirmed.
- Claude marked `conservation_mode` as supported and `rapid_charging` as DKMS-only.

**Resolution:** the provided scan confirms `charge_types`, not `conservation_mode` or firmware `rapid_charging`. Use `charge_types` as the primary battery control. Probe VPC2004 and DKMS paths only as optional compatibility features.

### 3. PPT/SPL/SPPT/FPPT status

- ChatGPT classified PPT attributes as probe-only.
- Gemini and Claude described them as effectively confirmed for 82WM/kernel 6.19.

**Resolution:** kernel documentation supports `lenovo-wmi-other` firmware-attributes for PPT controls, but the provided machine scan did not prove those directories exist. Classify as **probe-only**. If present, they require `custom` mode, min/max/increment validation, read-back verification, and advanced UI warnings.

### 4. `custom` and `max-power` platform profiles

- Gemini implied `custom` and `max-power` are available or introduced on the stack.
- ChatGPT reported the machine scan showed only `quiet`, `balanced`, `balanced-performance`, and `performance`.
- Claude claimed 16ARX8 has no `max-power` but also discussed `custom` as a usable mode.

**Resolution:** the app must read `platform_profile_choices` at runtime. Do not show `custom` or `max-power` unless they are literally listed. Never infer them from model number or kernel version.

### 5. Fan curve backend

- ChatGPT used the confirmed Legion hwmon 10-point fan curve nodes.
- Gemini emphasized debugfs/EC 5507 zero-return quirks.
- Claude said debugfs fan-curve reads can return zeros and that the user’s working `legion_hwmon` path is correct.

**Resolution:** use the confirmed hwmon path as primary. Treat debugfs fan curve paths as unsupported for this app. Always validate the full curve before writing, store last-known-good presets, and avoid read-modify-write if the readback is obviously bogus.

### 6. Ideapad toggles

- Gemini and Claude marked several VPC2004 paths as supported.
- ChatGPT marked them as not exposed by the scan and therefore probe-only.

**Resolution:** upstream ABI documents these files, but firmware decides whether they appear. The product should probe them and hide missing controls.

### 7. GPU/MUX switching

- Gemini recommended coordinating EnvyControl and raw WMI MUX method calls.
- Claude recommended avoiding runtime WMI MUX/G-Sync/IGPU toggles and using EnvyControl mostly for state reporting.
- ChatGPT recommended EnvyControl with an apply-and-reboot flow.

**Resolution:** MVP uses EnvyControl only, with explicit reboot-required state. No raw WMI MUX methods in v1.

### 8. Commercial/plugin strategy

- Claude included closed paid plugins and self-hosted repos.
- The requested output is open-source project documentation, not a business plan.

**Resolution:** keep the core plan open-source. Paid/cloud ideas are not part of the generated implementation plan.

## Final confidence rating per feature

| Feature | Classification | Confidence | Ship phase |
|---|---|---:|---|
| Runtime capability probing | Required | Very high | MVP |
| Privileged D-Bus daemon | Required | Very high | MVP |
| polkit-gated writes | Required | Very high | MVP |
| Platform profile selector | Confirmed | High | MVP |
| Battery charge type selector | Confirmed | High | MVP |
| Fan telemetry | Confirmed | High | MVP |
| Temperature telemetry | Confirmed | High | MVP |
| Fan presets via 10-point curve | Confirmed, safety-sensitive | High | MVP |
| Manual fan curve editor | Confirmed, safety-sensitive | Medium-high | v0.2 |
| Y-logo LED toggle | Confirmed | High | MVP/v0.2 |
| Fn-lock LED display | Confirmed indicator | Medium-high | v0.2 |
| Functional Fn-lock toggle | Probe-only | Medium | v0.2 if present |
| EnvyControl GPU mode flow | External confirmed wrapper | Medium-high | MVP/v0.2 |
| Backlight display/control | Confirmed but redundant | Medium | Optional |
| PPT/SPL/SPPT/FPPT | Probe-only advanced | Medium | Advanced |
| `custom` profile | Probe-only | Medium | Advanced if listed |
| `max-power` profile | Probe-only high-risk | Low | Not planned unless listed and tested |
| Camera power | Probe-only | Medium | v0.3 if present |
| Touchpad hardware toggle | Probe-only | Medium-low | v0.3 if present |
| Always-on USB charging | Probe-only | Medium | v0.3 if present |
| Legacy conservation mode | Probe-only compatibility | Low-medium | Compatibility only |
| Legacy fan mode | Probe-only, conflict-prone | Low | Advanced only |
| DKMS rapid charging | Probe-only, non-upstream | Low | Optional adapter only |
| Display overdrive | Probe-only, non-upstream | Low | Experimental |
| Keyboard RGB | Unsupported for native implementation | Low | Not planned |
| CPU/GPU overclocking | Unsupported | Very low | Not planned |
| Raw WMI/EC access | Unsupported | Very low | Never |

## Reference links

- Linux kernel platform profile documentation: <https://docs.kernel.org/userspace-api/sysfs-platform_profile.html>
- Linux kernel platform profile ABI names: <https://github.com/torvalds/linux/blob/master/Documentation/ABI/testing/sysfs-class-platform-profile>
- Linux kernel Lenovo WMI Gamezone documentation: <https://docs.kernel.org/wmi/devices/lenovo-wmi-gamezone.html>
- Linux kernel Lenovo WMI Other documentation: <https://docs.kernel.org/wmi/devices/lenovo-wmi-other.html>
- Linux kernel power supply documentation: <https://docs.kernel.org/power/power_supply_class.html>
- ideapad_laptop sysfs ABI mirror: <https://sbexr.rabexc.org/latest/sources/a4/59acbed9a7c320.html>
- TLP Lenovo non-ThinkPad battery care notes: <https://linrunner.de/tlp/settings/bc-vendors.html>
- Fedora AppIndicator extension package: <https://packages.fedoraproject.org/pkgs/gnome-shell-extension-appindicator/gnome-shell-extension-appindicator/>
- Fedora Ayatana AppIndicator GTK3 package: <https://packages.fedoraproject.org/pkgs/libayatana-appindicator/libayatana-appindicator-gtk3/>
