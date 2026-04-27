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

## Notes

- If you use **`install-dev-systemd-ratvantage.sh`**, each reinstall **replaces** the
  unit flags and the copied binary; **`systemctl restart legion-control-daemon.service`**
  after each reinstall so the running process matches. The install script already runs
  **`systemctl daemon-reload`** and tries **D-Bus `ReloadConfig`**; the extra
  **`daemon-reload`** in each block above is safe if you edited the unit by hand or
  want copy-paste consistency.
- **`install-dev-systemd-ratvantage.sh`** refuses to run if **`rpm -qa`** shows any
  package whose name starts with **`legion-control`** (packaged unit would conflict).
  Remove those RPMs first, or use the packaged unit instead of this dev path.
- Fan and GPU captures remain **plan-only** in the harness; see
  [fan-gpu-execution-policy.md](fan-gpu-execution-policy.md).
