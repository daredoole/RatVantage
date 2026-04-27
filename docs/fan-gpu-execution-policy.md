# Fan and GPU execution policy

RatVantage ships **dry-run planning** for fan presets, restore-to-auto, and EnvyControl
GPU mode. **Execution** for those surfaces is intentionally **not** enabled in the
default daemon/UI/tray.

## Current state

- **Fan:** `PlanFanPresetWrite` / `PlanRestoreAutoFanWrite` exist; `ApplyFanPreset` and
  `RestoreAutoFan` are **not** exposed as callable D-Bus write methods in shipping
  builds. There are **no** `--enable-fan-*` daemon CLI flags. The GTK Fans tab and
  tray remain preview/planning only for fan curves.
- **GPU:** `PlanGpuModeWrite` exists; `SetGpuMode` is **not** an executable D-Bus
  method. GPU mode changes remain **EnvyControl / reboot** territory outside the
  daemon write surface.

## Why

Fan sysfs and GPU mode switches are **high blast radius**: wrong curves or MUX
assumptions can cause thermal or boot issues. They require the same bar as other
writes — validators, polkit, read-back, rollback — **plus** narrow **live**
evidence bundles and explicit maintainer agreement before any execute path ships.

## What would need to happen before execution ships

1. **Live evidence** per machine class (not only fixtures): controlled captures with
   the write-validation harness policy updated only when execution is deliberately
   enabled for that family.
2. **Daemon policy + polkit** actions wired for the specific methods.
3. **Rollback tests** on hardware representative of supported models.
4. **Documentation** update in `docs/write-contracts.md` and release notes when the
   contract moves from “disabled” to “gated”.

Until then, treat fan and GPU rows in validation output as **planning evidence
only**, not proof that hardware mutation ran.
