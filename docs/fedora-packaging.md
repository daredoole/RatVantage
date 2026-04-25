# Fedora Packaging

## Fedora dependencies

### Build dependencies

Recommended Rust build stack:

```bash
sudo dnf install \
  rust cargo \
  gtk4-devel libadwaita-devel glib2-devel \
  dbus-devel systemd-devel polkit-devel \
  desktop-file-utils appstream appstream-compose \
  systemd-rpm-macros pkgconf-pkg-config
```

Optional tray helper build dependencies if the StatusNotifier backend is replaced
with an Ayatana helper:

```bash
sudo dnf install \
  libayatana-appindicator-gtk3 \
  libayatana-appindicator-gtk3-devel
```

Notes:

- `gtk4-devel` and `libadwaita-devel` are for the native dashboard.
- The current tray backend uses a pure Rust StatusNotifier implementation and does not require GTK3/Ayatana packages.
- `libayatana-appindicator-gtk3` is useful only if the tray helper is moved to Ayatana AppIndicator.

### Runtime dependencies

```bash
sudo dnf install \
  gtk4 libadwaita \
  polkit systemd dbus \
  power-profiles-daemon \
  lm_sensors brightnessctl
```

Optional runtime dependencies:

```bash
sudo dnf install gnome-shell-extension-appindicator
```

External tools to detect, not require:

- `envycontrol`
- `tlp`
- out-of-tree `legion_laptop` / LenovoLegionLinux module
- external keyboard RGB tools

Do not fail app startup because optional tools are missing.

## RPM packaging approach

Start with one source package and split binary subpackages:

| Package | Contents |
|---|---|
| `legion-control` | Common files, README, icons, metainfo, presets |
| `legion-control-daemon` | Root system daemon, systemd unit, D-Bus service config, polkit policy |
| `legion-control-ui` | GTK4/libadwaita dashboard, desktop file |
| `legion-control-tray` | Optional read-only StatusNotifier tray/status helper and disabled autostart placeholder |
| `legion-control-devel` | Optional D-Bus XML/spec files for integrations |

Recommended first distribution path:

1. local `.rpm` builds while developing;
2. COPR for early Fedora users;
3. official Fedora review only after hardware safety and packaging are mature.

Flatpak is not recommended for v1 because it cannot install or own the privileged daemon, system D-Bus service file, systemd unit, or polkit policy. A future Flatpak GUI can talk to a host-installed RPM daemon.

## Spec file notes

Use standard Fedora macros:

```spec
%global crate_name legion-control

Name:           legion-control
Version:        0.1.0
Release:        1%{?dist}
Summary:        Fedora-native control dashboard for Lenovo Legion laptops
License:        GPL-3.0-or-later
URL:            https://github.com/OWNER/legion-control
Source0:        %{url}/archive/v%{version}/%{name}-%{version}.tar.gz

BuildRequires:  cargo
BuildRequires:  rust
BuildRequires:  pkgconfig(gtk4)
BuildRequires:  pkgconfig(libadwaita-1)
BuildRequires:  pkgconfig(gio-2.0)
BuildRequires:  pkgconfig(polkit-gobject-1)
BuildRequires:  systemd-rpm-macros
BuildRequires:  desktop-file-utils
BuildRequires:  appstream

Requires:       %{name}-daemon%{?_isa} = %{version}-%{release}
Requires:       %{name}-ui%{?_isa} = %{version}-%{release}
```

Scriptlets for the daemon package:

```spec
%post daemon
%systemd_post legion-control-daemon.service

%preun daemon
%systemd_preun legion-control-daemon.service

%postun daemon
%systemd_postun_with_restart legion-control-daemon.service
```

Validate desktop and AppStream metadata in `%check`:

```spec
%check
desktop-file-validate %{buildroot}%{_datadir}/applications/org.ratvantage.LegionControl.desktop
appstreamcli validate --no-net %{buildroot}%{_datadir}/metainfo/org.ratvantage.LegionControl.metainfo.xml
```

## systemd unit

Install to:

```text
/usr/lib/systemd/system/legion-control-daemon.service
```

Suggested unit:

```ini
[Unit]
Description=Legion Control hardware daemon
Documentation=man:legion-control-daemon(8)
ConditionPathExists=/sys/firmware/acpi/platform_profile
After=multi-user.target

[Service]
Type=dbus
BusName=org.ratvantage.LegionControl1
ExecStart=/usr/libexec/legion-control/legion-control-daemon
Restart=on-failure
StateDirectory=legion-control
ConfigurationDirectory=legion-control
LogsDirectory=legion-control
ProtectSystem=strict
ProtectHome=true
PrivateTmp=true
NoNewPrivileges=true
MemoryDenyWriteExecute=true
RestrictAddressFamilies=AF_UNIX
SystemCallArchitectures=native
ReadWritePaths=/sys/firmware/acpi /sys/class/power_supply /sys/class/leds /sys/class/hwmon /sys/class/firmware-attributes /var/lib/legion-control /etc/legion-control

[Install]
WantedBy=multi-user.target
```

Install D-Bus activation file to:

```text
/usr/share/dbus-1/system-services/org.ratvantage.LegionControl1.service
```

Example:

```ini
[D-BUS Service]
Name=org.ratvantage.LegionControl1
Exec=/usr/libexec/legion-control/legion-control-daemon
User=root
SystemdService=legion-control-daemon.service
```

Install D-Bus policy to:

```text
/usr/share/dbus-1/system.d/org.ratvantage.LegionControl1.conf
```

Example:

```xml
<!DOCTYPE busconfig PUBLIC "-//freedesktop//DTD D-Bus Bus Configuration 1.0//EN"
 "http://www.freedesktop.org/standards/dbus/1.0/busconfig.dtd">
<busconfig>
  <policy user="root">
    <allow own="org.ratvantage.LegionControl1"/>
  </policy>
  <policy context="default">
    <allow send_destination="org.ratvantage.LegionControl1"/>
  </policy>
</busconfig>
```

Fine-grained authorization belongs in polkit, not D-Bus XML.

## polkit policy

Install to:

```text
/usr/share/polkit-1/actions/org.ratvantage.LegionControl1.policy
```

Example:

```xml
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE policyconfig PUBLIC "-//freedesktop//DTD PolicyKit Policy Configuration 1.0//EN"
 "http://www.freedesktop.org/standards/PolicyKit/1/policyconfig.dtd">
<policyconfig>
  <vendor>RatVantage</vendor>
  <vendor_url>https://github.com/OWNER/legion-control</vendor_url>

  <action id="org.ratvantage.LegionControl1.set-platform-profile">
    <description>Change Lenovo Legion platform profile</description>
    <message>Authentication is required to change the hardware platform profile.</message>
    <defaults>
      <allow_any>no</allow_any>
      <allow_inactive>no</allow_inactive>
      <allow_active>auth_admin_keep</allow_active>
    </defaults>
  </action>

  <action id="org.ratvantage.LegionControl1.set-battery-charge-type">
    <description>Change battery charge mode</description>
    <message>Authentication is required to change the battery charge mode.</message>
    <defaults>
      <allow_any>no</allow_any>
      <allow_inactive>no</allow_inactive>
      <allow_active>auth_admin_keep</allow_active>
    </defaults>
  </action>

  <action id="org.ratvantage.LegionControl1.apply-fan-preset">
    <description>Apply a fan preset</description>
    <message>Authentication is required to change the fan curve.</message>
    <defaults>
      <allow_any>no</allow_any>
      <allow_inactive>no</allow_inactive>
      <allow_active>auth_admin_keep</allow_active>
    </defaults>
  </action>

  <action id="org.ratvantage.LegionControl1.set-gpu-mode">
    <description>Change NVIDIA GPU mode</description>
    <message>Authentication is required to change GPU mode. Reboot required.</message>
    <defaults>
      <allow_any>no</allow_any>
      <allow_inactive>no</allow_inactive>
      <allow_active>auth_admin_keep</allow_active>
    </defaults>
  </action>

  <action id="org.ratvantage.LegionControl1.set-firmware-attribute">
    <description>Change advanced firmware power attributes</description>
    <message>Authentication is required to change advanced firmware power attributes.</message>
    <defaults>
      <allow_any>no</allow_any>
      <allow_inactive>no</allow_inactive>
      <allow_active>auth_admin</allow_active>
    </defaults>
  </action>
</policyconfig>
```

Optional wheel rule for routine actions during later releases:

```javascript
// /usr/share/polkit-1/rules.d/49-legion-control.rules
polkit.addRule(function(action, subject) {
  const routine = [
    "org.ratvantage.LegionControl1.set-platform-profile",
    "org.ratvantage.LegionControl1.set-battery-charge-type",
    "org.ratvantage.LegionControl1.apply-fan-preset"
  ];

  if (routine.indexOf(action.id) >= 0 &&
      subject.active && subject.local && subject.isInGroup("wheel")) {
    return polkit.Result.YES;
  }
});
```

Do not install an allow-all rule by default during early development.

## Desktop file

Install to:

```text
/usr/share/applications/org.ratvantage.LegionControl.desktop
```

Example:

```ini
[Desktop Entry]
Type=Application
Name=Legion Control
Comment=Control Lenovo Legion platform profiles, battery mode, fans, and GPU mode
Exec=legion-control-ui
Icon=org.ratvantage.LegionControl
Terminal=false
Categories=Utility;Settings;HardwareSettings;
StartupNotify=true
DBusActivatable=true
X-GNOME-UsesNotifications=true
```

Optional tray autostart placeholder:

```text
/etc/xdg/autostart/org.ratvantage.LegionControl.Tray.desktop
```

Current disabled placeholder:

```ini
[Desktop Entry]
Type=Application
Name=Legion Control Tray
Comment=Read-only Legion Control tray/status helper
Exec=legion-control-tray
Icon=org.ratvantage.LegionControl
Terminal=false
NoDisplay=true
Hidden=true
X-GNOME-Autostart-enabled=false
```

Do not enable tray autostart until the StatusNotifier backend is manually tested
on target desktops. The helper also keeps read-only `--status` and `--tooltip`
CLI output for diagnostics.

For a polished Fedora app, also ship:

```text
/usr/share/metainfo/org.ratvantage.LegionControl.metainfo.xml
/usr/share/icons/hicolor/scalable/apps/org.ratvantage.LegionControl.svg
/usr/share/icons/hicolor/symbolic/apps/org.ratvantage.LegionControl-symbolic.svg
/usr/share/legion-control/presets/*.toml
```

## AppIndicator / tray caveats

Fedora GNOME:

- The dashboard works normally as a GTK4/libadwaita app.
- The tray icon requires AppIndicator/KStatusNotifier support.
- Fedora packages `gnome-shell-extension-appindicator`.
- The extension may need enabling after install.

KDE Plasma:

- StatusNotifier tray support is native.
- AppIndicator/SNI behavior should be more predictable than GNOME.
- KDE's own power profile UI may also interact with platform profiles, so conflict detection matters.

Other desktops:

- Xfce/Cinnamon generally have tray concepts, but test separately.
- Wayland/X11 differences should not affect the daemon because it is headless.

## KDE and GNOME behavior differences

| Area | GNOME on Fedora | KDE Plasma on Fedora | App decision |
|---|---|---|---|
| Tray icon | Requires extension for AppIndicator/SNI | Native StatusNotifier support | Dashboard first; tray optional |
| Power profile UI | GNOME Settings / quick settings may use PowerProfiles D-Bus | Plasma power widget may use PowerProfiles D-Bus | Detect D-Bus owner and external changes |
| Notifications | GNOME notification portal/Gio works well | KDE notifications work through freedesktop stack | Use standard desktop notifications |
| Root GUI | Not acceptable under Wayland | Also not acceptable as product design | Never run GUI as root |
| App styling | libadwaita is native | libadwaita works but looks GNOME-style | Accept; focus Fedora-native GNOME first |
| Polkit agent | GNOME Shell agent | KDE polkit agent | Let desktop handle auth prompts |

## RPM build/install validation checklist

Asset-level packaging metadata is validated by `scripts/validate-packaging.sh`.
The checklist below tracks full RPM build and install verification, so unchecked
items do not mean the source asset files are missing.

- [ ] Build daemon and UI with release flags.
- [ ] Install daemon to `/usr/libexec/legion-control/`.
- [ ] Install systemd unit.
- [ ] Install D-Bus service and policy files.
- [ ] Install polkit policy.
- [ ] Install desktop file.
- [ ] Install AppStream metainfo.
- [ ] Install icons.
- [ ] Install default fan presets.
- [ ] Run `desktop-file-validate`.
- [ ] Run `appstreamcli validate --no-net`.
- [ ] Verify `%systemd_post` scriptlets.
- [ ] Verify package does not enable unsafe polkit allow-all rule.
- [ ] Verify app starts on unsupported hardware and shows “unsupported” cleanly.
