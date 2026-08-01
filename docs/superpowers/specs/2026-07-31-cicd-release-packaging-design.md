# CI/CD Release Packaging Design

**Date:** 2026-07-31
**Status:** Approved

## Objective

Create a GitHub Actions CI/CD pipeline that:
1. Tests and lints the project on every push/PR (already exists)
2. Builds RPM and DEB packages **only** on GitHub Release creation (tag `v*`)
3. Uploads the built packages as GitHub Release assets

## Decisions

| Decision | Choice | Rationale |
|----------|--------|-----------|
| Packaging approach | Separate matrix jobs with traditional `.spec` + `debian/` | Maximum flexibility, idiomatic GitHub Actions, parallel execution |
| Release trigger | GitHub Release only (tag `v*`) | Standard practice; CI (fmt/clippy/test) stays on every push/PR |
| Package upload | GitHub Release Assets | Simplest, no external repos or credentials needed |
| RPM container | `fedora:44` | User preference |
| DEB container | `ubuntu:26.04` | User preference |
| License | GPL-3.0-or-later | User preference; will also be added to Cargo.toml |

## File Structure

```
.github/
└── workflows/
    ├── ci.yml                              (existing, unchanged)
    └── release.yml                         (NEW)

packaging/
├── rpm/
│   └── kde-wallpaper-importer.spec         (NEW)
└── debian/
    ├── changelog                           (NEW)
    ├── compat                              (NEW)
    ├── control                             (NEW)
    ├── rules                               (NEW)
    ├── format                              (NEW)
    └── install                             (NEW)
```

The existing `ci.yml` is not modified.

## Workflow: `release.yml`

**Trigger:** `push` on tags matching `v*` (i.e., when a GitHub Release is created).

### Job: `rpm`

Runs in `fedora:44` container.

1. Checkout repository
2. Install build dependencies: `dnf install -y rust cargo gcc rpm-build make`
3. Cache Cargo registry + build artifacts (`Swatinem/rust-cache@v2`)
4. Build binary: `cargo build --release`
5. Set up RPM build tree: `~/rpmbuild/{BUILD,RPMS,SOURCES,SPECS,SRPMS}`
6. Extract version from `Cargo.toml`, inject into `.spec`
7. Prepare source tarball (binary, `.desktop.in` template, install script)
8. Copy spec to `~/rpmbuild/SPECS/`
9. `rpmbuild -bb` the spec
10. Upload `*.rpm` from `~/rpmbuild/RPMS/x86_64/` as release asset

### Job: `deb`

Runs in `ubuntu:26.04` container.

1. Checkout repository
2. Install build dependencies: `apt-get update && apt-get install -y rustc cargo gcc make dpkg-dev debhelper`
3. Cache Cargo registry + build artifacts (`Swatinem/rust-cache@v2`)
4. Build binary: `cargo build --release`
5. Set version in `debian/changelog` from `Cargo.toml`
6. Copy `packaging/debian/*` to project root for `dpkg-buildpackage`
7. `dpkg-buildpackage -us -uc -b`
8. Upload `*.deb` from parent directory as release asset

### Package Naming

- RPM: `kde-wallpaper-importer-{version}-1.fc44.x86_64.rpm`
- DEB: `kde-wallpaper-importer_{version}-1_amd64.deb`

Version is extracted from `Cargo.toml` `[package].version`.

## Packaging Files

### RPM: `packaging/rpm/kde-wallpaper-importer.spec`

- **Name:** `kde-wallpaper-importer`
- **Version:** dynamically set from Cargo.toml
- **Release:** `1%{?dist}`
- **Summary:** Import wallpapers into KDE Plasma from Dolphin
- **License:** GPL-3.0-or-later
- **BuildRequires:** `rust`, `cargo`, `gcc`, `make`
- **Requires:** `kdialog`, `notify-send`, `libnotify`, `plasma-workspace` (provides `plasma-apply-wallpaperimage`)
- **%install section:** Installs binary to `%{_bindir}`, generates `.desktop` from `.in` template to `%{_datadir}/kio/servicemenus/`
- **%files section:** Binary, desktop file

### DEB: `packaging/debian/control`

- **Section:** utils
- **Priority:** optional
- **Architecture:** any
- **Build-Depends:** `debhelper (>= 13)`, `rustc`, `cargo`, `gcc`, `make`
- **Depends:** `${shlibs:Depends}`, `${misc:Depends}`, `kdialog`, `libnotify-bin`, `plasma-workspace`
- **Description:** Import wallpapers into KDE Plasma from Dolphin

### DEB: `packaging/debian/rules`

Simple `dh`-based rules file that builds the binary with `cargo build --release` and installs it.

### DEB: `packaging/debian/install`

```
target/release/kde-wallpaper-import usr/bin
```

Desktop file is generated in the `rules` override.

## Cargo.toml Changes

Add `license = "GPL-3.0-or-later"` to the `[package]` section.

## Notes

- The `.desktop` file is generated from the `.in` template at build/install time by substituting `@BINARY@` with the final binary path. Both the `.spec` and `debian/rules` replicate this logic.
- `kbuildsycoca6` is called only in the Makefile `install` target for local development; in packages it is handled by the package manager's triggers or post-install scripts.
- No manpage or systemd service is required — this is a CLI tool invoked via Dolphin servicemenu.
