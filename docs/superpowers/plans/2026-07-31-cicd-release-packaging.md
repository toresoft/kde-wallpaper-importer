# CI/CD Release Packaging Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a GitHub Actions release workflow that builds RPM (Fedora) and DEB (Ubuntu) packages on tag push and uploads them as GitHub Release assets, including all supporting packaging files.

**Architecture:** Two parallel jobs in a new `release.yml` workflow (triggered on `v*` tags). Each job runs in its distro container, builds the Rust binary with `cargo build --release`, then packages it using traditional `.spec` (RPM) and `debian/` (DEB) metadata stored in `packaging/`. The existing `ci.yml` (fmt/clippy/test on push/PR) is untouched.

**Tech Stack:** Rust/Cargo, rpmbuild (Fedora 44), dpkg-buildpackage/debhelper (Ubuntu 26.04), GitHub Actions.

## Global Constraints

- Binary name: `kde-wallpaper-import` (lib crate `kde_wallpaper_importer`, package `kde-wallpaper-importer`).
- Current version: `0.1.0` (from `Cargo.toml`). Version is injected into spec/changelog by CI from Cargo.toml.
- License: `GPL-3.0-or-later` (update Cargo.toml; was `MIT`).
- The `.desktop` file is generated from `share/kio/servicemenus/kde-wallpaper-importer.desktop.in` by substituting `@BINARY@` with the final binary path (`/usr/bin/kde-wallpaper-import`).
- Binary install path: `/usr/bin/kde-wallpaper-import`. Desktop install path: `/usr/share/kio/servicemenus/kde-wallpaper-importer.desktop`.
- RPM container: `fedora:44`. DEB container: `ubuntu:26.04`.
- Runtime deps (Recommends/Requires): `kdialog`, `notify-send` (`libnotify`/`libnotify-bin`), `plasma-apply-wallpaperimage` (`plasma-workspace`).
- `ci.yml` must NOT be modified.

---

### Task 1: Update Cargo.toml license to GPL-3.0-or-later

**Files:**
- Modify: `Cargo.toml:6`

**Interfaces:**
- Consumes: nothing
- Produces: a Cargo.toml whose `license` field matches the packaging files (all later tasks reference `GPL-3.0-or-later`).

- [ ] **Step 1: Update the license field**

Change line 6 of `Cargo.toml` from `license = "MIT"` to `license = "GPL-3.0-or-later"`.

```toml
license = "GPL-3.0-or-later"
```

- [ ] **Step 2: Verify Cargo.toml still parses**

Run: `cargo metadata --no-deps --format-version 1 > /dev/null && echo OK`
Expected: prints `OK` (valid TOML, valid manifest).

- [ ] **Step 3: Verify the build still works**

Run: `cargo build --release`
Expected: compiles successfully (license change has no build impact).

- [ ] **Step 4: Commit**

```bash
git add Cargo.toml
git commit -m "chore: switch license to GPL-3.0-or-later"
```

---

### Task 2: Create the RPM spec file

**Files:**
- Create: `packaging/rpm/kde-wallpaper-importer.spec`

**Interfaces:**
- Consumes: the source tree (Cargo.toml, src/, share/kio/servicemenus/kde-wallpaper-importer.desktop.in).
- Produces: a `.spec` that `rpmbuild -bb` turns into `kde-wallpaper-importer-0.1.0-1.fc44.x86_64.rpm`. CI (Task 4) sed-replaces the `Version:` line with the Cargo.toml version.

**Notes:** The spec uses a source tarball `Source0: %{name}-%{version}.tar.gz` that CI creates from the git checkout. `%prep` unpacks it; `%build` runs `cargo build --release`; `%install` installs the binary and generates the `.desktop` file from the `.in` template with `@BINARY@` → `%{_bindir}/kde-wallpaper-import`.

- [ ] **Step 1: Create the spec file**

Create `packaging/rpm/kde-wallpaper-importer.spec` with this exact content:

```specfile
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
```

- [ ] **Step 2: Verify the spec parses (rpmspec)**

Run: `rpmspec -P packaging/rpm/kde-wallpaper-importer.spec > /dev/null && echo OK`
Expected: prints `OK` (no syntax errors). If `rpmspec` is unavailable, skip — Task 5 covers a full build.

- [ ] **Step 3: Commit**

```bash
git add packaging/rpm/kde-wallpaper-importer.spec
git commit -m "packaging: add RPM spec for kde-wallpaper-importer"
```

---

### Task 3: Create the DEB packaging files

**Files:**
- Create: `packaging/debian/control`
- Create: `packaging/debian/changelog`
- Create: `packaging/debian/compat`
- Create: `packaging/debian/rules`
- Create: `packaging/debian/source/format`

**Interfaces:**
- Consumes: the source tree (same as Task 2).
- Produces: a `debian/` tree that, after being copied to the project root, `dpkg-buildpackage -b` turns into `kde-wallpaper-importer_0.1.0-1_amd64.deb`. CI (Task 4) regenerates `changelog` with the Cargo.toml version.

**Notes:**
- Native source format (`3.0 (native)`) so no separate `.orig.tar.gz` is required.
- All install logic lives in `debian/rules` overrides (binary copy + `.desktop` generation). No `debian/install` file — doing it in `rules` avoids dh_install ordering issues.
- `rules` must be executable (`chmod +x`).

- [ ] **Step 1: Create debian/control**

Create `packaging/debian/control`:

```
Source: kde-wallpaper-importer
Section: utils
Priority: optional
Maintainer: CI <ci@example.com>
Build-Depends: debhelper (>= 13), rustc, cargo, gcc, make
Standards-Version: 4.7.0

Package: kde-wallpaper-importer
Architecture: any
Depends: ${shlibs:Depends}, ${misc:Depends}, kdialog, libnotify-bin, plasma-workspace
Description: Import wallpapers into KDE Plasma from Dolphin
 kde-wallpaper-importer adds Dolphin context-menu entries to import image
 files as KDE Plasma wallpaper packages. It deduplicates by SHA-256 and
 resolves name collisions. Imported wallpapers can optionally be set as the
 active wallpaper via plasma-apply-wallpaperimage.
```

- [ ] **Step 2: Create debian/compat**

Create `packaging/debian/compat`:

```
13
```

- [ ] **Step 3: Create debian/source/format**

Create `packaging/debian/source/format`:

```
3.0 (native)
```

- [ ] **Step 4: Create debian/changelog**

Create `packaging/debian/changelog`:

```
kde-wallpaper-importer (0.1.0-1) unstable; urgency=medium

  * Initial package.

 -- CI <ci@example.com>  Fri, 31 Jul 2026 00:00:00 +0000
```

- [ ] **Step 5: Create debian/rules**

Create `packaging/debian/rules` (makefile; install logic in overrides):

```makefile
#!/usr/bin/make -f
%:
	dh $@

override_dh_auto_build:
	cargo build --release

override_dh_auto_install:
	install -Dm755 target/release/kde-wallpaper-import \
	    debian/kde-wallpaper-importer/usr/bin/kde-wallpaper-import
	install -d debian/kde-wallpaper-importer/usr/share/kio/servicemenus
	sed 's|@BINARY@|/usr/bin/kde-wallpaper-import|g' \
	    share/kio/servicemenus/kde-wallpaper-importer.desktop.in \
	    > debian/kde-wallpaper-importer/usr/share/kio/servicemenus/kde-wallpaper-importer.desktop

override_dh_auto_test:
	# skip; CI runs cargo test separately in ci.yml
```

- [ ] **Step 6: Make rules executable**

Run: `chmod +x packaging/debian/rules`
Expected: file mode changes to executable (verify with `ls -l packaging/debian/rules`).

- [ ] **Step 7: Commit**

```bash
git add packaging/debian/
git commit -m "packaging: add DEB control files for kde-wallpaper-importer"
```

---

### Task 4: Create the release.yml GitHub Actions workflow

**Files:**
- Create: `.github/workflows/release.yml`

**Interfaces:**
- Consumes: `packaging/rpm/kde-wallpaper-importer.spec` (Task 2), `packaging/debian/*` (Task 3), the source tree.
- Produces: on push of a `v*` tag, two release assets attached to the corresponding GitHub Release: one `.rpm`, one `.deb`.

**Notes:**
- Two jobs (`rpm`, `deb`) run in parallel, each in its container (`fedora:44`, `ubuntu:26.04`).
- Each job: checkout → install deps → cache Cargo → extract version from `Cargo.toml` → inject version into spec/changelog → build binary → build package → upload asset.
- RPM job creates the `Source0` tarball and sets up the rpmbuild tree.
- DEB job copies `packaging/debian/*` to repo root, then runs `dpkg-buildpackage -b`.
- Upload uses `softprops/action-gh-release@v2` (attaches assets to the release tied to the tag).

- [ ] **Step 1: Create the workflow file**

Create `.github/workflows/release.yml`:

```yaml
name: Release

on:
  push:
    tags:
      - 'v*'

permissions:
  contents: write

jobs:
  rpm:
    name: Build RPM (Fedora)
    runs-on: ubuntu-latest
    container: fedora:44
    steps:
      - name: Checkout
        uses: actions/checkout@v4

      - name: Install build dependencies
        run: |
          dnf install -y rust cargo gcc make rpm-build

      - name: Extract version
        id: version
        run: |
          VERSION=$(grep -m1 '^version' Cargo.toml | sed 's/.*"\(.*\)".*/\1/')
          echo "version=$VERSION" >> "$GITHUB_OUTPUT"

      - name: Inject version into spec
        run: |
          sed -i "s/^Version: .*/Version:        ${{ steps.version.outputs.version }}/" \
            packaging/rpm/kde-wallpaper-importer.spec

      - name: Cache Cargo
        uses: Swatinem/rust-cache@v2

      - name: Build release binary
        run: cargo build --release

      - name: Set up RPM build tree
        run: |
          mkdir -p ~/rpmbuild/{BUILD,RPMS,SOURCES,SPECS,SRPMS}

      - name: Create source tarball
        run: |
          VERSION=${{ steps.version.outputs.version }}
          tarball=~/rpmbuild/SOURCES/kde-wallpaper-importer-${VERSION}.tar.gz
          # Stage a clean directory named after the package-version
          mkdir -p /tmp/stage/kde-wallpaper-importer-${VERSION}
          tar -czf "$tarball" \
            --transform "s|^|kde-wallpaper-importer-${VERSION}/|" \
            Cargo.toml Cargo.lock src/ share/

      - name: Copy spec
        run: |
          cp packaging/rpm/kde-wallpaper-importer.spec ~/rpmbuild/SPECS/

      - name: Build RPM
        run: |
          rpmbuild -bb ~/rpmbuild/SPECS/kde-wallpaper-importer.spec

      - name: Upload RPM to release
        uses: softprops/action-gh-release@v2
        with:
          files: |
            /root/rpmbuild/RPMS/*/*.rpm
          fail_on_unmatched_files: true

  deb:
    name: Build DEB (Ubuntu)
    runs-on: ubuntu-latest
    container: ubuntu:26.04
    steps:
      - name: Checkout
        uses: actions/checkout@v4

      - name: Install build dependencies
        run: |
          apt-get update
          DEBIAN_FRONTEND=noninteractive apt-get install -y \
            rustc cargo gcc make dpkg-dev debhelper

      - name: Extract version
        id: version
        run: |
          VERSION=$(grep -m1 '^version' Cargo.toml | sed 's/.*"\(.*\)".*/\1/')
          echo "version=$VERSION" >> "$GITHUB_OUTPUT"

      - name: Cache Cargo
        uses: Swatinem/rust-cache@v2

      - name: Build release binary
        run: cargo build --release

      - name: Stage debian/ for build
        run: |
          # dpkg-buildpackage expects debian/ at the source root
          cp -r packaging/debian ./debian
          # Regenerate changelog with the exact version
          VERSION=${{ steps.version.outputs.version }}
          cat > debian/changelog << EOF
          kde-wallpaper-importer (${VERSION}-1) unstable; urgency=medium

            * Release ${VERSION}.

           -- CI <ci@example.com>  $(date -R)
          EOF

      - name: Build DEB
        run: |
          dpkg-buildpackage -us -uc -b

      - name: Collect DEB
        run: |
          mkdir -p artifacts
          cp ../*.deb artifacts/

      - name: Upload DEB to release
        uses: softprops/action-gh-release@v2
        with:
          files: |
            artifacts/*.deb
          fail_on_unmatched_files: true
```

- [ ] **Step 2: Lint the workflow YAML**

Run: `python3 -c "import yaml,sys; yaml.safe_load(open('.github/workflows/release.yml')); print('OK')"`
Expected: prints `OK` (valid YAML). If PyYAML is missing, install with `pip install pyyaml` or use `python3 -c "import sys; ..."` fallback — alternatively run `yamllint` if available.

- [ ] **Step 3: Verify ci.yml is untouched**

Run: `git diff HEAD -- .github/workflows/ci.yml`
Expected: empty output (no changes to existing CI).

- [ ] **Step 4: Commit**

```bash
git add .github/workflows/release.yml
git commit -m "ci: add release workflow to build RPM and DEB packages"
```

---

### Task 5: Local verification of package builds

**Goal:** Prove the `.spec` and `debian/` files actually produce installable packages before relying on CI. The host is Fedora 44, so RPM builds natively; DEB builds in a container.

**Files:** none (verification only).

- [ ] **Step 1: Build the RPM natively on the Fedora host**

Run from repo root:
```bash
VERSION=$(grep -m1 '^version' Cargo.toml | sed 's/.*"\(.*\)".*/\1/')
mkdir -p ~/rpmbuild/{BUILD,RPMS,SOURCES,SPECS,SRPMS}
tar -czf ~/rpmbuild/SOURCES/kde-wallpaper-importer-${VERSION}.tar.gz \
    --transform "s|^|kde-wallpaper-importer-${VERSION}/|" \
    Cargo.toml Cargo.lock src/ share/
cp packaging/rpm/kde-wallpaper-importer.spec ~/rpmbuild/SPECS/
rpmbuild -bb ~/rpmbuild/SPECS/kde-wallpaper-importer.spec
ls -l ~/rpmbuild/RPMS/x86_64/*.rpm
```
Expected: an RPM file exists at `~/rpmbuild/RPMS/x86_64/kde-wallpaper-importer-${VERSION}-1.fc44.x86_64.rpm`. If a `Requires`/`BuildRequires` is unsatisfiable on the host, install it with `dnf install` and retry.

- [ ] **Step 2: Inspect the built RPM contents**

Run:
```bash
rpm -qlp ~/rpmbuild/RPMS/x86_64/kde-wallpaper-importer-*.rpm
```
Expected output includes:
```
/usr/bin/kde-wallpaper-import
/usr/share/kio/servicemenus/kde-wallpaper-importer.desktop
```

- [ ] **Step 3: Build the DEB in an ubuntu:26.04 container**

Run (requires podman or docker):
```bash
podman run --rm -v "$PWD":/work -w /work ubuntu:26.04 bash -c '
  apt-get update &&
  DEBIAN_FRONTEND=noninteractive apt-get install -y rustc cargo gcc make dpkg-dev debhelper &&
  cp -r packaging/debian ./debian &&
  VERSION=$(grep -m1 "^version" Cargo.toml | sed "s/.*\"\(.*\)\".*/\1/") &&
  printf "kde-wallpaper-importer (${VERSION}-1) unstable; urgency=medium\n\n  * Release ${VERSION}.\n\n -- CI <ci@example.com>  $(date -R)\n" > debian/changelog &&
  dpkg-buildpackage -us -uc -b &&
  ls -l ../*.deb
'
```
Expected: a DEB file is produced in the parent dir (`kde-wallpaper-importer_0.1.0-1_amd64.deb`). If `ubuntu:26.04` is not yet available, substitute `ubuntu:24.04` for this local check (CI uses 26.04 as specified).

- [ ] **Step 4: Inspect the built DEB contents**

Run (after copying the .deb into the repo dir):
```bash
dpkg-deb -c /path/to/kde-wallpaper-importer_*.deb | grep -E 'kde-wallpaper-import$|kde-wallpaper-importer.desktop$'
```
Expected: two entries:
```
/usr/bin/kde-wallpaper-import
/usr/share/kio/servicemenus/kde-wallpaper-importer.desktop
```

- [ ] **Step 5: Clean up local build artifacts and confirm git state**

Run:
```bash
rm -rf debian   # the staged copy, not packaging/debian
git status
```
Expected: working tree clean except for committed files; the staged `./debian` copy is gone (it is gitignored or removed). If `./debian` would show as untracked, add `/debian/` to `.gitignore`.

- [ ] **Step 6: Final commit (if .gitignore updated)**

If `.gitignore` was updated in Step 5:
```bash
git add .gitignore
git commit -m "chore: ignore locally staged debian/ build dir"
```

---

## Self-Review

**1. Spec coverage:**
- CI tests/compiles on push/PR → existing `ci.yml` (untouched, verified in Task 4 Step 3). ✓
- Builds RPM for release → Task 2 (spec) + Task 4 (rpm job) + Task 5 (verify). ✓
- Builds DEB for release → Task 3 (debian/) + Task 4 (deb job) + Task 5 (verify). ✓
- Uploads as release assets → Task 4 `softprops/action-gh-release` in both jobs. ✓
- Fedora RPM target → `fedora:44` container. ✓
- Debian/Ubuntu DEB target → `ubuntu:26.04` container. ✓
- GPL-3.0-or-later → Task 1 (Cargo.toml) + spec `License:` + control. ✓

**2. Placeholder scan:** No TBD/TODO. All code blocks contain final content. ✓

**3. Type/path consistency:** Binary name `kde-wallpaper-import` consistent across Cargo.toml, Makefile, spec (`%files`), rules (install). Desktop path `kde-wallpaper-importer.desktop` consistent. `@BINARY@` substitution present in both spec and rules with the same target (`/usr/bin/kde-wallpaper-import`). ✓
