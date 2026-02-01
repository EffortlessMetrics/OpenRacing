# OpenRacing Windows Installer

This directory contains the Windows MSI installer configuration for OpenRacing.

## Overview

The OpenRacing Windows installer provides:
- **wheeld** - Background service daemon for real-time FFB processing
- **wheelctl** - Command-line interface tool
- **openracing** - Graphical user interface (Tauri-based)
- Windows service registration with automatic startup
- Device permissions configuration for racing wheel access
- MMCSS real-time thread priority registration
- Power management optimization (USB selective suspend disabled)

## System Requirements

- Windows 10 version 1903 or later (64-bit)
- Administrator privileges for installation
- USB port for racing wheel connection

## Building the Installer

### Prerequisites

1. Install [WiX Toolset v3.11+](https://wixtoolset.org/releases/)
2. Build the OpenRacing binaries:
   ```powershell
   cargo build --release
   ```

### Build Command

```powershell
.\build-msi.ps1 -BinPath "..\..\target\release"
```

### Build Options

| Parameter | Description | Default |
|-----------|-------------|---------|
| `-BinPath` | Path to compiled binaries (required) | - |
| `-OutputPath` | Output directory for MSI | `dist` |
| `-ConfigPath` | Path to configuration files | `config` |
| `-DocsPath` | Path to documentation files | `docs` |
| `-SigningCert` | Code signing certificate path | `$null` |
| `-Configuration` | Build configuration | `Release` |
| `-Platform` | Target platform | `x64` |
| `-Verify` | Verify MSI signature after build | `$false` |
| `-SkipValidation` | Skip binary validation | `$false` |

## Installation

### GUI Installation

1. Double-click `OpenRacing-<version>-x64.msi`
2. Follow the installation wizard
3. The service starts automatically after installation

### Silent Installation

Silent installation allows automated deployment without user interaction. This is useful for:
- Enterprise deployments
- CI/CD pipelines
- Scripted installations
- System imaging

#### Basic Silent Installation

```powershell
msiexec /i OpenRacing-<version>-x64.msi /quiet /norestart
```

#### Silent Installation with Logging

For troubleshooting installation issues, enable verbose logging:

```powershell
msiexec /i OpenRacing-<version>-x64.msi /quiet /norestart /l*v install.log
```

Log file options:
- `/l*v` - Verbose logging (recommended for troubleshooting)
- `/l*vx` - Extra verbose with debugging information
- `/l*` - Standard logging

#### Silent Installation with Progress Bar

Show a progress bar without requiring user interaction:

```powershell
msiexec /i OpenRacing-<version>-x64.msi /passive /norestart
```

#### Silent Installation with Custom Install Path

```powershell
msiexec /i OpenRacing-<version>-x64.msi /quiet /norestart INSTALLFOLDER="D:\OpenRacing"
```

#### Silent Installation - Feature Selection

Install only specific features:

```powershell
# Core only (service + CLI, no UI)
msiexec /i OpenRacing-<version>-x64.msi /quiet /norestart ADDLOCAL=MainFeature,DeviceDriverFeature

# Full installation with all features
msiexec /i OpenRacing-<version>-x64.msi /quiet /norestart ADDLOCAL=ALL

# Exclude documentation
msiexec /i OpenRacing-<version>-x64.msi /quiet /norestart ADDLOCAL=ALL REMOVE=DocumentationFeature
```

Available features:
| Feature | Description |
|---------|-------------|
| `MainFeature` | Core binaries (wheeld, wheelctl), service registration |
| `UIFeature` | OpenRacing graphical user interface |
| `DeviceDriverFeature` | Device permissions and power management |
| `PluginsFeature` | Plugin directories and support |
| `DocumentationFeature` | User guides and documentation |

### Silent Uninstallation

#### Using Product Code

```powershell
msiexec /x {PRODUCT-CODE-GUID} /quiet /norestart
```

#### Using MSI File

```powershell
msiexec /x OpenRacing-<version>-x64.msi /quiet /norestart
```

#### Silent Uninstallation with Logging

```powershell
msiexec /x OpenRacing-<version>-x64.msi /quiet /norestart /l*v uninstall.log
```

### Upgrade Installation

The installer supports automatic upgrades. Running a newer version will:
1. Stop the existing service
2. Upgrade all components
3. Preserve configuration files
4. Restart the service

```powershell
# Silent upgrade
msiexec /i OpenRacing-<new-version>-x64.msi /quiet /norestart
```

## MSI Properties Reference

The following public properties can be set during installation:

| Property | Description | Default |
|----------|-------------|---------|
| `INSTALLFOLDER` | Installation directory | `C:\Program Files\OpenRacing` |
| `ADDLOCAL` | Features to install | All features |
| `REMOVE` | Features to exclude | None |

### Example: Enterprise Deployment Script

```powershell
# Enterprise silent deployment script
param(
    [string]$MsiPath = "\\server\share\OpenRacing-1.0.0-x64.msi",
    [string]$LogPath = "C:\Logs\OpenRacing"
)

# Create log directory
New-Item -ItemType Directory -Force -Path $LogPath | Out-Null

$timestamp = Get-Date -Format "yyyyMMdd-HHmmss"
$logFile = Join-Path $LogPath "install-$timestamp.log"

# Silent installation with logging
$process = Start-Process -FilePath "msiexec.exe" -ArgumentList @(
    "/i", "`"$MsiPath`"",
    "/quiet",
    "/norestart",
    "/l*v", "`"$logFile`"",
    "INSTALLFOLDER=`"C:\Program Files\OpenRacing`""
) -Wait -PassThru

if ($process.ExitCode -eq 0) {
    Write-Host "Installation successful" -ForegroundColor Green
} elseif ($process.ExitCode -eq 3010) {
    Write-Host "Installation successful - reboot required" -ForegroundColor Yellow
} else {
    Write-Host "Installation failed with exit code: $($process.ExitCode)" -ForegroundColor Red
    Write-Host "Check log file: $logFile"
}
```

## MSI Exit Codes

| Code | Description |
|------|-------------|
| 0 | Success |
| 1602 | User cancelled installation |
| 1603 | Fatal error during installation |
| 1618 | Another installation is in progress |
| 1619 | Installation package could not be opened |
| 3010 | Success, reboot required |

## Post-Installation Verification

After installation, verify the setup:

```powershell
# Check service status
sc query OpenRacingService

# Verify CLI is accessible
wheelctl --version

# Check system health
wheelctl health

# List connected devices
wheelctl device list
```

## Troubleshooting

### Service Won't Start

1. Check Windows Event Viewer for errors
2. Verify service configuration:
   ```powershell
   sc qc OpenRacingService
   ```
3. Check service logs in `C:\ProgramData\OpenRacing\logs`

### Installation Fails

1. Run with verbose logging:
   ```powershell
   msiexec /i OpenRacing.msi /l*vx detailed.log
   ```
2. Check for:
   - Insufficient permissions (run as Administrator)
   - Conflicting software
   - Missing prerequisites

### Device Not Detected

1. Verify device is connected and powered
2. Check Device Manager for driver issues
3. Run device diagnostics:
   ```powershell
   wheelctl diag test
   ```

## Files Installed

| Location | Contents |
|----------|----------|
| `C:\Program Files\OpenRacing\bin` | Executables (wheeld.exe, wheelctl.exe, openracing.exe) |
| `C:\Program Files\OpenRacing\config` | Configuration files |
| `C:\Program Files\OpenRacing\profiles` | FFB profiles |
| `C:\Program Files\OpenRacing\plugins` | Plugin directories (wasm/, native/) |
| `C:\Program Files\OpenRacing\logs` | Log files |
| `C:\Program Files\OpenRacing\docs` | Documentation |
| `C:\ProgramData\OpenRacing` | Service state and cache |

## Registry Keys

The installer creates the following registry entries:

| Key | Purpose |
|-----|---------|
| `HKLM\SOFTWARE\OpenRacing` | Application settings |
| `HKLM\SOFTWARE\OpenRacing\DeviceAccess` | Device permission configuration |
| `HKLM\SOFTWARE\OpenRacing\SupportedDevices` | Supported wheel VID/PIDs |
| `HKLM\SYSTEM\CurrentControlSet\Services\OpenRacingService` | Service registration |
| `HKLM\SOFTWARE\Microsoft\Windows NT\CurrentVersion\Multimedia\SystemProfile\Tasks\OpenRacing` | MMCSS real-time priority |

## Related Documentation

- [User Guide](../../docs/user_guide.md)
- [CLI Reference](../../docs/cli.md)
- [Troubleshooting Guide](../../docs/troubleshooting.md)

## Requirements Traceability

This installer implements the following requirements:
- **6.1**: MSI package using WiX toolset
- **6.2**: Windows service registration
- **6.3**: Device permissions via SetupAPI
- **6.4**: Clean uninstallation
- **6.5**: Silent installation support (`msiexec /quiet`)
- **6.6**: Automatic service startup with appropriate privileges
