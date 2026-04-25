# Safety Model

## Safety principles

1. Probe first. Never assume a control exists because the model is 82WM.
2. The GUI never writes hardware paths.
3. The daemon exposes only narrow, named operations.
4. Every write validates against current runtime capabilities.
5. Every write has a read-back check where possible.
6. Every safety-sensitive write is logged.
7. Every risky control has a restore path.
8. Missing sysfs paths are normal and must not crash the app.
9. “Unsupported” is better than “probably works” for raw firmware features.
10. The app must fail safe: no write is better than a bad write.

## Fan curve safety rules

Fan control is the highest-value feature and the highest MVP risk.

### Hard rules

- Never expose a raw fan sysfs writer.
- Never write a partial fan curve.
- Never write if any point fails validation.
- Never write if the detected fan count or point count is inconsistent.
- Never rely on `hwmonN` numbering.
- Never treat zeroed or incomplete fan curve readback as safe default data.
- Never write a user preset loaded from disk until the daemon validates it.

### Validation rules

A fan curve write must satisfy:

- fan count equals detected fan count;
- point count equals detected point count;
- each PWM/RPM value is within discovered limits or conservative configured clamps;
- temperature points, if writable, are monotonic and within sane bounds;
- no point is missing;
- values are integers;
- requested curve does not reduce cooling below the packaged minimum-safe preset unless an advanced override is explicitly enabled;
- all files are writable before any write starts.

### Apply process

1. Read current telemetry.
2. Validate the requested curve.
3. Save current curve as last-known-good if readback is trustworthy.
4. Write all points in a deterministic order.
5. Read back all writable points.
6. If readback fails or differs unexpectedly, re-apply last-known-good or packaged safe preset.
7. Log result.
8. Emit `FanCurveChanged` or `ErrorOccurred`.

### Reset behavior

Lenovo firmware or drivers may reset fan curves after:

- platform profile change;
- suspend/resume;
- reboot;
- driver reload;
- firmware events.

The daemon should detect these events and either:

- re-apply the selected fan preset if user enabled persistence; or
- notify that the curve may have reset.

## Battery mode safety rules

Primary battery control is `charge_types`.

### Hard rules

- Write only values listed by `/sys/class/power_supply/BAT0/charge_types`.
- Treat `Fast`, `Standard`, and `Long_Life` as firmware-defined charge algorithms.
- Do not claim exact thresholds unless the kernel exposes a real threshold attribute.
- Do not control `charge_types` and `conservation_mode` simultaneously.
- If both exist, prefer `charge_types` and mark `conservation_mode` as compatibility-only.

### UI wording

- `Fast`: “Fast charge; useful for quick top-ups; may increase battery heat/wear.”
- `Standard`: “Normal charging behavior.”
- `Long_Life`: “Battery longevity/conservation mode; firmware decides exact stop level.”

### Confirmation

- `Fast` should require at least a lightweight confirmation the first time.
- `Long_Life` and `Standard` can be routine polkit actions.
- DKMS-only rapid charge, if ever exposed, requires a stronger warning and should be mutually exclusive with conservation-style modes.

## Platform profile safety rules

### Hard rules

- Read `/sys/firmware/acpi/platform_profile_choices` every daemon start.
- Show only listed profiles.
- Write only exact listed strings.
- Do not synthesize `custom`, `max-power`, `extreme`, or vendor-specific names.
- After changing profile, refresh fan capabilities and fan curve state.
- If a write fails or read-back does not match, show an error and revert UI state.

### `custom` profile

`custom` is special. Kernel Lenovo Gamezone documentation describes it as the mode that enables user PPT and fan-curve modifications.

Rules:

- Show `custom` only if listed.
- If an advanced firmware attribute requires `custom`, the daemon must clearly tell the user it is switching platform profile first.
- If `custom` is not listed, hide PPT controls even if firmware attribute directories exist, unless proven effective by a documented future ABI.

### `max-power` profile

`max-power` is high risk.

Rules:

- Show only if listed in choices.
- Require explicit confirmation every time until real-world validation is complete.
- Disable on battery unless the user overrides in an advanced setting.
- Log separately with high-risk flag.

## GPU switching safety rules

MVP uses EnvyControl only.

### Hard rules

- GPU switching is a pending operation, not an instant toggle.
- Always show “reboot required.”
- Do not call raw Lenovo WMI MUX/G-Sync methods.
- Validate requested mode against EnvyControl-supported values.
- Capture EnvyControl output and log it.
- After setting a mode, store pending mode in daemon state.
- Provide clear rollback instructions.

### UI flow

1. User opens GPU page.
2. UI shows current mode from `envycontrol --query`.
3. User selects target mode.
4. UI shows warning:
   - “This changes GPU boot configuration and requires reboot.”
   - “A bad NVIDIA/Wayland setup can cause a black screen.”
5. User confirms.
6. Daemon runs EnvyControl through a controlled adapter.
7. UI shows pending reboot banner.
8. User may reboot now or later.

### Rollback text

The project should document recovery from a bad GPU switch:

```text
If the system boots to a black screen, switch to a TTY with Ctrl+Alt+F3,
log in, and run the documented recovery command to return to hybrid mode.
Keep a live USB available before testing GPU mode changes.
```

Exact commands should be generated from the detected EnvyControl installation and distro setup.

## Firmware attribute safety rules

Firmware attributes are advanced controls.

### Hard rules

- Hide the whole page if `/sys/class/firmware-attributes/lenovo-wmi-other/attributes` is missing.
- Hide any attribute missing `current_value`, `min_value`, `max_value`, `scalar_increment`, or `type`.
- Write only integer attributes the app explicitly supports.
- Validate value range and increment.
- Read back after write.
- Check pending reboot/save state if the firmware-attributes class exposes it.
- Require `custom` profile when Lenovo Gamezone docs say custom is required.
- Never expose raw firmware attribute names as free-form input.

### PPT-specific rules

- PL1/SPL: advanced but lower risk than PL3.
- PL2/SPPT: advanced with warning.
- PL3/FPPT: high risk; require confirmation every time.
- Values above packaged conservative maximum should require an extra “expert mode” setting.
- Values below a reasonable idle floor should be blocked unless future hardware testing proves safety.
- Always provide restore default.

## Rollback strategy

Rollback must be automatic where possible and documented where not.

### Stored rollback data

Store in `/var/lib/legion-control/state.toml`:

- last-known-good fan curve;
- packaged safe fan preset ID;
- last platform profile set by the daemon;
- previous battery charge type;
- previous LED states;
- firmware attribute defaults and previous values;
- pending GPU mode;
- schema version.

### Automatic rollback cases

- Fan curve write fails read-back: re-apply last-known-good or packaged safe preset.
- Platform profile write fails read-back: restore previous profile if still listed.
- Firmware attribute write fails validation/read-back: restore previous value if safe.
- Daemon startup detects invalid saved state: ignore saved state and log warning.

### Manual rollback cases

- GPU mode black screen after reboot.
- Firmware change requiring reboot.
- External tool changed NVIDIA configuration outside the daemon.

Manual rollback instructions belong in README and GPU page.

## Logging strategy

Log to journald from the daemon.

### Log levels

| Level | Examples |
|---|---|
| `INFO` | capability detected/missing, read-only telemetry startup, successful routine write |
| `WARN` | probe inconsistency, read-back mismatch, fan curve reset suspected, external owner conflict |
| `ERROR` | write failed, validation failed, rollback failed, EnvyControl failed |
| `DEBUG` | full probe details, raw path mapping, telemetry samples when debug enabled |

### Required log fields

- action ID;
- caller bus name;
- user ID if available;
- hardware capability ID;
- old value;
- requested value;
- result;
- rollback result;
- sysfs path ID, not necessarily full raw path in normal logs;
- timestamp.

Do not log secrets, environment variables, or arbitrary command lines with user data.

## What should require polkit confirmation

| Action | Suggested polkit level |
|---|---|
| Read capabilities/telemetry | No auth |
| Set platform profile | Routine auth or active local wheel allow rule |
| Set battery charge type | Routine auth |
| Toggle Y-logo LED | Routine auth or no prompt for active local wheel users |
| Apply packaged fan preset | Routine auth during development; active local wheel allow later |
| Apply custom fan curve | `auth_admin_keep` |
| Restore auto/safe fan | Routine auth |
| Set functional Fn-lock | Routine auth |
| Toggle always-on USB charging | `auth_admin_keep` |
| Toggle camera power | `auth_admin_keep` plus UI confirmation |
| Toggle touchpad hardware power | `auth_admin_keep` plus UI confirmation |
| Set GPU mode pending | `auth_admin_keep` plus UI confirmation |
| Reboot now | Desktop/system policy, usually auth depending session |
| Set PL1/SPL | `auth_admin` |
| Set PL2/SPPT | `auth_admin` |
| Set PL3/FPPT | `auth_admin` every time |
| Set `max-power` | `auth_admin` every time |
| Raw sysfs or raw WMI | No action; unsupported |
