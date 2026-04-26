# StatusNotifier Smoke Testing

Use this before enabling tray autostart.

## Supported Desktop Check

Build the tray and run the smoke script from the graphical desktop session:

```bash
cargo build -p legion-control-tray
cargo run -p legion-control-tray -- --desktop-check
scripts/smoke-statusnotifier-tray.sh --hold-seconds 15
```

For fixture-backed development smoke testing, run the read-only daemon on the
session bus in one terminal:

```bash
cargo run -p legion-control-daemon -- --session --sysfs-root tests/fixtures/sysfs-82wm-confirmed
```

Then run the tray smoke from another terminal:

```bash
cargo run -p legion-control-tray -- --desktop-check
scripts/smoke-statusnotifier-tray.sh --bus-address "$DBUS_SESSION_BUS_ADDRESS" --hold-seconds 15
```

To write a reusable smoke report bundle for review or attachment:

```bash
cargo run -p legion-control-tray -- --desktop-check
scripts/smoke-statusnotifier-tray.sh \
  --bus-address "$DBUS_SESSION_BUS_ADDRESS" \
  --hold-seconds 15 \
  --report-dir target/smoke/statusnotifier-<desktop>-<date>
```

`legion-control-tray --desktop-check` reports desktop/session values, watcher availability, and autostart gating without launching the tray UI.
The smoke script verifies that `org.kde.StatusNotifierWatcher` exists, the tray can read daemon status, autostart is still disabled, and the registered StatusNotifier item count increases while the tray process is running.
When `--report-dir` is set it also writes desktop/session metadata, `tray-desktop-check.txt`, tray status/tooltip text, watcher data, raw item properties, and a markdown smoke summary.

Manual checks during the hold window:

- tray icon appears in the desktop panel;
- tooltip shows the Legion Control hardware, platform profile, fan RPM, and capability summary;
- menu exposes dashboard, refresh, and quit;
- write actions remain disabled;
- quit removes the tray item;
- autostart remains disabled.

## Matrix

| Desktop | Expected result | Gate |
|---|---|---|
| KDE Plasma | StatusNotifier item appears natively. | Must pass before autostart. |
| GNOME with AppIndicator/KStatusNotifier extension | StatusNotifier item may appear through the extension. | Untested; do not rely on it for release readiness yet. |
| GNOME without extension or unsupported shell | Script fails because no watcher is available, or no tray item is visible. | Do not enable autostart. |

Do not flip `Hidden=true` or `X-GNOME-Autostart-enabled=false` yet. KDE smoke has
passed, but the GNOME AppIndicator extension path is explicitly untested.

## GNOME Availability Check

Before running GNOME smoke, confirm the shell and extension from an active GNOME
session:

```bash
printf 'desktop=%s session=%s wayland=%s display=%s bus=%s\n' \
  "$XDG_CURRENT_DESKTOP" "$XDG_SESSION_TYPE" "$WAYLAND_DISPLAY" "$DISPLAY" \
  "$DBUS_SESSION_BUS_ADDRESS"
gnome-shell --version
gnome-extensions list | grep -Ei 'appindicator|statusnotifier|kstatus'
gnome-extensions info appindicatorsupport@rgcjonas.gmail.com
```

The smoke only counts as GNOME coverage when `XDG_CURRENT_DESKTOP` is GNOME and
the AppIndicator/KStatusNotifier extension is enabled in that same session. Until
that happens, treat the GNOME extension JavaScript path as untested.

## Recorded Results

| Date | Desktop | Command | Result | Remaining check |
|---|---|---|---|---|
| 2026-04-25 | KDE Plasma Wayland | `scripts/smoke-statusnotifier-tray.sh --bus-address "$DBUS_SESSION_BUS_ADDRESS" --hold-seconds 1` with fixture daemon on `--session` | Automated registration passed, `before=6 after=7`, autostart disabled. | Completed by KDE desktop evidence below. |
| 2026-04-25 | KDE Plasma Wayland | `cargo run -p legion-control-tray -- --desktop-check` plus `scripts/smoke-statusnotifier-tray.sh --bus-address "$DBUS_SESSION_BUS_ADDRESS" --hold-seconds 1 --report-dir target/smoke/statusnotifier-kde-wayland-2026-04-25` with fixture daemon on `--session` | Report bundle captured environment, `tray-desktop-check.txt`, watcher/item properties, tray status, and tooltip text under `target/smoke/statusnotifier-kde-wayland-2026-04-25`. | GNOME-with-extension smoke still required before enabling autostart. |
| 2026-04-25 | KDE Plasma Wayland | Fixture daemon on `--session`, tray on session bus, `busctl` StatusNotifier/DBusMenu checks, screenshot at `target/smoke/statusnotifier-kde-wayland-2026-04-25.png` | Registered item exposed `Id=org.ratvantage.LegionControl`, `Title=Legion Control`, `Category=Hardware`, `Status=Active`, `IconName=applications-system`, tooltip `82WM Legion Pro 5 16ARX8: 7 read-only capabilities`; menu exported Open dashboard, Refresh status, Quit, and disabled write actions; DBusMenu Refresh succeeded; DBusMenu Quit removed the tray item. | GNOME-with-extension smoke still required before enabling autostart. |
| 2026-04-25 | GNOME with AppIndicator/KStatusNotifier extension | Local availability check from current session | Skipped for now: active graphical session is KDE Wayland (`XDG_CURRENT_DESKTOP=KDE`, `KDE_FULL_SESSION=true`). GNOME Shell 49.6 is installed, GNOME session files exist, and `/usr/share/gnome-shell/extensions/appindicatorsupport@rgcjonas.gmail.com` supports shell versions 45-49, but the extension JavaScript/rendering path is untested. | Optional future GNOME validation; not the next roadmap blocker. |
