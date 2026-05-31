# Session handoff

**Token discipline:** read **[AGENTS.md](../AGENTS.md)** for crate map, build/test/lint, safety rules, and PR workflow. This file stays **short** (next tasks + prompt + safety). Long completed-slice log, full implemented inventory, and extended CLI list live in **[session-handoff-archive.md](session-handoff-archive.md)** — open only when you need that depth.

## Repo snapshot

- Repository: `https://github.com/daredoole/RatVantage` (private for now).
- Branch: `main`.
- Before editing: `git status --short --branch` and `git log --oneline -5`.
- Toolchain: `rust-toolchain.toml`; GTK needs a recent stable rustc (see toolchain file).

## Milestone (compact)

Pre-alpha: Rust workspace builds with CI green; polkit-gated daemon + GTK + StatusNotifier tray. Reversible writes now cover platform profile, battery charge type, ylogo LED, ideapad toggles (`fn_lock`, `camera_power`, `usb_charging`, `fan_mode`), firmware PPT limits, CPU governor, CPU EPP, CPU boost, conservation mode, AMD GPU DPM force level, and EnvyControl GPU mode execution behind explicit `--enable-*-write` flags. GTK fan-mode UI labels `fan_mode` as `Auto (0)` / `Full speed (1)` and keeps it separate from fan curves. GTK AMD GPU DPM apply is disabled until an operator confirms the display/GPU stability risk. Experimental all-core Curve Optimizer writes use daemon-only RyzenAdj execution behind `--enable-curve-optimizer-write`; GTK hides those controls behind an explicit Advanced CPU Tuning switch and includes a reset-to-zero button through the same daemon path. Ryzen backend status is read-only through daemon/CLI/diagnostics/GTK and includes RyzenAdj detection plus a `ryzen_smu` setup assistant that only displays commands/notes. The path remains write-only until a `ryzen_smu` read-back provider exists. Fan curves remain plan/preview only until a live writable curve surface exists. GTK UI: **sidebar nav layout** matching design hero — fixed 196px sidebar with RVLogo (Cairo-drawn ember octagon), 9 nav buttons driving adw::ViewStack (no ViewSwitcher); risk badge pills with Adwaita CSS variables; `style.css` loaded via CssProvider at startup. All write-button handlers use `make_client()` (respects dev private bus); action buttons styled with `suggested-action`/`pill` CSS. **9-tab dashboard** (Overview, Power, Battery, GPU, Fans, Devices, Automations, Settings, Diagnostics). Daemon-owned hardware profiles persist supported actions, including battery charge type for fast-charge profiles, return validator-backed dry-run previews, execute manual apply via `ApplyHardwareProfile`, map supported triggers to profiles, and record the last per-action apply run for CLI/GTK/tray diagnostics; profiles that set battery charge type and `conservation_mode` together are rejected because those firmware behaviors overlap. Automation rules persist in daemon state; `fast_charge_until_threshold` evaluates AC-online + battery capacity, previews the selected profile, test-runs through normal hardware-profile apply, and can run automatically when the daemon starts with `--enable-automation-observer`. GTK exposes saved-rule last-run history plus a real editor for enabled state, threshold, profile pickers, AC requirement, cooldown, save, test-run, preview, and delete. The automation observer polls once per minute, records last runs, and suppresses repeated same-profile applies during each rule's cooldown. Current persistence covers automation rules/last-runs, hardware profiles, hardware profile trigger mappings, hardware profile last-apply state, GPU pending state, Curve Optimizer last-write state, fan preset mapping/reapply policy, and last-known-good fan curves. Workspace tests and clippy are green. Live execute evidence: [live-validation-evidence-runbook.md](live-validation-evidence-runbook.md), [live-write-validation.md](live-write-validation.md).

## Next tasks

1. Continue the automation engine described in [automation-engine-plan.md](automation-engine-plan.md): advanced CPU actions already execute through hardware profiles and GTK Automations now surfaces profile-level governor/EPP/boost/CO actions with read-back/write-only warnings when rules reference those profiles. The next automation slice is richer rule authoring beyond `fast_charge_until_threshold` and safer profile editing for advanced CPU action creation. Battery charge type is a real hardware-profile action; same-profile conflicts with conservation mode are rejected; GTK has a starter that saves `fast_charge` / `battery_protect` profiles plus an editable `fast_charge_until_80` rule; automatic execution is opt-in with `--enable-automation-observer`.
1. Continue live write-validation on supported Legion hardware, **one control at a time**. Accepted evidence now exists for CPU boost, CPU governor, CPU EPP, conservation mode, AMD GPU DPM force level, Curve Optimizer apply/reset, EnvyControl GPU mode execution, one saved hardware profile manual apply, and one hardware-profile trigger apply. Firmware PPT writes reached the Lenovo WMI attribute files but all three returned `Device or resource busy (os error 16)`, so PPT stays gated/experimental until the busy firmware state is understood. Fan mode has negative live evidence (`1` read back as `0`, then rollback restored `0`) and should stay unpromoted unless a different validated driver path appears. The full live verifier now accepts those PPT/fan bundles only as negative execute evidence with the exact failure signatures. The deliverable audit and evidence gate are summarized in [driver-surface-read-write-gui-plan-82wm.md](driver-surface-read-write-gui-plan-82wm.md). Use the exact `control_id` table in [live-write-validation.md](live-write-validation.md); advanced controls are plan-only unless `--execute-only` matches the family. Use the documented `validation_cpu_driver` seed plus trigger mapping for reproducible profile evidence; local CI now asserts that seeded fixture previews contain `SetCpuGovernor`, `SetCpuEpp`, and `SetCpuBoost`, and the live verifier requires live metadata (`sysfs_root=/`, `target_bus_mode=system` or `custom-address`), expected dry-run method names, expected polkit actions, expected readback flags, and per-action results for those same controls for both manual-profile and trigger-profile bundles. Include the required operator checklists for fan mode thermal/fan behavior, GPU mode EnvyControl reboot/recovery evidence, and Curve Optimizer reset/stability evidence; the Curve Optimizer bundle must also include daemon `GetLastCurveOptimizerAllCore` state after apply and reset. Review with `scripts/review-write-validation-bundle.sh --require-mode execute --require-control <control_id>=pass` where reversible apply+revert is expected, and use the matrix status for negative `executed` controls. The review gate has regression coverage in `scripts/test-review-write-validation-bundle.sh`. After all bundles exist, run `scripts/verify-82wm-live-evidence.sh --root target/validation`; its required control/status/daemon-flag/output-slug matrix is [82wm-live-evidence-requirements.tsv](../data/validation/82wm-live-evidence-requirements.tsv), and every bundle directory must match `target/validation/82wm-live-<output_slug>`. Verifier regression coverage lives in `scripts/test-verify-82wm-live-evidence.sh` plus `scripts/ci-local.sh`.
2. After reinstalling/restarting the system daemon, verify `legion-control-ui --overview` reports `desktop_power_profiles=bus=system ... active=...` on Fedora/Plasma.
3. If the KDE Wayland/NVIDIA GTK black-window issue recurs, treat it as a frontend/compositor bug: keep tray/CLI validation via `scripts/run-local-session-app.sh` while isolating the renderer path.
4. Keep tray autostart disabled until GNOME-with-extension smoke exists; KDE smoke is not the blocker.
5. GTK Fans manual curve work stays **after** the kernel exposes writable `pwm*_auto_point*` files and live evidence exists; fan preset execution stays disabled until policy and evidence gates are met.
6. Keep `docs/feature-roadmap.md` / `docs/implementation-plan.md` aligned when scope changes.
7. Do not enable higher-risk hardware mutation until validators, polkit, rollback, and manual validation exist.

## When you finish a slice

- Run `./scripts/ci-local.sh`, commit, **update this file** (Next tasks / milestone if needed).
- For local dogfood updates, use `scripts/update-dev-install.sh` to rebuild/install the tray + GTK dashboard and restart the tray; add `--daemon` when the system daemon/polkit/D-Bus install also needs refreshing.
- Optional audit trail: append one line under **Completed slice log** in [session-handoff-archive.md](session-handoff-archive.md).

## New session prompt

Start with:

```text
Read AGENTS.md and docs/session-handoff.md first. Open docs/session-handoff-archive.md only if you need the long completed-slice log or full command inventory. Act as orchestrator: inspect git status, pick the next roadmap slice, delegate bounded work when useful, validate with ./scripts/ci-local.sh, update docs, commit. Do not change safety constraints.
```

## Safety constraints

- No raw WMI calls.
- No raw EC writes.
- No arbitrary sysfs writer.
- No hardcoded `hwmonN`.
- No hardware writes until validators, polkit policy, rollback behavior, and manual validation exist.
