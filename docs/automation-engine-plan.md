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
- `desktop_power_profile_changed`
- `gpu_mode_reboot_completed`

`platform_profile_changed` is now daemon-owned when the opt-in automation
observer is enabled: the observer records a first-sample baseline, detects an
external `platform_profile` value change, and applies the mapped hardware
profile trigger through the existing profile-apply policy/validators/read-back
path.

`desktop_power_profile_changed` is daemon-owned when the opt-in automation
observer is enabled: the observer records a first-sample baseline from the
desktop PowerProfiles API, records recent changes, detects a subsequent KDE,
GNOME, or power-profiles-daemon active-profile change, and applies the mapped
hardware profile trigger through the existing profile-apply
policy/validators/read-back path.

`gpu_mode_reboot_completed` is daemon-owned when the opt-in automation observer
is enabled: if the durable pending GPU mode matches the current EnvyControl
probe after reboot, the daemon clears pending state and applies the mapped
hardware profile trigger through the existing profile-apply policy path.

Needed additions:

- Battery threshold trigger, sampled from `BAT*` capacity/status.
- Periodic idle trigger with cooldown, for state correction.

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
- EnvyControl GPU mode, reboot-gated. [implemented as a hardware-profile action; execution remains policy/reboot gated]
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
  Quiet on Battery [implemented starter], Integrated GPU on Battery [implemented starter],
  Resume Repair [implemented starter], GPU Reboot Repair [implemented starter],
  Fn+Q Tuning Repair [implemented starter],
  RGB Breathing Profile [implemented starter], CO Experimental Profile [implemented starter].
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
5. Done: add `ac_profile_router` rules that select one stored profile while AC
   is online and another while AC is offline, with preview, observer cooldown,
   and apply through the existing hardware-profile policy path.
6. Done: add a GTK Quick Template starter that saves
   `plugged_in_balanced` / `battery_saver` profiles, maps AC connect/disconnect
   triggers, and stores an `ac_router` rule.
7. Done: add GTK saved-rule editing for `ac_profile_router` rules, including
   AC/battery profile pickers, enabled state, cooldown, preview/test-run, save,
   and delete.
8. Done: add a GTK Quick Template starter that saves an `rgb_breathing_blue`
   hardware profile with staged `keyboard_rgb` actions; preview works through
   native RGB or the OpenRGB bridge, while apply remains evidence-gated.
9. Done: add a GTK Quick Template starter that saves a
   `co_experimental_minus10` hardware profile with staged all-core Curve
   Optimizer `-10`; preview works immediately, while apply remains write-only,
   policy-gated, and dependent on the existing CO execution evidence path.
10. Done: add `battery_profile_threshold` rules that select one stored profile
   from battery capacity telemetry, support at/below or at/above threshold
   matching, optional AC-online/offline requirements, observer cooldown, preview,
   test-run, GTK saved-rule rendering, and GTK editing for profile, threshold,
   direction, AC condition, enabled state, cooldown, save, preview/test-run, and
   delete.
11. Done: extend the GTK Quiet on Battery starter so it also saves a
   `quiet_below_30` `battery_profile_threshold` rule in addition to the
   AC-unplugged trigger and fan-preset app-state mapping.
12. Done: add a GTK Battery Threshold Rule starter that saves
    `battery_recovered_balanced` and maps battery capacity 80% or higher to it
    through `battery_recovered_80`.
13. Done: add `legion-control-ui --automation-diagnostics` as a combined
    read-only support snapshot for hardware profiles, trigger mappings,
    automation rules, last automation runs, and the last hardware-profile apply;
    GTK Diagnostics surfaces a copyable command for this snapshot.
14. Done: add backend setup status for RyzenAdj / `ryzen_smu` through daemon
    detection, `legion-control-ui --ryzen-backend-status`, the Profiles page,
    and the Diagnostics page.
15. Done: add a `ryzen_smu` setup assistant in read-only/generate-command mode
    through `legion-control-ui --ryzen-smu-setup` plus copyable Diagnostics
    commands. RatVantage does not install or load kernel modules automatically.
16. Done: add an AC CPU Performance Router GTK starter that saves
    `ac_cpu_performance` / `battery_cpu_efficiency` hardware profiles with
    platform profile, CPU governor, CPU EPP, and CPU boost actions, maps
    AC connect/disconnect triggers, and stores `ac_cpu_router`; daemon preview
    coverage proves the selected profile expands to platform/governor/EPP/boost
    plans through existing gates.
17. Done: add a GTK Create Automation Rule form for custom
    `battery_profile_threshold` rules. The form accepts rule ID, label, target
    hardware profile, threshold, direction, optional AC condition, and cooldown,
    then saves through `SetAutomationRule` so daemon validation remains the
    authority.
18. Done: add a GTK Create AC Router Rule form for custom
    `ac_profile_router` rules. The form accepts rule ID, label, AC-online
    profile, battery-power profile, and cooldown, rejects same-profile routing
    locally, then saves through `SetAutomationRule` so daemon validation remains
    the authority.
19. Done: add a GTK Create Fast Charge Rule form for custom
    `fast_charge_until_threshold` rules. The form accepts rule ID, label,
    fast-charge profile, protect profile, threshold, AC requirement, and
    cooldown, rejects same-profile routing locally, then saves through
    `SetAutomationRule` so daemon validation remains the authority.
20. Done: add a Balanced Daily Mixed GTK starter that saves
    `balanced_daily_mixed` with platform profile, battery charge type, CPU
    governor/EPP/boost, AMD GPU DPM, and staged keyboard RGB actions, plus a
    `balanced -> balanced-daily` fan-preset mapping. Daemon preview coverage
    proves the mixed profile expands to platform, battery, RGB, CPU, and AMD GPU
    DPM dry-run plans without enabling new execution paths.
21. Done: add a Quiet Battery Mixed GTK starter that saves
    `quiet_battery_mixed` with low-power platform profile, Conservation charge
    type, CPU governor/EPP/boost efficiency settings, AMD GPU DPM low, and staged
    keyboard RGB actions, plus a `low-power -> quiet-office` fan-preset mapping.
    Daemon preview coverage proves the mixed profile expands to platform,
    battery, RGB, CPU, and AMD GPU DPM dry-run plans without enabling new
    execution paths.
22. Done: add a Performance AC Mixed GTK starter that saves
    `performance_ac_mixed` with performance platform profile, Standard charge
    type, CPU governor/EPP/boost performance settings, AMD GPU DPM auto, and
    staged keyboard RGB actions, maps `ac_connected` to the profile, and records
    a `performance -> performance-sustained` fan-preset mapping. Daemon preview
    coverage proves the mixed profile expands to platform, battery, RGB, CPU,
    and AMD GPU DPM dry-run plans without enabling new execution paths.
23. Done: add a GPU Reboot Repair GTK starter that saves
    `gpu_reboot_completed_balanced` with a balanced platform-profile repair
    action and maps `gpu_mode_reboot_completed` to it. Execution remains behind
    the daemon's existing hardware-profile policy/read-back gates.
24. Done: promote the OpenRGB SDK fallback for daemon `SetKeyboardRgb` when the
    SDK backend is evidence-ready; live system-daemon dogfood applied Breathing
    with four-zone `#333333` read-back through `SetOpenRgbKeyboardRgbSdk`.
25. Done: add `desktop_power_profile_changed` as a daemon-owned hardware-profile
    trigger. The opt-in automation observer baselines and records desktop
    PowerProfiles changes, applies a mapped profile through existing gates, CLI
    exposes `--recent-desktop-power-profile-changes`, automation diagnostics
    includes the history, and GTK Automations renders the trigger and recent
    changes.
