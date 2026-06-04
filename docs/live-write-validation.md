# Live Write Validation

Related docs:

- [live-validation-evidence-runbook.md](live-validation-evidence-runbook.md) — copy-paste blocks per control family on real hardware.
- [fan-gpu-execution-policy.md](fan-gpu-execution-policy.md) — why fan/GPU execution stays off until explicitly shipped.

Use `scripts/capture-write-validation-report.sh` to capture evidence for the
currently implemented reversible write surface:

- `platform_profile`
- `battery_charge_type`
- `platform::ylogo`
- `fn_lock`
- `camera_power`
- `usb_charging`
- `fan_mode`
- `conservation_mode`
- `cpu_governor`
- `cpu_epp`
- `cpu_boost`
- `firmware_attribute:ppt_pl1_spl`
- `firmware_attribute:ppt_pl2_sppt`
- `firmware_attribute:ppt_pl3_fppt`
- `amd_gpu_dpm_force_level`
- `keyboard_rgb`
- `curve_optimizer_all_core`
- `hardware_profile`
- `hardware_profile_trigger`
- **Fan preset dry-run** (`--plan-fan-preset balanced-daily`) when the probe reports fan curves
- **Fan restore-to-auto dry-run** (`--plan-restore-auto-fan`) under the same condition
- **GPU mode dry-run** (`--plan-gpu-mode <mode>`) and explicit execute-only capture (`gpu_mode`) when the probe reports EnvyControl with `status: probe_only` and a known current mode (`integrated` / `hybrid` / `nvidia`) so an alternate mode exists

Fan rows are **plan capture only**: even with `--execute`, the harness never
calls `ApplyFanPreset` or `RestoreAutoFan` (those remain absent or policy-gated). GPU mode execution is available only through explicit `--execute-only gpu_mode` and is not auto-reverted. The
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

### From a git checkout (no packaged `legion-control-daemon.service` yet)

If `systemctl status legion-control-daemon.service` says **Unit could not be found**,
you have not installed the **RPM** or registered a **dev** unit. That is normal for
a pure source tree until you install one of those paths.

`cargo run … --diagnostics` talks to the **system** bus by default. Without a
running root daemon **and** the system D-Bus policy, you get:

`ServiceUnknown: The name is not activatable`.

**One-time system integration from the repo** (D-Bus policy + polkit actions):

```bash
cd /path/to/RatVantage
sudo ./scripts/install-dev-system-integration.sh
```

**Option A — dev systemd + D-Bus activation (recommended for `systemctl`):**

```bash
cargo build --release -p legion-control-daemon
sudo ./scripts/install-dev-systemd-ratvantage.sh ./target/release/legion-control-daemon -- --enable-platform-profile-write
sudo systemctl enable --now legion-control-daemon.service
```

Re-run `install-dev-systemd-ratvantage.sh` with different `-- …` flags when you
narrow writes per family; it refuses if any RPM matching **`^legion-control`**
is installed. Per-control copy-paste flows: **[live-validation-evidence-runbook.md](live-validation-evidence-runbook.md)**.

**Option B — foreground daemon** (spare terminal; adjust flags to match the
control family you are capturing):

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

Stop a **foreground** daemon with **Ctrl+C** when finished. For **systemd**,
use `sudo systemctl stop legion-control-daemon.service`. When you later install
the proper RPM/COPR package, prefer the **packaged** unit instead of the dev
installer.

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

For profile evidence, seed a narrow CPU driver-behavior validation profile
before capture so the bundle is reproducible. The full-set verifier expects
per-action results for `cpu_governor`, `cpu_epp`, and `cpu_boost`, so enable
those write flags together with hardware-profile apply for this bundle:

```bash
scripts/capture-write-validation-report.sh \
  --output target/validation/82wm-live-profile \
  --execute --execute-only hardware_profile --system-bus \
  --seed-hardware-profile 'validation_cpu_driver={"schema_version":1,"label":"Validation CPU driver behavior","actions":{"cpu_governor":"powersave","cpu_epp":"balance_performance","cpu_boost":"1"}}'
```

For `fan_mode`, the complete evidence bundle must also include
`operator-checklist.md` with the observed `Auto (0) -> Full speed (1) -> Auto (0)`
sequence and thermal/fan behavior notes. The full-set verifier checks this
checklist because there is no portable fan RPM readback on the current host.

For trigger evidence, seed both the profile and mapping:

```bash
scripts/capture-write-validation-report.sh \
  --output target/validation/82wm-live-profile-trigger \
  --execute --execute-only hardware_profile_trigger --system-bus \
  --seed-hardware-profile 'validation_cpu_driver={"schema_version":1,"label":"Validation CPU driver behavior","actions":{"cpu_governor":"powersave","cpu_epp":"balance_performance","cpu_boost":"1"}}' \
  --seed-hardware-profile-trigger manual=validation_cpu_driver
```

Use `PROFILE_ID=@path/to/profile.json` when shell quoting the JSON would be
awkward. Seeding updates daemon configuration only; hardware-changing writes
still require `--execute`, the matching `--execute-only`, daemon write flags,
and the normal policy/read-back path.

Valid `control_id` values (must match exactly):

| `control_id` | Daemon flags (system service override / manual run) |
|---|---|
| `platform_profile` | `--enable-platform-profile-write` |
| `battery_charge_type` | `--enable-battery-charge-type-write` |
| `platform::ylogo` | `--enable-led-state-write` |
| `fn_lock` | `--enable-ideapad-toggle-write` |
| `camera_power` | `--enable-camera-power-write` |
| `usb_charging` | `--enable-usb-charging-write` |
| `fan_mode` | `--enable-fan-mode-write` |
| `conservation_mode` | `--enable-conservation-mode-write` |
| `cpu_governor` | `--enable-cpu-governor-write` |
| `cpu_epp` | `--enable-cpu-epp-write` |
| `cpu_boost` | `--enable-cpu-boost-write` |
| `firmware_attribute:ppt_pl1_spl` | `--enable-firmware-attribute-write` |
| `firmware_attribute:ppt_pl2_sppt` | `--enable-firmware-attribute-write` |
| `firmware_attribute:ppt_pl3_fppt` | `--enable-firmware-attribute-write` |
| `amd_gpu_dpm_force_level` | `--enable-amd-gpu-dpm-write` |
| `keyboard_rgb` | `--enable-keyboard-rgb-write` plus `--openrgb-sdk-helper <path>` when using the OpenRGB SDK backend |
| `gpu_mode` | `--enable-gpu-mode-write` |
| `curve_optimizer_all_core` | `--enable-curve-optimizer-write` |
| `hardware_profile` | `--enable-hardware-profile-apply` plus every flag needed by the saved profile actions |
| `hardware_profile_trigger` | `--enable-hardware-profile-apply` plus every flag needed by the resolved profile actions |

Example: reinstall **`install-dev-systemd-ratvantage.sh`** with only the flag for
the family under test, or edit the unit drop-in / `ExecStart` the same way, then
reload, restart the daemon, run the harness, then turn flags off again.

Reference **combined** flags for a broad dry-run daemon (not recommended for first
live execute tests — prefer one flag at a time):

```text
cargo run -p legion-control-daemon -- --enable-platform-profile-write --enable-battery-charge-type-write --enable-led-state-write --enable-ideapad-toggle-write --enable-camera-power-write
```

Add only the extra flag for the specific family under test. Advanced controls
(`firmware_attribute:*`, `cpu_governor`, `cpu_epp`, `cpu_boost`, `conservation_mode`,
`amd_gpu_dpm_force_level`, `keyboard_rgb`, `gpu_mode`, `curve_optimizer_all_core`,
`hardware_profile`, and `hardware_profile_trigger`) are planned in every run
when detectable, but the harness only executes them when `--execute-only`
matches that exact `control_id`.

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
- `fan_mode`: current 82WM live evidence is negative: requesting full-speed mode
  read back unchanged as `0` and the daemon restored `0`. Keep collecting
  operator thermal/fan notes, but do not promote this as a passing write until a
  different live path validates.
- `conservation_mode`: confirm conservation-mode read-back and battery behavior.
- `cpu_governor`: confirm `scaling_governor` read-back changes and returns under the current desktop power profile context.
- `cpu_epp`: confirm `energy_performance_preference` read-back changes and returns under `amd-pstate-epp`.
- `cpu_boost`: confirm boost read-back changes under the current amd-pstate mode.
- `firmware_attribute:ppt_pl1_spl`, `firmware_attribute:ppt_pl2_sppt`,
  `firmware_attribute:ppt_pl3_fppt`: current 82WM live evidence is negative:
  the Lenovo WMI firmware attribute paths are detected, but writes return
  `Device or resource busy (os error 16)`. Keep the bundles as executed
  negative evidence and do not promote PPT sliders until the busy firmware state
  is understood.
- `amd_gpu_dpm_force_level`: confirm DPM force-level read-back, then revert.
- `keyboard_rgb`: confirm the mode/colors change and then revert to the captured
  mode/colors. On OpenRGB-backed machines, start the SDK server first and run the
  daemon with `--enable-keyboard-rgb-write --openrgb-sdk-helper <path>`.
- `gpu_mode`: explicit execute-only capture only; prepare reboot/logout recovery
  before running because this path is not auto-reverted by the harness.
- `curve_optimizer_all_core`: explicit execute-only capture only; verify RyzenAdj
  success text, record write-only state, reset to `0`, then run stability checks
  outside the harness. The aggregate verifier requires the bundle's
  `operator-checklist.md` to mention `curve_optimizer_all_core`, reset, and
  stability.
- `hardware_profile`: requires at least one saved daemon hardware profile; inspect
  per-action results and last-apply state.
- `hardware_profile_trigger`: requires at least one trigger mapping; inspect the
  resolved preview and per-action results.
- `fan_preset_balanced_daily`: read the plan JSON / stderr artifact only; do not
  expect apply execution from this harness.
- `restore_auto_fan`: read the plan JSON only; do not expect restore execution
  from this harness.

## Reviewing a bundle locally

From the repo root (requires `jq`):

```bash
scripts/review-write-validation-bundle.sh target/validation/<your-bundle-dir>
```

For PR-quality evidence, make the review command assert the control that should
have passed:

```bash
scripts/review-write-validation-bundle.sh \
  --require-mode execute \
  --require-control cpu_boost=pass \
  --control cpu_boost \
  target/validation/82wm-live-cpu_boost
```

Use `planned` instead of `pass` for plan-only fixture bundles. Use `executed`
for intentional one-way evidence where no revert artifact is expected, including
`gpu_mode`, `hardware_profile`, and `hardware_profile_trigger`.

To verify the complete 82WM advanced evidence set after collecting all bundles:

```bash
scripts/verify-82wm-live-evidence.sh --root target/validation
```

This aggregate gate requires execute-mode bundles for PPT limits, conservation
mode, CPU boost, fan mode, AMD GPU DPM, Curve Optimizer apply/reset, GPU mode,
hardware profile apply, and hardware profile trigger apply. It also checks that
each bundle was captured with the matching `execute_only` value and includes the
expected apply/revert result payloads.

The required control/status/daemon-flag/output-slug matrix lives in
[`data/validation/82wm-live-evidence-requirements.tsv`](../data/validation/82wm-live-evidence-requirements.tsv).
The verifier itself is covered by `scripts/test-verify-82wm-live-evidence.sh`,
which is included in `scripts/ci-local.sh`.

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
