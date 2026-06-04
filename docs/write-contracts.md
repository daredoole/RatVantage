# Write Contracts

`legion_common::WRITE_METHOD_CONTRACTS` is the source of truth for write-surface
metadata. Some reversible contracts are now implemented behind daemon policy
flags and polkit authorization, while higher-risk surfaces remain draft-only.

The active daemon exposes the following read-only methods:

- `GetHardwareSummary`
- `GetCapabilities`
- `RefreshCapabilities`
- `GetTelemetry`
- `GetRawProbeReport`
- `PlanPlatformProfileWrite`
- `PlanPrepareCustomThermalMode`
- `PlanBatteryChargeTypeWrite`
- `PlanLedStateWrite`
- `PlanKeyboardRgbWrite`
- `PlanIdeapadToggleWrite`
- `PlanGpuModeWrite`
- `PlanOpenRgbKeyboardRgbBridge`
- `PlanFirmwareAttributeWrite`
- `PlanCpuBoostWrite`
- `PlanCurveOptimizerAllCoreWrite`
- `PlanConservationModeWrite`
- `PlanAmdGpuDpmForceLevelWrite`
- `PlanCustomThermalFirmwareAttributeWrite`
- `PlanCustomThermalFirmwarePptPresetWrite`
- `PlanCustomThermalFanPresetWrite`
- `PlanCustomThermalRestoreAutoFanWrite`
- `PlanFanPresetWrite`
- `PlanFirmwareAttributeResetWrite`
- `PlanRestoreAutoFanWrite`
- `GetHardwareProfiles`
- `GetHardwareProfileTriggers`
- `GetHardwareProfileApplyPreview`
- `GetHardwareProfileTriggerApplyPreview`
- `GetLastHardwareProfileApply`

And the following gated write methods:

- `SetPlatformProfile`
- `SetBatteryChargeType`
- `SetLedState`
- `SetKeyboardRgb`
- `SetIdeapadToggle`
- `SetGpuMode`
- `SetCpuGovernor`
- `SetCpuEpp`
- `SetFirmwareAttribute`
- `SetCpuBoost`
- `SetConservationMode`
- `SetAmdGpuDpmForceLevel`
- `SetCurveOptimizerAllCore`
- `ApplyHardwareProfile`
- `ApplyHardwareProfileTrigger`

All of those execution paths still remain disabled by default unless the daemon
is started with the matching write-enable flags.

## Implemented Gated Writes

| Method | Capability | polkit action | Request | Safety scope |
|---|---|---|---|---|
| `SetPlatformProfile` | `platform_profile` | `org.ratvantage.LegionControl1.set-platform-profile` | `{"profile":"string"}` | exact runtime choice validation, read-back, rollback |
| `SetBatteryChargeType` | `battery_charge_type` | `org.ratvantage.LegionControl1.set-battery-charge-type` | `{"charge_type":"string"}` | exact runtime choice validation, read-back, rollback |
| `SetLedState` | `leds` | `org.ratvantage.LegionControl1.set-led-state` | `{"led_id":"platform::ylogo","enabled":"bool"}` | currently restricted to `platform::ylogo`, binary LED only |
| `SetKeyboardRgb` | `keyboard_rgb` | `org.ratvantage.LegionControl1.set-keyboard-rgb` | `{"effect":"string","colors":{"zone_id":"#RRGGBB"},"brightness":"u8","speed":"u8|null"}` | gated method with validator, SDK helper read-back, and rollback tests; dev daemon args can enable the OpenRGB SDK helper, and OpenRGB SDK `backend_ready` is now evidence-backed by snapshot support |
| `SetIdeapadToggle` | `ideapad_toggles` | `org.ratvantage.LegionControl1.set-ideapad-toggle` | `{"toggle_id":"fn_lock|camera_power|usb_charging|fan_mode","enabled":"bool"}` | allowlisted ideapad toggles only; `fn_lock` also requires paired `platform::fnlock` LED corroboration |
| `SetGpuMode` | `gpu` | `org.ratvantage.LegionControl1.set-gpu-mode` | `{"mode":"integrated|hybrid|nvidia"}` | EnvyControl daemon execution, reboot-pending state recorded after command success |
| `SetCpuGovernor` | `cpu_power` | `org.ratvantage.LegionControl1.set-cpu-governor` | `{"governor":"string"}` | exact runtime choice validation, per-policy opt-in |
| `SetCpuEpp` | `cpu_power` | `org.ratvantage.LegionControl1.set-cpu-epp` | `{"epp":"string"}` | exact runtime choice validation, per-policy opt-in |
| `SetFirmwareAttribute` | `firmware_attributes` | `org.ratvantage.LegionControl1.set-firmware-attribute` | `{"attribute_id":"ppt_pl1_spl|ppt_pl2_sppt|ppt_pl3_fppt","value":"integer"}` | 82WM PPT allowlist, scalar min/max/increment validation, read-back, rollback |
| `SetCpuBoost` | `cpu_power` | `org.ratvantage.LegionControl1.set-cpu-boost` | `{"boost":"0|1"}` | binary validator, read-back, rollback |
| `SetConservationMode` | `ideapad_toggles` | `org.ratvantage.LegionControl1.set-conservation-mode` | `{"enabled":"0|1"}` | dedicated conservation-mode write path through ideapad toggle validation |
| `SetAmdGpuDpmForceLevel` | `amd_gpu_power_dpm` | `org.ratvantage.LegionControl1.set-amd-gpu-dpm-force-level` | `{"level":"auto|low"}` | exact driver choice validation, read-back, rollback |
| `SetCurveOptimizerAllCore` | `curve_optimizer_all_core` | `org.ratvantage.LegionControl1.set-curve-optimizer` | `{"offset":"0|-1..-30"}` | experimental RyzenAdj backend, write-only without `ryzen_smu`, explicit reset-to-zero path |
| `ApplyHardwareProfile` | `hardware_profiles` | `org.ratvantage.LegionControl1.apply-hardware-profile` | `{"profile_id":"string"}` | validates stored profile first, requires `--enable-hardware-profile-apply`, executes existing gated actions including reboot-required `gpu_mode` in preview order, stops on first non-applied action, records last run |
| `ApplyHardwareProfileTrigger` | `hardware_profile_triggers` | `org.ratvantage.LegionControl1.apply-hardware-profile` | `{"trigger_id":"ac_connected|ac_disconnected|resume|platform_profile_changed|manual"}` | resolves a persisted trigger mapping to a stored profile, then uses the same profile apply gate and execution path |

## Plan-Only Prerequisites

| Method | Capability | polkit action | Request | Safety scope |
|---|---|---|---|---|
| `PrepareCustomThermalMode` | `platform_profile` | `org.ratvantage.LegionControl1.set-platform-profile` | `{}` | read-only dry-run plan for switching to `custom` before firmware PPT or fan plans; normal `SetPlatformProfile custom` remains blocked until live execution evidence proves safety |
| `PlanCustomThermalFirmwareAttributeWrite` | `platform_profile` + `firmware_attributes` | plan-only | `{"attribute_id":"ppt_pl1_spl|ppt_pl2_sppt|ppt_pl3_fppt","value":"integer"}` | read-only sequence preview: prepare `custom`, stage the dependent PPT plan as custom-satisfied, and list reverse rollback order |
| `PlanCustomThermalFirmwarePptPresetWrite` | `platform_profile` + `firmware_attributes` | plan-only | `{"preset_id":"conservative|balanced-custom|performance-custom|reset-defaults"}` | read-only sequence preview: prepare `custom`, stage validated PL1/PL2/PL3 PPT preset plans as custom-satisfied, and list reverse rollback order |
| `PlanCustomThermalFanPresetWrite` | `platform_profile` + `fan_curves` | plan-only | `{"preset_id":"string"}` | read-only sequence preview: prepare `custom`, stage fan preset plan as custom-satisfied, and list reverse rollback order |
| `PlanCustomThermalRestoreAutoFanWrite` | `platform_profile` + `fan_curves` | plan-only | `{}` | read-only sequence preview: prepare `custom`, stage restore-auto-fan plan as custom-satisfied, and list reverse rollback order |

## Disabled Drafts

| Method | Capability | Future polkit action | Request | Risk |
|---|---|---|---|---|
| `ApplyFanPreset` | `fan_curves` | `org.ratvantage.LegionControl1.apply-fan-preset` | `{"preset_id":"string"}` | experimental write |
| `RestoreAutoFan` | `fan_curves` | `org.ratvantage.LegionControl1.restore-auto-fan` | `{}` | experimental write |

## Required Gates Before Enabling

- Keep new D-Bus write methods absent until validators and rollback/read-back behavior exist; keep live execution disabled until a real backend has evidence.
- Authorize each write through polkit inside the daemon, never through GUI sudo.
- Validate requested values against choices read at runtime.
- Use `validate_platform_profile_choice` and
  `validate_battery_charge_type_choice` before any future sysfs write.
- Use `validate_led_state_request` and `validate_ideapad_toggle_request`
  before any future LED or ideapad-toggle write expansion.
- Use `validate_gpu_mode_choice` before any EnvyControl GPU mode write.
- Use `validate_keyboard_rgb_request` before any future keyboard RGB backend write.
- Use `validate_fan_preset_choice` before any future fan curve preset write.
- Use `plan_platform_profile_write`, `plan_battery_charge_type_write`,
  `plan_led_state_write`, `plan_keyboard_rgb_write`,
  `plan_ideapad_toggle_write`, `plan_gpu_mode_write`,
  `plan_fan_preset_write`, and `plan_restore_auto_fan_write` for
  validator-backed dry-run plans before any future daemon write implementation.
- Read back the changed sysfs value after each write.
- Store previous values before writing.
- Restore previous values on read-back failure when still safe and listed.
- Add private-bus contract tests for each enabled method.
- Add fixture coverage for success, unsupported, invalid choice, and rollback paths.

## Dry-Run Planning

Dry-run planning is pure shared logic in `legion-common`. It returns the future
method name, capability ID, polkit action, target path/tool, previous value,
requested value, read-back requirement, rollback value, rollback instructions,
reboot requirement, safety notes, and ordered execution step IDs.

The plan functions do not write sysfs. The daemon exposes them as read-only
D-Bus planning methods so clients can preview validation, polkit action, target
path, rollback value, and execution steps before any write method is attempted.
`legion-control-ui --plan-platform-profile <profile>` and
`legion-control-ui --plan-battery-charge-type <charge_type>` print sysfs-backed
plans as JSON for CLI inspection. `legion-control-ui --plan-led-state
<led_id=on|off>` and `legion-control-ui --plan-ideapad-toggle
<toggle_id=on|off>` do the same for the currently enabled reversible LED and
ideapad-toggle surfaces. `legion-control-ui --plan-gpu-mode <mode>` prints the
EnvyControl GPU mode plan, marks the future change as reboot required, and
includes rollback guidance for the previous mode.
`legion-control-ui --plan-prepare-custom-thermal` prints the plan-only
platform-profile preparation step when `platform_profile_choices` lists
`custom`; it does not unblock the ordinary `SetPlatformProfile custom` path.
`legion-control-ui --plan-custom-thermal-firmware-attribute <attribute=value>`,
`--plan-custom-thermal-firmware-ppt-preset <preset_id>`,
`--plan-custom-thermal-fan-preset <preset_id>`, and
`--plan-custom-thermal-restore-auto-fan` print sequence previews: prepare
`custom` first, then run the dependent PPT/fan plan or PL1/PL2/PL3 preset
plans, with rollback in reverse order. They remain read-only previews.
`legion-control-ui --plan-firmware-attribute-reset <attribute_id>` prints a
validated reset-to-default PPT plan only when firmware metadata exposes
`default_value`; missing default metadata blocks the plan.
`legion-control-ui --plan-fan-preset <preset_id>` prints the packaged fan preset
plan only when the preset schema is valid and the detected fan curve exposes a
complete 10-point writable shape. Fan preset, restore-auto-fan, and firmware
PPT plans annotate the custom thermal prerequisite from the detected
`platform_profile` capability: missing capability, unavailable `custom`, already
in `custom`, or required switch to `custom` with previous-profile rollback
guidance.
`legion-control-ui --plan-restore-auto-fan` prints the future restore/default
fan-control plan when a fan curve capability is detected.
`legion-control-ui --plan-keyboard-rgb <json>` prints a validated keyboard RGB
dry-run plan when the probe exposes a fixture-backed `KeyboardRgbCapability`.
`legion-control-ui --plan-openrgb-keyboard-rgb <json>` prints a dry-run-only
OpenRGB bridge command preview when OpenRGB readiness probing detects a Lenovo
keyboard RGB device; it still marks read-back as required and does not execute
OpenRGB.
`legion-control-ui --set-keyboard-rgb <json>` calls the gated D-Bus method and
currently reports policy/backend blocks unless tests inject the fake backend.
Real execution and GTK/tray controls must wait for a proven live backend with
read-back and reset evidence.
Use `scripts/capture-keyboard-rgb-evidence.sh --output <dir>` for read-only
candidate evidence bundles with descriptor hashes/hex; the script does not open
`/dev/hidraw` or send HID reports.
Use `scripts/compare-keyboard-rgb-evidence.sh --output <dir> <bundle...>` to
cluster one or more RGB evidence bundles by protocol signature and keep
promotion blockers visible. The comparison is read-only and must not be treated
as write-backend evidence.
Use `scripts/capture-keyboard-rgb-openrgb-bridge-evidence.sh --output <dir>`
for dry-run OpenRGB bridge evidence; add `--execute` only when the operator is
ready for a brief keyboard RGB change and restore. The bundle records profile
save/apply/mode-read-back/restore status, parses saved OpenRGB profile strings,
scans profiles for requested RGB/BGR color byte triplets, and removes stale
profile artifacts before each capture. Backend readiness is proven only when
mode read-back, restore read-back, profile saves, and requested color-byte
evidence all pass. A zero-exit OpenRGB CLI command is not sufficient: local live
evidence currently exits zero while mode read-back remains `Direct` and color
bytes are absent, so this path stays blocked until OpenRGB SDK/read-back or
another proven command path supplies stronger evidence.
Review the resulting bundle with
`scripts/review-keyboard-rgb-openrgb-bridge-evidence.sh --require-promotable
<bundle>` before any OpenRGB execution backend is added to the daemon.
Use `scripts/capture-keyboard-rgb-openrgb-sdk-evidence.sh --output <dir>` for
read-only OpenRGB SDK controller evidence. The SDK helper starts or connects to
the SDK server, requests controller data, parses active mode, modes, zones,
LEDs, and colors, and records blockers without setting RGB. Local evidence now
reads the Lenovo keyboard controller through the SDK path, and the
operator-triggered SDK write evidence proves mode/color write plus restore with
SDK read-back; the OpenRGB CLI bridge remains non-promotable because its live
mode/color read-back is negative.
Use `scripts/status-keyboard-rgb-openrgb-bridge-evidence.sh` to summarize the
current dry-run and execute bundle state plus the next promotion action.
The user-session installer also exposes these as
`ratvantage-capture-keyboard-rgb-openrgb-bridge-evidence` and
`ratvantage-review-keyboard-rgb-openrgb-bridge-evidence`, plus
`ratvantage-keyboard-rgb-openrgb-bridge-status` for the status summary, so the
GTK Appearance page can copy stable commands without relying on a repo checkout
path.
Use `sudo /usr/local/sbin/ratvantage-setup-keyboard-rgb-openrgb-access --user
<user>` for the one-time i2c group/module/udev setup when OpenRGB access is
missing. RatVantage also exposes `PlanOpenRgbAccessSetup` and gated
`SetupOpenRgbAccess` so the GUI can ask the root daemon through the dedicated
`org.ratvantage.LegionControl1.setup-openrgb-access` polkit action when the
daemon is started with `--enable-openrgb-access-setup`; the CLI mirrors this as
`legion-control-ui --plan-openrgb-access-setup <user>` and
`legion-control-ui --setup-openrgb-access <user>`. User-session installs also
copy the same setup script to
`$HOME/.local/libexec/ratvantage/ratvantage-setup-keyboard-rgb-openrgb-access`
and install a `ratvantage-setup-keyboard-rgb-openrgb-access` wrapper that uses
the root helper with `sudo -n` when the dev passwordless rule exists before
falling back to an interactive sudo command. The readiness checker is installed
as `ratvantage-check-keyboard-rgb-openrgb`, so the GTK setup command works
without relying on a repo checkout path.
Higher-risk draft write methods should remain outside the zbus `#[interface]`
implementation until write support is deliberately enabled.

## Out Of Scope

- Raw WMI calls.
- Raw EC writes.
- Arbitrary sysfs writes.
- Overclocking controls.
- Raw firmware attribute writes outside the explicit PPT allowlist and validator.
