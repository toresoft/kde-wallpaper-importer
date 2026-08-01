# KDE Wallpaper Importer

Adds two entries to the Dolphin context menu for importing an image into the
KDE wallpaper library:

- **Import as wallpaper** — installs the package only
- **Import and set as wallpaper** — installs and applies immediately

The image becomes a KPackage in
`~/.local/share/wallpapers/<Name>/`, so it appears in the Plasma wallpaper
selector with a preview and can be uninstalled from the GUI.

## Prerequisites

A stable Rust toolchain (1.80 or newer):

```bash
sudo dnf install -y rust cargo        # or: rustup toolchain install stable
```

If you installed Rust via rustup, make sure `~/.cargo/bin` is in your `PATH`
before running `make`.

At runtime, `kdialog`, `notify-send`, and `plasma-apply-wallpaperimage` are
required, all already present on a standard Plasma installation. The program
degrades gracefully without crashing if any are missing.

## Installation

```bash
make install              # to ~/.local
make install PREFIX=/usr/local   # system-wide for all users (requires sudo)
```

If the entries don't appear, close and reopen Dolphin.

## Uninstallation

```bash
make uninstall
```

Already imported wallpapers are not touched: remove them from the Plasma
selector or by deleting the directory under `~/.local/share/wallpapers/`.

## Command-line usage

```
kde-wallpaper-import [OPTIONS] <FILE>...
  --apply              set the last imported package as wallpaper
  --fill-mode <MODE>   passed to plasma-apply-wallpaperimage
  --dest <DIR>         destination root
  --no-ui              no dialogs or notifications
  --force              skip confirmations for small images
  --min-size <WxH>     warning threshold (default 1024x768)
```

Exit codes: `0` no errors, `1` partial success, `2` no files handled without
errors, `64` usage error.

## How it avoids collisions

The package name is derived from the filename (spaces → `_`, max 60
characters). Before writing:

1. if an already imported package has the **same content** (SHA-256), the file
   is skipped as a duplicate;
2. otherwise the first free name is found — `forest`, `forest-2`, … — comparing
   **case-insensitive** against all `wallpapers` directories in
   `$XDG_DATA_HOME` and `$XDG_DATA_DIRS`, including system ones.

Step 2 includes `/usr/share/wallpapers` because a user package with the same
Id as a system one would shadow it, making the original disappear from the
selector.

Writing is atomic: the package is built in a temporary directory and moved with
`renameat2(RENAME_NOREPLACE)`. There are no half-written packages, even if the
program is interrupted.

## Supported formats

JPEG, PNG, WebP, TIFF, BMP. AVIF and JXL are excluded: neither Pillow nor
ImageMagick decodes them on the reference Fedora installation, and the `image`
crate would require `libdav1d`.

## Note

If `setAsWallpaperFive.desktop` is present in
`~/.local/share/kio/servicemenus/`, that entry sets the wallpaper via
`gsettings` (GNOME) and has no effect on Plasma. This project does not modify
it.

## Development

```bash
cargo test
cargo clippy --all-targets -- -D warnings
cargo fmt --check
```
