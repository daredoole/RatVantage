# Codex Build Kickoff Prompt

You are building the first safe implementation of a Fedora-native Lenovo Legion hardware control app.

Project working name: **Legion Control**.

Target hardware for the first implementation:

- Lenovo Legion Pro 5 16ARX8
- Product type 82WM
- Fedora 43
- Modern Linux kernel with platform_profile, power_supply, hwmon, Lenovo WMI, and optional firmware-attributes support

## Non-negotiable safety rules

- Do not create a GUI that runs as root.
- Do not use `sudo` or `pkexec` from the GUI.
- Do not expose an arbitrary sysfs writer.
- Do not expose raw WMI calls.
- Do not expose raw EC memory writes.
- Do not hardcode `hwmonN`.
- Do not assume all Lenovo Legion laptops expose the same paths.
- Do not implement CPU/GPU overclocking.
- Do not implement keyboard RGB EC/HID writes.
- Do not expose `custom` or `max-power` platform profiles unless they are literally present in `/sys/firmware/acpi/platform_profile_choices`.
- All write methods must validate values against runtime-discovered choices or metadata.
- All hardware writes must happen through a root system daemon over D-Bus and polkit.

## Architecture to build

Create a Rust workspace with these crates:

```text
crates/legion-common
crates/legion-daemon
crates/legion-ui
crates/legion-probe
```

Optional later:

```text
crates/legion-tray
```

The first milestone is read-only probing plus daemon skeleton. Write support comes only after validators and polkit are in place.

## Required repo structure

```text
legion-control/
├── Cargo.toml
├── README.md
├── docs/
├── prompts/
├── crates/
│   ├── legion-common/
│   ├── legion-daemon/
│   ├── legion-ui/
│   └── legion-probe/
├── data/
│   ├── dbus/
│   ├── systemd/
│   ├── polkit/
│   ├── desktop/
│   ├── metainfo/
│   ├── icons/
│   └── presets/
├── packaging/rpm/
└── tests/fixtures/
```

## First implementation milestone

Implement a read-only probe CLI:

```bash
cargo run -p legion-probe -- --json
```

It should output JSON with:

- DMI vendor/product fields from `/sys/devices/virtual/dmi/id/`;
- platform profile current value and choices from:
  - `/sys/firmware/acpi/platform_profile`
  - `/sys/firmware/acpi/platform_profile_choices`
- battery charge type choices/current from:
  - `/sys/class/power_supply/BAT0/charge_types`
- hwmon fan RPM and temperature sensors from `/sys/class/hwmon/hwmon*`, discovered by `name` and labels;
- Legion fan curve capability if `pwm*_auto_point*`-style nodes are found;
- LED nodes:
  - `/sys/class/leds/platform::ylogo/brightness`
  - `/sys/class/leds/platform::fnlock/brightness`
- firmware attributes base path if present:
  - `/sys/class/firmware-attributes/lenovo-wmi-other/attributes`
- explicit PPT attributes if present:
  - `ppt_pl1_spl`
  - `ppt_pl2_sppt`
  - `ppt_pl3_fppt`
- VPC2004 ideapad toggles if present:
  - `fn_lock`
  - `touchpad`
  - `camera_power`
  - `usb_charging`
  - `conservation_mode`
  - `fan_mode`
- EnvyControl presence and `envycontrol --query` output if command exists.

Missing paths should produce capabilities with `status: "missing"` or should be absent from the capability list. Missing paths must not crash the program.

## Data model requirements

In `legion-common`, define typed structs and serialize them with `serde`:

```rust
CapabilityRegistry
HardwareSummary
Capability
CapabilityStatus
RiskLevel
PlatformProfileCapability
BatteryChargeTypeCapability
HwmonSensor
FanCurveCapability
LedCapability
FirmwareAttributeCapability
IdeapadToggleCapability
GpuCapability
TelemetrySnapshot
```

Use stable JSON field names.

## Daemon skeleton milestone

Implement `legion-daemon` as a system D-Bus service skeleton using `zbus`.

Interface:

```text
org.ratvantage.LegionControl1
/org/ratvantage/LegionControl1
```

Read-only methods first:

```text
GetHardwareSummary() -> s
GetCapabilities() -> s
RefreshCapabilities() -> s
GetTelemetry() -> s
GetRawProbeReport() -> s
```

Do not implement write methods until the validator modules exist.

## UI milestone

Implement a minimal GTK4/libadwaita UI:

- starts as normal user;
- connects to the system D-Bus daemon;
- shows capability summary;
- shows platform profile choices read-only;
- shows battery charge type choices read-only;
- shows fan RPM and temperatures if detected;
- shows warning if daemon is unavailable;
- contains no direct sysfs code.

## Packaging data files

Create placeholder files:

```text
data/systemd/legion-control-daemon.service
data/dbus/org.ratvantage.LegionControl1.service
data/dbus/org.ratvantage.LegionControl1.conf
data/polkit/org.ratvantage.LegionControl1.policy
data/desktop/org.ratvantage.LegionControl.desktop
data/metainfo/org.ratvantage.LegionControl.metainfo.xml
packaging/rpm/legion-control.spec
```

These can be initially incomplete, but must be syntactically reasonable.

## Tests

Create unit tests for:

- parsing platform profile choices;
- parsing battery charge type choices;
- detecting hwmon devices without hardcoded `hwmonN`;
- detecting missing sysfs paths cleanly;
- detecting firmware attributes only when metadata exists.

Create fake sysfs fixtures under:

```text
tests/fixtures/sysfs-82wm-confirmed/
```

Do not write to real `/sys` in tests.

## Write-method plan, but do not implement yet

Prepare module stubs for these future write methods:

```text
SetPlatformProfile(profile)
SetBatteryChargeType(charge_type)
ApplyFanPreset(preset_id)
ApplyFanCurve(curve_json)
RestoreAutoFan()
SetLedState(led_id, enabled)
SetGpuModePending(mode)
SetFirmwareAttribute(attr_id, value)
```

Each future method must:

1. check capability exists;
2. validate input;
3. check polkit;
4. write through a narrow adapter;
5. read back if possible;
6. log to journald;
7. rollback if needed;
8. emit a D-Bus signal.

## Development priorities

1. Read-only probe correctness.
2. Stable typed capability model.
3. Daemon skeleton.
4. UI read-only display.
5. Fedora packaging skeleton.
6. Validators.
7. polkit checks.
8. First safe write: platform profile.
9. Second safe write: battery charge type.
10. Fan preset write with rollback.

Stop and ask for review before implementing any hardware write path beyond platform profile and battery charge type.
