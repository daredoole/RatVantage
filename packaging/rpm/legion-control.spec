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

%description ui
The Legion Control UI is a GTK4/libadwaita dashboard for the read-only daemon.

%prep
%autosetup

%build
%{__cargo} build --release --locked \
    -p legion-probe \
    -p legion-control-daemon \
    -p legion-control-ui \
    --features legion-control-ui/gtk-ui

%install
install -Dpm0755 target/release/legion-probe \
    %{buildroot}%{_bindir}/legion-probe
install -Dpm0755 target/release/legion-control-ui \
    %{buildroot}%{_bindir}/legion-control-ui
install -Dpm0755 target/release/legion-control-daemon \
    %{buildroot}%{_libexecdir}/legion-control/legion-control-daemon

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
install -Dpm0644 data/metainfo/org.ratvantage.LegionControl.metainfo.xml \
    %{buildroot}%{_datadir}/metainfo/org.ratvantage.LegionControl.metainfo.xml

%check
desktop-file-validate %{buildroot}%{_datadir}/applications/org.ratvantage.LegionControl.desktop
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

%files daemon
%{_libexecdir}/legion-control/legion-control-daemon
%{_unitdir}/legion-control-daemon.service
%{_datadir}/dbus-1/system-services/org.ratvantage.LegionControl1.service
%{_datadir}/dbus-1/system.d/org.ratvantage.LegionControl1.conf
%{_datadir}/polkit-1/actions/org.ratvantage.LegionControl1.policy

%files ui
%{_bindir}/legion-control-ui
%{_datadir}/applications/org.ratvantage.LegionControl.desktop
