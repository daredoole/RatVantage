# Architecture

## Recommended architecture

Use a split, probe-driven architecture:

```text
User session                                             System context

┌──────────────────────────────┐                         ┌─────────────────────────────┐
│ legion-control-ui            │                         │ legion-control-daemon       │
│ GTK4/libadwaita dashboard    │                         │ root-owned systemd service  │
│ unprivileged                 │                         │ system D-Bus service        │
└──────────────┬───────────────┘                         └──────────────┬──────────────┘
               │ D-Bus method calls + signals                            │
┌──────────────▼───────────────┐                         ┌──────────────▼──────────────┐
│ legion-control-tray          │ optional                │ Hardware adapters           │
│ AppIndicator/SNI helper      │◄────────────────────────│ sysfs/hwmon/power_supply    │
│ unprivileged                 │                         │ firmware-attributes/tools   │
└──────────────────────────────┘                         └─────────────────────────────┘
```

Recommended implementation stack:

- **Daemon:** Rust, `zbus`, `tokio` or `async-io`, typed adapters, journald logging.
- **Dashboard UI:** Rust, GTK4, libadwaita, system D-Bus client.
- **Tray/status:** optional Rust StatusNotifier implementation. If that is not reliable enough on Fedora/GNOME, isolate a tiny GTK3/Ayatana tray helper as a separate user process.
- **Probe CLI:** Rust debug tool that prints the daemon's capability registry as JSON.
- **Packaging:** Fedora RPM first. Flatpak is GUI-only at best because it cannot install the root daemon, D-Bus system service file, systemd unit, or polkit policy.

The app should feel like a Fedora desktop app, but the hardware writes are system operations. The GUI is never root.

## UI process

`legion-control-ui` runs as the logged-in user.

Responsibilities:

- render Overview, Profiles, Fan Curves, Battery, GPU, Appearance, and Advanced pages;
- connect to the daemon over the system D-Bus;
- render only capabilities returned by the daemon;
- request privileged changes through D-Bus methods;
- display polkit-driven authentication prompts indirectly through the desktop polkit agent;
- show reboot-required banners for GPU mode and any firmware operation that reports pending reboot;
- show telemetry from daemon signals or low-rate polling;
- store user UI preferences only, not trusted hardware state.

The UI must not:

- write sysfs directly;
- shell out to `sudo`;
- run under `pkexec`;
- expose raw paths or raw write fields;
- assume `hwmonN` numbering;
- assume every Legion exposes the same nodes.

## Tray/status process

Tray support is useful on KDE and optional on GNOME.

Responsibilities:

- show read-only hardware and capability status in tooltip/status text;
- expose dashboard, refresh, and quit actions now;
- keep profile selection, fan preset, battery mode, and Y-logo actions disabled until daemon write methods exist;
- forward future write actions to the daemon;
- never include manual fan curve editing or firmware sliders.

Fedora GNOME caveat:

- GNOME does not natively show classic tray icons by default.
- Fedora packages `gnome-shell-extension-appindicator`, which integrates AppIndicators and KStatusNotifierItems into GNOME Shell.
- KDE Plasma supports StatusNotifier-style tray items natively.

## Privileged daemon

`legion-control-daemon` runs as root under systemd and owns the system bus name:

```text
org.ratvantage.LegionControl1
/org/ratvantage/LegionControl1
```

Responsibilities:

- runtime hardware probing;
- sysfs and wrapper reads;
- whitelisted sysfs writes;
- polkit authorization checks;
- safety validation;
- rollback storage;
- telemetry polling;
- journald logging;
- state restoration after boot and resume;
- emitting D-Bus signals when state changes.

The daemon should expose high-level methods only. It must never expose a method like `WriteSysfs(path, value)`.

## D-Bus API

Use a versioned interface from day one:

```text
org.ratvantage.LegionControl1
```

Suggested object path:

```text
/org/ratvantage/LegionControl1
```

Current read-only methods:

```text
GetHardwareSummary() -> s                         # JSON
GetCapabilities() -> s                            # JSON
RefreshCapabilities() -> s                        # JSON
GetTelemetry() -> s                               # JSON
GetRawProbeReport() -> s                          # JSON; no secrets
PlanPlatformProfileWrite(s requested) -> s        # JSON dry-run plan
PlanBatteryChargeTypeWrite(s requested) -> s      # JSON dry-run plan
PlanGpuModeWrite(s requested) -> s                # JSON dry-run plan, reboot required
PlanFanPresetWrite(s requested) -> s              # JSON dry-run plan
```

Future write-capable methods must remain absent until their validators, polkit
checks, rollback behavior, and manual validation exist. Disabled draft contracts
live in [write-contracts.md](write-contracts.md) and
`legion_common::WRITE_METHOD_CONTRACTS`.

Future candidate methods:

```text
GetPlatformProfiles() -> as
GetPlatformProfile() -> s
SetPlatformProfile(s profile) -> ()

GetBatteryChargeTypes() -> as
GetBatteryChargeType() -> s
SetBatteryChargeType(s charge_type) -> ()

GetFanTelemetry() -> s                            # JSON
GetFanCurve() -> s                                # JSON
ValidateFanCurve(s curve_json) -> s               # JSON result
ApplyFanCurve(s curve_json) -> ()
ApplyFanPreset(s preset_id) -> ()
RestoreAutoFan() -> ()

GetGpuMode() -> s
GetGpuModePending() -> s
SetGpuModePending(s mode) -> ()
ClearGpuModePending() -> ()
RequestReboot() -> ()

GetLedState(s led_id) -> b
SetLedState(s led_id, b enabled) -> ()

GetIdeapadToggles() -> s                          # JSON
SetIdeapadToggle(s toggle_id, b enabled) -> ()

GetFirmwareAttributes() -> s                      # JSON
SetFirmwareAttribute(s attr_id, i value) -> ()
ResetFirmwareAttribute(s attr_id) -> ()
```

Signals:

```text
CapabilitiesChanged(s capabilities_json)
TelemetryChanged(s telemetry_json)
PlatformProfileChanged(s profile)
BatteryChargeTypeChanged(s charge_type)
FanCurveChanged(s curve_json)
FanPresetChanged(s preset_id)
GpuModePendingChanged(s mode)
LedStateChanged(s led_id, b enabled)
FirmwareAttributeChanged(s attr_id, i value)
ErrorOccurred(s code, s message)
```

API rules:

- Method names describe intent, not file paths.
- All writes validate against current capabilities.
- All writes log caller bus name, action, old value, new value, and result.
- All write methods check polkit unless explicitly read-only.
- Input strings must match discovered choices exactly.
- JSON payloads must be schema-validated before use.

## polkit authorization

Use polkit inside the daemon, not `sudo` from the GUI.

Action tiers:

| Tier | Examples | Suggested policy |
|---|---|---|
| Routine local hardware changes | platform profile, battery charge type, Y-logo LED, fan preset | `auth_admin_keep` during development; possibly allow active local wheel users later |
| Safety-sensitive changes | custom fan curve, camera power, GPU mode switch | `auth_admin_keep` or `auth_admin` depending on risk |
| High-risk advanced changes | firmware PPT/SPPT/FPPT, `max-power`, fan target RPM | `auth_admin` every time |
| Read-only | telemetry, capabilities, current profile | allow |

Do not rely only on D-Bus policy XML for authorization. D-Bus policy can restrict who may send to the service, but polkit should decide whether a caller may perform a specific write action.

## systemd service

Install a system service, not a user service, for hardware writes.

Suggested unit:

```ini
[Unit]
Description=Legion Control hardware daemon
Documentation=man:legion-control-daemon(8)
ConditionPathExists=/sys/firmware/acpi/platform_profile
After=multi-user.target

[Service]
Type=dbus
BusName=org.ratvantage.LegionControl1
ExecStart=/usr/libexec/legion-control/legion-control-daemon
Restart=on-failure
StateDirectory=legion-control
ConfigurationDirectory=legion-control
LogsDirectory=legion-control

# Hardening. Keep sysfs writable only where needed.
ProtectSystem=strict
ProtectHome=true
PrivateTmp=true
NoNewPrivileges=true
MemoryDenyWriteExecute=true
RestrictAddressFamilies=AF_UNIX
SystemCallArchitectures=native
ReadWritePaths=/sys/firmware/acpi /sys/class/power_supply /sys/class/leds /sys/class/hwmon /sys/class/firmware-attributes /var/lib/legion-control /etc/legion-control

[Install]
WantedBy=multi-user.target
```

Notes:

- This is a starting point. Test hardening flags on the actual laptop.
- `ProtectKernelTunables=yes` may make needed sysfs paths unwritable; do not enable it until verified.
- Consider D-Bus activation with a matching system service file.

## Hardware probe layer

The probe layer runs at daemon startup, after resume, and when manually refreshed.

Probe outputs a normalized capability registry:

```json
{
  "machine": {
    "vendor": "LENOVO",
    "product_name": "Legion Pro 5 16ARX8",
    "product_version": "82WM"
  },
  "capabilities": {
    "platform_profile": {
      "status": "confirmed",
      "path": "/sys/firmware/acpi/platform_profile",
      "choices": ["quiet", "balanced", "balanced-performance", "performance"]
    },
    "battery_charge_type": {
      "status": "confirmed",
      "path": "/sys/class/power_supply/BAT0/charge_types",
      "choices": ["Fast", "Standard", "Long_Life"]
    },
    "fan_curve": {
      "status": "confirmed",
      "provider": "legion_hwmon",
      "fans": 2,
      "points": 10,
      "writable": true
    },
    "firmware_attributes": {
      "status": "probe_only",
      "base_path": "/sys/class/firmware-attributes/lenovo-wmi-other/attributes",
      "present": false
    }
  }
}
```

Probe rules:

- Discover hwmon devices by `name` and labels, not by `hwmonN`.
- For every candidate write path, perform read validation before showing UI.
- Do not write during probing except in explicit hardware validation mode.
- Cache capabilities in memory only; refresh after suspend/resume and driver reload.
- Missing paths are normal and should log once at `INFO`.

## Safety validation layer

All write requests pass through validators before touching hardware.

Validator examples:

- platform profile must be in discovered choices;
- charge type must be in discovered choices;
- LED ID must map to a known LED path;
- fan curve must contain exactly the discovered number of fans and points;
- fan curve point values must be within discovered or configured safe bounds;
- temperature points, if writable, must be monotonic;
- firmware attribute values must satisfy `min_value <= value <= max_value` and align with `scalar_increment`;
- PPT writes require `custom` profile to be listed and active, or the daemon must set it explicitly after user confirmation;
- GPU modes must be one of EnvyControl's supported values;
- no write occurs if any validation step fails.

## Telemetry polling strategy

Telemetry should be low overhead and predictable.

| Source | Strategy | Suggested cadence |
|---|---|---:|
| Fan RPM | daemon polls hwmon while UI/tray subscribed | 1-2s while dashboard visible; 5-10s in tray-only mode |
| Temperatures | daemon polls hwmon | 1-2s while dashboard visible; 5-10s background |
| Battery capacity/status | UPower signal if available; power_supply fallback | signal-driven or 30-60s fallback |
| Platform profile | read on startup/menu-open; poll or POLLPRI where supported | 5s while UI visible; after Fn+Q/profile events |
| Fan curve | read only on page open, after apply, after profile change, after resume | on demand |
| GPU mode | read on GPU page open and after EnvyControl operation | on demand |
| Firmware attributes | read on Advanced page open and after write | on demand |

Use simple plausibility filters for telemetry display:

- discard one-sample temperature jumps that exceed a configured physical delta limit unless repeated;
- mark sensors stale if a read fails repeatedly;
- avoid averaging values used for safety decisions; filtering is for UI display only.

## Config and preset storage

Recommended locations:

```text
/etc/legion-control/daemon.toml                 # system policy/config
/var/lib/legion-control/state.toml              # last applied safe state
/usr/share/legion-control/presets/*.toml        # packaged presets
$XDG_CONFIG_HOME/legion-control/config.toml     # user UI preferences
$XDG_CONFIG_HOME/legion-control/presets/*.toml  # user fan presets
$XDG_CACHE_HOME/legion-control/probe.json       # optional debug cache
```

State should include:

- last applied platform profile;
- last known good fan curve;
- last selected fan preset;
- last battery charge type;
- pending GPU mode;
- firmware attributes applied by this daemon;
- versioned schema number.

Never trust user-writable preset files until they pass daemon validation.

## Why the GUI must not write sysfs directly

The GUI must not write sysfs directly because:

1. many writes require root, and a root GUI under Wayland is both fragile and unsafe;
2. direct writes bypass polkit authorization and auditability;
3. arbitrary sysfs access turns a desktop app into a privileged hardware mutation tool;
4. fan curves and firmware attributes need whole-request validation, not ad hoc writes;
5. `hwmonN` numbering changes across boots;
6. several features reset after suspend/profile changes and need daemon-owned state restoration;
7. a system daemon can log to journald, apply rollback, and serialize conflicting writes.

The UI is a client. The daemon is the only hardware owner.
