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

## Flatpak

The manifest is `packaging/io.github.matforge.MatForge.yaml` (GNOME Platform 47
runtime + the `rust-stable` SDK extension). Cargo builds offline in the sandbox,
so dependencies are vendored via `cargo-sources.json`:

```sh
flatpak install flathub org.gnome.Sdk//47 org.gnome.Platform//47 \
    org.freedesktop.Sdk.Extension.rust-stable//24.08
# one-time: vendor the crate sources (flatpak-builder-tools)
python3 flatpak-cargo-generator.py Cargo.lock -o packaging/cargo-sources.json
flatpak-builder --user --install --force-clean build-dir \
    packaging/io.github.matforge.MatForge.yaml
```

`matlabc` is an external native toolchain and is not bundled; the manifest grants
`--filesystem=host` so the app finds it via `$MATLABC_PATH` or
`~/.config/matforge/config.toml` on the host.
