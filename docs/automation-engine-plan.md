# Automation engine plan

Goal: make RatVantage automations configurable enough for daily use without
letting the GUI silently perform risky hardware changes. Automations should
compose validated daemon write plans, explain unavailable controls, and keep
driver/backend setup as an explicit operator action.

## Product shape

Automations are named rules:

- **When**: event trigger, optional conditions, optional debounce.
- **If**: current hardware state predicates.
- **Then**: ordered profile actions through daemon dry-run/write methods.
- **Safety**: confirmation level, rollback policy, cooldown, and failure mode.
- **Evidence**: last run, per-action results, readback, skipped actions, and
  user-facing reason text.

The UI should expose this as:

- Preset templates for common flows.
- A rule builder for advanced users.
- A dry-run preview before saving.
- A test-run button that executes through the same daemon/polkit path.
- A clear event log with the first failed action and rollback result.

## Example: fast charge then protect battery

Template: `Fast charge until threshold`

Inputs:

- Fast-charge mode: `Fast`.
- Protect mode: `Long_Life` / conservation mode.
- Threshold: default `80%`, user configurable.
- Optional AC-only condition.
- Optional quiet/performance profile to apply while charging.

Rules:

1. On `ac_connected`:
   - If battery `< threshold`, set battery charge type to `Fast` and set
     `conservation_mode=0` if available.
   - Optionally set platform profile / CPU governor / EPP for the user's chosen
     charging profile.
2. On periodic battery sample while AC is connected:
   - If battery `>= threshold`, set battery charge type to `Long_Life` or
     `conservation_mode=1`.
3. On `ac_disconnected`:
   - Apply the user's battery profile, for example `platform_profile=low-power`,
     `cpu_governor=powersave`, `cpu_epp=power`, `cpu_boost=0`, and AMD DPM
     `low` or `auto`.

The rule must show that exact sequence in dry-run before saving. It must not
fight itself by setting battery charge type and conservation mode in conflicting
ways in the same action group.

## Automation triggers

Already modeled trigger IDs:

- `manual`
- `ac_connected`
- `ac_disconnected`
- `resume`
- `platform_profile_changed`

Needed additions:

- Battery threshold trigger, sampled from `BAT*` capacity/status.
- Periodic idle trigger with cooldown, for state correction.
- GPU mode pending/reboot completion trigger.
- Optional desktop power profile change trigger.

Each trigger should be daemon-owned. The GTK UI configures mappings; it should
not run background hardware writes itself.

## Profile action model

Profile actions should cover validated controls:

- Platform profile.
- Battery charge type.
- Conservation mode.
- CPU governor.
- CPU EPP.
- CPU boost.
- AMD GPU DPM force level.
- EnvyControl GPU mode, reboot-gated.
- Curve Optimizer all-core, advanced/write-only.
- Firmware attributes only when the live evidence marks them promotable.

On this 82WM today:

- PPT firmware attributes are detected but not promotable: live writes return
  `Device or resource busy (os error 16)`.
- Fan mode is detected but not promotable: writing `1` reads back `0`.
- Fan curves and fan RPM telemetry are unavailable because the kernel exposes no
  writable `pwm*_auto_point*` files and no `fan*_input`.
- AMD GPU manual clocks stay read-only/deferred.
- Curve Optimizer works through RyzenAdj, but is write-only until a readback
  backend exists.

The automation builder should render those as unavailable or experimental
actions with the evidence reason, not as normal toggles.

## ryzen_smu setup assistant

RatVantage should not auto-install an out-of-tree kernel module from the GUI.
Instead, add a setup assistant that:

1. Detects whether `/sys/kernel/ryzen_smu_drv` exists.
2. Detects whether `ryzen_smu` is loaded.
3. Detects kernel headers, DKMS, Secure Boot state, and package-manager support.
4. Shows the exact backend consequence:
   - without `ryzen_smu`: RyzenAdj Curve Optimizer is write-only.
   - with `ryzen_smu`: enable provider probing for readback/validation where
     supported.
5. Generates distro-specific install commands for the operator to run.
6. Re-probes after reboot/module load and records backend evidence.

Current upstream context:

- `leogx9r/ryzen_smu` describes a Linux kernel driver exposing AMD Ryzen SMU
  access and the `/sys/kernel/ryzen_smu_drv/pm_table` interface.
- `amkillam/ryzen_smu` is an updated fork of the now-unmaintained original with
  merge requests and updates applied.

The first implementation should only detect and explain. Any build/install
button must be separate, explicit, root-gated, logged, and reversible through
normal package/DKMS mechanisms.

## Backend/provider interface

Add a provider abstraction for advanced CPU telemetry:

- `RyzenAdjProvider`: existing command backend, write-only for CO on this host.
- `RyzenSmuProvider`: future sysfs/backend readback provider.
- `AmdctlProvider`: experimental P-state VID provider, separate from Curve
  Optimizer.

Provider capability fields:

- backend id/version/path.
- supported controls.
- readback support.
- write support.
- safety level.
- setup status.
- last probe error.

The UI should show backend status before allowing advanced CPU actions in
automations.

## Execution policy

Every automation run must:

1. Refresh capabilities.
2. Resolve actions to dry-run plans.
3. Reject unavailable or negative-evidence controls unless the rule explicitly
   allows experimental actions.
4. Execute actions in order through daemon/polkit.
5. Stop on first non-applied action unless the rule says skip-on-failure.
6. Run rollback for reversible actions when readback fails.
7. Persist an `AutomationRun` with per-action results.

## Data model additions

Add daemon-owned state:

- `AutomationRule`
- `AutomationTrigger`
- `AutomationCondition`
- `AutomationAction`
- `AutomationRun`
- `AutomationRunActionResult`
- `BackendSetupStatus`

Rules should be serializable TOML/JSON and import/exportable for issue reports.

## GTK UX

Automations page should have:

- Template cards: Battery Saver, AC Performance, Fast Charge Until Threshold,
  Quiet on Battery, Integrated GPU on Battery, CO Experimental Profile.
- Rule list with enabled/disabled switch.
- Trigger selector.
- Condition editor.
- Ordered action list.
- Dry-run preview.
- Test run.
- Last run summary.
- Evidence warnings for negative/unavailable controls.

Do not hide unsupported controls; show them as disabled with the reason.

## Implementation order

1. Done: extend profile actions to include battery charge type and reject saved
   profiles that try to set battery charge type and conservation mode in the
   same action group. GTK also has a fast-charge starter that saves
   `fast_charge` / `battery_protect` profiles and maps `ac_connected` to
   `fast_charge`.
2. Done: add `AutomationRule` persistence and the first rule kind,
   `fast_charge_until_threshold`, with AC-online and battery-capacity evaluation.
3. Done: add daemon preview/test-run methods and GTK controls for saved rules.
4. Done: add an opt-in daemon-owned AC/battery observer loop
   (`--enable-automation-observer`) that evaluates saved rules every minute,
   applies matching profiles through the daemon write path, records the last
   run, and suppresses repeated same-profile applies during cooldown.
5. Add CLI diagnostics for automation rules and last runs.
6. Add backend setup status for RyzenAdj / `ryzen_smu`.
7. Add `ryzen_smu` setup assistant in read-only/generate-command mode.
8. Add advanced CPU automation actions only after backend status is visible.
