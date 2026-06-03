# Packaging & install

MatForge builds to a single `matforge` binary that needs GTK 4 (`libgtk-4-1`) at
runtime. The compiler (`matlabc`) is located at runtime via `$MATLABC_PATH` or
`~/.config/matforge/config.toml`, so it is **not** bundled.

## Local install (no root, recommended for dev)

```sh
just install          # builds release, installs under ~/.local
```

This drops:

| File | Destination |
|------|-------------|
| `matforge` binary | `~/.local/bin/matforge` |
| desktop entry | `~/.local/share/applications/matforge.desktop` |
| app icon | `~/.local/share/icons/hicolor/scalable/apps/matforge.svg` |

Ensure `~/.local/bin` is on `PATH`. Override the prefix with `PREFIX=/usr/local
just install` (needs write access). Remove with `just uninstall`.

## Tarball

```sh
just dist             # -> dist/matforge-linux-x86_64.tar.gz
```

A relocatable archive with the binary, desktop entry, and icon.

## Debian package

```sh
cargo install cargo-deb     # one-time
just deb                    # -> target/debian/matforge_<version>_amd64.deb
```

Packaging metadata (maintainer, depends on `libgtk-4-1`, asset map) lives in the
`[package.metadata.deb]` block of `crates/app/Cargo.toml`.

## Flatpak (sketch)

A Flatpak manifest is not yet checked in. The runtime needs the
`org.gnome.Platform` runtime (provides GTK 4); the `matlabc` toolchain would be
exposed via a host-files filesystem permission or a separate extension, since it
is an external native compiler rather than a bundled dependency.
