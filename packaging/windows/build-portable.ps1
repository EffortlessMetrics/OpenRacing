# OpenRacing Portable ZIP Build Script
# Builds a portable Windows ZIP package that doesn't require installation
#
# Requirements: 19.2 - Windows packages: MSI installer and portable ZIP
#
# Usage:
#   .\build-portable.ps1 -BinPath <path> [-Version <version>] [-OutputPath <dir>]

param(
    [Parameter(Mandatory=$true)]
    [string]$BinPath,
    
    [Parameter(Mandatory=$false)]
    [string]$OutputPath = "dist",
    
    [Parameter(Mandatory=$false)]
    [string]$Version = "",
    
    [Parameter(Mandatory=$false)]
    [string]$Platform = "x64"
)

$ErrorActionPreference = "Stop"
$ScriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$ProjectRoot = Split-Path -Parent (Split-Path -Parent $ScriptDir)

Write-Host "========================================" -ForegroundColor Cyan
Write-Host "OpenRacing Portable ZIP Builder" -ForegroundColor Cyan
Write-Host "========================================" -ForegroundColor Cyan

# Get version from Cargo.toml if not specified
if ([string]::IsNullOrEmpty($Version)) {
    $CargoToml = Join-Path $ProjectRoot "Cargo.toml"
    if (Test-Path $CargoToml) {
        $VersionMatch = Select-String -Path $CargoToml -Pattern '^version\s*=\s*"([^"]+)"'
        if ($VersionMatch) {
            $Version = $VersionMatch.Matches[0].Groups[1].Value
        }
    }
    
    if ([string]::IsNullOrEmpty($Version)) {
        Write-Error "Could not determine version. Please specify -Version"
        exit 1
    }
}

Write-Host ""
Write-Host "Configuration:" -ForegroundColor Yellow
Write-Host "  Version:  $Version"
Write-Host "  Platform: $Platform"
Write-Host "  BinPath:  $BinPath"
Write-Host "  Output:   $OutputPath"
Write-Host ""

# Resolve paths
$BinPath = Resolve-Path $BinPath -ErrorAction Stop

# Create output directory
New-Item -ItemType Directory -Force -Path $OutputPath | Out-Null

# Define package name
$ZipName = "openracing-${Version}-windows-${Platform}-portable"
$ZipDir = Join-Path $OutputPath $ZipName

# Clean and create directory
if (Test-Path $ZipDir) {
    Remove-Item -Recurse -Force $ZipDir
}
New-Item -ItemType Directory -Force -Path $ZipDir | Out-Null
New-Item -ItemType Directory -Force -Path "$ZipDir\bin" | Out-Null
New-Item -ItemType Directory -Force -Path "$ZipDir\config" | Out-Null
New-Item -ItemType Directory -Force -Path "$ZipDir\profiles" | Out-Null
New-Item -ItemType Directory -Force -Path "$ZipDir\plugins\wasm" | Out-Null
New-Item -ItemType Directory -Force -Path "$ZipDir\plugins\native" | Out-Null
New-Item -ItemType Directory -Force -Path "$ZipDir\logs" | Out-Null
New-Item -ItemType Directory -Force -Path "$ZipDir\docs" | Out-Null

# Copy binaries
Write-Host "Copying binaries..." -ForegroundColor Yellow

$RequiredBinaries = @("wheeld.exe", "wheelctl.exe")
$OptionalBinaries = @("openracing.exe", "WebView2Loader.dll")

foreach ($binary in $RequiredBinaries) {
    $srcPath = Join-Path $BinPath $binary
    if (Test-Path $srcPath) {
        Copy-Item $srcPath "$ZipDir\bin\"
        Write-Host "  [OK] $binary" -ForegroundColor Green
    } else {
        Write-Error "Required binary not found: $srcPath"
        exit 1
    }
}

foreach ($binary in $OptionalBinaries) {
    $srcPath = Join-Path $BinPath $binary
    if (Test-Path $srcPath) {
        Copy-Item $srcPath "$ZipDir\bin\"
        Write-Host "  [OK] $binary (optional)" -ForegroundColor Green
    } else {
        Write-Host "  [SKIP] $binary (optional)" -ForegroundColor Yellow
    }
}

# Create default configuration
Write-Host "Creating configuration files..." -ForegroundColor Yellow

@"
# OpenRacing Service Configuration (Portable Mode)
[service]
log_level = "info"
ipc_socket = "openracing.sock"
portable_mode = true

[rt]
target_frequency_hz = 1000
watchdog_timeout_ms = 100

[safety]
max_torque_nm = 25.0
emergency_stop_enabled = true
"@ | Set-Content -Path "$ZipDir\config\service.toml" -Encoding UTF8

@"
# OpenRacing Default Configuration
[general]
first_run = true
telemetry_enabled = true

[ui]
theme = "dark"
language = "en"
"@ | Set-Content -Path "$ZipDir\config\default.toml" -Encoding UTF8

# Copy documentation
Write-Host "Copying documentation..." -ForegroundColor Yellow

$DocFiles = @(
    @{ Src = "README.md"; Dst = "docs\README.md" },
    @{ Src = "CHANGELOG.md"; Dst = "docs\CHANGELOG.md" },
    @{ Src = "LICENSE-MIT"; Dst = "docs\LICENSE-MIT" },
    @{ Src = "LICENSE-APACHE"; Dst = "docs\LICENSE-APACHE" },
    @{ Src = "LICENSE"; Dst = "docs\LICENSE" }
)

foreach ($doc in $DocFiles) {
    $srcPath = Join-Path $ProjectRoot $doc.Src
    if (Test-Path $srcPath) {
        Copy-Item $srcPath (Join-Path $ZipDir $doc.Dst)
        Write-Host "  [OK] $($doc.Src)" -ForegroundColor Green
    }
}

# Create README for portable version
@"
OpenRacing v${Version} - Portable Edition
==========================================

This is the portable version of OpenRacing. No installation required.

Quick Start
-----------
1. Run bin\wheelctl.exe health to verify the system
2. Run bin\wheeld.exe to start the service manually
3. Run bin\openracing.exe to launch the UI (if included)

Directory Structure
-------------------
bin\        - Executable files
config\     - Configuration files
profiles\   - FFB profiles
plugins\    - Plugin directory (wasm\ and native\)
logs\       - Log files

Running as a Service
--------------------
For best performance, you may want to run wheeld as a Windows service.
Use the MSI installer for automatic service registration.

Manual service registration (requires admin):
  sc create OpenRacingService binPath= "$(Get-Location)\bin\wheeld.exe --service"
  sc start OpenRacingService

Configuration
-------------
Edit config\service.toml to customize settings.
Edit config\default.toml for general preferences.

More Information
----------------
See docs\README.md for full documentation.
Visit: https://github.com/openracing/openracing
"@ | Set-Content -Path "$ZipDir\README.txt" -Encoding UTF8

# Create launcher batch files
@"
@echo off
REM OpenRacing Service Launcher
cd /d "%~dp0"
bin\wheeld.exe %*
"@ | Set-Content -Path "$ZipDir\start-service.bat" -Encoding ASCII

@"
@echo off
REM OpenRacing UI Launcher
cd /d "%~dp0"
if exist bin\openracing.exe (
    start "" bin\openracing.exe %*
) else (
    echo UI not included in this package.
    echo Use wheelctl for command-line interface.
    pause
)
"@ | Set-Content -Path "$ZipDir\start-ui.bat" -Encoding ASCII

@"
@echo off
REM OpenRacing Health Check
cd /d "%~dp0"
bin\wheelctl.exe health
bin\wheelctl.exe device list
pause
"@ | Set-Content -Path "$ZipDir\check-health.bat" -Encoding ASCII

# Create ZIP archive
Write-Host ""
Write-Host "Creating ZIP archive..." -ForegroundColor Yellow

$ZipPath = Join-Path $OutputPath "${ZipName}.zip"
if (Test-Path $ZipPath) {
    Remove-Item $ZipPath
}

Compress-Archive -Path $ZipDir -DestinationPath $ZipPath -CompressionLevel Optimal

# Clean up directory
Remove-Item -Recurse -Force $ZipDir

# Generate checksum
Write-Host "Generating checksum..." -ForegroundColor Yellow
$Hash = Get-FileHash $ZipPath -Algorithm SHA256
$ChecksumFile = "${ZipPath}.sha256"
"$($Hash.Hash.ToLower())  $(Split-Path -Leaf $ZipPath)" | Set-Content -Path $ChecksumFile -Encoding ASCII

# Summary
$ZipInfo = Get-Item $ZipPath

Write-Host ""
Write-Host "========================================" -ForegroundColor Cyan
Write-Host "Build Complete!" -ForegroundColor Green
Write-Host "========================================" -ForegroundColor Cyan
Write-Host ""
Write-Host "Output files:" -ForegroundColor Yellow
Write-Host "  ZIP:      $ZipPath"
Write-Host "  Size:     $([math]::Round($ZipInfo.Length / 1MB, 2)) MB"
Write-Host "  Checksum: $ChecksumFile"
Write-Host "  SHA256:   $($Hash.Hash.ToLower())"
