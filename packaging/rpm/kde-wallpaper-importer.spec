Name:           kde-wallpaper-importer
Version:        0.1.0
Release:        1%{?dist}
Summary:        Import wallpapers into KDE Plasma from Dolphin

License:        GPL-3.0-or-later
URL:            https://github.com/toresoft/kde-wallpaper-importer
Source0:        %{name}-%{version}.tar.gz

BuildRequires:  rust >= 1.80
BuildRequires:  cargo
BuildRequires:  gcc
BuildRequires:  make
Requires:       kdialog
Requires:       plasma-workspace
Requires:       libnotify

%global debug_package %{nil}

%description
kde-wallpaper-importer adds Dolphin context-menu entries to import image
files as KDE Plasma wallpaper packages. It deduplicates by SHA-256 and
resolves name collisions. Imported wallpapers can optionally be set as
the active wallpaper via plasma-apply-wallpaperimage.

%prep
%setup -q

%build
cargo build --release

%install
install -Dm755 target/release/kde-wallpaper-import \
    %{buildroot}%{_bindir}/kde-wallpaper-import
install -d %{buildroot}%{_datadir}/kio/servicemenus
sed 's|@BINARY@|%{_bindir}/kde-wallpaper-import|g' \
    share/kio/servicemenus/kde-wallpaper-importer.desktop.in \
    > %{buildroot}%{_datadir}/kio/servicemenus/kde-wallpaper-importer.desktop

%files
%{_bindir}/kde-wallpaper-import
%{_datadir}/kio/servicemenus/kde-wallpaper-importer.desktop

%changelog
* Fri Jul 31 2026 CI <ci@example.com> - 0.1.0-1
- Initial package
