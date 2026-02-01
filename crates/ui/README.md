# Racing Wheel UI

Tauri-based graphical user interface for OpenRacing.

## Overview

This crate provides a cross-platform GUI for managing racing wheel devices, profiles, and telemetry. It uses [Tauri 2.x](https://v2.tauri.app/) with WebKitGTK 4.1 on Linux for modern distribution compatibility.

## Platform Requirements

### Windows

No additional dependencies required. The UI crate builds out-of-the-box on Windows 10 and later.

### Linux

The UI crate requires WebKitGTK 4.1 and related GTK dependencies. The build script (`build.rs`) validates these dependencies at compile time and provides clear error messages if they are missing.

#### Ubuntu 22.04 LTS (Jammy Jellyfish)

Ubuntu 22.04 ships with webkit2gtk 2.38.x, which is the minimum supported version.

```bash
# Install required dependencies
sudo apt update
sudo apt install -y \
    libwebkit2gtk-4.1-dev \
    libgtk-3-dev \
    libayatana-appindicator3-dev \
    librsvg2-dev \
    pkg-config
```

**Note:** Ubuntu 22.04 provides webkit2gtk-4.1 version 2.38.x, which meets the minimum requirement (≥2.38.0) for Tauri 2.x.

#### Ubuntu 24.04 LTS (Noble Numbat)

Ubuntu 24.04 ships with webkit2gtk 2.44.x, which provides full Tauri 2.x compatibility with improved performance and security.

```bash
# Install required dependencies
sudo apt update
sudo apt install -y \
    libwebkit2gtk-4.1-dev \
    libgtk-3-dev \
    libayatana-appindicator3-dev \
    librsvg2-dev \
    pkg-config
```

**Note:** Ubuntu 24.04 provides webkit2gtk-4.1 version 2.44.x, which is recommended for optimal Tauri 2.x support.

#### Fedora 36+

```bash
sudo dnf install -y \
    webkit2gtk4.1-devel \
    gtk3-devel \
    libappindicator-gtk3-devel \
    librsvg2-devel \
    pkgconf-pkg-config
```

#### Arch Linux

```bash
sudo pacman -S \
    webkit2gtk-4.1 \
    gtk3 \
    libappindicator-gtk3 \
    librsvg \
    pkgconf
```

### macOS

macOS support is planned for a future release. The UI crate currently does not build on macOS.

## Version Compatibility Matrix

| Distribution | webkit2gtk Version | Tauri 2.x Support | Notes |
|--------------|-------------------|-------------------|-------|
| Ubuntu 22.04 | 2.38.x | ✅ Supported | Minimum supported version |
| Ubuntu 24.04 | 2.44.x | ✅ Recommended | Full feature support |
| Fedora 36+ | 2.38+ | ✅ Supported | |
| Debian 12 | 2.40.x | ✅ Supported | |
| Arch Linux | Latest | ✅ Recommended | Rolling release |

## Build Verification

The `build.rs` script performs the following checks on Linux:

1. **pkg-config availability**: Ensures the `pkg-config` tool is installed
2. **webkit2gtk-4.1 presence**: Verifies that webkit2gtk-4.1 is installed (not the older 4.0)
3. **Version validation**: Checks that the webkit2gtk version is ≥2.38.0

If any check fails, the build script emits a clear error message with installation instructions for your distribution.

## Troubleshooting

### "WEBKIT2GTK 4.1 NOT FOUND"

This error indicates that webkit2gtk-4.1 is not installed. Install it using the commands above for your distribution.

### "INCOMPATIBLE WEBKIT2GTK VERSION"

This error indicates that only webkit2gtk-4.0 is installed. Tauri 2.x requires webkit2gtk-4.1, which is available on:
- Ubuntu 22.04 and newer
- Fedora 36 and newer
- Debian 12 and newer

If your distribution doesn't provide webkit2gtk-4.1, you may need to upgrade to a newer distribution version.

### "WEBKIT2GTK VERSION TOO OLD"

This error indicates that webkit2gtk-4.1 is installed but the version is older than 2.38.0. Update your system packages:

```bash
# Ubuntu/Debian
sudo apt update && sudo apt upgrade libwebkit2gtk-4.1-dev

# Fedora
sudo dnf upgrade webkit2gtk4.1-devel
```

### "PKG-CONFIG NOT FOUND"

Install pkg-config using your distribution's package manager:

```bash
# Ubuntu/Debian
sudo apt install pkg-config

# Fedora
sudo dnf install pkgconf-pkg-config

# Arch Linux
sudo pacman -S pkgconf
```

## Development

### Building

```bash
# Build the UI crate
cargo build -p racing-wheel-ui

# Build with release optimizations
cargo build -p racing-wheel-ui --release
```

### Testing

```bash
# Run UI crate tests
cargo test -p racing-wheel-ui
```

## Related Documentation

- [Tauri 2.x Prerequisites](https://v2.tauri.app/start/prerequisites/)
- [Tauri 2.x Linux Setup](https://v2.tauri.app/start/prerequisites/#linux)
- [WebKitGTK](https://webkitgtk.org/)

## License

MIT OR Apache-2.0 (same as the OpenRacing project)
