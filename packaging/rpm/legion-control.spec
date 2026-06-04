Name:           legion-control
Version:        0.1.0
Release:        0%{?dist}
Summary:        Fedora-native Lenovo Legion hardware capability probe
License:        MIT
URL:            https://github.com/daredoole/RatVantage
Source0:        %{name}-%{version}.tar.gz

BuildRequires:  cargo
BuildRequires:  rust
BuildRequires:  pkgconfig(gtk4)
BuildRequires:  pkgconfig(libadwaita-1)
BuildRequires:  dbus
BuildRequires:  desktop-file-utils
BuildRequires:  appstream
BuildRequires:  systemd-rpm-macros

Requires:       %{name}-daemon%{?_isa} = %{version}-%{release}
Requires:       %{name}-ui%{?_isa} = %{version}-%{release}

%description
Legion Control is an experimental, probe-first hardware control project for Fedora.
It is not affiliated with or endorsed by Lenovo.

%package daemon
Summary:        Privileged D-Bus daemon for Legion Control
Requires(post): systemd
Requires(preun): systemd
Requires(postun): systemd

%description daemon
The Legion Control daemon exposes the read-only hardware capability API over
system D-Bus. Hardware write methods are intentionally not packaged yet.

%package ui
Summary:        GTK dashboard for Legion Control
Requires:       %{name}-daemon%{?_isa} = %{version}-%{release}
Requires:       %{name}-helpers = %{version}-%{release}

%description ui
The Legion Control UI is a GTK4/libadwaita dashboard for the read-only daemon.

%package helpers
Summary:        RatVantage support and evidence helper scripts
Requires:       bash
Requires:       python3

%description helpers
Support helpers for read-only compatibility bundles, keyboard RGB evidence,
GPU switching evidence, OpenRGB readiness checks, and the user-session OpenRGB SDK server/client path.
The access setup fallback must still be run explicitly with administrator
authorization; no setuid helper is packaged.

%package tray
Summary:        Read-only tray/status helper for Legion Control
Requires:       %{name}-daemon%{?_isa} = %{version}-%{release}

%description tray
The Legion Control tray helper provides a read-only StatusNotifier tray item
plus status and tooltip CLI output. Autostart remains packaged disabled.

%prep
%autosetup

%build
%{__cargo} build --release --locked \
    -p legion-probe \
    -p legion-control-daemon \
    -p legion-control-tray \
    -p legion-control-ui \
    --features legion-control-ui/gtk-ui,legion-control-tray/status-notifier

%install
install -Dpm0755 target/release/legion-probe \
    %{buildroot}%{_bindir}/legion-probe
install -Dpm0755 target/release/legion-control-ui \
    %{buildroot}%{_bindir}/legion-control-ui
install -Dpm0755 target/release/legion-control-tray \
    %{buildroot}%{_bindir}/legion-control-tray
install -Dpm0755 target/release/legion-control-daemon \
    %{buildroot}%{_libexecdir}/legion-control/legion-control-daemon

install -Dpm0755 scripts/check-keyboard-rgb-openrgb.sh \
    %{buildroot}%{_bindir}/ratvantage-check-keyboard-rgb-openrgb
install -Dpm0755 scripts/capture-keyboard-rgb-evidence.sh \
    %{buildroot}%{_bindir}/ratvantage-capture-keyboard-rgb-evidence
install -Dpm0755 scripts/compare-keyboard-rgb-evidence.sh \
    %{buildroot}%{_bindir}/ratvantage-compare-keyboard-rgb-evidence
install -Dpm0755 scripts/setup-keyboard-rgb-openrgb-access.sh \
    %{buildroot}%{_bindir}/ratvantage-setup-keyboard-rgb-openrgb-access
install -Dpm0755 scripts/capture-keyboard-rgb-openrgb-bridge-evidence.sh \
    %{buildroot}%{_bindir}/ratvantage-capture-keyboard-rgb-openrgb-bridge-evidence
install -Dpm0755 scripts/review-keyboard-rgb-openrgb-bridge-evidence.sh \
    %{buildroot}%{_bindir}/ratvantage-review-keyboard-rgb-openrgb-bridge-evidence
install -Dpm0755 scripts/status-keyboard-rgb-openrgb-bridge-evidence.sh \
    %{buildroot}%{_bindir}/ratvantage-keyboard-rgb-openrgb-bridge-status
install -Dpm0755 scripts/capture-keyboard-rgb-openrgb-sdk-evidence.sh \
    %{buildroot}%{_bindir}/ratvantage-capture-keyboard-rgb-openrgb-sdk-evidence
install -Dpm0755 scripts/capture-keyboard-rgb-openrgb-sdk-write-evidence.sh \
    %{buildroot}%{_bindir}/ratvantage-capture-keyboard-rgb-openrgb-sdk-write-evidence
install -Dpm0755 scripts/openrgb-keyboard-rgb-sdk-helper.sh \
    %{buildroot}%{_bindir}/ratvantage-openrgb-keyboard-rgb-sdk-helper
install -Dpm0755 scripts/openrgb-sdk-server-session.sh \
    %{buildroot}%{_bindir}/ratvantage-openrgb-sdk-server
install -Dpm0755 scripts/capture-compatibility-bundle.sh \
    %{buildroot}%{_bindir}/ratvantage-capture-compatibility-bundle
install -Dpm0755 scripts/capture-gpu-mux-evidence.sh \
    %{buildroot}%{_bindir}/ratvantage-capture-gpu-mux-evidence
install -Dpm0755 scripts/review-gpu-mux-evidence.sh \
    %{buildroot}%{_bindir}/ratvantage-review-gpu-mux-evidence

install -Dpm0644 data/systemd/legion-control-daemon.service \
    %{buildroot}%{_unitdir}/legion-control-daemon.service
install -Dpm0644 data/dbus/org.ratvantage.LegionControl1.service \
    %{buildroot}%{_datadir}/dbus-1/system-services/org.ratvantage.LegionControl1.service
install -Dpm0644 data/dbus/org.ratvantage.LegionControl1.conf \
    %{buildroot}%{_datadir}/dbus-1/system.d/org.ratvantage.LegionControl1.conf
install -Dpm0644 data/polkit/org.ratvantage.LegionControl1.policy \
    %{buildroot}%{_datadir}/polkit-1/actions/org.ratvantage.LegionControl1.policy
install -Dpm0644 data/desktop/org.ratvantage.LegionControl.desktop \
    %{buildroot}%{_datadir}/applications/org.ratvantage.LegionControl.desktop
install -Dpm0644 data/desktop/org.ratvantage.LegionControl.Tray.desktop \
    %{buildroot}%{_sysconfdir}/xdg/autostart/org.ratvantage.LegionControl.Tray.desktop
install -Dpm0644 data/metainfo/org.ratvantage.LegionControl.metainfo.xml \
    %{buildroot}%{_datadir}/metainfo/org.ratvantage.LegionControl.metainfo.xml
install -dm0755 %{buildroot}%{_datadir}/legion-control/presets
install -pm0644 data/presets/*.toml \
    %{buildroot}%{_datadir}/legion-control/presets/

%check
desktop-file-validate %{buildroot}%{_datadir}/applications/org.ratvantage.LegionControl.desktop
desktop-file-validate %{buildroot}%{_sysconfdir}/xdg/autostart/org.ratvantage.LegionControl.Tray.desktop
appstreamcli validate --no-net %{buildroot}%{_datadir}/metainfo/org.ratvantage.LegionControl.metainfo.xml

%post daemon
%systemd_post legion-control-daemon.service

%preun daemon
%systemd_preun legion-control-daemon.service

%postun daemon
%systemd_postun_with_restart legion-control-daemon.service

%files
%license LICENSE
%doc README.md BRAND.md
%{_bindir}/legion-probe
%{_datadir}/metainfo/org.ratvantage.LegionControl.metainfo.xml
%{_datadir}/legion-control/presets/*.toml

%files daemon
%{_libexecdir}/legion-control/legion-control-daemon
%{_unitdir}/legion-control-daemon.service
%{_datadir}/dbus-1/system-services/org.ratvantage.LegionControl1.service
%{_datadir}/dbus-1/system.d/org.ratvantage.LegionControl1.conf
%{_datadir}/polkit-1/actions/org.ratvantage.LegionControl1.policy

%files helpers
%{_bindir}/ratvantage-check-keyboard-rgb-openrgb
%{_bindir}/ratvantage-capture-keyboard-rgb-evidence
%{_bindir}/ratvantage-compare-keyboard-rgb-evidence
%{_bindir}/ratvantage-setup-keyboard-rgb-openrgb-access
%{_bindir}/ratvantage-capture-keyboard-rgb-openrgb-bridge-evidence
%{_bindir}/ratvantage-review-keyboard-rgb-openrgb-bridge-evidence
%{_bindir}/ratvantage-keyboard-rgb-openrgb-bridge-status
%{_bindir}/ratvantage-capture-keyboard-rgb-openrgb-sdk-evidence
%{_bindir}/ratvantage-capture-keyboard-rgb-openrgb-sdk-write-evidence
%{_bindir}/ratvantage-openrgb-keyboard-rgb-sdk-helper
%{_bindir}/ratvantage-openrgb-sdk-server
%{_bindir}/ratvantage-capture-compatibility-bundle
%{_bindir}/ratvantage-capture-gpu-mux-evidence
%{_bindir}/ratvantage-review-gpu-mux-evidence

%files ui
%{_bindir}/legion-control-ui
%{_datadir}/applications/org.ratvantage.LegionControl.desktop

%files tray
%{_bindir}/legion-control-tray
%config(noreplace) %{_sysconfdir}/xdg/autostart/org.ratvantage.LegionControl.Tray.desktop
