# Live Write Validation

Use `scripts/capture-write-validation-report.sh` to capture evidence for the
currently implemented reversible write surface:

- `platform_profile`
- `battery_charge_type`
- `platform::ylogo`
- `fn_lock`
- `camera_power`
- `usb_charging`
- **Fan preset dry-run** (`--plan-fan-preset balanced-daily`) when the probe reports fan curves
- **Fan restore-to-auto dry-run** (`--plan-restore-auto-fan`) under the same condition
- **GPU mode dry-run** (`--plan-gpu-mode <mode>`) when the probe reports EnvyControl with `status: probe_only` and a known current mode (`integrated` / `hybrid` / `nvidia`) so an alternate mode exists

Fan and GPU plan rows are **plan capture only**: even with `--execute`, the harness never
calls `ApplyFanPreset`, `RestoreAutoFan`, or GPU execution (those remain absent or policy-gated). The
primary `tests/fixtures/sysfs-82wm-confirmed` tree includes a full 10-point
`pwm1_auto_point{1..10}_{temp,pwm}` set so packaged preset dry-run planning
matches CI expectations; slimmer local trees may still show `plan-failed` until
they expose the same shape.

This harness does not bypass existing safety constraints. It only drives the
validated D-Bus surface that already exists in the daemon and records evidence
around it.

## Modes

### Plan-only

Default mode is safe and read-mostly:

```bash
scripts/capture-write-validation-report.sh \
  --output target/validation/<machine-label>-plan
```

This mode:

- starts a private session bus
- starts a private read-mostly daemon against the selected `--sysfs-root`
- captures `--status`, `--overview`, `--diagnostics`, tray text output, and
  all relevant `--plan-*` commands
- optionally attaches a nested compatibility bundle
- optionally attaches a tray smoke bundle when run from a graphical session

Use this mode in CI against fixtures and on real hardware before any live write
attempt.

### Execute mode

Execute mode is explicit:

```bash
scripts/capture-write-validation-report.sh \
  --output target/validation/<machine-label>-live \
  --execute \
  --system-bus
```

Or target an existing private/session daemon:

```bash
scripts/capture-write-validation-report.sh \
  --output target/validation/<machine-label>-live \
  --execute \
  --bus-address <dbus-address>
```

Execute mode expects an already-running privileged daemon. The harness does not
start a root-capable daemon for you.

## Recommended live workflow

1. Start the daemon with only the write flag needed for the control under test.
2. Run the harness in plan-only mode first and inspect the generated plan files.
3. Run the harness in execute mode.
4. Review the generated `WriteExecutionResult` JSON for the apply step.
5. Confirm the manual hardware behavior for that control.
6. Confirm the revert step restores the original state.

Do not batch multiple unrelated writes into one manual decision. The harness
records them all, but operator review should still happen one control at a
time.

## Per-control operator checks

- `platform_profile`: confirm overview/tray state and basic system behavior
  reflect the requested profile before reverting.
- `battery_charge_type`: confirm charge-type read-back and battery telemetry
  remain consistent before reverting.
- `platform::ylogo`: confirm the physical LED changes and returns.
- `fn_lock`: confirm both the indicator LED and actual Fn key behavior change
  and then revert.
- `camera_power`: confirm camera apps lose and regain the device; restart apps
  if needed.
- `usb_charging`: confirm sysfs read-back first; treat off-state charging
  behavior as a separate slower manual check.
- `fan_preset_balanced_daily`: read the plan JSON / stderr artifact only; do not
  expect apply execution from this harness.
- `restore_auto_fan`: read the plan JSON only; do not expect restore execution
  from this harness.
- `gpu_mode`: read the plan JSON only; `SetGpuMode` is not an executable D-Bus method in RatVantage.

## Bundle contents

The harness writes a report bundle containing:

- `validation-report.json`
- `validation-report.md`
- `operator-checklist.md`
- `commands.log`
- `environment.txt`
- `before/`
- `after/`
- `steps/`
- optional `compat/`
- optional `tray-smoke/`

Fixture-backed rollback tests and GTK/tray smoke remain necessary, but they do
not count as live-device write validation by themselves.
