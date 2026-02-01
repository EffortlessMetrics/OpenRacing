# OpenRacing MSI Build Script
# Builds Windows installer using WiX Toolset
# 
# Requirements: 6.1, 6.2, 6.3
# - Produces MSI package using WiX toolset
# - Registers wheeld as Windows service
# - Configures device permissions via SetupAPI
#
# Requires WiX Toolset v3.11+ to be installed

param(
    [Parameter(Mandatory=$true)]
    [string]$BinPath,
    
    [Parameter(Mandatory=$false)]
    [string]$OutputPath = "dist",
    
    [Parameter(Mandatory=$false)]
    [string]$ConfigPath = "config",
    
    [Parameter(Mandatory=$false)]
    [string]$DocsPath = "docs",
    
    [Parameter(Mandatory=$false)]
    [string]$AssetsPath = "assets",
    
    [Parameter(Mandatory=$false)]
    [string]$IconPath = "assets",
    
    [Parameter(Mandatory=$false)]
    [string]$SigningCert = $null,
    
    [Parameter(Mandatory=$false)]
    [string]$Configuration = "Release",
    
    [Parameter(Mandatory=$false)]
    [string]$Platform = "x64",
    
    [Parameter(Mandatory=$false)]
    [switch]$Verify = $false,
    
    [Parameter(Mandatory=$false)]
    [switch]$SkipValidation = $false,
    
    [Parameter(Mandatory=$false)]
    [string]$TimestampUrl = "http://timestamp.digicert.com"
)

$ErrorActionPreference = "Stop"
$ScriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path

Write-Host "========================================" -ForegroundColor Cyan
Write-Host "OpenRacing MSI Build Script" -ForegroundColor Cyan
Write-Host "========================================" -ForegroundColor Cyan

# Verify WiX is installed
$wixPath = Get-Command "candle.exe" -ErrorAction SilentlyContinue
if (-not $wixPath) {
    $wixPaths = @(
        "${env:ProgramFiles(x86)}\WiX Toolset v3.11\bin",
        "${env:ProgramFiles(x86)}\WiX Toolset v3.14\bin",
        "${env:ProgramFiles}\WiX Toolset v3.11\bin",
        "${env:ProgramFiles}\WiX Toolset v3.14\bin"
    )
    
    foreach ($path in $wixPaths) {
        if (Test-Path "$path\candle.exe") {
            $env:PATH = "$path;$env:PATH"
            $wixPath = Get-Command "candle.exe" -ErrorAction SilentlyContinue
            break
        }
    }
    
    if (-not $wixPath) {
        Write-Error "WiX Toolset not found. Please install WiX Toolset v3.11 or later from https://wixtoolset.org/releases/"
        exit 1
    }
}

Write-Host "WiX Toolset found: $($wixPath.Source)" -ForegroundColor Green

# Get version from Cargo.toml
$ProjectRoot = Split-Path -Parent (Split-Path -Parent $ScriptDir)
$CargoToml = Join-Path $ProjectRoot "Cargo.toml"
$Version = "0.1.0"
if (Test-Path $CargoToml) {
    $VersionMatch = Select-String -Path $CargoToml -Pattern '^version\s*=\s*"([^"]+)"'
    if ($VersionMatch) {
        $Version = $VersionMatch.Matches[0].Groups[1].Value
    }
}

Write-Host ""
Write-Host "Build Configuration:" -ForegroundColor Yellow
Write-Host "  Version:       $Version"
Write-Host "  Configuration: $Configuration"
Write-Host "  Platform:      $Platform"
Write-Host "  BinPath:       $BinPath"
Write-Host "  OutputPath:    $OutputPath"
Write-Host ""

# Resolve paths
$BinPath = Resolve-Path $BinPath -ErrorAction Stop
$ConfigPath = if (Test-Path $ConfigPath) { Resolve-Path $ConfigPath } else { Join-Path $ScriptDir "config" }
$DocsPath = if (Test-Path $DocsPath) { Resolve-Path $DocsPath } else { Join-Path $ScriptDir "docs" }
$AssetsPath = if (Test-Path $AssetsPath) { Resolve-Path $AssetsPath } else { Join-Path $ScriptDir "assets" }
$IconPath = if (Test-Path $IconPath) { Resolve-Path $IconPath } else { Join-Path $ScriptDir "assets" }

# Create required directories
$requiredDirs = @($ConfigPath, $DocsPath, $AssetsPath, $IconPath, $OutputPath)
foreach ($dir in $requiredDirs) {
    if (-not (Test-Path $dir)) {
        Write-Host "Creating directory: $dir" -ForegroundColor Yellow
        New-Item -ItemType Directory -Force -Path $dir | Out-Null
    }
}

# Create placeholder configuration files
$serviceToml = Join-Path $ConfigPath "service.toml"
if (-not (Test-Path $serviceToml)) {
    @"
# OpenRacing Service Configuration
[service]
log_level = "info"
ipc_socket = "openracing.sock"

[rt]
target_frequency_hz = 1000
watchdog_timeout_ms = 100

[safety]
max_torque_nm = 25.0
emergency_stop_enabled = true
"@ | Set-Content -Path $serviceToml -Encoding UTF8
}

$defaultToml = Join-Path $ConfigPath "default.toml"
if (-not (Test-Path $defaultToml)) {
    @"
# OpenRacing Default Configuration
[general]
first_run = true
telemetry_enabled = true

[ui]
theme = "dark"
language = "en"
"@ | Set-Content -Path $defaultToml -Encoding UTF8
}

$loggingToml = Join-Path $ConfigPath "logging.toml"
if (-not (Test-Path $loggingToml)) {
    @"
# OpenRacing Logging Configuration
[logging]
level = "info"
file = "logs/openracing.log"
max_size_mb = 10
max_files = 5
"@ | Set-Content -Path $loggingToml -Encoding UTF8
}

# Create placeholder documentation files
$pluginsReadme = Join-Path $DocsPath "plugins_readme.txt"
if (-not (Test-Path $pluginsReadme)) {
    @"
OpenRacing Plugin Directory

Place your plugins in the appropriate subdirectory:
  wasm/   - WebAssembly plugins (safe, sandboxed)
  native/ - Native plugins (.dll files, require signature)

For plugin development documentation, visit:
https://openracing.io/docs/plugins
"@ | Set-Content -Path $pluginsReadme -Encoding UTF8
}

$licenseTxt = Join-Path $DocsPath "LICENSE.txt"
if (-not (Test-Path $licenseTxt)) {
    @"
MIT License OR Apache License 2.0

Copyright (c) 2024 OpenRacing Contributors

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND.
"@ | Set-Content -Path $licenseTxt -Encoding UTF8
}

$changelogMd = Join-Path $DocsPath "CHANGELOG.md"
if (-not (Test-Path $changelogMd)) {
    @"
# Changelog

## [$Version] - $(Get-Date -Format "yyyy-MM-dd")

### Added
- Initial Windows installer release
- wheeld service with automatic startup
- wheelctl CLI tool
- OpenRacing UI application
"@ | Set-Content -Path $changelogMd -Encoding UTF8
}

$userGuide = Join-Path $DocsPath "user_guide.html"
if (-not (Test-Path $userGuide)) {
    @"
<!DOCTYPE html>
<html>
<head><title>OpenRacing User Guide</title></head>
<body>
<h1>OpenRacing User Guide</h1>
<p>Welcome to OpenRacing! Visit <a href="https://openracing.io">openracing.io</a> for documentation.</p>
</body>
</html>
"@ | Set-Content -Path $userGuide -Encoding UTF8
}

$licenseRtf = Join-Path $DocsPath "LICENSE.rtf"
if (-not (Test-Path $licenseRtf)) {
    @"
{\rtf1\ansi\deff0
{\fonttbl{\f0 Segoe UI;}}
\f0\fs20
\b MIT License OR Apache License 2.0\b0\par
\par
Copyright (c) 2024 OpenRacing Contributors\par
}
"@ | Set-Content -Path $licenseRtf -Encoding UTF8
}

# Create placeholder icon if needed
$iconFile = Join-Path $IconPath "openracing.ico"
if (-not (Test-Path $iconFile)) {
    Write-Host "Creating placeholder icon: $iconFile" -ForegroundColor Yellow
    $icoHeader = [byte[]]@(0,0,1,0,1,0,16,16,0,0,1,0,1,0,40,1,0,0,22,0,0,0)
    $bmpHeader = [byte[]]@(40,0,0,0,16,0,0,0,32,0,0,0,1,0,1,0,0,0,0,0,0,1,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0)
    $colorTable = [byte[]]@(0,0,0,0,255,255,255,0)
    $pixelData = [byte[]]@(0xFF,0xFF,0,0) * 32
    $icoData = $icoHeader + $bmpHeader + $colorTable + $pixelData
    [System.IO.File]::WriteAllBytes($iconFile, $icoData)
}

# Create placeholder banner/dialog images
$bannerFile = Join-Path $AssetsPath "banner.bmp"
$dialogFile = Join-Path $AssetsPath "dialog.bmp"

if (-not (Test-Path $bannerFile)) {
    Write-Host "Creating placeholder banner: $bannerFile" -ForegroundColor Yellow
    Add-Type -AssemblyName System.Drawing
    $banner = New-Object System.Drawing.Bitmap(493, 58)
    $g = [System.Drawing.Graphics]::FromImage($banner)
    $g.Clear([System.Drawing.Color]::FromArgb(37, 99, 235))
    $font = New-Object System.Drawing.Font("Segoe UI", 18, [System.Drawing.FontStyle]::Bold)
    $g.DrawString("OpenRacing", $font, [System.Drawing.Brushes]::White, 10, 12)
    $banner.Save($bannerFile, [System.Drawing.Imaging.ImageFormat]::Bmp)
    $g.Dispose()
    $banner.Dispose()
}

if (-not (Test-Path $dialogFile)) {
    Write-Host "Creating placeholder dialog: $dialogFile" -ForegroundColor Yellow
    Add-Type -AssemblyName System.Drawing
    $dialog = New-Object System.Drawing.Bitmap(493, 312)
    $g = [System.Drawing.Graphics]::FromImage($dialog)
    $g.Clear([System.Drawing.Color]::FromArgb(37, 99, 235))
    $font = New-Object System.Drawing.Font("Segoe UI", 24, [System.Drawing.FontStyle]::Bold)
    $g.DrawString("OpenRacing", $font, [System.Drawing.Brushes]::White, 20, 120)
    $subfont = New-Object System.Drawing.Font("Segoe UI", 12)
    $g.DrawString("Racing Wheel Force Feedback Suite", $subfont, [System.Drawing.Brushes]::White, 20, 160)
    $dialog.Save($dialogFile, [System.Drawing.Imaging.ImageFormat]::Bmp)
    $g.Dispose()
    $dialog.Dispose()
}

# Verify required binaries
$requiredBinaries = @(
    @{ Name = "wheeld.exe"; Required = $true; Description = "Service daemon" },
    @{ Name = "wheelctl.exe"; Required = $true; Description = "CLI tool" },
    @{ Name = "openracing.exe"; Required = $false; Description = "UI application" },
    @{ Name = "WebView2Loader.dll"; Required = $false; Description = "WebView2 runtime" }
)

Write-Host "Checking binaries..." -ForegroundColor Yellow
$missingRequired = $false
foreach ($binary in $requiredBinaries) {
    $binaryPath = Join-Path $BinPath $binary.Name
    if (Test-Path $binaryPath) {
        $fileInfo = Get-Item $binaryPath
        Write-Host "  [OK] $($binary.Name) ($([math]::Round($fileInfo.Length / 1KB)) KB)" -ForegroundColor Green
    } elseif ($binary.Required) {
        Write-Host "  [MISSING] $($binary.Name) - $($binary.Description)" -ForegroundColor Red
        $missingRequired = $true
    } else {
        Write-Host "  [SKIP] $($binary.Name) - $($binary.Description) (optional)" -ForegroundColor Yellow
    }
}

if ($missingRequired -and -not $SkipValidation) {
    Write-Error "Required binaries not found. Please build the project first: cargo build --release"
    exit 1
}

# Create output directory
New-Item -ItemType Directory -Force -Path $OutputPath | Out-Null

# Compile WiX source
Write-Host ""
Write-Host "Compiling WiX source..." -ForegroundColor Yellow

$wixObj = Join-Path $OutputPath "wheel-suite.wixobj"
$wxsFile = Join-Path $ScriptDir "wheel-suite.wxs"

$candleArgs = @(
    "-nologo",
    "-arch", "x64",
    "-dBinPath=$BinPath",
    "-dConfigPath=$ConfigPath",
    "-dDocsPath=$DocsPath",
    "-dAssetsPath=$AssetsPath",
    "-dIconPath=$IconPath",
    "-dVersion=$Version",
    "-ext", "WixUtilExtension",
    "-ext", "WixFirewallExtension",
    "-ext", "WixUIExtension",
    "-out", $wixObj,
    $wxsFile
)

Write-Host "  candle.exe $($candleArgs -join ' ')" -ForegroundColor DarkGray
& candle.exe @candleArgs

if ($LASTEXITCODE -ne 0) {
    Write-Error "WiX compilation failed (candle.exe returned $LASTEXITCODE)"
    exit 1
}

Write-Host "  Compilation successful" -ForegroundColor Green

# Link MSI
Write-Host ""
Write-Host "Linking MSI..." -ForegroundColor Yellow

$msiPath = Join-Path $OutputPath "OpenRacing-$Version-$Platform.msi"

$lightArgs = @(
    "-nologo",
    "-ext", "WixUtilExtension",
    "-ext", "WixFirewallExtension",
    "-ext", "WixUIExtension",
    "-cultures:en-us",
    "-out", $msiPath,
    $wixObj
)

Write-Host "  light.exe $($lightArgs -join ' ')" -ForegroundColor DarkGray
& light.exe @lightArgs

if ($LASTEXITCODE -ne 0) {
    Write-Error "MSI linking failed (light.exe returned $LASTEXITCODE)"
    exit 1
}

Write-Host "  MSI created: $msiPath" -ForegroundColor Green

# Sign MSI if certificate provided
if ($SigningCert) {
    Write-Host ""
    Write-Host "Signing MSI..." -ForegroundColor Yellow
    
    $signtoolArgs = @(
        "sign",
        "/f", $SigningCert,
        "/t", $TimestampUrl,
        "/d", "OpenRacing - Racing Wheel Force Feedback Suite",
        "/v",
        $msiPath
    )
    
    & signtool.exe @signtoolArgs
    
    if ($LASTEXITCODE -ne 0) {
        Write-Error "MSI signing failed"
        exit 1
    }
    Write-Host "  MSI signed successfully" -ForegroundColor Green
}

# Verify signature if requested
if ($Verify) {
    Write-Host ""
    Write-Host "Verifying MSI signature..." -ForegroundColor Yellow
    & signtool.exe verify /pa /v $msiPath
    if ($LASTEXITCODE -ne 0) {
        Write-Warning "MSI signature verification failed or MSI is not signed"
    } else {
        Write-Host "  MSI signature verified" -ForegroundColor Green
    }
}

# Generate checksums
Write-Host ""
Write-Host "Generating checksums..." -ForegroundColor Yellow

$Sha256Hash = (Get-FileHash -Path $msiPath -Algorithm SHA256).Hash
$Sha512Hash = (Get-FileHash -Path $msiPath -Algorithm SHA512).Hash

$ChecksumFile = "$msiPath.sha256"
"$Sha256Hash  $(Split-Path -Leaf $msiPath)" | Set-Content -Path $ChecksumFile
Write-Host "  SHA256: $Sha256Hash" -ForegroundColor DarkGray
Write-Host "  Checksum file: $ChecksumFile" -ForegroundColor Green

# Generate build metadata
$msiInfo = Get-ItemProperty $msiPath
$BuildMetadata = @{
    product = "OpenRacing"
    version = $Version
    platform = $Platform
    configuration = $Configuration
    build_time = (Get-Date).ToUniversalTime().ToString("yyyy-MM-ddTHH:mm:ssZ")
    build_host = $env:COMPUTERNAME
    msi_path = $msiPath
    msi_size_bytes = $msiInfo.Length
    msi_size_mb = [math]::Round($msiInfo.Length / 1MB, 2)
    sha256 = $Sha256Hash
    sha512 = $Sha512Hash
    signed = ($SigningCert -ne $null)
    features = @(
        "Core binaries (wheeld, wheelctl)",
        "UI application (openracing)",
        "Windows service registration",
        "Device permissions configuration",
        "Power management optimization",
        "MMCSS real-time priority",
        "Start menu shortcuts"
    )
    requirements = @{
        os = "Windows 10 version 1903 or later"
        architecture = "x64"
        admin_required = $true
    }
} | ConvertTo-Json -Depth 3

$MetadataFile = "$msiPath.metadata.json"
$BuildMetadata | Set-Content -Path $MetadataFile -Encoding UTF8
Write-Host "  Metadata file: $MetadataFile" -ForegroundColor Green

# Generate installation instructions
$installMd = @"
# OpenRacing Installation Instructions

## System Requirements
- Windows 10 version 1903 or later (64-bit)
- Administrator privileges for installation
- USB port for racing wheel connection

## Installation

### GUI Installation
1. Double-click OpenRacing-$Version-$Platform.msi
2. Follow the installation wizard
3. The service will start automatically after installation

### Silent Installation

Silent installation allows automated deployment without user interaction.

#### Basic Silent Installation
``````powershell
msiexec /i OpenRacing-$Version-$Platform.msi /quiet /norestart
``````

#### Silent Installation with Logging
``````powershell
msiexec /i OpenRacing-$Version-$Platform.msi /quiet /norestart /l*v install.log
``````

#### Silent Installation with Progress Bar
``````powershell
msiexec /i OpenRacing-$Version-$Platform.msi /passive /norestart
``````

#### Silent Installation with Custom Path
``````powershell
msiexec /i OpenRacing-$Version-$Platform.msi /quiet /norestart INSTALLFOLDER="D:\OpenRacing"
``````

#### Silent Installation - Core Only (No UI)
``````powershell
msiexec /i OpenRacing-$Version-$Platform.msi /quiet /norestart ADDLOCAL=MainFeature,DeviceDriverFeature
``````

### Silent Uninstallation
``````powershell
msiexec /x OpenRacing-$Version-$Platform.msi /quiet /norestart
``````

### Silent Uninstallation with Logging
``````powershell
msiexec /x OpenRacing-$Version-$Platform.msi /quiet /norestart /l*v uninstall.log
``````

## Available Features

| Feature | Description |
|---------|-------------|
| MainFeature | Core binaries (wheeld, wheelctl), service registration |
| UIFeature | OpenRacing graphical user interface |
| DeviceDriverFeature | Device permissions and power management |
| PluginsFeature | Plugin directories and support |
| DocumentationFeature | User guides and documentation |

## MSI Exit Codes

| Code | Description |
|------|-------------|
| 0 | Success |
| 1602 | User cancelled |
| 1603 | Fatal error |
| 3010 | Success, reboot required |

## Post-Installation

### Verify Installation
``````powershell
sc query OpenRacingService
wheelctl health
wheelctl device list
``````

## Checksums
- SHA256: $Sha256Hash

## More Information
See packaging/windows/README.md for complete documentation.
"@

$installMd | Out-File -FilePath (Join-Path $OutputPath "INSTALL.md") -Encoding UTF8
Write-Host "  Install guide: $(Join-Path $OutputPath 'INSTALL.md')" -ForegroundColor Green

# Summary
Write-Host ""
Write-Host "========================================" -ForegroundColor Cyan
Write-Host "Build Complete!" -ForegroundColor Green
Write-Host "========================================" -ForegroundColor Cyan
Write-Host ""
Write-Host "Output files:" -ForegroundColor Yellow
Write-Host "  MSI:      $msiPath"
Write-Host "  Size:     $([math]::Round($msiInfo.Length / 1MB, 2)) MB"
Write-Host "  Checksum: $ChecksumFile"
Write-Host "  Metadata: $MetadataFile"
Write-Host ""
Write-Host "To install:" -ForegroundColor Yellow
Write-Host "  msiexec /i `"$msiPath`""
Write-Host ""
