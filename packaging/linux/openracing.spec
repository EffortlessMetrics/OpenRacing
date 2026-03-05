# OpenRacing RPM spec file
#
# Build with:
#   rpmbuild -bb openracing.spec \
#     --define "_topdir $(pwd)/rpmbuild" \
#     --define "version 0.1.0" \
#     --define "bin_path target/release"
#
# Or via build-packages.sh:
#   ./packaging/linux/build-packages.sh --bin-path target/release --rpm-only

%if 0%{!?version:1}
%define version 0.1.0
%endif

Name:           openracing
Version:        %{version}
Release:        1%{?dist}
Summary:        Professional racing wheel force feedback software suite

License:        MIT OR Apache-2.0
URL:            https://github.com/EffortlessMetrics/OpenRacing
Source0:        %{name}-%{version}.tar.gz

BuildRequires:  systemd-rpm-macros
Requires:       systemd-libs
Requires:       libudev
Recommends:     rtkit
Suggests:       webkit2gtk4.1

%description
OpenRacing provides real-time force feedback processing at 1kHz
with safety-critical design for sim-racing enthusiasts.

Features:
- Real-time FFB at 1kHz with sub-millisecond latency
- Multi-game integration: iRacing, ACC, AMS2, rFactor 2
- Safety-critical design with FMEA analysis
- Plugin architecture (WASM + native)

%prep
%setup -q

%install
rm -rf %{buildroot}
mkdir -p %{buildroot}%{_bindir}
mkdir -p %{buildroot}%{_userunitdir}
mkdir -p %{buildroot}%{_udevrulesdir}
mkdir -p %{buildroot}/etc/udev/hwdb.d
mkdir -p %{buildroot}/etc/modprobe.d
mkdir -p %{buildroot}%{_docdir}/%{name}
mkdir -p %{buildroot}%{_datadir}/%{name}/config

install -m 755 bin/wheeld %{buildroot}%{_bindir}/
install -m 755 bin/wheelctl %{buildroot}%{_bindir}/
install -m 644 systemd/openracing.service %{buildroot}%{_userunitdir}/
install -m 644 udev/99-racing-wheel-suite.rules %{buildroot}%{_udevrulesdir}/

if [ -f hwdb/99-racing-wheel-suite.hwdb ]; then
    install -m 644 hwdb/99-racing-wheel-suite.hwdb %{buildroot}/etc/udev/hwdb.d/
fi
if [ -f modprobe/90-racing-wheel-quirks.conf ]; then
    install -m 644 modprobe/90-racing-wheel-quirks.conf %{buildroot}/etc/modprobe.d/
fi

if [ -f docs/README.md ]; then
    install -m 644 docs/README.md %{buildroot}%{_docdir}/%{name}/
fi
if [ -f docs/CHANGELOG.md ]; then
    install -m 644 docs/CHANGELOG.md %{buildroot}%{_docdir}/%{name}/
fi

%post
%udev_rules_update
udevadm trigger || true
systemd-hwdb update || true

%preun
systemctl --user stop openracing.service 2>/dev/null || true
systemctl --user disable openracing.service 2>/dev/null || true

%postun
%udev_rules_update

%files
%license LICENSE-MIT LICENSE-APACHE
%{_bindir}/wheeld
%{_bindir}/wheelctl
%{_userunitdir}/openracing.service
%{_udevrulesdir}/99-racing-wheel-suite.rules
%config(noreplace) /etc/udev/hwdb.d/99-racing-wheel-suite.hwdb
%config(noreplace) /etc/modprobe.d/90-racing-wheel-quirks.conf
%{_docdir}/%{name}
%{_datadir}/%{name}

%changelog
