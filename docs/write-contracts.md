# Draft Write Contracts

These contracts are design metadata only. They do not expose D-Bus write methods,
do not install active polkit actions, and do not add sysfs write code.

The active daemon must continue to expose only:

- `GetHardwareSummary`
- `GetCapabilities`
- `RefreshCapabilities`
- `GetTelemetry`
- `GetRawProbeReport`

## Disabled Drafts

| Method | Capability | Future polkit action | Request | Risk |
|---|---|---|---|---|
| `SetPlatformProfile` | `platform_profile` | `org.ratvantage.LegionControl1.set-platform-profile` | `{"profile":"string"}` | reversible write |
| `SetBatteryChargeType` | `battery_charge_type` | `org.ratvantage.LegionControl1.set-battery-charge-type` | `{"charge_type":"string"}` | reversible write |

The source of truth for draft metadata is
`legion_common::WRITE_METHOD_CONTRACTS`.

## Required Gates Before Enabling

- Keep the D-Bus methods absent until validators and rollback exist.
- Authorize each write through polkit inside the daemon, never through GUI sudo.
- Validate requested values against choices read at runtime.
- Use `validate_platform_profile_choice` and
  `validate_battery_charge_type_choice` before any future write.
- Use `plan_platform_profile_write` and `plan_battery_charge_type_write` for
  validator-backed dry-run plans before any future daemon write implementation.
- Read back the changed sysfs value after each write.
- Store previous values before writing.
- Restore previous values on read-back failure when still safe and listed.
- Add private-bus contract tests for each enabled method.
- Add fixture coverage for success, unsupported, invalid choice, and rollback paths.

## Dry-Run Planning

Dry-run planning is pure shared logic in `legion-common`. It returns the future
method name, capability ID, polkit action, sysfs path, previous value, requested
value, read-back requirement, rollback value, and ordered execution step IDs.

The plan functions do not write sysfs and are not exposed through D-Bus.

## Out Of Scope

- Raw WMI calls.
- Raw EC writes.
- Arbitrary sysfs writes.
- Overclocking controls.
- Firmware PPT writes before expert-mode policy, conservative bounds, and manual hardware validation.
