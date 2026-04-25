# StatusNotifier Smoke Testing

Use this before enabling tray autostart.

## Supported Desktop Check

Build the tray and run the smoke script from the graphical desktop session:

```bash
cargo build -p legion-control-tray
scripts/smoke-statusnotifier-tray.sh --hold-seconds 15
```

For fixture-backed development smoke testing, run the read-only daemon on the
session bus in one terminal:

```bash
cargo run -p legion-control-daemon -- --session --sysfs-root tests/fixtures/sysfs-82wm-confirmed
```

Then run the tray smoke from another terminal:

```bash
scripts/smoke-statusnotifier-tray.sh --bus-address "$DBUS_SESSION_BUS_ADDRESS" --hold-seconds 15
```

The script verifies that `org.kde.StatusNotifierWatcher` exists, the tray can read daemon status, autostart is still disabled, and the registered StatusNotifier item count increases while the tray process is running.

Manual checks during the hold window:

- tray icon appears in the desktop panel;
- tooltip shows the Legion Control hardware/capability summary;
- menu exposes dashboard, refresh, and quit;
- write actions remain disabled;
- quit removes the tray item;
- autostart remains disabled.

## Matrix

| Desktop | Expected result | Gate |
|---|---|---|
| KDE Plasma | StatusNotifier item appears natively. | Must pass before autostart. |
| GNOME with AppIndicator/KStatusNotifier extension | StatusNotifier item appears through the extension. | Must pass before autostart. |
| GNOME without extension or unsupported shell | Script fails because no watcher is available, or no tray item is visible. | Do not enable autostart. |

Do not flip `Hidden=true` or `X-GNOME-Autostart-enabled=false` until KDE and GNOME-with-extension smoke checks pass.

## Recorded Results

| Date | Desktop | Command | Result | Remaining check |
|---|---|---|---|---|
| 2026-04-25 | KDE Plasma Wayland | `scripts/smoke-statusnotifier-tray.sh --bus-address "$DBUS_SESSION_BUS_ADDRESS" --hold-seconds 1` with fixture daemon on `--session` | Automated registration passed, `before=6 after=7`, autostart disabled. | Completed by KDE desktop evidence below. |
| 2026-04-25 | KDE Plasma Wayland | Fixture daemon on `--session`, tray on session bus, `busctl` StatusNotifier/DBusMenu checks, screenshot at `target/smoke/statusnotifier-kde-wayland-2026-04-25.png` | Registered item exposed `Id=org.ratvantage.LegionControl`, `Title=Legion Control`, `Category=Hardware`, `Status=Active`, `IconName=applications-system`, tooltip `82WM Legion Pro 5 16ARX8: 7 read-only capabilities`; menu exported Open dashboard, Refresh status, Quit, and disabled write actions; DBusMenu Refresh succeeded; DBusMenu Quit removed the tray item. | GNOME-with-extension smoke still required before enabling autostart. |
