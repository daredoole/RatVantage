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

## Fan execution gate (policy)

`ApplyFanPreset` and `RestoreAutoFan` are **not** exposed as executable D-Bus
methods in shipping builds, and there are **no** `--enable-fan-*` daemon CLI
flags yet. The UI and tray remain **dry-run / planning only** for fan curves.

Shipping fan execution requires **all** of:

1. Narrow **live** write-validation bundles (per-control style) once a maintainer
   workflow exists for fan sysfs, not only fixture dry-runs.
2. Explicit **maintainer agreement** and polkit actions wired for those methods.
3. Rollback/read-back tests on real hardware classes you intend to support.

Until then, treat fan rows in validation output as **evidence of planning only**.

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

With `--execute`, the script **fails fast** if `before/diagnostics.json` is not
valid JSON with a `raw_probe_report` object (for example a one-line
`ServiceUnknown` error), so you do not get an empty `steps/` directory by mistake.

### From a git checkout (no `legion-control-daemon.service` yet)

If `systemctl status legion-control-daemon.service` says **Unit could not be found**,
you have not installed the **RPM** (or copied the unit into systemd). That is
normal for pure source trees.

`cargo run … --diagnostics` talks to the **system** bus by default. Without a
running root daemon **and** the system D-Bus policy, you get:

`ServiceUnknown: The name is not activatable`.

**One-time system integration from the repo** (D-Bus policy + polkit actions;
still no systemd unit):

```bash
cd /path/to/RatVantage
sudo ./scripts/install-dev-system-integration.sh
```

**Build and run the daemon in a spare terminal** (leave it running; adjust flags
to match the control family you are capturing):

```bash
cargo build --release -p legion-control-daemon
sudo mkdir -p /var/lib/legion-control
sudo ./target/release/legion-control-daemon --enable-platform-profile-write
```

**Verify from another terminal** (should print JSON, not `ServiceUnknown`):

```bash
cargo run -q -p legion-control-ui -- --diagnostics | head -40
busctl --system list | grep -i ratvantage || true
```

**Then capture** (example):

```bash
scripts/capture-write-validation-report.sh \
  --output target/validation/82wm-live-platform_profile \
  --execute --execute-only platform_profile --system-bus
```

Stop the foreground daemon with **Ctrl+C** when finished. When you later install
the proper RPM/COPR package, prefer the packaged **systemd** unit instead of a
manual `sudo ./target/...` process.

### Execute-only (one write family per bundle)

For PRs and release notes, **prefer** a separate output directory per control
family so each bundle is easy to review:

```bash
scripts/capture-write-validation-report.sh \
  --output target/validation/<machine>-live-platform_profile \
  --execute \
  --execute-only platform_profile \
  --system-bus
```

With `--execute-only <control_id>`, the script still records **plans** for every
available control, but runs **apply + revert** only for the matching `control_id`
(see table below). Other controls show status `execute-skipped-filter` in
`validation-report.json` when their plan succeeded.

Valid `control_id` values (must match exactly):

| `control_id` | Daemon flags (system service override / manual run) |
|---|---|
| `platform_profile` | `--enable-platform-profile-write` |
| `battery_charge_type` | `--enable-battery-charge-type-write` |
| `platform::ylogo` | `--enable-led-state-write` |
| `fn_lock` | `--enable-ideapad-toggle-write` |
| `camera_power` | `--enable-camera-power-write` |
| `usb_charging` | `--enable-usb-charging-write` |

Example: edit the systemd unit drop-in or ExecStart so **only** the flag for the
family under test is enabled, reload, restart the daemon, run the harness, then
turn flags off again.

Reference dry-run / plan-only daemon flags (from `docs/session-handoff.md`):

```text
cargo run -p legion-control-daemon -- --enable-platform-profile-write --enable-battery-charge-type-write --enable-led-state-write --enable-ideapad-toggle-write --enable-camera-power-write
```

Add `--enable-usb-charging-write` when capturing USB charging evidence.

## Recommended live workflow

1. Pick **one** row from the table above.
2. Start the root daemon with **only** the write flag(s) needed for that row
   (`SetIdeapadToggle` is shared; `fn_lock` still uses the ideapad flag, while
   `camera_power` / `usb_charging` use their dedicated flags).
3. Run plan-only capture on `/` sysfs first and inspect plan JSON under `steps/`.
4. Run execute capture with `--execute-only <control_id>` and `--system-bus`
   (or `--bus-address` for a session daemon started with the same policy).
5. Review `validation-report.md`, `validation-report.json`, and `steps/*apply*.json`
   for `WriteExecutionResult` / polkit outcomes.
6. Confirm the manual hardware behavior for that control, then confirm revert
   restored the original state.

Do not batch unrelated daemon flag groups into one “first live test” decision.
The harness can still **plan** every control in one run; **execute-only** keeps
apply+revert narrow.

## GNOME tray smoke (deferred when on KDE-only)

StatusNotifier tray autostart stays **off** until GNOME + AppIndicator smoke
exists. If your daily session is **KDE Plasma**, use the existing KDE tray smoke
workflow under `scripts/smoke-statusnotifier-tray.sh` and `target/smoke/`;
GNOME-specific capture is **not** required for your own evidence bundles.

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

## Reviewing a bundle locally

From the repo root (requires `jq`):

```bash
scripts/review-write-validation-bundle.sh target/validation/<your-bundle-dir>
```

To produce a single shareable archive (requires `zip` only; default name is
`<bundle-name>.zip` beside the bundle directory):

```bash
scripts/archive-validation-bundle.sh target/validation/<your-bundle-dir>
```

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
