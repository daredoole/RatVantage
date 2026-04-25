# StatusNotifier Smoke Testing

Use this before enabling tray autostart.

## Supported Desktop Check

Build the tray and run the smoke script from the graphical desktop session:

```bash
cargo build -p legion-control-tray
scripts/smoke-statusnotifier-tray.sh --hold-seconds 15
```

If testing against a private or non-system daemon, pass its bus address:

```bash
scripts/smoke-statusnotifier-tray.sh --bus-address <dbus-address> --hold-seconds 15
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
