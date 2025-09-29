# Racing Wheel Suite MSI Build Script
# Requires WiX Toolset v3.11+ to be installed

param(
    [Parameter(Mandatory=$true)]
    [string]$BinPath,
    
    [Parameter(Mandatory=$false)]
    [string]$OutputPath = "dist",
    
    [Parameter(Mandatory=$false)]
    [string]$SigningCert = $null
)

$ErrorActionPreference = "Stop"

# Verify WiX is installed
$wixPath = Get-Command "candle.exe" -ErrorAction SilentlyContinue
if (-not $wixPath) {
    Write-Error "WiX Toolset not found. Please install WiX Toolset v3.11 or later."
    exit 1
}

# Verify binaries exist
$requiredBinaries = @("wheeld.exe", "wheelctl.exe", "wheel-ui.exe")
foreach ($binary in $requiredBinaries) {
    $binaryPath = Join-Path $BinPath $binary
    if (-not (Test-Path $binaryPath)) {
        Write-Error "Required binary not found: $binaryPath"
        exit 1
    }
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
$msiPath = Join-Path $OutputPath "RacingWheelSuite.msi"
& light.exe -out "$msiPath" "$wixObj"
if ($LASTEXITCODE -ne 0) {
    Write-Error "MSI linking failed"
    exit 1
}

# Sign MSI if certificate provided
if ($SigningCert) {
    Write-Host "Signing MSI..."
    & signtool.exe sign /f "$SigningCert" /t http://timestamp.digicert.com "$msiPath"
    if ($LASTEXITCODE -ne 0) {
        Write-Error "MSI signing failed"
        exit 1
    }
}

Write-Host "MSI created successfully: $msiPath"

# Verify MSI integrity
Write-Host "Verifying MSI integrity..."
$msiInfo = Get-ItemProperty $msiPath
Write-Host "MSI Size: $($msiInfo.Length) bytes"
Write-Host "MSI Created: $($msiInfo.CreationTime)"

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