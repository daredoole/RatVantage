# Live validation evidence runbook (82WM-style machine)

Use this after **`scripts/install-dev-system-integration.sh`** (D-Bus + polkit)
and either:

- **`scripts/install-dev-systemd-ratvantage.sh`** (recommended: `systemctl` unit), or  
- a **foreground** `sudo ./target/release/legion-control-daemon …` process.

Each **execute** capture should enable **only** the daemon flags needed for that
`--execute-only` control. Prefer a **fresh `--output` directory** per family.

## 0. Build

```bash
cd /path/to/RatVantage
cargo build --release -p legion-control-daemon
```

## 1. `platform_profile`

```bash
sudo ./scripts/install-dev-systemd-ratvantage.sh ./target/release/legion-control-daemon -- --enable-platform-profile-write
sudo systemctl daemon-reload
sudo systemctl restart legion-control-daemon.service

scripts/capture-write-validation-report.sh \
  --output target/validation/82wm-live-platform_profile \
  --execute --execute-only platform_profile --system-bus

scripts/review-write-validation-bundle.sh target/validation/82wm-live-platform_profile
scripts/archive-validation-bundle.sh target/validation/82wm-live-platform_profile
```

Add review assertions once the bundle should prove an execute result, for
example:

```bash
scripts/review-write-validation-bundle.sh \
  --require-mode execute \
  --require-control platform_profile=pass \
  --control platform_profile \
  target/validation/82wm-live-platform_profile
```

## 2. `battery_charge_type`

```bash
sudo ./scripts/install-dev-systemd-ratvantage.sh ./target/release/legion-control-daemon -- --enable-battery-charge-type-write
sudo systemctl daemon-reload
sudo systemctl restart legion-control-daemon.service

scripts/capture-write-validation-report.sh \
  --output target/validation/82wm-live-battery_charge_type \
  --execute --execute-only battery_charge_type --system-bus

scripts/review-write-validation-bundle.sh target/validation/82wm-live-battery_charge_type
scripts/archive-validation-bundle.sh target/validation/82wm-live-battery_charge_type
```

## 3. `platform::ylogo`

```bash
sudo ./scripts/install-dev-systemd-ratvantage.sh ./target/release/legion-control-daemon -- --enable-led-state-write
sudo systemctl daemon-reload
sudo systemctl restart legion-control-daemon.service

scripts/capture-write-validation-report.sh \
  --output target/validation/82wm-live-ylogo \
  --execute --execute-only 'platform::ylogo' --system-bus

scripts/review-write-validation-bundle.sh target/validation/82wm-live-ylogo
scripts/archive-validation-bundle.sh target/validation/82wm-live-ylogo
```

## 4. `fn_lock`

```bash
sudo ./scripts/install-dev-systemd-ratvantage.sh ./target/release/legion-control-daemon -- --enable-ideapad-toggle-write
sudo systemctl daemon-reload
sudo systemctl restart legion-control-daemon.service

scripts/capture-write-validation-report.sh \
  --output target/validation/82wm-live-fn_lock \
  --execute --execute-only fn_lock --system-bus

scripts/review-write-validation-bundle.sh target/validation/82wm-live-fn_lock
scripts/archive-validation-bundle.sh target/validation/82wm-live-fn_lock
```

## 5. `camera_power`

```bash
sudo ./scripts/install-dev-systemd-ratvantage.sh ./target/release/legion-control-daemon -- --enable-camera-power-write
sudo systemctl daemon-reload
sudo systemctl restart legion-control-daemon.service

scripts/capture-write-validation-report.sh \
  --output target/validation/82wm-live-camera_power \
  --execute --execute-only camera_power --system-bus

scripts/review-write-validation-bundle.sh target/validation/82wm-live-camera_power
scripts/archive-validation-bundle.sh target/validation/82wm-live-camera_power
```

## 6. `usb_charging`

```bash
sudo ./scripts/install-dev-systemd-ratvantage.sh ./target/release/legion-control-daemon -- --enable-usb-charging-write
sudo systemctl daemon-reload
sudo systemctl restart legion-control-daemon.service

scripts/capture-write-validation-report.sh \
  --output target/validation/82wm-live-usb_charging \
  --execute --execute-only usb_charging --system-bus

scripts/review-write-validation-bundle.sh target/validation/82wm-live-usb_charging
scripts/archive-validation-bundle.sh target/validation/82wm-live-usb_charging
```

## 7. `fan_mode`

```bash
sudo ./scripts/install-dev-systemd-ratvantage.sh ./target/release/legion-control-daemon -- --enable-fan-mode-write
sudo systemctl daemon-reload
sudo systemctl restart legion-control-daemon.service

scripts/capture-write-validation-report.sh \
  --output target/validation/82wm-live-fan_mode \
  --execute --execute-only fan_mode --system-bus

cat >target/validation/82wm-live-fan_mode/operator-checklist.md <<'EOF'
# Operator checklist

- `fan_mode`: record whether Auto (0) -> Full speed (1) is accepted or whether read-back stays unchanged.
- Thermal/fan behavior: <record audible fan response, temperature trend, and any instability here>.
- Negative 82WM evidence is acceptable when the bundle shows read-back stayed at Auto (0) and the daemon restored the previous value.
EOF

scripts/review-write-validation-bundle.sh \
  --require-mode execute \
  --require-control fan_mode=executed \
  --control fan_mode \
  target/validation/82wm-live-fan_mode
scripts/archive-validation-bundle.sh target/validation/82wm-live-fan_mode
```

## 8. `conservation_mode`

```bash
sudo ./scripts/install-dev-systemd-ratvantage.sh ./target/release/legion-control-daemon -- --enable-conservation-mode-write
sudo systemctl daemon-reload
sudo systemctl restart legion-control-daemon.service

scripts/capture-write-validation-report.sh \
  --output target/validation/82wm-live-conservation_mode \
  --execute --execute-only conservation_mode --system-bus

scripts/review-write-validation-bundle.sh \
  --require-mode execute \
  --require-control conservation_mode=pass \
  --control conservation_mode \
  target/validation/82wm-live-conservation_mode
scripts/archive-validation-bundle.sh target/validation/82wm-live-conservation_mode
```

## 9. `cpu_boost`

```bash
sudo ./scripts/install-dev-systemd-ratvantage.sh ./target/release/legion-control-daemon -- --enable-cpu-boost-write
sudo systemctl daemon-reload
sudo systemctl restart legion-control-daemon.service

scripts/capture-write-validation-report.sh \
  --output target/validation/82wm-live-cpu_boost \
  --execute --execute-only cpu_boost --system-bus

scripts/review-write-validation-bundle.sh \
  --require-mode execute \
  --require-control cpu_boost=pass \
  --control cpu_boost \
  target/validation/82wm-live-cpu_boost
scripts/archive-validation-bundle.sh target/validation/82wm-live-cpu_boost
```

## 10. `cpu_governor`

```bash
sudo ./scripts/install-dev-systemd-ratvantage.sh ./target/release/legion-control-daemon -- --enable-cpu-governor-write
sudo systemctl daemon-reload
sudo systemctl restart legion-control-daemon.service

scripts/capture-write-validation-report.sh \
  --output target/validation/82wm-live-cpu_governor \
  --execute --execute-only cpu_governor --system-bus

scripts/review-write-validation-bundle.sh \
  --require-mode execute \
  --require-control cpu_governor=pass \
  --control cpu_governor \
  target/validation/82wm-live-cpu_governor
scripts/archive-validation-bundle.sh target/validation/82wm-live-cpu_governor
```

## 11. `cpu_epp`

```bash
sudo ./scripts/install-dev-systemd-ratvantage.sh ./target/release/legion-control-daemon -- --enable-cpu-epp-write
sudo systemctl daemon-reload
sudo systemctl restart legion-control-daemon.service

scripts/capture-write-validation-report.sh \
  --output target/validation/82wm-live-cpu_epp \
  --execute --execute-only cpu_epp --system-bus

scripts/review-write-validation-bundle.sh \
  --require-mode execute \
  --require-control cpu_epp=pass \
  --control cpu_epp \
  target/validation/82wm-live-cpu_epp
scripts/archive-validation-bundle.sh target/validation/82wm-live-cpu_epp
```

## 12. Firmware PPT power limits

Run one PPT attribute per bundle. The harness selects a one-step in-range
request and reverts to the captured original value.

```bash
sudo ./scripts/install-dev-systemd-ratvantage.sh ./target/release/legion-control-daemon -- --enable-firmware-attribute-write
sudo systemctl daemon-reload
sudo systemctl restart legion-control-daemon.service

scripts/capture-write-validation-report.sh \
  --output target/validation/82wm-live-ppt_pl1_spl \
  --execute --execute-only firmware_attribute:ppt_pl1_spl --system-bus

scripts/review-write-validation-bundle.sh \
  --require-mode execute \
  --require-control firmware_attribute:ppt_pl1_spl=executed \
  --control firmware_attribute:ppt_pl1_spl \
  target/validation/82wm-live-ppt_pl1_spl

scripts/capture-write-validation-report.sh \
  --output target/validation/82wm-live-ppt_pl2_sppt \
  --execute --execute-only firmware_attribute:ppt_pl2_sppt --system-bus

scripts/review-write-validation-bundle.sh \
  --require-mode execute \
  --require-control firmware_attribute:ppt_pl2_sppt=executed \
  --control firmware_attribute:ppt_pl2_sppt \
  target/validation/82wm-live-ppt_pl2_sppt

scripts/capture-write-validation-report.sh \
  --output target/validation/82wm-live-ppt_pl3_fppt \
  --execute --execute-only firmware_attribute:ppt_pl3_fppt --system-bus

scripts/review-write-validation-bundle.sh \
  --require-mode execute \
  --require-control firmware_attribute:ppt_pl3_fppt=executed \
  --control firmware_attribute:ppt_pl3_fppt \
  target/validation/82wm-live-ppt_pl3_fppt
```

Review and archive each output directory separately.

## 11. `amd_gpu_dpm_force_level`

```bash
sudo ./scripts/install-dev-systemd-ratvantage.sh ./target/release/legion-control-daemon -- --enable-amd-gpu-dpm-write
sudo systemctl daemon-reload
sudo systemctl restart legion-control-daemon.service

scripts/capture-write-validation-report.sh \
  --output target/validation/82wm-live-amd_gpu_dpm_force_level \
  --execute --execute-only amd_gpu_dpm_force_level --system-bus

scripts/review-write-validation-bundle.sh \
  --require-mode execute \
  --require-control amd_gpu_dpm_force_level=pass \
  --control amd_gpu_dpm_force_level \
  target/validation/82wm-live-amd_gpu_dpm_force_level
scripts/archive-validation-bundle.sh target/validation/82wm-live-amd_gpu_dpm_force_level
```

## 12. `curve_optimizer_all_core`

This is experimental and write-only on the current 82WM without a `ryzen_smu`
read-back backend. The harness applies `-20`, records the daemon result, and
resets to `0`; run stability checks separately before keeping any CO value.
The aggregate verifier requires `operator-checklist.md` in this bundle to
mention `curve_optimizer_all_core`, reset, and stability.

```bash
sudo ./scripts/install-dev-systemd-ratvantage.sh ./target/release/legion-control-daemon -- --enable-curve-optimizer-write
sudo systemctl daemon-reload
sudo systemctl restart legion-control-daemon.service

scripts/capture-write-validation-report.sh \
  --output target/validation/82wm-live-curve_optimizer_all_core \
  --execute --execute-only curve_optimizer_all_core --system-bus

scripts/review-write-validation-bundle.sh \
  --require-mode execute \
  --require-control curve_optimizer_all_core=pass \
  --control curve_optimizer_all_core \
  target/validation/82wm-live-curve_optimizer_all_core
scripts/archive-validation-bundle.sh target/validation/82wm-live-curve_optimizer_all_core
```

## 13. `gpu_mode`

GPU mode changes may require logout or reboot and the harness does not
auto-revert them. Capture only after preparing a recovery path.

```bash
sudo ./scripts/install-dev-systemd-ratvantage.sh ./target/release/legion-control-daemon -- --enable-gpu-mode-write
sudo systemctl daemon-reload
sudo systemctl restart legion-control-daemon.service

scripts/capture-write-validation-report.sh \
  --output target/validation/82wm-live-gpu_mode \
  --execute --execute-only gpu_mode --system-bus

scripts/review-write-validation-bundle.sh \
  --require-mode execute \
  --require-control gpu_mode=executed \
  --control gpu_mode \
  target/validation/82wm-live-gpu_mode
```

## 14. Hardware profile apply / trigger apply

These require saved daemon state first. Create or import a narrow CPU driver
profile, map a trigger if needed, then enable profile apply plus every write
flag required by the profile actions. The full-set verifier requires per-action
results for `cpu_governor`, `cpu_epp`, and `cpu_boost`.

```bash
sudo ./scripts/install-dev-systemd-ratvantage.sh ./target/release/legion-control-daemon -- --enable-hardware-profile-apply --enable-cpu-governor-write --enable-cpu-epp-write --enable-cpu-boost-write
sudo systemctl daemon-reload
sudo systemctl restart legion-control-daemon.service

scripts/capture-write-validation-report.sh \
  --output target/validation/82wm-live-hardware_profile \
  --execute --execute-only hardware_profile --system-bus \
  --seed-hardware-profile 'validation_cpu_driver={"schema_version":1,"label":"Validation CPU driver behavior","actions":{"cpu_governor":"powersave","cpu_epp":"balance_performance","cpu_boost":"1"}}'

scripts/review-write-validation-bundle.sh \
  --require-mode execute \
  --require-control hardware_profile=executed \
  --control hardware_profile \
  target/validation/82wm-live-hardware_profile

scripts/capture-write-validation-report.sh \
  --output target/validation/82wm-live-hardware_profile_trigger \
  --execute --execute-only hardware_profile_trigger --system-bus \
  --seed-hardware-profile 'validation_cpu_driver={"schema_version":1,"label":"Validation CPU driver behavior","actions":{"cpu_governor":"powersave","cpu_epp":"balance_performance","cpu_boost":"1"}}' \
  --seed-hardware-profile-trigger manual=validation_cpu_driver

scripts/review-write-validation-bundle.sh \
  --require-mode execute \
  --require-control hardware_profile_trigger=executed \
  --control hardware_profile_trigger \
  target/validation/82wm-live-hardware_profile_trigger
```

## Notes

After all required execute bundles are captured, run the aggregate verifier:

```bash
scripts/verify-82wm-live-evidence.sh --root target/validation
```

The verifier fails until execute-mode evidence exists for each required 82WM
advanced control with the expected status, matching `execute_only`, and the
expected apply/revert payloads.

- If you use **`install-dev-systemd-ratvantage.sh`**, each reinstall **replaces** the
  unit flags and the copied binary; **`systemctl restart legion-control-daemon.service`**
  after each reinstall so the running process matches. The install script already runs
  **`systemctl daemon-reload`** and tries **D-Bus `ReloadConfig`**; the extra
  **`daemon-reload`** in each block above is safe if you edited the unit by hand or
  want copy-paste consistency.
- **`install-dev-systemd-ratvantage.sh`** refuses to run if **`rpm -qa`** shows any
  package whose name starts with **`legion-control`** (packaged unit would conflict).
  Remove those RPMs first, or use the packaged unit instead of this dev path.
- Fan curve captures remain **plan-only** in the harness. GPU mode is available
  only through explicit `--execute-only gpu_mode` and is not auto-reverted; see
  [fan-gpu-execution-policy.md](fan-gpu-execution-policy.md).
