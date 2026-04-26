# GTK Smoke Testing

Use `scripts/capture-gtk-smoke-report.sh` to render the GTK dashboard against a
private session-bus daemon under Xvfb and capture reviewable screenshots.

This workflow is meant for deterministic UI smoke coverage and artifact capture.
It does not replace live tray testing or live write validation on real
hardware.

## Fixture-backed smoke

```bash
scripts/capture-gtk-smoke-report.sh \
  --sysfs-root tests/fixtures/sysfs-82wm-confirmed \
  --output target/smoke/gtk-fixture
```

This captures the default page set:

- `status`
- `profiles`
- `battery`
- `gpu`
- `fans`
- `appearance`
- `diagnostics`

Use `--pages` to limit the capture set:

```bash
scripts/capture-gtk-smoke-report.sh \
  --sysfs-root tests/fixtures/sysfs-82wm-confirmed \
  --pages status,battery \
  --output target/smoke/gtk-status-battery
```

## Live local smoke

To render your current machine data through the same Xvfb path:

```bash
scripts/capture-gtk-smoke-report.sh \
  --output target/smoke/gtk-live
```

This still uses a private session bus and a local session-mode daemon, so it is
safe for read-only dashboard validation.

## Bundle contents

The report bundle includes:

- `report.md`
- `environment.txt`
- `commands.log`
- `daemon.log`
- `status.txt`
- `overview.txt`
- `diagnostics.json`
- `screenshots/<page>.png`
- `*-ui.log`

## Notes

- The script defaults to `GSK_RENDERER=cairo` because the real KDE
  Wayland/NVIDIA session can still show a black full window even when the data
  path is healthy.
- The smoke uses `--gtk-page` plus `--gtk-auto-quit-ms` internally so page
  capture is deterministic and does not depend on pointer automation.
- Battery charge type is part of the captured `battery` page and the supporting
  `overview.txt` / `diagnostics.json` artifacts.
