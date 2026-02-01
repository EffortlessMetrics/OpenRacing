//! Build script for racing-wheel-ui Tauri application
//!
//! This build script is required by Tauri 2.x to properly configure
//! the application at compile time. Tauri 2.x uses WebKitGTK 4.1 on Linux,
//! which provides compatibility with Ubuntu 24.04 and other modern distributions.
//!
//! On Linux, this script validates that webkit2gtk 4.1+ is installed and provides
//! clear error messages if the dependency is missing or incompatible.
//!
//! ## Ubuntu Version Support
//!
//! | Ubuntu Version | webkit2gtk Version | Support Status |
//! |----------------|-------------------|----------------|
//! | 22.04 LTS      | 2.38.x            | ✅ Minimum supported |
//! | 24.04 LTS      | 2.44.x            | ✅ Recommended |
//!
//! ## Installation Commands
//!
//! Ubuntu 22.04/24.04:
//! ```bash
//! sudo apt install libwebkit2gtk-4.1-dev libgtk-3-dev libayatana-appindicator3-dev
//! ```
//!
//! See `crates/ui/README.md` for complete installation instructions.

fn main() {
    // Platform-specific webkit2gtk validation
    #[cfg(target_os = "linux")]
    validate_webkit2gtk();

    // Tauri build process - handles platform-specific configuration
    // On Linux, this ensures proper WebKitGTK 4.1 linkage
    tauri_build::build();
}

/// Validates webkit2gtk installation and version on Linux.
///
/// Tauri 2.x requires WebKitGTK 4.1 for compatibility with modern Linux
/// distributions like Ubuntu 24.04. This function checks:
/// 1. That webkit2gtk-4.1 is installed
/// 2. That the version is compatible (>= 2.40.0 for full Tauri 2.x support)
///
/// If validation fails, a clear compile-time error is emitted with
/// resolution steps for the user.
#[cfg(target_os = "linux")]
fn validate_webkit2gtk() {
    use std::process::Command;

    // Tell cargo to re-run this script if pkg-config output changes
    println!("cargo:rerun-if-env-changed=PKG_CONFIG_PATH");

    // First, try to find webkit2gtk-4.1 (required for Tauri 2.x)
    let webkit_41_result = Command::new("pkg-config")
        .args(["--exists", "webkit2gtk-4.1"])
        .status();

    match webkit_41_result {
        Ok(status) if status.success() => {
            // webkit2gtk-4.1 is installed, now check the version
            if let Err(e) = check_webkit2gtk_version() {
                emit_version_error(&e);
            }
        }
        Ok(_) => {
            // pkg-config ran but webkit2gtk-4.1 was not found
            // Check if the older webkit2gtk-4.0 is installed (common on older distros)
            let webkit_40_exists = Command::new("pkg-config")
                .args(["--exists", "webkit2gtk-4.0"])
                .status()
                .map(|s| s.success())
                .unwrap_or(false);

            if webkit_40_exists {
                emit_wrong_version_error();
            } else {
                emit_not_installed_error();
            }
        }
        Err(_) => {
            // pkg-config itself is not available
            emit_pkg_config_missing_error();
        }
    }
}

/// Checks the webkit2gtk-4.1 version and returns an error if incompatible.
#[cfg(target_os = "linux")]
fn check_webkit2gtk_version() -> Result<(), String> {
    use std::process::Command;

    let output = Command::new("pkg-config")
        .args(["--modversion", "webkit2gtk-4.1"])
        .output()
        .map_err(|e| format!("Failed to query webkit2gtk version: {}", e))?;

    if !output.status.success() {
        return Err("pkg-config --modversion webkit2gtk-4.1 failed".to_string());
    }

    let version_str = String::from_utf8_lossy(&output.stdout);
    let version_str = version_str.trim();

    // Parse version (format: major.minor.patch)
    let parts: Vec<&str> = version_str.split('.').collect();
    if parts.len() < 2 {
        return Err(format!(
            "Invalid webkit2gtk version format: {}",
            version_str
        ));
    }

    let major: u32 = parts[0]
        .parse()
        .map_err(|_| format!("Invalid major version: {}", parts[0]))?;
    let minor: u32 = parts[1]
        .parse()
        .map_err(|_| format!("Invalid minor version: {}", parts[1]))?;

    // Tauri 2.x works best with webkit2gtk >= 2.40.0
    // Minimum supported is 2.38.0 (Ubuntu 22.04 ships 2.38.x)
    const MIN_MAJOR: u32 = 2;
    const MIN_MINOR: u32 = 38;

    if major < MIN_MAJOR || (major == MIN_MAJOR && minor < MIN_MINOR) {
        return Err(format!(
            "webkit2gtk version {} is too old. Minimum required: {}.{}.0",
            version_str, MIN_MAJOR, MIN_MINOR
        ));
    }

    // Emit the detected version for build logs
    println!(
        "cargo:warning=Detected webkit2gtk-4.1 version: {}",
        version_str
    );

    Ok(())
}

/// Emits a compile error when webkit2gtk-4.1 is not installed.
#[cfg(target_os = "linux")]
fn emit_not_installed_error() -> ! {
    eprintln!();
    eprintln!("╔══════════════════════════════════════════════════════════════════════════════╗");
    eprintln!("║                     WEBKIT2GTK 4.1 NOT FOUND                                 ║");
    eprintln!("╠══════════════════════════════════════════════════════════════════════════════╣");
    eprintln!("║ The racing-wheel-ui crate requires webkit2gtk-4.1 for Tauri 2.x support.    ║");
    eprintln!("║                                                                              ║");
    eprintln!("║ To install on Ubuntu/Debian (22.04+):                                        ║");
    eprintln!(
        "║   sudo apt install libwebkit2gtk-4.1-dev libgtk-3-dev libayatana-appindicator3-dev ║"
    );
    eprintln!("║                                                                              ║");
    eprintln!("║ To install on Fedora (36+):                                                  ║");
    eprintln!("║   sudo dnf install webkit2gtk4.1-devel gtk3-devel libappindicator-gtk3-devel║");
    eprintln!("║                                                                              ║");
    eprintln!("║ To install on Arch Linux:                                                    ║");
    eprintln!("║   sudo pacman -S webkit2gtk-4.1 gtk3 libappindicator-gtk3                   ║");
    eprintln!("║                                                                              ║");
    eprintln!("║ For other distributions, please consult your package manager documentation. ║");
    eprintln!("║ See: https://v2.tauri.app/start/prerequisites/#linux                        ║");
    eprintln!("╚══════════════════════════════════════════════════════════════════════════════╝");
    eprintln!();
    std::process::exit(1);
}

/// Emits a compile error when only webkit2gtk-4.0 is installed (wrong version).
#[cfg(target_os = "linux")]
fn emit_wrong_version_error() -> ! {
    eprintln!();
    eprintln!("╔══════════════════════════════════════════════════════════════════════════════╗");
    eprintln!("║                   INCOMPATIBLE WEBKIT2GTK VERSION                            ║");
    eprintln!("╠══════════════════════════════════════════════════════════════════════════════╣");
    eprintln!("║ Found webkit2gtk-4.0, but Tauri 2.x requires webkit2gtk-4.1.                 ║");
    eprintln!("║                                                                              ║");
    eprintln!("║ webkit2gtk-4.1 is available on:                                              ║");
    eprintln!("║   - Ubuntu 22.04 LTS and newer                                               ║");
    eprintln!("║   - Fedora 36 and newer                                                      ║");
    eprintln!("║   - Debian 12 (Bookworm) and newer                                           ║");
    eprintln!("║   - Arch Linux (rolling release)                                             ║");
    eprintln!("║                                                                              ║");
    eprintln!("║ To upgrade on Ubuntu/Debian:                                                 ║");
    eprintln!("║   sudo apt install libwebkit2gtk-4.1-dev                                     ║");
    eprintln!("║                                                                              ║");
    eprintln!("║ If webkit2gtk-4.1 is not available in your distribution's repositories,     ║");
    eprintln!("║ you may need to upgrade to a newer distribution version.                    ║");
    eprintln!("║                                                                              ║");
    eprintln!("║ See: https://v2.tauri.app/start/prerequisites/#linux                        ║");
    eprintln!("╚══════════════════════════════════════════════════════════════════════════════╝");
    eprintln!();
    std::process::exit(1);
}

/// Emits a compile error when the webkit2gtk version is too old.
#[cfg(target_os = "linux")]
fn emit_version_error(details: &str) -> ! {
    eprintln!();
    eprintln!("╔══════════════════════════════════════════════════════════════════════════════╗");
    eprintln!("║                   WEBKIT2GTK VERSION TOO OLD                                 ║");
    eprintln!("╠══════════════════════════════════════════════════════════════════════════════╣");
    eprintln!("║ {:<76} ║", details);
    eprintln!("║                                                                              ║");
    eprintln!("║ Tauri 2.x requires webkit2gtk-4.1 version 2.38.0 or newer.                   ║");
    eprintln!("║                                                                              ║");
    eprintln!("║ To upgrade on Ubuntu/Debian:                                                 ║");
    eprintln!("║   sudo apt update && sudo apt upgrade libwebkit2gtk-4.1-dev                  ║");
    eprintln!("║                                                                              ║");
    eprintln!("║ If your distribution doesn't provide a newer version, you may need to       ║");
    eprintln!("║ upgrade to a newer distribution release.                                    ║");
    eprintln!("║                                                                              ║");
    eprintln!("║ See: https://v2.tauri.app/start/prerequisites/#linux                        ║");
    eprintln!("╚══════════════════════════════════════════════════════════════════════════════╝");
    eprintln!();
    std::process::exit(1);
}

/// Emits a compile error when pkg-config is not available.
#[cfg(target_os = "linux")]
fn emit_pkg_config_missing_error() -> ! {
    eprintln!();
    eprintln!("╔══════════════════════════════════════════════════════════════════════════════╗");
    eprintln!("║                       PKG-CONFIG NOT FOUND                                   ║");
    eprintln!("╠══════════════════════════════════════════════════════════════════════════════╣");
    eprintln!("║ The pkg-config tool is required to detect webkit2gtk installation.          ║");
    eprintln!("║                                                                              ║");
    eprintln!("║ To install on Ubuntu/Debian:                                                 ║");
    eprintln!("║   sudo apt install pkg-config                                                ║");
    eprintln!("║                                                                              ║");
    eprintln!("║ To install on Fedora:                                                        ║");
    eprintln!("║   sudo dnf install pkgconf-pkg-config                                        ║");
    eprintln!("║                                                                              ║");
    eprintln!("║ To install on Arch Linux:                                                    ║");
    eprintln!("║   sudo pacman -S pkgconf                                                     ║");
    eprintln!("║                                                                              ║");
    eprintln!("║ After installing pkg-config, you will also need webkit2gtk-4.1:             ║");
    eprintln!("║   Ubuntu/Debian: sudo apt install libwebkit2gtk-4.1-dev                      ║");
    eprintln!("║   Fedora: sudo dnf install webkit2gtk4.1-devel                               ║");
    eprintln!("║   Arch: sudo pacman -S webkit2gtk-4.1                                        ║");
    eprintln!("╚══════════════════════════════════════════════════════════════════════════════╝");
    eprintln!();
    std::process::exit(1);
}
