# Implementation Plan

## Implemented baseline

The repository now has a working pre-alpha scaffold:

- Rust workspace with `legion-common`, `legion-probe`, `legion-daemon`, `legion-ui`, and `ratvantage-test-support`.
- Read-only probe that builds hardware summary, capability, telemetry, and raw report JSON.
- Daemon exposing read-only hardware/capability/telemetry/raw-report methods, dry-run planning methods for GPU mode, fan presets, fan restore/default, and ideapad toggles, gated platform-profile, battery charge type, ylogo LED, and restricted `fn_lock` execution paths with rollback coverage, plus app-state-only GPU pending-reboot tracking and fan curve snapshots.
- Private D-Bus contract tests that verify method introspection and JSON contracts.
- UI status, overview, diagnostics, app-state, tray, and dry-run planning clients with deterministic output, including reboot-required GPU mode planning with rollback guidance, pending-reboot display, fan curve snapshot capture, overview/tray/GTK state visibility, diagnostics/export parity for durable app-state fields `gpu_mode_pending` and `last_known_good_fan_curve`, fan preset and fan restore/default validation, and appearance/peripheral values, plus optional GTK4/libadwaita shell with gated Profiles/Battery/Appearance quick-apply controls and tray quick actions for reversible platform profile, battery charge type, ylogo LED, and restricted `fn_lock` writes behind `gtk-ui`.
- Packaged read-only fan preset TOML assets with CI schema validation and runtime dry-run planning.
- Read-only compatibility bundle workflow for external Legion submissions, including generated probe summaries and PR template support.
- KDE StatusNotifier smoke report workflow with reusable report bundles under `target/smoke/`.
- Read-only tray desktop diagnostics via `legion-control-tray --desktop-check`, plus runtime-derived tray menu diagnostics via `legion-control-tray --menu-check`, with unit and CLI coverage.
- Local CI script, Fedora dependency installer, GitHub Actions CI, and pinned stable Rust toolchain.

Next implementation work should collect more captured fixtures through the compatibility bundle workflow when additional supported Legion machines are available, extend the same reversible write pattern beyond the current `fn_lock` rollout to another carefully bounded low-risk control if no new hardware reports are available, and leave autostart disabled until GNOME-with-extension smoke exists. Platform-profile, battery charge type, ylogo LED, and restricted `fn_lock` execution now exist behind daemon policy flags and real `pkcheck` authorization, and tray reloads now cover periodic refresh plus suspend-like gaps, but broader hardware writes still remain gated until write-specific manual validation and recovery instructions are complete.

## Repo structure

Current scaffold:

```text
RatVantage/
├── Cargo.toml
├── Cargo.lock
├── README.md
├── BRAND.md
├── AGENTS.md
├── rust-toolchain.toml
├── docs/
│   ├── architecture.md
│   ├── fedora-packaging.md
│   ├── feature-roadmap.md
│   ├── fixture-capture.md
│   ├── hardware-control-matrix.md
│   ├── implementation-plan.md
│   ├── research-summary.md
│   ├── safety-model.md
│   ├── session-handoff.md
│   └── write-contracts.md
├── prompts/
│   └── codex-build-kickoff.md
├── scripts/
│   ├── capture-compat-report.sh
│   ├── capture-sysfs-fixture.sh
│   ├── ci-local.sh
│   ├── install-dev-deps-fedora.sh
│   └── validate-packaging.sh
├── crates/
│   ├── legion-common/
│   ├── legion-daemon/
│   ├── legion-ui/
│   ├── legion-tray/
│   ├── legion-probe/
│   └── test-support/
├── data/
│   ├── dbus/
│   ├── desktop/
│   ├── icons/
│   ├── metainfo/
│   ├── polkit/
│   ├── presets/
│   └── systemd/
├── packaging/
│   └── rpm/
├── tests/
│   └── fixtures/
│       ├── sysfs-82wm-confirmed/
│       └── sysfs-82wm-runtime-capture/
└── target/
```

Target layout:

```text
RatVantage/
├── Cargo.toml
├── README.md
├── BRAND.md
├── AGENTS.md
├── docs/
│   ├── architecture.md
│   ├── fedora-packaging.md
│   ├── feature-roadmap.md
│   ├── hardware-control-matrix.md
│   ├── implementation-plan.md
│   ├── research-summary.md
│   ├── safety-model.md
│   └── session-handoff.md
├── prompts/
│   └── codex-build-kickoff.md
├── scripts/
│   ├── capture-compat-report.sh
│   ├── capture-sysfs-fixture.sh
│   ├── ci-local.sh
│   └── install-dev-deps-fedora.sh
├── crates/
│   ├── legion-common/
│   ├── legion-daemon/
│   ├── legion-ui/
│   ├── legion-tray/
│   ├── legion-probe/
│   └── test-support/
├── data/
│   ├── dbus/
│   │   ├── org.ratvantage.LegionControl1.conf
│   │   └── org.ratvantage.LegionControl1.service
│   ├── systemd/
│   │   └── legion-control-daemon.service
│   ├── polkit/
│   │   └── org.ratvantage.LegionControl1.policy
│   ├── desktop/
│   │   ├── org.ratvantage.LegionControl.desktop
│   │   └── org.ratvantage.LegionControl.Tray.desktop
│   ├── metainfo/
│   │   └── org.ratvantage.LegionControl.metainfo.xml
│   ├── icons/
│   │   └── hicolor/
│   └── presets/
│       ├── quiet-office.toml
│       ├── balanced-daily.toml
│       ├── gaming.toml
│       └── max-safe.toml
├── packaging/
│   └── rpm/
│       └── legion-control.spec
├── tests/
│   ├── fixtures/
│   │   ├── sysfs-82wm-confirmed/
│   │   └── sysfs-82wm-runtime-capture/
│   └── integration/
└── xtask/
    └── src/main.rs
```

Keep the current scaffold accurate, but preserve the target layout so future work has a clear destination.

## Rust crate layout

### `legion-common`

Shared types and schemas.

```text
crates/legion-common/src/
├── lib.rs
├── capabilities.rs
├── telemetry.rs
├── fan_curve.rs
├── firmware.rs
├── gpu.rs
├── errors.rs
├── dbus_types.rs
└── paths.rs
```

Key types:

```rust
pub struct HardwareSummary;
pub struct CapabilityRegistry;
pub struct PlatformProfileCapability;
pub struct BatteryChargeTypeCapability;
pub struct FanCurveCapability;
pub struct FirmwareAttributeCapability;
pub struct TelemetrySnapshot;
pub struct FanCurve;
pub struct FanCurvePoint;
pub enum CapabilityStatus { Confirmed, ProbeOnly, Unsupported }
pub enum RiskLevel { Low, Medium, High, Experimental }
```

Use `serde` for JSON. Keep D-Bus payloads stable by using versioned JSON structs for complex data.

### `legion-daemon`

Root system service.

```text
crates/legion-daemon/src/
├── main.rs
├── dbus_api.rs
├── polkit.rs
├── service_state.rs
├── logging.rs
├── config.rs
├── probe/
│   ├── mod.rs
│   ├── dmi.rs
│   ├── sysfs_walk.rs
│   └── capability_builder.rs
├── hardware/
│   ├── mod.rs
│   ├── sysfs.rs
│   ├── power_supply.rs
│   ├── platform_profile.rs
│   ├── hwmon.rs
│   ├── legion_hwmon.rs
│   ├── firmware_attributes.rs
│   ├── ideapad.rs
│   ├── leds.rs
│   ├── envycontrol.rs
│   ├── power_profiles.rs
│   └── brightness.rs
├── safety/
│   ├── mod.rs
│   ├── validators.rs
│   ├── fan_curve_limits.rs
│   ├── firmware_limits.rs
│   ├── gpu_validation.rs
│   └── rollback.rs
└── telemetry/
    ├── mod.rs
    ├── poller.rs
    └── filters.rs
```

### `legion-ui`

GTK4/libadwaita dashboard.

```text
crates/legion-ui/src/
├── main.rs
├── app.rs
├── daemon_client.rs
├── pages/
│   ├── overview.rs
│   ├── profiles.rs
│   ├── fan_curves.rs
│   ├── battery.rs
│   ├── gpu.rs
│   ├── appearance.rs
│   └── advanced.rs
├── widgets/
│   ├── capability_badge.rs
│   ├── telemetry_card.rs
│   ├── risk_banner.rs
│   ├── fan_curve_editor.rs
│   └── reboot_banner.rs
└── config.rs
```

### `legion-tray`

Optional tray/status process.

```text
crates/legion-tray/src/
├── main.rs
├── daemon_client.rs
├── menu.rs
├── status_icon.rs
└── desktop_detection.rs
```

Implementation options:

- pure Rust StatusNotifier/KStatusNotifier implementation if reliable;
- fallback separate GTK3/Ayatana helper if needed.

Keep it small. The dashboard is the main UI.

Current read-only implementation:

- `menu.rs` builds a state-driven menu from daemon-reported profile choices, battery charge choices, battery telemetry, packaged preset labels, capability summaries, and pending app state.
- `main.rs` exposes `--status`, `--tooltip`, `--desktop-check`, and `--menu-check` so private-bus tests and smoke bundles can validate the exact derived tray output without a shell UI.
- `status_notifier.rs` feeds the same derived menu into the live StatusNotifier tray backend, keeping dashboard, refresh, and quit enabled while all hardware-changing rows remain informational only.

### `legion-probe`

Read-only debug CLI.

```text
crates/legion-probe/src/main.rs
```

Commands:

```bash
legion-probe --json
legion-probe --pretty
legion-probe --paths
legion-probe --redact
```

No writes in this tool.

## GTK/libadwaita UI layout

Use a single main window:

```text
AdwApplicationWindow
└── AdwToolbarView
    ├── HeaderBar
    │   ├── title: Legion Control
    │   ├── profile badge
    │   └── daemon status menu
    └── AdwNavigationSplitView or AdwViewStack
        ├── Overview
        ├── Profiles
        ├── Fan Curves
        ├── Battery
        ├── GPU
        ├── Appearance
        └── Advanced
```

### Overview page

Cards:

- Platform profile.
- Battery charge type.
- Fan RPM.
- Temperatures.
- GPU mode.
- Daemon status.

### Profiles page

- Radio/list rows for detected platform profiles.
- Tray menu uses the same detected profile choices for read-only diagnostics. [implemented]
- Badge for external changes.
- Optional generic PowerProfiles section.
- Warning if generic PowerProfiles owner may conflict.

### Fan Curves page

- Preset list.
- Tray menu surfaces packaged preset labels as read-only state. [implemented]
- Apply preset button.
- Restore safe/default button.
- Manual editor in v0.2.
- Validation result panel.

### Battery page

- Charge type segmented controls.
- Tray menu surfaces current charge type, detected charge choices, and battery telemetry as read-only state. [implemented]
- Battery telemetry.
- Explanation of `Fast`, `Standard`, `Long_Life`.
- Optional VPC2004/USB charging controls if present.

### GPU page

- Current EnvyControl mode.
- Target mode selector.
- Pending reboot banner.
- Reboot button.
- Recovery instructions.

### Appearance page

- Y-logo LED toggle.
- Fn-lock LED display.
- Optional IO-port LED if present.

### Advanced page

- Probe report.
- Firmware attributes only if present.
- High-risk controls hidden behind confirmation.

## D-Bus method list

Use interface:

```text
org.ratvantage.LegionControl1
```

Current read-only methods:

```text
GetHardwareSummary() -> s
GetCapabilities() -> s
RefreshCapabilities() -> s
GetTelemetry() -> s
GetRawProbeReport() -> s
PlanPlatformProfileWrite(s requested) -> s
PlanBatteryChargeTypeWrite(s requested) -> s
PlanGpuModeWrite(s requested) -> s
PlanFanPresetWrite(s requested) -> s
```

Future write-capable methods are design-only until validators, polkit checks,
rollback, and manual validation exist. Disabled first drafts live in
[write-contracts.md](write-contracts.md) and
`legion_common::WRITE_METHOD_CONTRACTS`.

Future candidate methods:

```text

GetPlatformProfiles() -> as
GetPlatformProfile() -> s
SetPlatformProfile(s profile) -> s

GetBatteryChargeTypes() -> as
GetBatteryChargeType() -> s
SetBatteryChargeType(s charge_type) -> s

GetFanTelemetry() -> s
GetFanCurve() -> s
ValidateFanCurve(s curve_json) -> s
ApplyFanCurve(s curve_json) -> ()
ApplyFanPreset(s preset_id) -> ()
RestoreAutoFan() -> ()
GetLastKnownGoodFanCurve() -> s
CaptureLastKnownGoodFanCurve() -> s

GetGpuMode() -> s
GetGpuModePending() -> s
SetGpuModePending(s mode) -> ()
ClearGpuModePending() -> ()
RequestReboot() -> ()

GetLedState(s led_id) -> b
SetLedState(s led_id, b enabled) -> ()

GetIdeapadToggles() -> s
SetIdeapadToggle(s toggle_id, b enabled) -> ()

GetFirmwareAttributes() -> s
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

## Hardware adapter modules

### `sysfs.rs`

- Safe read/write helpers.
- Atomic-ish write pattern for sysfs strings.
- Permission and existence checks.
- No public arbitrary write API over D-Bus.

### `power_supply.rs`

- Detect batteries.
- Read `charge_types` choices/current.
- Set charge type.
- Read capacity/status/health fields.

### `platform_profile.rs`

- Read current profile.
- Read choices.
- Set profile.
- Detect class devices if useful.

### `hwmon.rs`

- Discover hwmon devices.
- Read `name` and labels.
- Map fan/temp channels.
- Avoid hardcoded `hwmonN`.

### `legion_hwmon.rs`

- Detect Legion fan curve files.
- Determine fan count and point count.
- Read fan curve if trustworthy.
- Apply full fan curve.
- Restore safe/default.

### `firmware_attributes.rs`

- Detect `lenovo-wmi-other` base.
- Enumerate explicit supported attributes.
- Read metadata.
- Validate integer writes.
- Check read-back and pending state.

### `ideapad.rs`

- Probe VPC2004 paths.
- Support only named toggles.
- Mark legacy conflicts.

### `leds.rs`

- Probe LED names.
- Support known LED IDs only.
- Map `platform::ylogo`, `platform::fnlock`, and optionally `platform::ioport`.

### `envycontrol.rs`

- Detect executable.
- Query current mode.
- Validate target mode.
- Run controlled switch command.
- Capture output and errors.
- Store pending reboot state.

### `power_profiles.rs`

- Detect PowerProfiles D-Bus owner.
- Read active profile.
- Optionally sync or warn.
- Do not fight platform profile by default.

## First 10 coding tasks

1. [x] Create Rust workspace with `legion-common`, `legion-daemon`, `legion-ui`, `legion-probe`.
2. [x] Implement `legion-common` capability and telemetry structs with serde JSON output.
3. [x] Implement read-only `legion-probe` for DMI, platform profile, battery charge type, hwmon fan/temp telemetry, LED nodes, and EnvyControl presence.
4. [x] Add fixture-based tests for probe parsing using fake sysfs directories.
5. [x] Implement daemon skeleton with zbus service and read-only hardware, capability, telemetry, refresh, and raw-report methods.
6. [x] Add systemd, D-Bus, polkit, desktop, metainfo, and RPM packaging assets while keeping write methods absent.
7. [x] Add a read-only sysfs fixture capture workflow for real hardware reports.
8. [x] Capture and add a runtime fixture from the supported local 82WM machine.
9. [x] Draft write-method D-Bus contracts without enabling writes.
10. [x] Implement validators, pure dry-run planning, and read-only daemon planning methods before any write methods.
11. [x] Add read-only tray/status helper with runtime-derived menu diagnostics.
12. [x] Add disabled tray autostart packaging placeholder.
13. [x] Add bracketed battery `charge_types` parsing from the runtime fixture.
14. [x] Add read-only StatusNotifier tray backend while keeping runtime menu rows informational-only.
15. [x] Add repeatable StatusNotifier tray smoke workflow before enabling autostart.
16. [x] Add read-only daemon session-bus dev mode and record automated KDE StatusNotifier registration smoke.
17. [x] Record KDE Plasma Wayland StatusNotifier tooltip/menu/quit smoke evidence.
18. [x] Record GNOME-with-extension smoke blocker from the current KDE session.
19. [x] Mark GNOME AppIndicator extension path untested and continue read-only MVP work.
20. [x] Add read-only `BAT0` capacity/status/health telemetry for overview data.
21. [x] Add read-only EnvyControl GPU mode query when installed.
22. [x] Add read-only UI `--overview` summary for MVP overview data, LED brightness, and firmware toggle values.
23. [x] Add rollback guidance to read-only GPU dry-run plans.
24. [x] Add read-only fan restore/default dry-run planning.
25. [x] Add app-state-only GPU pending-reboot state with durable TOML storage.
26. [x] Add app-state-only last-known-good fan curve capture.
27. [x] Surface durable app state in tray and GTK read-only views.
28. [x] Surface saved fan curve state in read-only `--overview` output.

## Test strategy

### Unit tests

- Parse platform profile choices.
- Parse `charge_types` choices.
- Detect hwmon devices by labels/name.
- Validate fan curve schemas.
- Validate firmware attribute metadata.
- Validate EnvyControl output parsing.
- Validate missing paths produce hidden capabilities.

### Fixture tests

Use fake sysfs layouts:

```text
tests/fixtures/sysfs-82wm-confirmed/
├── sys/firmware/acpi/platform_profile
├── sys/firmware/acpi/platform_profile_choices
├── sys/class/power_supply/BAT0/charge_types
├── sys/class/hwmon/hwmon0/name
├── sys/class/hwmon/hwmon0/fan1_input
└── sys/class/leds/platform::ylogo/brightness
```

Test cases:

- confirmed 82WM layout;
- runtime-captured 82WM layout with bracketed `charge_types` current value;
- missing battery charge type;
- missing fan curve;
- firmware attributes present with valid metadata;
- firmware attributes present with missing metadata;
- bogus zero fan curve readback;
- changing hwmon numbering.

### Integration tests

Mark hardware tests so they never run in CI by accident:

```bash
LEGION_CONTROL_HW_TEST=1 cargo test --test hardware_82wm -- --ignored
```

Hardware tests should start read-only. Write tests require explicit environment variables, for example:

```bash
LEGION_CONTROL_ALLOW_FAN_WRITE=1
LEGION_CONTROL_ALLOW_BATTERY_WRITE=1
```

### Manual validation checklist

#### Read-only probe

- [x] Confirm product/vendor DMI fields.
- [x] Confirm platform profile current value.
- [x] Confirm platform profile choices.
- [x] Confirm battery charge type choices.
- [x] Confirm fan RPM sensors.
- [x] Confirm temperature sensors.
- [x] Confirm Y-logo LED node.
- [x] Confirm Fn-lock LED node.
- [x] Confirm EnvyControl presence/mode.
- [x] Confirm firmware attributes are correctly present or absent.

Read-only evidence from the current 82WM target on 2026-04-25:

- Product: `82WM Legion Pro 5 16ARX8`.
- Platform profile: `performance`; choices: `quiet`, `balanced`, `balanced-performance`, `performance`.
- Battery charge type: `Long_Life`; choices: `Fast`, `Standard`, `Long_Life`.
- Sensors: 2 fan RPM sensors and 15 temperature sensors; one fan curve capability.
- LEDs: `platform::fnlock`, `platform::ylogo`.
- Firmware toggles: `camera_power`, `conservation_mode`, `fan_mode`, `fn_lock`, `usb_charging`.
- GPU: EnvyControl reports `nvidia`.

#### Platform profile write

- [ ] Switch to `quiet`.
- [ ] Read back `quiet`.
- [ ] Switch to `balanced`.
- [ ] Read back `balanced`.
- [ ] Switch to `balanced-performance` if listed.
- [ ] Switch to `performance`.
- [ ] Confirm Fn+Q external changes are detected.

#### Battery charge type write

- [ ] Read current value.
- [ ] Set `Standard`.
- [ ] Read back `Standard`.
- [ ] Set `Long_Life`.
- [ ] Read back `Long_Life`.
- [ ] Set `Fast` only after warning.
- [ ] Restore preferred mode.

#### Fan preset write

- [ ] Save current curve if trustworthy.
- [ ] Apply quiet preset.
- [ ] Verify fan response and no thermal runaway.
- [ ] Apply balanced preset.
- [ ] Verify readback or expected driver behavior.
- [ ] Test restore safe/default.
- [ ] Change platform profile and verify fan state refresh.
- [ ] Suspend/resume and verify selected behavior.

#### GPU mode flow

- [x] Query current mode.
- [x] Set pending target without reboot as app state only.
- [x] Verify overview pending-reboot output.
- [ ] Reboot manually.
- [ ] Confirm new mode.
- [ ] Confirm rollback instructions are accurate before encouraging users.

#### Packaging

- [ ] Install RPM on clean Fedora 43.
- [ ] Confirm daemon starts or fails cleanly on unsupported hardware.
- [ ] Confirm polkit prompts appear.
- [ ] Confirm desktop file validates.
- [ ] Confirm AppStream validates.
- [ ] Confirm GNOME tray caveat text is shown if extension missing.
- [ ] Confirm KDE tray behavior.
