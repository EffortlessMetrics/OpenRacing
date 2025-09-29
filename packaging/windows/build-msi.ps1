# Racing Wheel Suite MSI Build Script
# Enhanced with security features and reproducible builds
# Requires WiX Toolset v3.11+ to be installed

param(
    [Parameter(Mandatory=$true)]
    [string]$BinPath,
    
    [Parameter(Mandatory=$false)]
    [string]$OutputPath = "dist",
    
    [Parameter(Mandatory=$false)]
    [string]$SigningCert = $null,
    
    [Parameter(Mandatory=$false)]
    [string]$Configuration = "Release",
    
    [Parameter(Mandatory=$false)]
    [string]$Platform = "x64",
    
    [Parameter(Mandatory=$false)]
    [switch]$Verify = $false,
    
    [Parameter(Mandatory=$false)]
    [string]$TimestampUrl = "http://timestamp.digicert.com"
)

$ErrorActionPreference = "Stop"

# Verify WiX is installed
$wixPath = Get-Command "candle.exe" -ErrorAction SilentlyContinue
if (-not $wixPath) {
    Write-Error "WiX Toolset not found. Please install WiX Toolset v3.11 or later."
    exit 1
}

# Get version from Cargo.toml
$ProjectRoot = Split-Path -Parent (Split-Path -Parent $PSScriptRoot)
$CargoToml = Join-Path $ProjectRoot "Cargo.toml"
$Version = "0.1.0"  # Default version
if (Test-Path $CargoToml) {
    $VersionMatch = Select-String -Path $CargoToml -Pattern '^version\s*=\s*"([^"]+)"'
    if ($VersionMatch) {
        $Version = $VersionMatch.Matches[0].Groups[1].Value
    }
}

Write-Host "Building MSI installer for Racing Wheel Suite v$Version" -ForegroundColor Green
Write-Host "Configuration: $Configuration" -ForegroundColor Cyan
Write-Host "Platform: $Platform" -ForegroundColor Cyan

# Verify binaries exist
$requiredBinaries = @("wheeld.exe", "wheelctl.exe", "wheel-ui.exe")
foreach ($binary in $requiredBinaries) {
    $binaryPath = Join-Path $BinPath $binary
    if (-not (Test-Path $binaryPath)) {
        Write-Error "Required binary not found: $binaryPath"
        Write-Host "Please build the project first with: cargo build --release" -ForegroundColor Yellow
        exit 1
    }
    Write-Host "Found binary: $binary" -ForegroundColor Green
}

# Create output directory
New-Item -ItemType Directory -Force -Path $OutputPath | Out-Null

# Compile WiX source
Write-Host "Compiling WiX source..."
$wixObj = Join-Path $OutputPath "wheel-suite.wixobj"
& candle.exe -dBinPath="$BinPath" -out "$wixObj" "wheel-suite.wxs"
if ($LASTEXITCODE -ne 0) {
    Write-Error "WiX compilation failed"
    exit 1
}

# Link MSI
Write-Host "Linking MSI..."
$msiPath = Join-Path $OutputPath "RacingWheelSuite-$Version-$Platform.msi"
& light.exe -out "$msiPath" "$wixObj"
if ($LASTEXITCODE -ne 0) {
    Write-Error "MSI linking failed"
    exit 1
}

Write-Host "MSI created: $msiPath" -ForegroundColor Green

# Sign MSI if certificate provided
if ($SigningCert) {
    Write-Host "Signing MSI..." -ForegroundColor Yellow
    & signtool.exe sign /f "$SigningCert" /t $TimestampUrl /v "$msiPath"
    if ($LASTEXITCODE -ne 0) {
        Write-Error "MSI signing failed"
        exit 1
    }
    Write-Host "MSI signed successfully" -ForegroundColor Green
}

# Verify signature if requested
if ($Verify) {
    Write-Host "Verifying MSI signature..." -ForegroundColor Yellow
    & signtool.exe verify /pa /v "$msiPath"
    if ($LASTEXITCODE -ne 0) {
        Write-Warning "MSI signature verification failed or MSI is not signed"
    } else {
        Write-Host "MSI signature verified" -ForegroundColor Green
    }
}

# Generate checksums
Write-Host "Generating checksums..." -ForegroundColor Yellow
$Sha256Hash = (Get-FileHash -Path $msiPath -Algorithm SHA256).Hash
$Sha512Hash = (Get-FileHash -Path $msiPath -Algorithm SHA512).Hash

$ChecksumFile = "$msiPath.checksums"
@"
SHA256: $Sha256Hash
SHA512: $Sha512Hash
"@ | Set-Content -Path $ChecksumFile

Write-Host "Checksums saved to: $ChecksumFile" -ForegroundColor Green

# Generate build metadata
$msiInfo = Get-ItemProperty $msiPath
$BuildMetadata = @{
    version = $Version
    platform = $Platform
    configuration = $Configuration
    build_time = (Get-Date).ToUniversalTime().ToString("yyyy-MM-ddTHH:mm:ssZ")
    msi_path = $msiPath
    msi_size = $msiInfo.Length
    sha256 = $Sha256Hash
    sha512 = $Sha512Hash
    signed = $SigningCert -ne $null
    verified = $Verify
} | ConvertTo-Json -Depth 2

$MetadataFile = "$msiPath.metadata.json"
$BuildMetadata | Set-Content -Path $MetadataFile

Write-Host "Build metadata saved to: $MetadataFile" -ForegroundColor Green
Write-Host "MSI created successfully!" -ForegroundColor Green

# Generate installation instructions
$instructions = @"
# Racing Wheel Suite Installation Instructions

## System Requirements
- Windows 10 version 1903 or later
- .NET Runtime 6.0 or later
- Administrator privileges for initial setup (udev rules equivalent)

## Installation Steps
1. Run RacingWheelSuite.msi as administrator for first-time setup
2. Follow the installation wizard
3. The service will start automatically on user login
4. Launch "Racing Wheel Suite" from Start Menu

## Post-Installation Configuration
1. Connect your racing wheel
2. Open Racing Wheel Suite UI
3. Complete the first-run wizard
4. Configure power management settings if needed

## Uninstallation
Use "Add or Remove Programs" in Windows Settings, or run:
msiexec /x {ProductCode} /quiet

## Troubleshooting
- Check Windows Event Log for service errors
- Verify USB selective suspend is disabled for racing wheels
- Ensure Windows is in High Performance power mode for best results
"@

$instructions | Out-File -FilePath (Join-Path $OutputPath "INSTALL.md") -Encoding UTF8

Write-Host "Installation complete. See $OutputPath\INSTALL.md for deployment instructions."