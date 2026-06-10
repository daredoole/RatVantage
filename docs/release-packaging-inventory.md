# Release Packaging Inventory

This inventory covers packaging and install assets that can be validated without
real hardware, host installs, or hardware writes.

| Artifact | Purpose | Expected install path | Scope | Owner/mode | Validation | Risk notes |
|---|---|---|---|---|---|---|
| `data/systemd/legion-control-daemon.service` | System daemon unit | `/usr/lib/systemd/system/legion-control-daemon.service` | system | `root:root 0644` | `systemd-analyze verify`, static ExecStart checks | Must start daemon only; packaged unit must not include `--enable-*-write` flags. |
| `data/dbus/org.ratvantage.LegionControl1.service` | D-Bus system activation | `/usr/share/dbus-1/system-services/org.ratvantage.LegionControl1.service` | system | `root:root 0644` | INI/static service-name checks | Must activate daemon as root, not GUI/tray. |
| `data/dbus/org.ratvantage.LegionControl1.conf` | D-Bus system bus policy | `/usr/share/dbus-1/system.d/org.ratvantage.LegionControl1.conf` | system | `root:root 0644` | XML parse and bus-name checks | Allows clients to reach daemon; authorization remains polkit/daemon-side. |
| `data/polkit/org.ratvantage.LegionControl1.policy` | Privileged action policy | `/usr/share/polkit-1/actions/org.ratvantage.LegionControl1.policy` | system | `root:root 0644` | XML parse and action/default checks | Write actions must require `auth_admin_keep`; read action may be allowed. |
| `data/desktop/org.ratvantage.LegionControl.desktop` | GTK dashboard launcher | `/usr/share/applications/org.ratvantage.LegionControl.desktop` | user session | `root:root 0644` | `desktop-file-validate`, static Exec checks | Must not use `sudo`/`pkexec`; GUI remains unprivileged. |
| `data/desktop/org.ratvantage.LegionControl.Tray.desktop` | Tray autostart launcher | `/etc/xdg/autostart/org.ratvantage.LegionControl.Tray.desktop` | user session | `root:root 0644` | `desktop-file-validate`, static Exec checks | Must start tray only; no root tray. |
| `data/metainfo/org.ratvantage.LegionControl.metainfo.xml` | AppStream metadata | `/usr/share/metainfo/org.ratvantage.LegionControl.metainfo.xml` | package metadata | `root:root 0644` | `appstreamcli validate --no-net` | Launchable must match desktop ID. |
| `data/icons/hicolor/scalable/apps/org.ratvantage.LegionControl.svg` | App icon asset | `/usr/share/icons/hicolor/scalable/apps/org.ratvantage.LegionControl.svg` | package metadata | `root:root 0644` | release validator icon checks | Generic project-owned icon; no vendor logo or trademark asset. |
| `data/presets/*.toml` | Packaged fan preset definitions | `/usr/share/legion-control/presets/*.toml` | data | `root:root 0644` | TOML schema/static checks | Presets are data only; no write execution by install. |
| `packaging/rpm/legion-control.spec` | RPM packaging | RPM buildroot paths | package | RPM-owned | `rpmspec -P`, staged smoke checks | Packaged daemon description is read-only by default. |
| `scripts/install-user-session.sh` | Dev user-session install | `~/.local/bin`, autostart dirs | user | user-owned executables | metadata tests, manual dev use | Installs GUI/tray/helpers only; no root GUI. |
| `scripts/install-dev-system-integration.sh` | Dev D-Bus/polkit install | `/etc`, `/usr/share/polkit-1` | system | root-owned | reviewed/static checks | Host-mutating; not run by release validator. |
| `scripts/install-dev-systemd-ratvantage.sh` | Dev daemon systemd install | `/usr/local/libexec`, `/etc/systemd/system` | system | root-owned | script tests/static checks | Dev path may enable write flags explicitly; packaged service must not. |
| `scripts/update-dev-install.sh` | Dev refresh helper | local user + dev daemon paths | mixed | user/root | script tests/static checks | Host-mutating; not used for release validation. |
| `scripts/validate-release-packaging.sh` | Release metadata validation | n/a | test | executable | local CI | Safe static/staged checks only. |
| `scripts/smoke-install-staged.sh` | Temp-root staging smoke | temp `DESTDIR` | test | temp files | local CI via release validator | Does not start services or modify host. |

Run:

```bash
scripts/validate-release-packaging.sh
scripts/smoke-install-staged.sh
```
